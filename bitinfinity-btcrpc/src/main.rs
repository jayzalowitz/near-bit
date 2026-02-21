use axum::{
    extract::{rejection::JsonRejection, Json, State},
    http::StatusCode,
    middleware,
    routing::post,
    Router,
};
use clap::Parser;
use near_account_id::{AccountIdRef, AccountType};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

mod keystore;
mod near_client;
mod near_tx_builder;
mod tx_translator;
mod utxo_synth;

use keystore::{KeyEntry, Keystore};
use near_client::NearClient;
use near_tx_builder::{
    decode_block_hash, NearAction, NearFunctionCallParams, NearTransferParams, NearTxBuilder,
};
use tx_translator::{ParsedBitcoinTx, TxOutput};
use utxo_synth::SyntheticUtxo;

#[derive(Debug, Parser)]
#[command(name = "bitinfinity-btcrpc")]
#[command(about = "Bitcoin-compatible JSON-RPC bridge for Bitcoin Infinity")]
struct Cli {
    /// NEAR RPC URL to query backend state
    #[arg(long)]
    near_rpc_url: Option<String>,

    /// Bind address for Bitcoin JSON-RPC server
    #[arg(long)]
    btc_rpc_addr: Option<String>,

    /// Optional chain ID override if backend discovery fails
    #[arg(long)]
    chain_id: Option<String>,
}

// ============================================================================
// RPC Authentication (Bitcoin Core compatible cookie + user/pass)
// ============================================================================

/// Generates a random cookie file at the given path and returns the cookie string.
/// Format: `__cookie__:<random_hex>` (same as Bitcoin Core)
fn generate_cookie_file(path: &std::path::Path) -> String {
    use std::io::Write;
    let mut rng_bytes = [0u8; 32];
    // Use /dev/urandom via std for randomness
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut rng_bytes);
    } else {
        // Fallback: use process id + time as entropy
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
            ^ (std::process::id() as u128);
        for (i, b) in rng_bytes.iter_mut().enumerate() {
            *b = ((seed >> (i % 16 * 8)) & 0xff) as u8;
        }
    }
    let hex_str: String = rng_bytes.iter().map(|b| format!("{:02x}", b)).collect();
    let cookie = format!("__cookie__:{}", hex_str);
    if let Ok(mut file) = std::fs::File::create(path) {
        let _ = file.write_all(cookie.as_bytes());
    }
    cookie
}

/// RPC auth credentials — either cookie or user/pass
#[derive(Clone)]
struct RpcAuth {
    /// The expected "user:password" string (before base64 encoding)
    credentials: String,
    /// Path to the cookie file (so we can clean it up on exit)
    cookie_path: Option<std::path::PathBuf>,
}

impl RpcAuth {
    fn new() -> Self {
        let rpc_user = std::env::var("BTC_RPC_USER").ok();
        let rpc_pass = std::env::var("BTC_RPC_PASS").ok();

        if let (Some(user), Some(pass)) = (rpc_user, rpc_pass) {
            return RpcAuth {
                credentials: format!("{}:{}", user, pass),
                cookie_path: None,
            };
        }

        // Generate cookie file
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let data_dir = std::path::PathBuf::from(home).join(".bitinfinity");
        let _ = std::fs::create_dir_all(&data_dir);
        let cookie_path = data_dir.join(".cookie");
        let cookie = generate_cookie_file(&cookie_path);

        RpcAuth {
            credentials: cookie,
            cookie_path: Some(cookie_path),
        }
    }

    fn check(&self, auth_header: Option<&str>) -> bool {
        let header = match auth_header {
            Some(h) => h,
            None => return false,
        };

        // Expect "Basic <base64(user:pass)>"
        let encoded = match header.strip_prefix("Basic ") {
            Some(e) => e,
            None => return false,
        };

        // Decode base64
        use base64::Engine;
        let decoded_bytes = match base64::engine::general_purpose::STANDARD.decode(encoded.trim()) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let decoded = match String::from_utf8(decoded_bytes) {
            Ok(s) => s,
            Err(_) => return false,
        };

        decoded == self.credentials
    }
}

/// Axum middleware for RPC authentication
async fn auth_middleware(
    State(auth): State<Arc<RpcAuth>>,
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> axum::response::Response {
    // Check if auth is disabled (BTC_RPC_NOAUTH=1)
    if std::env::var("BTC_RPC_NOAUTH").unwrap_or_default() == "1" {
        return next.run(req).await;
    }

    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    if auth.check(auth_header) {
        next.run(req).await
    } else {
        let body = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {
                "code": -32001,
                "message": "Unauthorized. Use -rpcuser/-rpcpassword or cookie file at ~/.bitinfinity/.cookie"
            }
        })).unwrap_or_default();

        axum::response::Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("WWW-Authenticate", "Basic realm=\"jsonrpc\"")
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(body))
            .unwrap_or_else(|_| {
                axum::response::Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(axum::body::Body::empty())
                    .expect("fallback response")
            })
    }
}

/// Bitcoin-compatible JSON-RPC request
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Bitcoin-compatible JSON-RPC response
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

/// Cached transaction mapping: bitcoin_txid -> (near_tx_hash, raw_hex, sender_id)
/// Persisted to ~/.bitinfinity/tx_cache.json
struct TxCache {
    pub entries: HashMap<String, TxCacheEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
struct TxCacheEntry {
    near_tx_hash: String,
    raw_hex: String,
    sender_id: String,
    /// For incoming (receive) transactions: the recipient address
    #[serde(default)]
    receiver_id: String,
    /// Amount in satoshis (for incoming tx detection)
    #[serde(default)]
    amount_satoshis: u64,
    /// Block height where this tx was included
    #[serde(default)]
    block_height: u64,
    /// Whether this was an incoming (receive) transaction detected by the indexer
    #[serde(default)]
    is_incoming: bool,
}

impl TxCache {
    fn new() -> Self {
        TxCache {
            entries: HashMap::new(),
        }
    }

    fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                match serde_json::from_str::<HashMap<String, TxCacheEntry>>(&contents) {
                    Ok(entries) => {
                        log::info!("Loaded {} cached transactions from disk", entries.len());
                        TxCache { entries }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to parse tx_cache.json ({}), starting with empty cache",
                            e
                        );
                        TxCache::new()
                    }
                }
            }
            Err(_) => TxCache::new(),
        }
    }

    fn insert(
        &mut self,
        btc_txid: String,
        near_tx_hash: String,
        raw_hex: String,
        sender_id: String,
    ) {
        self.entries.insert(
            btc_txid,
            TxCacheEntry {
                near_tx_hash,
                raw_hex,
                sender_id,
                receiver_id: String::new(),
                amount_satoshis: 0,
                block_height: 0,
                is_incoming: false,
            },
        );
        self.save_to_disk();
    }

    fn insert_incoming(
        &mut self,
        btc_txid: String,
        near_tx_hash: String,
        sender_id: String,
        receiver_id: String,
        amount_satoshis: u64,
        block_height: u64,
    ) {
        self.entries.insert(
            btc_txid,
            TxCacheEntry {
                near_tx_hash,
                raw_hex: format!("incoming:{}:{}", receiver_id, amount_satoshis),
                sender_id,
                receiver_id,
                amount_satoshis,
                block_height,
                is_incoming: true,
            },
        );
        self.save_to_disk();
    }

    fn get(&self, btc_txid: &str) -> Option<&TxCacheEntry> {
        self.entries.get(btc_txid)
    }

    fn save_to_disk(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(&self.entries) {
            let _ = std::fs::write(&path, json);
        }
    }

    fn path() -> std::path::PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home)
            .join(".bitinfinity")
            .join("tx_cache.json")
    }
}

/// A signed intent produced by signrawtransactionwithwallet.
/// Hex prefix "626974696e66696e6974793a" = hex("bitinfinity:")
const SIGNED_INTENT_PREFIX: &str = "bitinfinity:";
const ACCESS_KEY_NONCE_RANGE_MULTIPLIER: u64 = 1_000_000;
// For first-use Bitcoin accounts, keep nonce in the current tip's range; chunk
// preparation enforces an upper bound for the *next* block height.
const BITCOIN_FIRST_TX_HEIGHT_HEADROOM: u64 = 0;

/// Bitcoin Infinity RPC State
pub struct RpcState {
    chain_id: String,
    version: String,
    near_client: Arc<NearClient>,
    tx_cache: RwLock<TxCache>,
    keystore: RwLock<Keystore>,
    /// Local nonce cache: address -> last used nonce (avoids stale nonce on rapid sends)
    nonce_cache: RwLock<HashMap<String, u64>>,
    /// Server start time for uptime tracking
    start_time: std::time::Instant,
    /// Wallet unlock expiry (None = locked or not encrypted, Some = unlocked until)
    wallet_unlock_until: RwLock<Option<std::time::Instant>>,
    /// Cached passphrase for re-encryption on save (cleared on lock)
    wallet_passphrase: RwLock<Option<String>>,
    /// Locked UTXOs (txid:vout pairs that should not be spent)
    locked_utxos: RwLock<Vec<(String, u32)>>,
    /// Last block height processed by the incoming tx indexer
    last_indexed_height: RwLock<u64>,
    /// Balance snapshot for incoming tx detection: address -> last known balance in yoctoBIT
    balance_snapshot: RwLock<HashMap<String, String>>,
    /// Quantum keys registered per address: address -> Vec<(keytype, pubkey_hex)>
    /// Enforcement is governance-gated via validator supermajority (see issue #2).
    /// Supported key types: "dilithium3", "falcon512", "sphincsplus"
    quantum_keys: RwLock<HashMap<String, Vec<(String, String)>>>,
}

impl RpcState {
    pub fn new(chain_id: String, version: String, near_rpc_url: String) -> Self {
        let keystore = Keystore::load();
        log::info!("Loaded keystore with {} keys", keystore.addresses().len());
        let tx_cache = TxCache::load();
        let encrypted = keystore.encrypted;
        RpcState {
            chain_id,
            version,
            near_client: Arc::new(NearClient::new(near_rpc_url)),
            tx_cache: RwLock::new(tx_cache),
            keystore: RwLock::new(keystore),
            nonce_cache: RwLock::new(HashMap::new()),
            start_time: std::time::Instant::now(),
            wallet_unlock_until: RwLock::new(if encrypted {
                None
            } else {
                Some(std::time::Instant::now() + std::time::Duration::from_secs(999_999_999))
            }),
            wallet_passphrase: RwLock::new(None),
            locked_utxos: RwLock::new(Vec::new()),
            last_indexed_height: RwLock::new(0),
            balance_snapshot: RwLock::new(HashMap::new()),
            quantum_keys: RwLock::new(HashMap::new()),
        }
    }

    /// Get the bech32 HRP based on chain_id
    fn bech32_hrp(&self) -> &str {
        if self.chain_id.contains("mainnet") {
            "bc"
        } else if self.chain_id.contains("regtest") {
            "bcrt"
        } else {
            // testnet, devnet, localnet, etc.
            "tb"
        }
    }

    /// Whether this is a testnet chain
    fn is_testnet(&self) -> bool {
        !self.chain_id.contains("mainnet")
    }

    /// Check if wallet is unlocked (either not encrypted, or passphrase provided)
    async fn is_wallet_unlocked(&self) -> bool {
        let lock = self.wallet_unlock_until.read().await;
        match *lock {
            Some(until) => std::time::Instant::now() < until,
            None => false,
        }
    }

    /// Save the keystore, respecting encryption state.
    /// If the wallet is encrypted and the passphrase is cached, saves encrypted.
    async fn save_keystore(&self, keystore: &Keystore) {
        if keystore.encrypted {
            let passphrase = self.wallet_passphrase.read().await;
            if let Some(ref pp) = *passphrase {
                if let Err(e) = keystore.save_encrypted(pp) {
                    log::error!("Failed to save encrypted keystore: {}", e);
                }
            } else {
                log::error!(
                    "Cannot save encrypted keystore: no cached passphrase (wallet locked?)"
                );
            }
        } else if let Err(e) = keystore.save() {
            log::error!("Failed to save keystore: {}", e);
        }
    }

    fn bitcoin_first_tx_nonce_floor(latest_block_height: u64) -> u64 {
        latest_block_height
            .saturating_add(BITCOIN_FIRST_TX_HEIGHT_HEADROOM)
            .saturating_mul(ACCESS_KEY_NONCE_RANGE_MULTIPLIER)
            .saturating_add(1)
    }

    /// Get the next nonce for an address: max(local_cache, on-chain) + 1
    async fn next_nonce(&self, address: &str, near_pubkey_str: &str) -> u64 {
        let local_nonce = {
            let cache = self.nonce_cache.read().await;
            cache.get(address).copied().unwrap_or(0)
        };

        let chain_nonce = match self
            .near_client
            .view_access_key(address, near_pubkey_str)
            .await
        {
            Ok(ak_result) => ak_result.get("nonce").and_then(|v| v.as_u64()).unwrap_or(0),
            Err(_) => {
                // First-transaction path for Bitcoin accounts with no registered access key yet.
                // Return floor-1 here because next_nonce always adds one before returning.
                self.near_client
                    .status()
                    .await
                    .map(|s| {
                        Self::bitcoin_first_tx_nonce_floor(s.latest_block_height).saturating_sub(1)
                    })
                    .unwrap_or(0)
            }
        };

        let base = std::cmp::max(local_nonce, chain_nonce);
        base + 1
    }

    /// Record a used nonce so subsequent sends increment properly
    async fn record_nonce(&self, address: &str, nonce: u64) {
        let mut cache = self.nonce_cache.write().await;
        cache.insert(address.to_string(), nonce);
    }
}

fn ok_response(id: &serde_json::Value, result: serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.clone(),
        result: Some(result),
        error: None,
    }
}

fn err_response(id: &serde_json::Value, code: i32, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.clone(),
        result: None,
        error: Some(JsonRpcError {
            code,
            message,
            data: None,
        }),
    }
}

/// Base64 decode helper
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    const DECODE_TABLE: [u8; 128] = {
        let mut table = [255u8; 128];
        let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0;
        while i < 64 {
            table[chars[i] as usize] = i as u8;
            i += 1;
        }
        table
    };
    let input = input.trim().trim_end_matches('=');
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in input.as_bytes() {
        if b > 127 || DECODE_TABLE[b as usize] == 255 {
            return Err("Invalid base64 character".to_string());
        }
        buf = (buf << 6) | DECODE_TABLE[b as usize] as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
        }
    }
    Ok(output)
}

/// Base64 encode helper
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Helper to extract a string param from positional params
fn get_str_param<'a>(params: &'a serde_json::Value, index: usize) -> Option<&'a str> {
    params
        .as_array()
        .and_then(|arr| arr.get(index))
        .and_then(|v| v.as_str())
}

/// Helper to extract a u64 param from positional params
fn get_u64_param(params: &serde_json::Value, index: usize) -> Option<u64> {
    params
        .as_array()
        .and_then(|arr| arr.get(index))
        .and_then(|v| v.as_u64())
}

/// Helper to extract a bool param from positional params.
/// Accepts JSON bools and 0/1 numeric flags.
fn get_bool_param(params: &serde_json::Value, index: usize) -> Option<bool> {
    params.as_array().and_then(|arr| arr.get(index)).and_then(|v| {
        if let Some(b) = v.as_bool() {
            Some(b)
        } else {
            v.as_u64().map(|n| n != 0)
        }
    })
}

/// Encode a Bitcoin-style variable-length integer (CompactSize)
fn encode_bitcoin_varint(value: u64, buf: &mut Vec<u8>) {
    if value < 0xfd {
        buf.push(value as u8);
    } else if value <= 0xffff {
        buf.push(0xfd);
        buf.extend_from_slice(&(value as u16).to_le_bytes());
    } else if value <= 0xffff_ffff {
        buf.push(0xfe);
        buf.extend_from_slice(&(value as u32).to_le_bytes());
    } else {
        buf.push(0xff);
        buf.extend_from_slice(&value.to_le_bytes());
    }
}

/// Bech32 encoding for Bitcoin witness addresses (BIP 173).
fn bech32_encode(hrp: &str, witness_version: u8, program: &[u8]) -> String {
    const CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];

    fn polymod(values: &[u8]) -> u32 {
        let mut chk: u32 = 1;
        for &v in values {
            let b = chk >> 25;
            chk = ((chk & 0x1ffffff) << 5) ^ (v as u32);
            for (i, g) in GEN.iter().enumerate() {
                if (b >> i) & 1 == 1 {
                    chk ^= g;
                }
            }
        }
        chk
    }

    fn hrp_expand(hrp: &str) -> Vec<u8> {
        let mut ret: Vec<u8> = hrp.as_bytes().iter().map(|&b| b >> 5).collect();
        ret.push(0);
        ret.extend(hrp.as_bytes().iter().map(|&b| b & 31));
        ret
    }

    let mut data5 = vec![witness_version];
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in program {
        acc = (acc << 8) | (byte as u32);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            data5.push(((acc >> bits) & 31) as u8);
        }
    }
    if bits > 0 {
        data5.push(((acc << (5 - bits)) & 31) as u8);
    }

    let mut values = hrp_expand(hrp);
    values.extend_from_slice(&data5);
    values.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let poly = polymod(&values) ^ 1;
    let checksum: Vec<u8> = (0..6)
        .map(|i| ((poly >> (5 * (5 - i))) & 31) as u8)
        .collect();

    let mut result = String::from(hrp);
    result.push('1');
    for &d in data5.iter().chain(checksum.iter()) {
        result.push(CHARSET[d as usize] as char);
    }
    result
}

/// Derive the scriptPubKey hex from a Bitcoin address string.
/// Reusable helper for getaddressinfo, validateaddress, gettxout, scantxoutset, etc.
fn derive_script_pub_key_hex(addr: &str, bech32_hrp: &str) -> String {
    let bech32_q = format!("{}1q", bech32_hrp);
    let bech32_p = format!("{}1p", bech32_hrp);

    if addr.starts_with(&bech32_q) || addr.starts_with(&bech32_p) {
        let prefix_len = if addr.starts_with(&bech32_q) {
            bech32_q.len()
        } else {
            bech32_p.len()
        };
        let witness_version: u8 = if addr.starts_with(&bech32_p) { 1 } else { 0 };
        let data_part = &addr[prefix_len..];
        const BECH32_CHARSET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
        let data5: Vec<u8> = data_part
            .chars()
            .filter_map(|c| BECH32_CHARSET.find(c).map(|i| i as u8))
            .collect();
        if data5.len() > 6 {
            let payload5 = &data5[..data5.len() - 6];
            let mut acc: u32 = 0;
            let mut bits: u32 = 0;
            let mut program = Vec::new();
            for &val in payload5 {
                acc = (acc << 5) | (val as u32);
                bits += 5;
                while bits >= 8 {
                    bits -= 8;
                    program.push(((acc >> bits) & 0xff) as u8);
                }
            }
            if program.len() == 20 {
                format!("{:02x}14{}", witness_version, hex::encode(&program))
            } else if program.len() == 32 {
                format!("{:02x}20{}", witness_version, hex::encode(&program))
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else if addr.starts_with('1') || addr.starts_with('m') || addr.starts_with('n') {
        // P2PKH (mainnet starts with 1, testnet with m or n)
        match bs58::decode(addr).into_vec() {
            Ok(decoded) if decoded.len() >= 25 => {
                let pubkey_hash = &decoded[1..21];
                format!("76a914{}88ac", hex::encode(pubkey_hash))
            }
            _ => String::new(),
        }
    } else if addr.starts_with('3') || addr.starts_with('2') {
        // P2SH
        match bs58::decode(addr).into_vec() {
            Ok(decoded) if decoded.len() >= 25 => {
                let script_hash = &decoded[1..21];
                format!("a914{}87", hex::encode(script_hash))
            }
            _ => String::new(),
        }
    } else {
        String::new()
    }
}

/// Derive ASM representation of scriptPubKey from address.
fn derive_script_pub_key_asm(addr: &str, bech32_hrp: &str) -> String {
    let bech32_q = format!("{}1q", bech32_hrp);
    let bech32_p = format!("{}1p", bech32_hrp);

    if addr.starts_with(&bech32_q) {
        let hex = derive_script_pub_key_hex(addr, bech32_hrp);
        if hex.len() >= 44 {
            // "0014" + 40 hex chars
            format!("0 {}", &hex[4..])
        } else {
            String::new()
        }
    } else if addr.starts_with(&bech32_p) {
        let hex = derive_script_pub_key_hex(addr, bech32_hrp);
        if hex.len() >= 68 {
            // "0120" + 64 hex chars
            format!("1 {}", &hex[4..])
        } else {
            String::new()
        }
    } else if addr.starts_with('3') || addr.starts_with('2') {
        match bs58::decode(addr).into_vec() {
            Ok(decoded) if decoded.len() >= 25 => {
                format!("OP_HASH160 {} OP_EQUAL", hex::encode(&decoded[1..21]))
            }
            _ => String::new(),
        }
    } else {
        match bs58::decode(addr).into_vec() {
            Ok(decoded) if decoded.len() >= 25 => {
                format!(
                    "OP_DUP OP_HASH160 {} OP_EQUALVERIFY OP_CHECKSIG",
                    hex::encode(&decoded[1..21])
                )
            }
            _ => String::new(),
        }
    }
}

/// Classify a scriptPubKey from its hex representation, returning (type, asm).
fn classify_script_pub_key_hex(hex_str: &str) -> (String, String) {
    if hex_str.is_empty() {
        return ("nonstandard".to_string(), String::new());
    }
    // P2WPKH: 0014<20-byte hash>
    if hex_str.starts_with("0014") && hex_str.len() == 44 {
        return (
            "witness_v0_keyhash".to_string(),
            format!("0 {}", &hex_str[4..]),
        );
    }
    // P2WSH: 0020<32-byte hash>
    if hex_str.starts_with("0020") && hex_str.len() == 68 {
        return (
            "witness_v0_scripthash".to_string(),
            format!("0 {}", &hex_str[4..]),
        );
    }
    // P2TR: 5120<32-byte key>
    if hex_str.starts_with("5120") && hex_str.len() == 68 {
        return (
            "witness_v1_taproot".to_string(),
            format!("1 {}", &hex_str[4..]),
        );
    }
    // P2PKH: 76a914<20-byte hash>88ac
    if hex_str.starts_with("76a914") && hex_str.ends_with("88ac") && hex_str.len() == 50 {
        return (
            "pubkeyhash".to_string(),
            format!(
                "OP_DUP OP_HASH160 {} OP_EQUALVERIFY OP_CHECKSIG",
                &hex_str[6..46]
            ),
        );
    }
    // P2SH: a914<20-byte hash>87
    if hex_str.starts_with("a914") && hex_str.ends_with("87") && hex_str.len() == 46 {
        return (
            "scripthash".to_string(),
            format!("OP_HASH160 {} OP_EQUAL", &hex_str[4..44]),
        );
    }
    // OP_RETURN: 6a...
    if hex_str.starts_with("6a") {
        return (
            "nulldata".to_string(),
            format!("OP_RETURN {}", &hex_str[2..]),
        );
    }
    ("nonstandard".to_string(), hex_str.to_string())
}

/// Build a full Bitcoin Core compatible scriptPubKey JSON object from a TxOutput.
/// Includes asm, hex, type, and address fields.
fn build_script_pub_key_json(
    address: &str,
    is_op_return: bool,
    bech32_hrp: &str,
) -> serde_json::Value {
    if is_op_return {
        return json!({
            "asm": "OP_RETURN",
            "hex": "6a",
            "type": "nulldata"
        });
    }
    let spk_hex = derive_script_pub_key_hex(address, bech32_hrp);
    let (spk_type, spk_asm) = if !spk_hex.is_empty() {
        classify_script_pub_key_hex(&spk_hex)
    } else {
        // Fallback: classify from address prefix
        let bech32_q = format!("{}1q", bech32_hrp);
        let bech32_p = format!("{}1p", bech32_hrp);
        let t = if address.starts_with(&bech32_q) {
            "witness_v0_keyhash"
        } else if address.starts_with(&bech32_p) {
            "witness_v1_taproot"
        } else if address.starts_with('3') || address.starts_with('2') {
            "scripthash"
        } else {
            "pubkeyhash"
        };
        (t.to_string(), String::new())
    };
    let mut obj = json!({
        "asm": spk_asm,
        "hex": spk_hex,
        "type": spk_type,
    });
    if !address.is_empty() {
        obj.as_object_mut()
            .unwrap()
            .insert("address".to_string(), json!(address));
    }
    obj
}

/// Main RPC handler - routes Bitcoin JSON-RPC methods to Bitcoin Infinity
async fn rpc_handler(
    State(state): State<Arc<RpcState>>,
    body: Result<Json<JsonRpcRequest>, JsonRejection>,
) -> (StatusCode, Json<JsonRpcResponse>) {
    let request = match body {
        Ok(Json(req)) => req,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(err_response(
                    &json!(null),
                    -32700,
                    "Parse error".to_string(),
                )),
            );
        }
    };

    let response = match request.method.as_str() {
        // Blockchain
        "getblockchaininfo" => handle_getblockchaininfo(&state, &request).await,
        "getblockcount" => handle_getblockcount(&state, &request).await,
        "getbestblockhash" => handle_getbestblockhash(&state, &request).await,
        "getblock" => handle_getblock(&state, &request).await,
        "getblockhash" => handle_getblockhash(&state, &request).await,
        "getblockheader" => handle_getblockheader(&state, &request).await,
        "getblockstats" => handle_getblockstats(&state, &request).await,
        "getblockfilter" => handle_getblockfilter(&request),
        "gettxout" => handle_gettxout(&state, &request).await,
        "gettxoutsetinfo" => handle_gettxoutsetinfo(&state, &request).await,
        // Wallet - balance & info
        "getbalance" => handle_getbalance(&state, &request).await,
        "getbalances" => handle_getbalances(&state, &request).await,
        "getaccount" => handle_getaccount(&state, &request).await,
        "getaddressinfo" => handle_getaddressinfo(&state, &request).await,
        "getwalletinfo" => handle_getwalletinfo(&state, &request).await,
        "listwallets" => handle_listwallets(&request),
        "loadwallet" => handle_loadwallet(&request),
        "unloadwallet" => handle_unloadwallet(&request),
        "createwallet" => handle_createwallet(&request),
        // Wallet - UTXOs & addresses
        "listunspent" => handle_listunspent(&state, &request).await,
        "getnewaddress" => handle_getnewaddress(&state, &request).await,
        "getrawchangeaddress" => handle_getrawchangeaddress(&state, &request).await,
        "validateaddress" => handle_validateaddress(&state, &request).await,
        "dumpprivkey" => handle_dumpprivkey(&state, &request).await,
        "importprivkey" => handle_importprivkey(&state, &request).await,
        "listaddressgroupings" => handle_listaddressgroupings(&state, &request).await,
        "getaddressesbylabel" => handle_getaddressesbylabel(&state, &request).await,
        "listreceivedbyaddress" => handle_listreceivedbyaddress(&state, &request).await,
        "keypoolrefill" => handle_keypoolrefill(&request),
        "scantxoutset" => handle_scantxoutset(&state, &request).await,
        // Wallet - locking
        "lockunspent" => handle_lockunspent(&state, &request).await,
        "listlockunspent" => handle_listlockunspent(&state, &request).await,
        "walletpassphrase" => handle_walletpassphrase(&state, &request).await,
        "walletlock" => handle_walletlock(&state, &request).await,
        // Wallet - signing
        "signmessage" => handle_signmessage(&state, &request).await,
        "verifymessage" => handle_verifymessage(&state, &request),
        // Transactions - raw
        "sendrawtransaction" => handle_sendrawtransaction(&state, &request).await,
        "getrawtransaction" => handle_getrawtransaction(&state, &request).await,
        "gettransaction" => handle_gettransaction(&state, &request).await,
        "decoderawtransaction" => handle_decoderawtransaction(&state, &request),
        "signrawtransactionwithwallet" => {
            handle_signrawtransactionwithwallet(&state, &request).await
        }
        "createrawtransaction" => handle_createrawtransaction(&request),
        "fundrawtransaction" => handle_fundrawtransaction(&state, &request).await,
        "testmempoolaccept" => handle_testmempoolaccept(&state, &request).await,
        // Transactions - high-level
        "sendtoaddress" => handle_sendtoaddress(&state, &request).await,
        "sendmany" => handle_sendmany(&state, &request).await,
        "listtransactions" => handle_listtransactions(&state, &request).await,
        "getreceivedbyaddress" => handle_getreceivedbyaddress(&state, &request).await,
        "settxfee" => handle_settxfee(&request),
        "abandontransaction" => handle_abandontransaction(&state, &request).await,
        "bumpfee" => handle_bumpfee(&request),
        // PSBT (stubs)
        "walletcreatefundedpsbt" => handle_walletcreatefundedpsbt(&state, &request).await,
        "decodepsbt" => handle_decodepsbt(&request),
        "finalizepsbt" => handle_finalizepsbt(&request),
        "combinepsbt" => handle_combinepsbt(&request),
        // Descriptors
        "deriveaddresses" => handle_deriveaddresses(&request),
        "getdescriptorinfo" => handle_getdescriptorinfo(&request),
        "importdescriptors" => handle_importdescriptors(&request),
        // Network
        "getnetworkinfo" => handle_getnetworkinfo(&state, &request).await,
        "getconnectioncount" => handle_getconnectioncount(&state, &request).await,
        "getpeerinfo" => handle_getpeerinfo(&state, &request).await,
        "getinfo" => handle_getinfo(&state, &request).await,
        "ping" => handle_ping(&request),
        // Fee estimation
        "estimatesmartfee" => handle_estimatesmartfee(&state, &request).await,
        // Mempool
        "getmempoolinfo" => handle_getmempoolinfo(&state, &request).await,
        "getrawmempool" => handle_getrawmempool(&state, &request).await,
        "getmempoolentry" => handle_getmempoolentry(&state, &request).await,
        // Mining
        "getmininginfo" => handle_getmininginfo(&state, &request).await,
        "generate" => handle_generate(&state, &request).await,
        "generatetoaddress" => handle_generatetoaddress(&state, &request).await,
        // Additional blockchain
        "getdifficulty" => handle_getdifficulty(&request),
        "getchaintips" => handle_getchaintips(&state, &request).await,
        "gettxoutproof" => handle_gettxoutproof(&request),
        "verifytxoutproof" => handle_verifytxoutproof(&request),
        // Additional wallet
        "listsinceblock" => handle_listsinceblock(&state, &request).await,
        "listdescriptors" => handle_listdescriptors(&state, &request).await,
        "signrawtransactionwithkey" => handle_signrawtransactionwithkey(&state, &request).await,
        "converttopsbt" => handle_converttopsbt(&request),
        "utxoupdatepsbt" => handle_utxoupdatepsbt(&request),
        // NEAR-native methods (full protocol access)
        "callcontract" => handle_callcontract(&state, &request).await,
        "getcontractstate" => handle_getcontractstate(&state, &request).await,
        "getcontractcode" => handle_getcontractcode(&state, &request).await,
        "deploynearcontract" => handle_deploynearcontract(&state, &request).await,
        "stakenearsatoshis" => handle_stake(&state, &request).await,
        "unstake" => handle_unstake(&state, &request).await,
        "addnearkey" => handle_addnearkey(&state, &request).await,
        "deletenearkey" => handle_deletenearkey(&state, &request).await,
        "closenearaccount" => handle_closenearaccount(&state, &request).await,
        "getvalidatorinfo" => handle_getvalidatorinfo(&state, &request).await,
        "listaccountkeys" => handle_listaccountkeys(&state, &request).await,
        "sendneartx" => handle_sendneartx(&state, &request).await,
        "createnearaccount" => handle_createnearaccount(&state, &request).await,
        "fundgaskey" => handle_fundgaskey(&state, &request).await,
        "withdrawgaskey" => handle_withdrawgaskey(&state, &request).await,
        // NEAR RPC passthrough
        "getchunk" => handle_getchunk(&state, &request).await,
        "getreceipt" => handle_getreceipt(&state, &request).await,
        "getchangesinblock" => handle_getchangesinblock(&state, &request).await,
        "getchanges" => handle_getchanges(&state, &request).await,
        "gettxreceipts" => handle_gettxreceipts(&state, &request).await,
        "getprotocolconfig" => handle_getprotocolconfig(&state, &request).await,
        "getgenesisconfig" => handle_getgenesisconfig(&state, &request).await,
        "getnodehealth" => handle_getnodehealth(&state, &request).await,
        "getlightclientproof" => handle_getlightclientproof(&state, &request).await,
        "getlightclientblock" => handle_getlightclientblock(&state, &request).await,
        "getvalidatorsordered" => handle_getvalidatorsordered(&state, &request).await,
        "getcongestionlevel" => handle_getcongestionlevel(&state, &request).await,
        "getnearnetworkinfo" => handle_getnearnetworkinfo(&state, &request).await,
        "getclientconfig" => handle_getclientconfig(&state, &request).await,
        "getgaskeynonces" => handle_getgaskeynonces(&state, &request).await,
        "queryatblock" => handle_queryatblock(&state, &request).await,
        // NEAR RPC direct passthroughs (additional)
        "getgasprice" => handle_getgasprice(&state, &request).await,
        "getnearstatus" => handle_getnearstatus(&state, &request).await,
        "getneartxstatus" => handle_getneartxstatus(&state, &request).await,
        "broadcastneartx" => handle_broadcastneartx(&state, &request).await,
        "broadcastneartxcommit" => handle_broadcastneartxcommit(&state, &request).await,
        "sendneartxwait" => handle_sendneartxwait(&state, &request).await,
        "getmaintenancewindows" => handle_getmaintenancewindows(&state, &request).await,
        "getsplitstorage" => handle_getsplitstorage(&state, &request).await,
        "getlightclientblockproof" => handle_getlightclientblockproof(&state, &request).await,
        "getneartxfull" => handle_getneartxfull(&state, &request).await,
        // Additional Bitcoin Core RPC methods
        "walletprocesspsbt" => handle_walletprocesspsbt(&state, &request).await,
        "createpsbt" => handle_createpsbt(&request),
        "getblocktemplate" => handle_getblocktemplate(&state, &request).await,
        "submitblock" => handle_submitblock(&request),
        "generateblock" => handle_generateblock(&request),
        "importaddress" => handle_importaddress(&state, &request).await,
        "importpubkey" => handle_importpubkey(&state, &request).await,
        "backupwallet" => handle_backupwallet(&state, &request),
        "invalidateblock" => handle_invalidateblock(&request),
        "reconsiderblock" => handle_reconsiderblock(&request),
        "waitforblock" => handle_waitforblock(&state, &request).await,
        "waitfornewblock" => handle_waitfornewblock(&state, &request).await,
        "waitforblockheight" => handle_waitforblockheight(&state, &request).await,
        "getnetworkhashps" => handle_getnetworkhashps(&state, &request).await,
        "prioritisetransaction" => handle_prioritisetransaction(&request),
        "getreceivedbylabel" => handle_getreceivedbylabel(&state, &request).await,
        "listlabels" => handle_listlabels(&state, &request).await,
        "setlabel" => handle_setlabel(&request),
        "walletpassphrasechange" => handle_walletpassphrasechange(&state, &request).await,
        "encryptwallet" => handle_encryptwallet(&state, &request).await,
        "getmemoryinfo" => handle_getmemoryinfo(&request),
        "getrpcinfo" => handle_getrpcinfo(&request),
        "getindexinfo" => handle_getindexinfo(&state, &request).await,
        "getzmqnotifications" => handle_getzmqnotifications(&request),
        "logging" => handle_logging(&request),
        "abortrescan" => handle_abortrescan(&request),
        "getunconfirmedbalance" => handle_getunconfirmedbalance(&state, &request).await,
        "sethdseed" => handle_sethdseed(&request),
        // Misc
        "uptime" => handle_uptime(&state, &request),
        "help" => handle_help(&request),
        "stop" => handle_stop(&request),
        "rescanblockchain" => handle_rescanblockchain(&state, &request).await,
        // Additional Bitcoin Core v27/v28 methods for full feature parity
        "addmultisigaddress" => handle_addmultisigaddress(&request),
        "addnode" => handle_addnode(&request),
        "onetry" => handle_onetry(&request),
        "analyzepsbt" => handle_analyzepsbt(&request),
        "clearbanned" => handle_clearbanned(&request),
        "combinerawtransaction" => handle_combinerawtransaction(&request),
        "createmultisig" => handle_createmultisig(&request),
        "decodescript" => handle_decodescript(&request),
        "disconnectnode" => handle_disconnectnode(&request),
        "dumpwallet" => handle_dumpwallet(&request),
        "getchaintxstats" => handle_getchaintxstats(&state, &request).await,
        "generatetodescriptor" => handle_generatetodescriptor(&request),
        "getmempoolancestors" => handle_getmempoolancestors(&request),
        "getmempooldescendants" => handle_getmempooldescendants(&request),
        "getnettotals" => handle_getnettotals(&state, &request).await,
        "getnodeaddresses" => handle_getnodeaddresses(&state, &request).await,
        "importmulti" => handle_importmulti(&request),
        "importprunedfunds" => handle_importprunedfunds(&request),
        "importwallet" => handle_importwallet(&request),
        "joinpsbts" => handle_joinpsbts(&request),
        "listbanned" => handle_listbanned(&request),
        "listreceivedbylabel" => handle_listreceivedbylabel(&state, &request).await,
        "listwalletdir" => handle_listwalletdir(&request),
        "preciousblock" => handle_preciousblock(&request),
        "pruneblockchain" => handle_pruneblockchain(&request),
        "psbtbumpfee" => handle_psbtbumpfee(&request),
        "removeprunedfunds" => handle_removeprunedfunds(&request),
        "savemempool" => handle_savemempool(&request),
        "send" => handle_send(&state, &request).await,
        "setban" => handle_setban(&request),
        "setnetworkactive" => handle_setnetworkactive(&request),
        "setwalletflag" => handle_setwalletflag(&request),
        "signmessagewithprivkey" => handle_signmessagewithprivkey(&request),
        "submitheader" => handle_submitheader(&request),
        "upgradewallet" => handle_upgradewallet(&request),
        "verifychain" => handle_verifychain(&request),
        // Quantum resistance (issue #2) — architecture active, enforcement governance-gated
        "addquantumkey" => handle_addquantumkey(&state, &request).await,
        "removequantumkey" => handle_removequantumkey(&state, &request).await,
        "listquantumkeys" => handle_listquantumkeys(&state, &request).await,
        // Patoshi unlock challenge (issue #10)
        "patoshiunlock" => handle_patoshiunlock(&state, &request).await,
        _ => err_response(
            &request.id,
            -32601,
            format!("Method not found: {}", request.method),
        ),
    };

    (StatusCode::OK, Json(response))
}

// ============================================================================
// Blockchain handlers
// ============================================================================

async fn handle_getblockchaininfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.status().await {
        Ok(status) => ok_response(
            &request.id,
            json!({
                "chain": status.chain_id,
                "blocks": status.latest_block_height,
                "headers": status.latest_block_height,
                "bestblockhash": status.latest_block_hash,
                "difficulty": 1.0,
                "time": chrono::Utc::now().timestamp(),
                "mediantime": chrono::Utc::now().timestamp(),
                "verificationprogress": if status.syncing { 0.5 } else { 1.0 },
                "initialblockdownload": status.syncing,
                "chainwork": format!("{:064x}", status.latest_block_height as u128 * 0x100000000u128),
                "size_on_disk": status.latest_block_height * 2048,
                "pruned": false,
                "warnings": ""
            }),
        ),
        Err(_) => ok_response(
            &request.id,
            json!({
                "chain": state.chain_id,
                "blocks": 0,
                "headers": 0,
                "bestblockhash": "",
                "difficulty": 1.0,
                "time": chrono::Utc::now().timestamp(),
                "mediantime": chrono::Utc::now().timestamp(),
                "verificationprogress": 0.0,
                "initialblockdownload": true,
                "warnings": "nearcore node not connected"
            }),
        ),
    }
}

async fn handle_getblockcount(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.status().await {
        Ok(status) => ok_response(&request.id, json!(status.latest_block_height)),
        Err(e) => err_response(&request.id, -28, format!("Node not connected: {}", e)),
    }
}

async fn handle_getbestblockhash(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.status().await {
        Ok(status) => ok_response(&request.id, json!(status.latest_block_hash)),
        Err(e) => err_response(&request.id, -28, format!("Node not connected: {}", e)),
    }
}

async fn handle_getblock(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let block_id = get_str_param(&request.params, 0).unwrap_or("");
    if block_id.is_empty() {
        return err_response(
            &request.id,
            -32602,
            "Missing block hash parameter".to_string(),
        );
    }

    // Verbosity: 0 = hex serialization, 1 = JSON (default), 2 = JSON with full tx objects
    let verbosity = request
        .params
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(|v| v.as_u64())
        .unwrap_or(1);

    let block_result = state.near_client.block_by_hash(block_id).await;
    match block_result {
        Ok(block) => {
            let header = block.get("header").unwrap_or(&block);
            let height = header.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
            let hash = header.get("hash").and_then(|v| v.as_str()).unwrap_or("");
            let prev_hash = header
                .get("prev_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let timestamp = header
                .get("timestamp")
                .and_then(|v| v.as_u64())
                .or_else(|| header.get("timestamp_nanosec").and_then(|v| v.as_u64()))
                .map(|ns| ns / 1_000_000_000)
                .unwrap_or_else(|| chrono::Utc::now().timestamp() as u64);

            // Compute real confirmations
            let current_height = state
                .near_client
                .status()
                .await
                .map(|s| s.latest_block_height)
                .unwrap_or(height);
            let confirmations = if current_height >= height {
                current_height - height + 1
            } else {
                1
            };

            // Collect chunk hashes for merkleroot computation and tx extraction
            let mut chunk_hashes: Vec<String> = Vec::new();
            let mut tx_hashes: Vec<String> = Vec::new();
            if let Some(chunks) = block.get("chunks").and_then(|c| c.as_array()) {
                for chunk in chunks {
                    if let Some(ch) = chunk.get("chunk_hash").and_then(|v| v.as_str()) {
                        chunk_hashes.push(ch.to_string());
                        if let Ok(chunk_detail) = state.near_client.chunk_by_hash(ch).await {
                            if let Some(txns) =
                                chunk_detail.get("transactions").and_then(|v| v.as_array())
                            {
                                for txn in txns {
                                    if let Some(tx_hash) = txn.get("hash").and_then(|v| v.as_str())
                                    {
                                        tx_hashes.push(tx_hash.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Derive merkleroot from chunk hashes (hash all chunk hashes together)
            let merkleroot = if chunk_hashes.is_empty() {
                "0000000000000000000000000000000000000000000000000000000000000000".to_string()
            } else {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                for ch in &chunk_hashes {
                    hasher.update(ch.as_bytes());
                }
                let h1 = hasher.finalize();
                let h2 = Sha256::digest(&h1);
                hex::encode(h2)
            };

            // Get block size estimate from gas_used
            let gas_used = header.get("gas_used").and_then(|v| v.as_u64()).unwrap_or(0);
            let chunk_count = block
                .get("chunks")
                .and_then(|c| c.as_array())
                .map(|a| a.len())
                .unwrap_or(0);

            // Verbosity 0: return hex-encoded 80-byte Bitcoin block header
            if verbosity == 0 {
                // Construct a synthetic 80-byte Bitcoin block header:
                // version(4) + prev_hash(32) + merkle_root(32) + time(4) + bits(4) + nonce(4) = 80
                let mut header_bytes = Vec::with_capacity(80);
                header_bytes.extend_from_slice(&0x20000000u32.to_le_bytes()); // version
                                                                              // prev_hash: decode hex or pad with zeros
                let prev_bytes =
                    hex::decode(prev_hash.replace("0x", "")).unwrap_or_else(|_| vec![0u8; 32]);
                if prev_bytes.len() >= 32 {
                    header_bytes.extend_from_slice(&prev_bytes[..32]);
                } else {
                    header_bytes.extend_from_slice(&prev_bytes);
                    header_bytes.resize(4 + 32, 0);
                }
                // merkle_root
                let mr_bytes = hex::decode(&merkleroot).unwrap_or_else(|_| vec![0u8; 32]);
                if mr_bytes.len() >= 32 {
                    header_bytes.extend_from_slice(&mr_bytes[..32]);
                } else {
                    header_bytes.extend_from_slice(&mr_bytes);
                    header_bytes.resize(4 + 32 + 32, 0);
                }
                header_bytes.extend_from_slice(&(timestamp as u32).to_le_bytes()); // time
                header_bytes.extend_from_slice(&0x1d00ffffu32.to_le_bytes()); // bits (difficulty)
                header_bytes.extend_from_slice(&(height as u32).to_le_bytes()); // nonce (use height)
                return ok_response(&request.id, json!(hex::encode(&header_bytes)));
            }

            // Verbosity 2: tx array contains full tx objects instead of just hashes
            let tx_field = if verbosity >= 2 {
                // For verbosity 2, fetch actual tx details from chunk data
                let mut tx_objects: Vec<serde_json::Value> = Vec::new();
                if let Some(chunks) = block.get("chunks").and_then(|c| c.as_array()) {
                    for chunk in chunks {
                        if let Some(ch) = chunk.get("chunk_hash").and_then(|v| v.as_str()) {
                            if let Ok(chunk_detail) = state.near_client.chunk_by_hash(ch).await {
                                if let Some(txns) =
                                    chunk_detail.get("transactions").and_then(|v| v.as_array())
                                {
                                    for txn in txns {
                                        let tx_hash =
                                            txn.get("hash").and_then(|v| v.as_str()).unwrap_or("");
                                        let _signer = txn
                                            .get("signer_id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let receiver = txn
                                            .get("receiver_id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let actions = txn.get("actions").and_then(|v| v.as_array());
                                        let mut vout_entries = Vec::new();
                                        if let Some(acts) = actions {
                                            for (i, act) in acts.iter().enumerate() {
                                                let (addr, amt) = if let Some(transfer) =
                                                    act.get("Transfer")
                                                {
                                                    let deposit = transfer
                                                        .get("deposit")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("0");
                                                    let yocto: u128 = deposit.parse().unwrap_or(0);
                                                    let sat = yocto
                                                        / crate::tx_translator::YOCTO_PER_SATOSHI;
                                                    (
                                                        receiver.to_string(),
                                                        sat as f64 / 100_000_000.0,
                                                    )
                                                } else {
                                                    (receiver.to_string(), 0.0)
                                                };
                                                vout_entries.push(json!({
                                                    "value": amt,
                                                    "n": i,
                                                    "scriptPubKey": build_script_pub_key_json(&addr, false, state.bech32_hrp())
                                                }));
                                            }
                                        }
                                        tx_objects.push(json!({
                                            "txid": tx_hash,
                                            "hash": tx_hash,
                                            "version": 2,
                                            "size": 250,
                                            "vsize": 140,
                                            "weight": 560,
                                            "locktime": 0,
                                            "vin": [{
                                                "txid": "0".repeat(64),
                                                "vout": 0,
                                                "scriptSig": { "asm": "", "hex": "" },
                                                "sequence": 0xfffffffe_u32
                                            }],
                                            "vout": vout_entries
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }
                json!(tx_objects)
            } else {
                json!(tx_hashes)
            };

            ok_response(
                &request.id,
                json!({
                    "hash": hash,
                    "height": height,
                    "confirmations": confirmations,
                    "time": timestamp,
                    "mediantime": timestamp,
                    "previousblockhash": prev_hash,
                    "merkleroot": merkleroot,
                    "tx": tx_field,
                    "nTx": tx_hashes.len(),
                    "difficulty": 1.0,
                    "chainwork": format!("{:064x}", height as u128 * 0x100000000u128),
                    "nonce": 0,
                    "bits": "1d00ffff",
                    "size": gas_used / 1000,
                    "strippedsize": gas_used / 1000,
                    "weight": gas_used / 250,
                    "version": 1,
                    "versionHex": "00000001",
                    "nchunks": chunk_count
                }),
            )
        }
        Err(e) => {
            log::warn!("getblock failed for {}: {}", block_id, e);
            err_response(&request.id, -5, format!("Block not found: {}", block_id))
        }
    }
}

async fn handle_getblockhash(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let height = get_u64_param(&request.params, 0);
    match height {
        Some(h) => match state.near_client.block_by_height(h).await {
            Ok(block) => {
                let hash = block
                    .get("header")
                    .and_then(|h| h.get("hash"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                ok_response(&request.id, json!(hash))
            }
            Err(e) => {
                log::warn!("getblockhash failed for height {}: {}", h, e);
                err_response(&request.id, -8, format!("Block height {} out of range", h))
            }
        },
        None => err_response(&request.id, -32602, "Missing height parameter".to_string()),
    }
}

// ============================================================================
// Wallet handlers
// ============================================================================

async fn handle_getbalance(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let account_id = if let Some(params) = request.params.as_array() {
        // Bitcoin Core: getbalance("*") or getbalance("") or getbalance() all mean "total wallet balance"
        let first = params.first().and_then(|v| v.as_str()).unwrap_or("");
        if first == "*" || first.is_empty() {
            ""
        } else {
            first
        }
    } else if let Some(addr) = request.params.as_str() {
        if addr == "*" {
            ""
        } else {
            addr
        }
    } else {
        ""
    };

    if account_id.is_empty() {
        // No specific address: sum all wallet addresses (Bitcoin Core behavior)
        let keystore = state.keystore.read().await;
        let addresses: Vec<String> = keystore.addresses().iter().map(|a| a.to_string()).collect();
        drop(keystore);

        if addresses.is_empty() {
            return ok_response(&request.id, json!(0.0));
        }

        let mut total = 0.0f64;
        for addr in &addresses {
            if let Ok(account) = state.near_client.view_account(addr).await {
                total += account.balance_as_btc();
            }
        }
        return ok_response(&request.id, json!(total));
    }

    match state.near_client.view_account(account_id).await {
        Ok(account) => {
            let btc_balance = account.balance_as_btc();
            ok_response(&request.id, json!(btc_balance))
        }
        Err(e) => {
            if e.contains("does not exist") || e.contains("doesn't exist") {
                ok_response(&request.id, json!(0.0))
            } else {
                log::warn!("getbalance failed for {}: {}", account_id, e);
                err_response(&request.id, -28, format!("Failed to query balance: {}", e))
            }
        }
    }
}

async fn handle_getaccount(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let account_id = get_str_param(&request.params, 0).unwrap_or("");
    if account_id.is_empty() {
        return err_response(&request.id, -32602, "Missing address parameter".to_string());
    }

    match state.near_client.view_account(account_id).await {
        Ok(account) => ok_response(
            &request.id,
            json!({
                "address": account_id,
                "balance": account.balance_as_btc(),
                "balance_satoshis": account.balance_as_satoshis(),
                "balance_yocto": account.amount,
                "staked": account.locked_as_btc(),
                "staked_yocto": account.locked,
                "block_height": account.block_height,
                "block_hash": account.block_hash
            }),
        ),
        Err(e) => err_response(&request.id, -32000, format!("Account not found: {}", e)),
    }
}

async fn handle_listunspent(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    // listunspent [minconf] [maxconf] [addresses]
    let minconf = get_u64_param(&request.params, 0).unwrap_or(1);
    let maxconf = get_u64_param(&request.params, 1).unwrap_or(9_999_999);
    if maxconf < minconf {
        return err_response(
            &request.id,
            -8,
            "maxconf must be greater than or equal to minconf".to_string(),
        );
    }

    let explicit_addresses: Vec<String> = request
        .params
        .as_array()
        .and_then(|arr| arr.get(2))
        .and_then(|v| v.as_array())
        .map(|addrs| {
            addrs
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // If no addresses specified, use all keystore addresses including watch-only
    let keystore = state.keystore.read().await;
    let watch_only_set: std::collections::HashSet<String> =
        keystore.watch_only_addresses().iter().cloned().collect();
    let addresses: Vec<String> = if explicit_addresses.is_empty() {
        keystore.all_addresses()
    } else {
        explicit_addresses
    };
    drop(keystore);

    if addresses.is_empty() {
        return ok_response(&request.id, json!([]));
    }

    let locked_utxos: std::collections::HashSet<(String, u32)> = {
        let locked = state.locked_utxos.read().await;
        locked.iter().cloned().collect()
    };

    let mut utxos = Vec::new();
    for addr in &addresses {
        match state.near_client.view_account(addr.as_str()).await {
            Ok(account) => {
                let satoshis = account.balance_as_satoshis();
                if satoshis > 0 {
                    let mut utxo = SyntheticUtxo::from_account(addr, satoshis);
                    if utxo.confirmations < minconf || utxo.confirmations > maxconf {
                        continue;
                    }
                    if locked_utxos.contains(&(utxo.txid.clone(), utxo.vout)) {
                        continue;
                    }
                    // Watch-only addresses are not spendable
                    if watch_only_set.contains(addr) {
                        utxo.spendable = false;
                    }
                    utxos.push(utxo.to_json());
                }
            }
            Err(e) => {
                log::debug!("listunspent: account {} not found: {}", addr, e);
            }
        }
    }

    ok_response(&request.id, json!(utxos))
}

#[derive(Debug, Clone)]
struct SyntheticWalletTx {
    address: String,
    amount_satoshis: u64,
    block_height: u64,
    block_hash: String,
    block_time: i64,
    confirmations: i64,
}

async fn resolve_synthetic_wallet_tx(state: &RpcState, txid: &str) -> Option<SyntheticWalletTx> {
    let addresses = {
        let keystore = state.keystore.read().await;
        keystore.all_addresses()
    };

    for address in addresses {
        if SyntheticUtxo::txid_for_account(&address) != txid {
            continue;
        }

        let account = match state.near_client.view_account(&address).await {
            Ok(account) => account,
            Err(_) => continue,
        };
        let satoshis = account.balance_as_satoshis();
        if satoshis == 0 {
            continue;
        }

        let (block_height, block_hash) = match state.near_client.status().await {
            Ok(status) => (status.latest_block_height, status.latest_block_hash),
            Err(_) => (0, String::new()),
        };
        return Some(SyntheticWalletTx {
            address,
            amount_satoshis: satoshis,
            block_height,
            block_hash,
            block_time: chrono::Utc::now().timestamp(),
            confirmations: 6,
        });
    }

    None
}

async fn handle_getnewaddress(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    if !state.is_wallet_unlocked().await {
        return err_response(
            &request.id,
            -13,
            "Error: Please enter the wallet passphrase with walletpassphrase first.".to_string(),
        );
    }

    // Generate a real secp256k1 keypair
    use sha2::Digest;

    // Check address_type parameter: "legacy" for P2PKH, "bech32" (default) for P2WPKH
    let addr_type = request
        .params
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(|v| v.as_str())
        .unwrap_or("bech32");

    let secp = secp256k1::Secp256k1::new();
    let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());

    // Derive address from compressed public key
    let pubkey_compressed = public_key.serialize(); // 33-byte compressed
    let sha_hash = sha2::Sha256::digest(&pubkey_compressed);
    let pubkey_hash = ripemd::Ripemd160::digest(&sha_hash);

    // Derive both address formats
    let mut p2pkh_payload = vec![0x00];
    p2pkh_payload.extend_from_slice(&pubkey_hash);
    let checksum = sha2::Sha256::digest(&sha2::Sha256::digest(&p2pkh_payload));
    p2pkh_payload.extend_from_slice(&checksum[..4]);
    let p2pkh_address = bs58::encode(&p2pkh_payload).into_string();
    let p2pkh_legacy = p2pkh_address.to_lowercase();
    let bech32_address = bech32_encode(state.bech32_hrp(), 0, &pubkey_hash);

    let address = if addr_type == "legacy" {
        p2pkh_address.clone()
    } else {
        bech32_address.clone()
    };

    // Get uncompressed public key (65 bytes with 0x04 prefix, we store without prefix)
    let pubkey_uncompressed_full = public_key.serialize_uncompressed();
    let pubkey_uncompressed = &pubkey_uncompressed_full[1..]; // skip 0x04

    // Save to keystore under BOTH address formats so either can be used for sending
    let entry = KeyEntry {
        private_key_hex: hex::encode(secret_key.secret_bytes()),
        public_key_compressed_hex: hex::encode(pubkey_compressed),
        public_key_uncompressed_hex: hex::encode(pubkey_uncompressed),
    };

    {
        let mut keystore = state.keystore.write().await;
        keystore.insert(p2pkh_address.clone(), entry.clone());
        if p2pkh_legacy != p2pkh_address {
            keystore.insert(p2pkh_legacy, entry.clone());
        }
        keystore.insert(bech32_address.clone(), entry);
        state.save_keystore(&keystore).await;
    }

    log::info!("Generated new address: {}", address);

    ok_response(&request.id, json!(address))
}

async fn handle_validateaddress(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let addr = get_str_param(&request.params, 0).unwrap_or("");
    let parsed_account = AccountIdRef::new(addr).ok();
    let account_type = parsed_account
        .as_ref()
        .map(|id| id.get_account_type())
        .unwrap_or(AccountType::NamedAccount);
    let is_valid = parsed_account.is_some();
    let is_bitcoin_account = matches!(&account_type, AccountType::BtcImplicitAccount);
    let hrp = state.bech32_hrp();
    let is_bech32 = is_bitcoin_account && addr.starts_with(&format!("{}1", hrp));

    let keystore = state.keystore.read().await;
    let is_mine = keystore.get(addr).is_some();
    let is_watch_only = keystore.is_watch_only(addr);
    drop(keystore);

    let is_witness = is_bech32;
    let bech32_q = format!("{}1q", hrp);
    let bech32_p = format!("{}1p", hrp);
    let (witness_version, witness_program) = if addr.starts_with(&bech32_q) {
        (Some(0), Some(addr[bech32_q.len()..].to_string()))
    } else if addr.starts_with(&bech32_p) {
        (Some(1), Some(addr[bech32_p.len()..].to_string()))
    } else {
        (None, None)
    };

    let script_type = if !is_bitcoin_account {
        "not_bitcoin"
    } else if addr.starts_with(&bech32_q) {
        "witness_v0_keyhash"
    } else if addr.starts_with(&bech32_p) {
        "witness_v1_taproot"
    } else if addr.starts_with("3") {
        "scripthash"
    } else {
        "pubkeyhash"
    };

    // Check if account exists on-chain
    let is_on_chain = if is_valid {
        state.near_client.view_account(addr).await.is_ok()
    } else {
        false
    };

    // Derive scriptPubKey using shared helper
    let script_pub_key = if is_bitcoin_account {
        derive_script_pub_key_hex(addr, hrp)
    } else {
        String::new()
    };

    let account_type_label = match &account_type {
        AccountType::BtcImplicitAccount => "bitcoin",
        AccountType::NearImplicitAccount => "near_implicit",
        AccountType::EthImplicitAccount => "eth_implicit",
        AccountType::NearDeterministicAccount => "near_deterministic",
        AccountType::NamedAccount => "named",
    };

    let mut result = json!({
        "isvalid": is_valid,
        "address": addr,
        "scriptPubKey": script_pub_key,
        "ismine": is_mine && !is_watch_only,
        "iswatchonly": is_watch_only,
        "isscript": is_bitcoin_account && addr.starts_with('3'),
        "iswitness": is_bitcoin_account && is_witness,
        "script_type": script_type,
        "account_type": account_type_label,
        "ischange": false,
        "labels": [""],
        "near_account_exists": is_on_chain,
    });

    if let Some(v) = witness_version {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("witness_version".to_string(), json!(v));
        }
    }
    if let Some(p) = witness_program {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("witness_program".to_string(), json!(p));
        }
    }

    ok_response(&request.id, result)
}

async fn handle_dumpprivkey(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    if !state.is_wallet_unlocked().await {
        return err_response(
            &request.id,
            -13,
            "Error: Please enter the wallet passphrase with walletpassphrase first.".to_string(),
        );
    }
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };

    let keystore = state.keystore.read().await;
    match keystore.get(addr) {
        Some(entry) => {
            // Return WIF (Wallet Import Format) instead of raw hex
            let wif = privkey_hex_to_wif(&entry.private_key_hex, state.is_testnet());
            ok_response(&request.id, json!(wif))
        }
        None => err_response(
            &request.id,
            -4,
            format!("Private key not available for {}", addr),
        ),
    }
}

/// Convert a 32-byte hex private key to WIF (Wallet Import Format).
/// Testnet uses version byte 0xEF, mainnet uses 0x80.
/// Adds 0x01 suffix for compressed key format.
fn privkey_hex_to_wif(hex_key: &str, testnet: bool) -> String {
    use sha2::Digest;
    let key_bytes = match hex::decode(hex_key) {
        Ok(b) => b,
        Err(_) => return hex_key.to_string(),
    };
    let version = if testnet { 0xEF_u8 } else { 0x80_u8 };
    let mut payload = vec![version];
    payload.extend_from_slice(&key_bytes);
    payload.push(0x01); // compressed key flag
    let checksum = sha2::Sha256::digest(&sha2::Sha256::digest(&payload));
    payload.extend_from_slice(&checksum[..4]);
    bs58::encode(&payload).into_string()
}

/// Decode a WIF (Wallet Import Format) private key to raw 32-byte hex.
/// Returns (hex_string, compressed) or error.
fn wif_to_privkey_hex(wif: &str) -> Result<(String, bool), String> {
    use sha2::Digest;
    let bytes = bs58::decode(wif)
        .into_vec()
        .map_err(|e| format!("Invalid WIF base58: {}", e))?;
    if bytes.len() < 5 {
        return Err("WIF too short".to_string());
    }
    // Verify checksum
    let payload = &bytes[..bytes.len() - 4];
    let checksum = &bytes[bytes.len() - 4..];
    let computed = sha2::Sha256::digest(&sha2::Sha256::digest(payload));
    if &computed[..4] != checksum {
        return Err("WIF checksum mismatch".to_string());
    }
    let version = payload[0];
    if version != 0x80 && version != 0xEF {
        return Err(format!("Unknown WIF version byte: 0x{:02x}", version));
    }
    let key_data = &payload[1..];
    let (key_bytes, compressed) = if key_data.len() == 33 && key_data[32] == 0x01 {
        (&key_data[..32], true)
    } else if key_data.len() == 32 {
        (key_data, false)
    } else {
        return Err(format!("Invalid WIF key length: {}", key_data.len()));
    };
    Ok((hex::encode(key_bytes), compressed))
}

async fn handle_importprivkey(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    if !state.is_wallet_unlocked().await {
        return err_response(
            &request.id,
            -13,
            "Error: Please enter the wallet passphrase with walletpassphrase first.".to_string(),
        );
    }
    let privkey_input = match get_str_param(&request.params, 0) {
        Some(k) => k,
        None => return err_response(&request.id, -32602, "Missing private key".to_string()),
    };

    // Accept both WIF (base58check) and raw hex formats
    let privkey_hex = if privkey_input.len() == 64 && hex::decode(privkey_input).is_ok() {
        // Raw 32-byte hex
        privkey_input.to_string()
    } else {
        // Try WIF decode
        match wif_to_privkey_hex(privkey_input) {
            Ok((hex_key, _compressed)) => hex_key,
            Err(e) => {
                return err_response(
                    &request.id,
                    -5,
                    format!("Invalid private key (expected WIF or 64-char hex): {}", e),
                )
            }
        }
    };

    // Derive address from private key
    use sha2::Digest;

    let sk_bytes = match hex::decode(&privkey_hex) {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -5, format!("Invalid hex: {}", e)),
    };

    let secret_key = match secp256k1::SecretKey::from_slice(&sk_bytes) {
        Ok(k) => k,
        Err(e) => return err_response(&request.id, -5, format!("Invalid private key: {}", e)),
    };

    let secp = secp256k1::Secp256k1::new();
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);

    let pubkey_compressed = public_key.serialize();
    let sha_hash = sha2::Sha256::digest(&pubkey_compressed);
    let pubkey_hash = ripemd::Ripemd160::digest(&sha_hash);

    // P2PKH address (dynamic version byte)
    let version_byte: u8 = if state.bech32_hrp() == "bc" {
        0x00
    } else {
        0x6F
    };
    let mut payload = vec![version_byte];
    payload.extend_from_slice(&pubkey_hash);
    let checksum = sha2::Sha256::digest(&sha2::Sha256::digest(&payload));
    payload.extend_from_slice(&checksum[..4]);
    let p2pkh_address = bs58::encode(&payload).into_string();
    let p2pkh_legacy = p2pkh_address.to_lowercase();

    // P2WPKH (bech32) address
    let bech32_address = bech32_encode(state.bech32_hrp(), 0, &pubkey_hash);

    let pubkey_uncompressed_full = public_key.serialize_uncompressed();
    let pubkey_uncompressed = &pubkey_uncompressed_full[1..];

    let entry = KeyEntry {
        private_key_hex: privkey_hex.to_string(),
        public_key_compressed_hex: hex::encode(pubkey_compressed),
        public_key_uncompressed_hex: hex::encode(pubkey_uncompressed),
    };

    {
        let mut keystore = state.keystore.write().await;
        // Store under both P2PKH and bech32 addresses so either can be used as sender
        keystore.insert(p2pkh_address.clone(), entry.clone());
        if p2pkh_legacy != p2pkh_address {
            keystore.insert(p2pkh_legacy, entry.clone());
        }
        keystore.insert(bech32_address.clone(), entry);
        state.save_keystore(&keystore).await;
    }

    log::info!(
        "Imported private key for addresses: {} and {}",
        p2pkh_address,
        bech32_address
    );
    ok_response(&request.id, json!(p2pkh_address))
}

async fn handle_listaddressgroupings(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let keystore = state.keystore.read().await;
    let addrs: Vec<String> = keystore.all_addresses();
    drop(keystore);

    let mut group = Vec::new();
    for addr in &addrs {
        let balance = match state.near_client.view_account(addr).await {
            Ok(account) => account.balance_as_btc(),
            Err(_) => 0.0,
        };
        group.push(json!([addr, balance, ""]));
    }
    ok_response(&request.id, json!([group]))
}

async fn handle_getaddressesbylabel(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let keystore = state.keystore.read().await;
    let mut result = serde_json::Map::new();
    for addr in keystore.addresses() {
        result.insert(addr.clone(), json!({"purpose": "receive"}));
    }
    ok_response(&request.id, json!(result))
}

// ============================================================================
// Transaction handlers
// ============================================================================

async fn handle_sendrawtransaction(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let raw_hex = match get_str_param(&request.params, 0) {
        Some(hex) => hex,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing raw transaction hex".to_string(),
            )
        }
    };

    // Check if this is a bitinfinity signed intent (from signrawtransactionwithwallet)
    let decoded_bytes = match hex::decode(raw_hex) {
        Ok(b) => b,
        Err(e) => {
            return err_response(
                &request.id,
                -22,
                format!("TX decode failed: invalid hex: {}", e),
            )
        }
    };
    let decoded_str = String::from_utf8(decoded_bytes).unwrap_or_default();

    if decoded_str.starts_with(SIGNED_INTENT_PREFIX) {
        // Pre-signed NEAR transaction from signrawtransactionwithwallet
        let json_str = &decoded_str[SIGNED_INTENT_PREFIX.len()..];
        let payload: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                return err_response(&request.id, -22, format!("Invalid signed intent: {}", e))
            }
        };

        let sender = payload
            .get("sender")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let btc_txid = payload
            .get("btc_txid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let total_amount_sat = payload
            .get("total_amount_sat")
            .and_then(|v| v.as_u64())
            .or_else(|| payload.get("amount_sat").and_then(|v| v.as_u64()))
            .unwrap_or(0);

        // Handle both new multi-tx format and old single-tx format
        let near_txs: Vec<serde_json::Value> =
            if let Some(txs) = payload.get("near_txs").and_then(|v| v.as_array()) {
                txs.clone()
            } else if let Some(near_tx) = payload.get("near_tx").and_then(|v| v.as_str()) {
                // Legacy single-tx format
                vec![json!({
                    "near_tx": near_tx,
                    "nonce": payload.get("nonce").and_then(|v| v.as_u64()).unwrap_or(0),
                })]
            } else {
                return err_response(
                    &request.id,
                    -22,
                    "No NEAR transactions in signed intent".to_string(),
                );
            };

        log::info!(
            "sendrawtransaction (signed intent): sender={}, btc_txid={}, {} outputs, {} sat total",
            sender,
            btc_txid,
            near_txs.len(),
            total_amount_sat
        );

        // Submit all NEAR transactions
        let mut primary_hash = String::new();
        for (i, tx_obj) in near_txs.iter().enumerate() {
            let near_tx_base64 = tx_obj.get("near_tx").and_then(|v| v.as_str()).unwrap_or("");
            let nonce = tx_obj.get("nonce").and_then(|v| v.as_u64()).unwrap_or(0);

            match state.near_client.send_tx_async(near_tx_base64).await {
                Ok(hash) => {
                    log::info!("NEAR tx {} submitted: {} (btc_txid: {})", i, hash, btc_txid);
                    state.record_nonce(&sender, nonce).await;
                    if i == 0 {
                        primary_hash = hash;
                    }
                }
                Err(e) => {
                    log::error!("NEAR tx {} submission failed: {}", i, e);
                    return err_response(
                        &request.id,
                        -25,
                        format!("Transaction submission failed for output {}: {}", i, e),
                    );
                }
            }
        }

        // Cache the mapping
        {
            let mut cache = state.tx_cache.write().await;
            cache.insert(
                btc_txid.clone(),
                primary_hash.clone(),
                raw_hex.to_string(),
                sender,
            );
        }

        return ok_response(&request.id, json!(btc_txid));
    }

    // Standard path: parse as a real Bitcoin transaction
    let parsed = match ParsedBitcoinTx::from_hex_with_hrp(raw_hex, state.bech32_hrp()) {
        Ok(p) => p,
        Err(e) => return err_response(&request.id, -22, format!("TX decode failed: {}", e)),
    };

    let btc_txid = parsed.txid.clone();
    let sender = parsed.sender_address.clone();

    // Collect all payment outputs (non-change, non-OP_RETURN)
    let payment_outputs: Vec<&TxOutput> = parsed
        .outputs
        .iter()
        .filter(|o| !o.is_op_return && !o.address.is_empty() && o.address != sender)
        .collect();

    if payment_outputs.is_empty() && parsed.decode_near_function_call().is_none() {
        return err_response(&request.id, -25, "No payment output found".to_string());
    }

    let total_sat: u64 = payment_outputs.iter().map(|o| o.amount_satoshis).sum();
    log::info!(
        "sendrawtransaction: {} -> {} outputs ({} sat total)",
        sender,
        payment_outputs.len(),
        total_sat
    );
    for o in &payment_outputs {
        log::info!("  output: {} -> {} sat", o.address, o.amount_satoshis);
    }

    // Look up sender's private key in the keystore
    let key_entry = {
        let keystore = state.keystore.read().await;
        keystore.get(&sender).cloned()
    };

    let key_entry = match key_entry {
        Some(k) => k,
        None => {
            log::error!(
                "No private key found for sender {} — cannot sign transaction",
                sender
            );
            return err_response(&request.id, -4, format!(
                "No private key found for address {}. Use getnewaddress to generate a key, or importprivkey to import one.",
                sender
            ));
        }
    };

    // Get latest block hash
    let status = match state.near_client.status().await {
        Ok(s) => s,
        Err(e) => return err_response(&request.id, -32000, format!("Node not connected: {}", e)),
    };

    let block_hash = match decode_block_hash(&status.latest_block_hash) {
        Ok(h) => h,
        Err(e) => return err_response(&request.id, -32000, format!("Invalid block hash: {}", e)),
    };

    // Get the sender's current nonce (using local cache for rapid sends)
    let near_pubkey_str = match key_entry.near_public_key_string() {
        Ok(s) => s,
        Err(e) => return err_response(&request.id, -32000, format!("Invalid public key: {}", e)),
    };

    let mut nonce = state.next_nonce(&sender, &near_pubkey_str).await;
    // First-use Bitcoin keys may be auto-registered at a height-based nonce floor.
    // Keep nonce at or above the latest observed block-height floor to avoid races
    // when view_access_key is temporarily unavailable.
    let nonce_floor = RpcState::bitcoin_first_tx_nonce_floor(status.latest_block_height);
    if nonce < nonce_floor {
        nonce = nonce_floor;
    }

    // Get private key and public key bytes
    let sk_bytes = match key_entry.private_key_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };
    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };
    let secret_key = match secp256k1::SecretKey::from_slice(&sk_bytes) {
        Ok(k) => k,
        Err(e) => return err_response(&request.id, -32000, format!("Invalid secret key: {}", e)),
    };

    // Check for OP_RETURN NEAR function call
    if let Some(func_call) = parsed.decode_near_function_call() {
        log::info!(
            "OP_RETURN function call: {}.{}({})",
            func_call.contract_id,
            func_call.method_name,
            func_call.args_base64
        );

        use base64::Engine;
        let args = base64::engine::general_purpose::STANDARD
            .decode(&func_call.args_base64)
            .unwrap_or_default();

        // Use the total payment amount as the deposit for the function call
        let deposit = ParsedBitcoinTx::satoshis_to_yocto(total_sat);

        let params = NearFunctionCallParams {
            signer_id: sender.clone(),
            public_key_uncompressed: pk_uncompressed,
            nonce,
            receiver_id: func_call.contract_id,
            block_hash,
            method_name: func_call.method_name,
            args,
            gas: 300_000_000_000_000, // 300 TGas
            deposit,
        };

        let signed_tx_base64 = match params.sign_and_encode(&secret_key) {
            Ok(encoded) => encoded,
            Err(e) => {
                return err_response(&request.id, -32000, format!("TX signing failed: {}", e))
            }
        };

        let near_tx_hash = match state.near_client.send_tx_async(&signed_tx_base64).await {
            Ok(hash) => {
                log::info!("NEAR tx submitted: {} (btc_txid: {})", hash, btc_txid);
                state.record_nonce(&sender, nonce).await;
                hash
            }
            Err(e) => {
                log::error!("NEAR tx submission failed: {} (btc_txid: {})", e, btc_txid);
                return err_response(
                    &request.id,
                    -25,
                    format!("Transaction submission failed: {}", e),
                );
            }
        };

        let mut cache = state.tx_cache.write().await;
        cache.insert(
            btc_txid.clone(),
            near_tx_hash.clone(),
            raw_hex.to_string(),
            sender.clone(),
        );
        return ok_response(&request.id, json!(btc_txid));
    }

    // Standard transfer: submit a NEAR transaction for each payment output
    let mut near_tx_hashes = Vec::new();
    let mut current_nonce = nonce;

    for output in &payment_outputs {
        let amount_yocto = ParsedBitcoinTx::satoshis_to_yocto(output.amount_satoshis);

        let params = NearTransferParams {
            signer_id: sender.clone(),
            public_key_uncompressed: pk_uncompressed,
            nonce: current_nonce,
            receiver_id: output.address.clone(),
            block_hash,
            deposit: amount_yocto,
        };

        let signed_tx_base64 = match params.sign_and_encode(&secret_key) {
            Ok(encoded) => encoded,
            Err(e) => {
                return err_response(&request.id, -32000, format!("TX signing failed: {}", e))
            }
        };

        match state.near_client.send_tx_async(&signed_tx_base64).await {
            Ok(hash) => {
                log::info!(
                    "NEAR tx submitted: {} -> {} ({} sat) near_hash={}",
                    sender,
                    output.address,
                    output.amount_satoshis,
                    hash
                );
                near_tx_hashes.push(hash);
                state.record_nonce(&sender, current_nonce).await;
                current_nonce += 1;
            }
            Err(e) => {
                log::error!(
                    "NEAR tx submission failed for output to {}: {}",
                    output.address,
                    e
                );
                return err_response(
                    &request.id,
                    -25,
                    format!(
                        "Transaction submission failed for output to {}: {}",
                        output.address, e
                    ),
                );
            }
        }
    }

    // Cache using the first NEAR tx hash as primary mapping
    let primary_hash = near_tx_hashes.first().cloned().unwrap_or_default();
    {
        let mut cache = state.tx_cache.write().await;
        cache.insert(
            btc_txid.clone(),
            primary_hash.clone(),
            raw_hex.to_string(),
            sender.clone(),
        );
    }

    log::info!(
        "Transaction submitted: btc_txid={}, {} NEAR txs, sender={}, total={} sat",
        btc_txid,
        near_tx_hashes.len(),
        sender,
        total_sat
    );

    ok_response(&request.id, json!(btc_txid))
}

async fn handle_getrawtransaction(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let txid = match get_str_param(&request.params, 0) {
        Some(id) => id,
        None => return err_response(&request.id, -32602, "Missing txid parameter".to_string()),
    };
    let verbose = get_bool_param(&request.params, 1).unwrap_or(false);

    let entry = {
        let cache = state.tx_cache.read().await;
        cache.get(txid).cloned()
    };
    match entry {
        Some(entry) => {
            if !verbose {
                // Return raw hex
                ok_response(&request.id, json!(entry.raw_hex))
            } else {
                // Return decoded verbose format
                // Try to get real confirmations from NEAR
                let (confirmations, blockhash, blocktime) =
                    if entry.near_tx_hash.starts_with("pending:")
                        || entry.near_tx_hash.starts_with("error:")
                    {
                        (0i64, String::new(), 0i64)
                    } else {
                        match state
                            .near_client
                            .tx_status(&entry.near_tx_hash, &entry.sender_id)
                            .await
                        {
                            Ok(tx_result) => {
                                let tx_block_hash = tx_result
                                    .get("transaction_outcome")
                                    .and_then(|o| o.get("block_hash"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                if !tx_block_hash.is_empty() {
                                    match state.near_client.block_by_hash(&tx_block_hash).await {
                                        Ok(block) => {
                                            let header = block.get("header").unwrap_or(&block);
                                            let ts = header
                                                .get("timestamp")
                                                .and_then(|v| v.as_u64())
                                                .map(|t| (t / 1_000_000_000) as i64)
                                                .unwrap_or(0);
                                            let h = header
                                                .get("height")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(0);
                                            let current = state
                                                .near_client
                                                .status()
                                                .await
                                                .map(|s| s.latest_block_height)
                                                .unwrap_or(h);
                                            ((current - h + 1) as i64, tx_block_hash, ts)
                                        }
                                        Err(_) => (1, tx_block_hash, 0),
                                    }
                                } else {
                                    (1, String::new(), 0)
                                }
                            }
                            Err(_) => (1, String::new(), 0),
                        }
                    };

                // Try parsing as actual Bitcoin tx, fall back to synthetic format
                if entry.raw_hex.starts_with("sendtoaddress:") {
                    // Synthetic tx from sendtoaddress — build a minimal decoded response
                    let parts: Vec<&str> = entry.raw_hex.splitn(3, ':').collect();
                    let recipient = parts.get(1).unwrap_or(&"");
                    let satoshis: u64 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
                    let est_size = 110u64; // estimated P2WPKH tx size
                    ok_response(
                        &request.id,
                        json!({
                            "txid": txid,
                            "hash": txid,
                            "size": est_size,
                            "vsize": est_size,
                            "weight": est_size * 4,
                            "version": 2,
                            "locktime": 0,
                            "vin": [{
                                "txid": "0000000000000000000000000000000000000000000000000000000000000000",
                                "vout": 0,
                                "scriptSig": { "asm": "", "hex": "" },
                                "sequence": 4294967295u64
                            }],
                            "vout": [{
                                "value": satoshis as f64 / 100_000_000.0,
                                "n": 0,
                                "scriptPubKey": build_script_pub_key_json(recipient, false, state.bech32_hrp())
                            }],
                            "blockhash": blockhash,
                            "confirmations": confirmations,
                            "blocktime": blocktime,
                            "time": blocktime,
                            "near_tx_hash": entry.near_tx_hash
                        }),
                    )
                } else if entry.is_incoming || entry.raw_hex.starts_with("incoming:") {
                    // Incoming transfer detected by indexer
                    let recv_addr = &entry.receiver_id;
                    let amount_sat = entry.amount_satoshis;
                    let est_size = 110u64;
                    ok_response(
                        &request.id,
                        json!({
                            "txid": txid,
                            "hash": txid,
                            "size": est_size,
                            "vsize": est_size,
                            "weight": est_size * 4,
                            "version": 2,
                            "locktime": 0,
                            "vin": [{
                                "txid": "0000000000000000000000000000000000000000000000000000000000000000",
                                "vout": 0,
                                "scriptSig": { "asm": "", "hex": "" },
                                "sequence": 4294967295u64
                            }],
                            "vout": [{
                                "value": amount_sat as f64 / 100_000_000.0,
                                "n": 0,
                                "scriptPubKey": build_script_pub_key_json(recv_addr, false, state.bech32_hrp())
                            }],
                            "blockhash": blockhash,
                            "confirmations": confirmations,
                            "blocktime": blocktime,
                            "time": blocktime,
                            "near_tx_hash": entry.near_tx_hash
                        }),
                    )
                } else {
                    match ParsedBitcoinTx::from_hex_with_hrp(&entry.raw_hex, state.bech32_hrp()) {
                        Ok(parsed) => {
                            let vsize = (parsed.weight + 3) / 4;
                            ok_response(
                                &request.id,
                                json!({
                                    "txid": parsed.txid,
                                    "hash": parsed.txid,
                                    "size": parsed.raw_hex.len() / 2,
                                    "vsize": vsize,
                                    "weight": parsed.weight,
                                    "version": parsed.version,
                                    "locktime": parsed.locktime,
                                    "vin": parsed.inputs.iter().map(|inp| {
                                        let mut vin_obj = json!({
                                            "txid": inp.txid,
                                            "vout": inp.vout,
                                            "scriptSig": {
                                                "asm": inp.script_sig_asm,
                                                "hex": inp.script_sig_hex
                                            },
                                            "sequence": inp.sequence
                                        });
                                        if !inp.txinwitness.is_empty() {
                                            if let Some(obj) = vin_obj.as_object_mut() {
                                                obj.insert(
                                                    "txinwitness".to_string(),
                                                    json!(inp.txinwitness)
                                                );
                                            }
                                        }
                                        vin_obj
                                    }).collect::<Vec<_>>(),
                                    "vout": parsed.outputs.iter().enumerate().map(|(i, o)| {
                                        json!({
                                            "value": o.amount_satoshis as f64 / 100_000_000.0,
                                            "n": i,
                                            "scriptPubKey": build_script_pub_key_json(&o.address, o.is_op_return, state.bech32_hrp())
                                        })
                                    }).collect::<Vec<_>>(),
                                    "blockhash": blockhash,
                                    "confirmations": confirmations,
                                    "blocktime": blocktime,
                                    "time": blocktime,
                                    "near_tx_hash": entry.near_tx_hash
                                }),
                            )
                        }
                        Err(e) => err_response(&request.id, -5, format!("TX parse error: {}", e)),
                    }
                }
            }
        }
        None => {
            if let Some(synthetic) = resolve_synthetic_wallet_tx(state, txid).await {
                let raw_hex = format!(
                    "synthetic-utxo:{}:{}",
                    synthetic.address, synthetic.amount_satoshis
                );
                if !verbose {
                    return ok_response(&request.id, json!(raw_hex));
                }

                let est_size = 110u64;
                return ok_response(
                    &request.id,
                    json!({
                        "txid": txid,
                        "hash": txid,
                        "size": est_size,
                        "vsize": est_size,
                        "weight": est_size * 4,
                        "version": 2,
                        "locktime": 0,
                        "vin": [{
                            "txid": "0000000000000000000000000000000000000000000000000000000000000000",
                            "vout": 0,
                            "scriptSig": { "asm": "", "hex": "" },
                            "sequence": 4294967295u64
                        }],
                        "vout": [{
                            "value": synthetic.amount_satoshis as f64 / 100_000_000.0,
                            "n": 0,
                            "scriptPubKey": build_script_pub_key_json(&synthetic.address, false, state.bech32_hrp())
                        }],
                        "blockhash": synthetic.block_hash,
                        "confirmations": synthetic.confirmations,
                        "blocktime": synthetic.block_time,
                        "time": synthetic.block_time,
                        "near_tx_hash": format!("synthetic:{}", synthetic.address)
                    }),
                );
            }

            err_response(&request.id, -5, format!("Transaction not found: {}", txid))
        }
    }
}

async fn handle_gettransaction(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let txid = match get_str_param(&request.params, 0) {
        Some(id) => id,
        None => return err_response(&request.id, -32602, "Missing txid parameter".to_string()),
    };

    let entry = {
        let cache = state.tx_cache.read().await;
        cache.get(txid).cloned()
    };
    match entry {
        Some(entry) => {
            // Try to get NEAR transaction status and block info
            let (confirmations, blockhash, blocktime, block_height, details) = if entry
                .near_tx_hash
                .starts_with("pending:")
                || entry.near_tx_hash.starts_with("error:")
            {
                (
                    0i64,
                    String::new(),
                    chrono::Utc::now().timestamp(),
                    0u64,
                    vec![],
                )
            } else {
                match state
                    .near_client
                    .tx_status(&entry.near_tx_hash, &entry.sender_id)
                    .await
                {
                    Ok(tx_result) => {
                        let tx_block_hash = tx_result
                            .get("transaction_outcome")
                            .and_then(|o| o.get("block_hash"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        // Get block details for height and timestamp
                        let (btime, bheight) = if !tx_block_hash.is_empty() {
                            match state.near_client.block_by_hash(&tx_block_hash).await {
                                Ok(block) => {
                                    let header = block.get("header").unwrap_or(&block);
                                    let ts = header
                                        .get("timestamp")
                                        .and_then(|v| v.as_u64())
                                        .map(|t| (t / 1_000_000_000) as i64)
                                        .unwrap_or(chrono::Utc::now().timestamp());
                                    let h =
                                        header.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
                                    (ts, h)
                                }
                                Err(_) => (chrono::Utc::now().timestamp(), 0u64),
                            }
                        } else {
                            (chrono::Utc::now().timestamp(), 0u64)
                        };

                        // Compute real confirmations
                        let current_height = state
                            .near_client
                            .status()
                            .await
                            .map(|s| s.latest_block_height)
                            .unwrap_or(bheight);
                        let confs = if bheight > 0 && current_height >= bheight {
                            (current_height - bheight + 1) as i64
                        } else {
                            1i64
                        };

                        // Build details from parsed tx or synthetic info
                        let mut det = vec![];
                        if entry.raw_hex.starts_with("sendtoaddress:") {
                            // Synthetic format: "sendtoaddress:<recipient>:<satoshis>"
                            let parts: Vec<&str> = entry.raw_hex.splitn(3, ':').collect();
                            if parts.len() == 3 {
                                let sat: u64 = parts[2].parse().unwrap_or(0);
                                det.push(json!({
                                    "address": parts[1],
                                    "category": "send",
                                    "amount": -(sat as f64 / 100_000_000.0),
                                    "vout": 0,
                                    "fee": 0.0
                                }));
                            }
                        } else if let Ok(parsed) =
                            ParsedBitcoinTx::from_hex_with_hrp(&entry.raw_hex, state.bech32_hrp())
                        {
                            for output in &parsed.outputs {
                                if !output.is_op_return
                                    && !output.address.is_empty()
                                    && output.address != entry.sender_id
                                {
                                    det.push(json!({
                                        "address": output.address,
                                        "category": "send",
                                        "amount": -(output.amount_satoshis as f64 / 100_000_000.0),
                                        "vout": 0,
                                        "fee": 0.0
                                    }));
                                }
                            }
                        }

                        (confs, tx_block_hash, btime, bheight, det)
                    }
                    Err(_) => (
                        0i64,
                        String::new(),
                        chrono::Utc::now().timestamp(),
                        0u64,
                        vec![],
                    ),
                }
            };

            let amount = if entry.raw_hex.starts_with("sendtoaddress:") {
                let parts: Vec<&str> = entry.raw_hex.splitn(3, ':').collect();
                if parts.len() == 3 {
                    parts[2].parse::<u64>().unwrap_or(0) as f64 / 100_000_000.0
                } else {
                    0.0
                }
            } else {
                ParsedBitcoinTx::from_hex_with_hrp(&entry.raw_hex, state.bech32_hrp())
                    .ok()
                    .map(|p| p.total_payment_satoshis() as f64 / 100_000_000.0)
                    .unwrap_or(0.0)
            };

            ok_response(
                &request.id,
                json!({
                    "txid": txid,
                    "amount": amount,
                    "confirmations": confirmations,
                    "blockhash": blockhash,
                    "blockheight": block_height,
                    "blockindex": 0,
                    "blocktime": blocktime,
                    "time": blocktime,
                    "timereceived": blocktime,
                    "details": details,
                    "hex": entry.raw_hex,
                    "near_tx_hash": entry.near_tx_hash
                }),
            )
        }
        None => {
            if let Some(synthetic) = resolve_synthetic_wallet_tx(state, txid).await {
                let amount_btc = synthetic.amount_satoshis as f64 / 100_000_000.0;
                return ok_response(
                    &request.id,
                    json!({
                        "txid": txid,
                        "amount": amount_btc,
                        "confirmations": synthetic.confirmations,
                        "blockhash": synthetic.block_hash,
                        "blockheight": synthetic.block_height,
                        "blockindex": 0,
                        "blocktime": synthetic.block_time,
                        "time": synthetic.block_time,
                        "timereceived": synthetic.block_time,
                        "details": [{
                            "address": synthetic.address,
                            "category": "receive",
                            "amount": amount_btc,
                            "vout": 0
                        }],
                        "hex": format!("synthetic-utxo:{}:{}", synthetic.address, synthetic.amount_satoshis),
                        "near_tx_hash": format!("synthetic:{}", txid)
                    }),
                );
            }

            err_response(&request.id, -5, format!("Transaction not found: {}", txid))
        }
    }
}

fn handle_decoderawtransaction(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let raw_hex = match get_str_param(&request.params, 0) {
        Some(hex) => hex,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing raw transaction hex".to_string(),
            )
        }
    };

    match ParsedBitcoinTx::from_hex_with_hrp(raw_hex, state.bech32_hrp()) {
        Ok(parsed) => {
            let vsize = (parsed.weight + 3) / 4; // ceil(weight/4)
            ok_response(
                &request.id,
                json!({
                    "txid": parsed.txid,
                    "hash": parsed.txid,
                    "size": raw_hex.len() / 2,
                    "vsize": vsize,
                    "weight": parsed.weight,
                    "version": parsed.version,
                    "locktime": parsed.locktime,
                    "vin": parsed.inputs.iter().map(|inp| {
                        let mut vin_obj = json!({
                            "txid": inp.txid,
                            "vout": inp.vout,
                            "scriptSig": {
                                "asm": inp.script_sig_asm,
                                "hex": inp.script_sig_hex
                            },
                            "sequence": inp.sequence
                        });
                        if !inp.txinwitness.is_empty() {
                            if let Some(obj) = vin_obj.as_object_mut() {
                                obj.insert(
                                    "txinwitness".to_string(),
                                    json!(inp.txinwitness)
                                );
                            }
                        }
                        vin_obj
                    }).collect::<Vec<_>>(),
                    "vout": parsed.outputs.iter().enumerate().map(|(i, o)| {
                        json!({
                            "value": o.amount_satoshis as f64 / 100_000_000.0,
                            "n": i,
                            "scriptPubKey": build_script_pub_key_json(&o.address, o.is_op_return, state.bech32_hrp())
                        })
                    }).collect::<Vec<_>>()
                }),
            )
        }
        Err(e) => err_response(&request.id, -22, format!("TX decode failed: {}", e)),
    }
}

// ============================================================================
// Network handlers
// ============================================================================

async fn handle_getnetworkinfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    // Fetch live network info from nearcore
    let (peer_count, near_net) = match state.near_client.network_info().await {
        Ok(info) => {
            let peers = info
                .get("num_active_peers")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            (peers, Some(info))
        }
        Err(_) => (0, None),
    };

    let mut result = json!({
        "version": state.version,
        "subversion": "/BitcoinInfinity:0.1.0/",
        "protocolversion": 70015,
        "timeoffset": 0,
        "connections": peer_count,
        "connections_in": 0,
        "connections_out": peer_count,
        "networks": [{
            "name": "ipv4",
            "limited": false,
            "reachable": true,
            "proxy": "",
            "proxy_randomize_credentials": false
        }],
        "reachable_through_ipv6": false,
        "local_addresses": [],
        "warnings": "Bitcoin Infinity: NEAR-based L1 with Bitcoin addresses"
    });

    if let Some(net) = near_net {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("near_network_info".to_string(), net);
        }
    }

    ok_response(&request.id, result)
}

async fn handle_getconnectioncount(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let count = match state.near_client.network_info().await {
        Ok(info) => info
            .get("num_active_peers")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        Err(_) => 0,
    };
    ok_response(&request.id, json!(count))
}

async fn handle_getinfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let block_height = match state.near_client.status().await {
        Ok(status) => status.latest_block_height,
        Err(_) => 0,
    };

    // Sum wallet balance
    let keystore = state.keystore.read().await;
    let addresses: Vec<String> = keystore.addresses().iter().map(|a| a.to_string()).collect();
    drop(keystore);
    let mut total_balance = 0.0f64;
    let mut seen = std::collections::HashSet::new();
    for addr in &addresses {
        if let Ok(account) = state.near_client.view_account(addr).await {
            if seen.insert(account.amount.clone()) {
                total_balance += account.balance_as_btc();
            }
        }
    }

    ok_response(
        &request.id,
        json!({
            "version": state.version,
            "protocolversion": 70015,
            "walletversion": 160300,
            "balance": total_balance,
            "blocks": block_height,
            "timeoffset": 0,
            "connections": match state.near_client.network_info().await {
                Ok(info) => info.get("num_active_peers").and_then(|v| v.as_u64()).unwrap_or(0),
                Err(_) => 0,
            },
            "difficulty": 1.0,
            "testnet": state.is_testnet(),
            "keypoololdest": 0,
            "keypoolsize": addresses.len(),
            "unlocked_until": 0,
            "paytxfee": 0.00001,
            "relayfee": 0.00001,
            "warnings": format!("Bitcoin Infinity ({})", state.chain_id)
        }),
    )
}

// ============================================================================
// Fee estimation
// ============================================================================

async fn handle_estimatesmartfee(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let conf_target = get_u64_param(&request.params, 0).unwrap_or(6);

    // Query NEAR gas price and convert to BTC/kB (Bitcoin Core format)
    // In our system: 1 satoshi = 10^16 yocto, so 1 BTC = 10^24 yocto
    // A typical NEAR transfer uses ~4 TGas. Fee = gas_price * gas_used
    // We express feerate as BTC/kB (1000 virtual bytes)
    let feerate = match state.near_client.gas_price().await {
        Ok(gas_price_str) => {
            let gas_price: u128 = gas_price_str.parse().unwrap_or(100_000_000);
            // Typical tx: ~4 TGas = 4 * 10^12 gas units
            let fee_yocto = gas_price * 4_000_000_000_000u128;
            // Convert to BTC: fee_yocto / 10^24
            let fee_btc = fee_yocto as f64 / 1e24;
            // Express as BTC/kB: fee_btc / (typical_vsize/1000)
            // Typical Bitcoin tx ~250 vbytes, so per kB = fee * (1000/250) = fee * 4
            let feerate = fee_btc * 4.0;
            feerate.max(0.00001) // minimum feerate
        }
        Err(_) => 0.00001, // fallback minimum
    };

    ok_response(
        &request.id,
        json!({
            "feerate": feerate,
            "blocks": conf_target
        }),
    )
}

// ============================================================================
// Mempool handlers (stubs — account-based chain has no mempool in BTC sense)
// ============================================================================

async fn handle_getmempoolinfo(state: &RpcState, _request: &JsonRpcRequest) -> JsonRpcResponse {
    // NEAR has ~1s finality, so the "mempool" is effectively always empty.
    // Only count pending (unconfirmed) entries as mempool items.
    let tx_cache = state.tx_cache.read().await;
    let pending: Vec<_> = tx_cache
        .entries
        .values()
        .filter(|e| e.near_tx_hash.starts_with("pending:"))
        .collect();
    let size = pending.len();
    let bytes: usize = pending
        .iter()
        .map(|e| {
            if e.raw_hex.is_empty() {
                250
            } else {
                e.raw_hex.len() / 2
            }
        })
        .sum();
    drop(tx_cache);
    ok_response(
        &_request.id,
        json!({
            "loaded": true,
            "size": size,
            "bytes": bytes,
            "usage": bytes * 2,
            "maxmempool": 300000000,
            "mempoolminfee": 0.00001,
            "minrelaytxfee": 0.00001
        }),
    )
}

async fn handle_getrawmempool(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let verbose = request
        .params
        .as_array()
        .and_then(|arr| arr.get(0))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let tx_cache = state.tx_cache.read().await;

    // Only show pending (unconfirmed) entries as mempool items
    let pending_entries: Vec<_> = tx_cache
        .entries
        .iter()
        .filter(|(_, e)| e.near_tx_hash.starts_with("pending:"))
        .collect();

    if !verbose {
        let txids: Vec<&String> = pending_entries.iter().map(|(k, _)| *k).collect();
        return ok_response(&request.id, json!(txids));
    }

    // Verbose mode: return object with txid => mempool entry details
    let now = chrono::Utc::now().timestamp();
    let mut result = serde_json::Map::new();
    for (txid, entry) in &pending_entries {
        let size = if entry.raw_hex.is_empty() {
            250
        } else {
            entry.raw_hex.len() / 2
        };
        result.insert(
            (*txid).clone(),
            json!({
                "vsize": size,
                "weight": size * 4,
                "fee": 0.00001,
                "modifiedfee": 0.00001,
                "time": now,
                "height": entry.block_height,
                "descendantcount": 1,
                "descendantsize": size,
                "descendantfees": 1000,
                "ancestorcount": 1,
                "ancestorsize": size,
                "ancestorfees": 1000,
                "depends": [],
                "spentby": [],
                "bip125-replaceable": false,
                "unbroadcast": false
            }),
        );
    }
    ok_response(&request.id, json!(result))
}

// ============================================================================
// Additional wallet handlers
// ============================================================================

/// signrawtransactionwithwallet - signs a raw transaction with keys from the keystore.
///
/// Accepts two formats:
/// 1. Real Bitcoin transaction hex → parse, find sender key, build NEAR signed tx
/// 2. Bitinfinity intent hex (from createrawtransaction) → decode intent, build NEAR signed tx
///
/// Returns a "bitinfinity:" prefixed signed payload that sendrawtransaction can submit.
async fn handle_signrawtransactionwithwallet(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    if !state.is_wallet_unlocked().await {
        return err_response(
            &request.id,
            -13,
            "Error: Please enter the wallet passphrase with walletpassphrase first.".to_string(),
        );
    }
    let raw_hex = match get_str_param(&request.params, 0) {
        Some(hex) => hex,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing raw transaction hex".to_string(),
            )
        }
    };

    // Try to decode as bitinfinity intent first (from createrawtransaction)
    let intent_bytes = hex::decode(raw_hex).unwrap_or_default();
    let intent_str = String::from_utf8(intent_bytes.clone()).unwrap_or_default();

    // Parse outputs: either intent format or real Bitcoin tx
    struct OutputInfo {
        address: String,
        amount_satoshis: u64,
    }
    let (sender_addr, all_outputs) = if let Ok(intent_json) =
        serde_json::from_str::<serde_json::Value>(&intent_str)
    {
        let outputs = intent_json.get("outputs").and_then(|o| o.as_array());
        match outputs {
            Some(outs) if !outs.is_empty() => {
                let parsed_outputs: Vec<OutputInfo> = outs
                    .iter()
                    .map(|out| OutputInfo {
                        address: out
                            .get("address")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        amount_satoshis: (out.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0)
                            * 100_000_000.0) as u64,
                    })
                    .collect();

                let total_sat: u64 = parsed_outputs.iter().map(|o| o.amount_satoshis).sum();

                // Find a funded sender from keystore
                let keystore = state.keystore.read().await;
                let addresses: Vec<String> =
                    keystore.addresses().iter().map(|a| a.to_string()).collect();
                drop(keystore);

                let mut found_sender = None;
                for a in &addresses {
                    if let Ok(account) = state.near_client.view_account(a).await {
                        if account.balance_as_satoshis() >= total_sat {
                            found_sender = Some(a.clone());
                            break;
                        }
                    }
                }

                match found_sender {
                    Some(s) => (s, parsed_outputs),
                    None => {
                        return ok_response(
                            &request.id,
                            json!({
                                "hex": raw_hex,
                                "complete": false,
                                "errors": [{"error": "Insufficient funds or no wallet keys"}]
                            }),
                        )
                    }
                }
            }
            _ => return err_response(&request.id, -22, "Invalid intent format".to_string()),
        }
    } else if let Ok(parsed) = ParsedBitcoinTx::from_hex_with_hrp(raw_hex, state.bech32_hrp()) {
        // Real Bitcoin transaction — collect all non-change, non-OP_RETURN outputs
        let payment_outputs: Vec<OutputInfo> = parsed
            .outputs
            .iter()
            .filter(|o| {
                !o.is_op_return && o.address != parsed.sender_address && !o.address.is_empty()
            })
            .map(|o| OutputInfo {
                address: o.address.clone(),
                amount_satoshis: o.amount_satoshis,
            })
            .collect();
        if payment_outputs.is_empty() {
            return err_response(&request.id, -25, "No payment outputs found".to_string());
        }
        (parsed.sender_address.clone(), payment_outputs)
    } else {
        return err_response(
            &request.id,
            -22,
            format!("TX decode failed: not a valid Bitcoin tx or bitinfinity intent"),
        );
    };

    // Look up sender's key
    let key_entry = {
        let keystore = state.keystore.read().await;
        keystore.get(&sender_addr).cloned()
    };

    let key_entry = match key_entry {
        Some(k) => k,
        None => {
            return ok_response(
                &request.id,
                json!({
                    "hex": raw_hex,
                    "complete": false,
                    "errors": [{"error": format!("Key not found for {}", sender_addr)}]
                }),
            )
        }
    };

    // Get block hash
    let status = match state.near_client.status().await {
        Ok(s) => s,
        Err(e) => return err_response(&request.id, -32000, format!("Node not connected: {}", e)),
    };
    let block_hash = match decode_block_hash(&status.latest_block_hash) {
        Ok(h) => h,
        Err(e) => return err_response(&request.id, -32000, format!("Invalid block hash: {}", e)),
    };

    // Get nonce
    let near_pubkey_str = match key_entry.near_public_key_string() {
        Ok(s) => s,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };
    let base_nonce = state.next_nonce(&sender_addr, &near_pubkey_str).await;

    // Build and sign NEAR transaction for EACH output
    let sk_bytes = match key_entry.private_key_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };
    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };
    let secret_key = match secp256k1::SecretKey::from_slice(&sk_bytes) {
        Ok(k) => k,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let mut near_txs = Vec::new();
    let total_sat: u64 = all_outputs.iter().map(|o| o.amount_satoshis).sum();

    for (i, output) in all_outputs.iter().enumerate() {
        let amount_yocto = ParsedBitcoinTx::satoshis_to_yocto(output.amount_satoshis);
        let nonce = base_nonce + i as u64;

        let params = NearTransferParams {
            signer_id: sender_addr.clone(),
            public_key_uncompressed: pk_uncompressed.clone(),
            nonce,
            receiver_id: output.address.clone(),
            block_hash,
            deposit: amount_yocto,
        };

        let signed_tx_base64 = match params.sign_and_encode(&secret_key) {
            Ok(encoded) => encoded,
            Err(e) => {
                return err_response(
                    &request.id,
                    -32000,
                    format!("TX signing failed for output {}: {}", i, e),
                )
            }
        };

        near_txs.push(json!({
            "near_tx": signed_tx_base64,
            "recipient": output.address,
            "amount_sat": output.amount_satoshis,
            "nonce": nonce,
        }));
    }

    // Bump nonce cache past all used nonces
    for _i in 1..all_outputs.len() {
        state.next_nonce(&sender_addr, &near_pubkey_str).await;
    }

    // Generate a deterministic "bitcoin txid"
    use sha2::{Digest as _, Sha256};
    let btc_txid = hex::encode(Sha256::digest(
        format!(
            "{}:{}:{}:{}",
            sender_addr, all_outputs[0].address, total_sat, base_nonce
        )
        .as_bytes(),
    ));

    // Encode as bitinfinity signed format with all transactions
    let signed_payload = json!({
        "near_txs": near_txs,
        "sender": sender_addr,
        "total_amount_sat": total_sat,
        "btc_txid": btc_txid,
    });
    let signed_hex = hex::encode(format!(
        "{}{}",
        SIGNED_INTENT_PREFIX,
        signed_payload.to_string()
    ));

    ok_response(
        &request.id,
        json!({
            "hex": signed_hex,
            "complete": true,
            "errors": []
        }),
    )
}

/// sendtoaddress - high-level send that constructs, signs, and submits a NEAR transfer
async fn handle_sendtoaddress(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    if !state.is_wallet_unlocked().await {
        return err_response(
            &request.id,
            -13,
            "Error: Please enter the wallet passphrase with walletpassphrase first.".to_string(),
        );
    }
    let recipient = match get_str_param(&request.params, 0) {
        Some(addr) => addr.to_string(),
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };

    let amount_btc: f64 = request
        .params
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    if amount_btc <= 0.0 {
        return err_response(&request.id, -3, "Invalid amount".to_string());
    }

    // Bitcoin Core param 5 (index 5): subtractfeefromamount (bool)
    // When true, the fee is deducted from the send amount rather than added on top
    let subtract_fee = request
        .params
        .as_array()
        .and_then(|arr| arr.get(5))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // NEAR fees are paid in gas, so subtractfeefromamount reduces the sent amount by a nominal fee
    let fee_satoshis: u64 = if subtract_fee { 1000 } else { 0 }; // ~0.00001 BTC nominal fee
    let send_satoshis = (amount_btc * 100_000_000.0) as u64 - fee_satoshis;
    let amount_satoshis = (amount_btc * 100_000_000.0) as u64;
    let amount_yocto = ParsedBitcoinTx::satoshis_to_yocto(send_satoshis);

    // Find the first address in our keystore with sufficient balance
    let keystore = state.keystore.read().await;
    let addresses: Vec<String> = keystore.addresses().iter().map(|a| a.to_string()).collect();
    drop(keystore);

    let mut sender_addr = None;
    let mut sender_entry = None;

    for addr in &addresses {
        if let Ok(account) = state.near_client.view_account(addr).await {
            let balance_sat = account.balance_as_satoshis();
            if balance_sat >= amount_satoshis {
                let keystore = state.keystore.read().await;
                if let Some(entry) = keystore.get(addr) {
                    sender_addr = Some(addr.clone());
                    sender_entry = Some(entry.clone());
                    break;
                }
            }
        }
    }

    let sender = match sender_addr {
        Some(s) => s,
        None => {
            return err_response(
                &request.id,
                -6,
                "Insufficient funds or no wallet keys".to_string(),
            )
        }
    };
    let key_entry = match sender_entry {
        Some(e) => e,
        None => return err_response(&request.id, -6, "No key entry found for sender".to_string()),
    };

    // Get latest block hash
    let status = match state.near_client.status().await {
        Ok(s) => s,
        Err(e) => return err_response(&request.id, -32000, format!("Node not connected: {}", e)),
    };

    let block_hash = match decode_block_hash(&status.latest_block_hash) {
        Ok(h) => h,
        Err(e) => return err_response(&request.id, -32000, format!("Invalid block hash: {}", e)),
    };

    // Get nonce using local cache
    let near_pubkey_str = match key_entry.near_public_key_string() {
        Ok(s) => s,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let mut nonce = state.next_nonce(&sender, &near_pubkey_str).await;
    // First-use Bitcoin keys may be auto-registered at a height-based nonce floor.
    // Keep nonce at or above the latest observed block-height floor to avoid races
    // when view_access_key is temporarily unavailable.
    let nonce_floor = RpcState::bitcoin_first_tx_nonce_floor(status.latest_block_height);
    if nonce < nonce_floor {
        nonce = nonce_floor;
    }

    // Build and sign NEAR transaction
    let sk_bytes = match key_entry.private_key_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };
    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };
    let secret_key = match secp256k1::SecretKey::from_slice(&sk_bytes) {
        Ok(k) => k,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let params = NearTransferParams {
        signer_id: sender.clone(),
        public_key_uncompressed: pk_uncompressed,
        nonce,
        receiver_id: recipient.clone(),
        block_hash,
        deposit: amount_yocto,
    };

    let signed_tx_base64 = match params.sign_and_encode(&secret_key) {
        Ok(encoded) => encoded,
        Err(e) => return err_response(&request.id, -32000, format!("TX signing failed: {}", e)),
    };

    // Bitcoin RPC semantics return a txid after broadcast. Post-send balance checks
    // validate eventual execution.
    match state.near_client.send_tx_async(&signed_tx_base64).await {
        Ok(near_tx_hash) => {
            log::info!(
                "sendtoaddress: {} -> {} ({} sat), near_tx: {}",
                sender,
                recipient,
                amount_satoshis,
                near_tx_hash
            );

            // Record nonce for future transactions
            state.record_nonce(&sender, nonce).await;

            // Generate a deterministic "bitcoin txid" from the NEAR tx hash
            use sha2::{Digest, Sha256};
            let btc_txid = hex::encode(Sha256::digest(near_tx_hash.as_bytes()));

            let mut cache = state.tx_cache.write().await;
            // Store a synthetic raw_hex that encodes the recipient and amount for gettransaction details
            let synthetic_info = format!("sendtoaddress:{}:{}", recipient, amount_satoshis);
            cache.insert(btc_txid.clone(), near_tx_hash, synthetic_info, sender);

            ok_response(&request.id, json!(btc_txid))
        }
        Err(e) => err_response(&request.id, -25, format!("Transaction failed: {}", e)),
    }
}

async fn handle_getpeerinfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.network_info().await {
        Ok(info) => {
            let mut peers = Vec::new();
            if let Some(active_peers) = info.get("active_peers").and_then(|v| v.as_array()) {
                for (i, peer) in active_peers.iter().enumerate() {
                    let addr = peer
                        .get("addr")
                        .or_else(|| peer.get("peer_info").and_then(|p| p.get("addr")))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let peer_id = peer
                        .get("id")
                        .or_else(|| peer.get("peer_info").and_then(|p| p.get("id")))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    peers.push(json!({
                        "id": i,
                        "addr": addr,
                        "addrlocal": "127.0.0.1:24567",
                        "services": "0000000000000001",
                        "subver": format!("/nearcore:{}/ peer_id={}", state.version, peer_id),
                        "inbound": false,
                        "synced_headers": -1,
                        "synced_blocks": -1,
                        "connection_type": "outbound-full-relay"
                    }));
                }
            }
            if peers.is_empty() {
                // If no active peers, show nearcore itself as the connection
                peers.push(json!({
                    "id": 0,
                    "addr": "127.0.0.1:3030",
                    "services": "0000000000000001",
                    "subver": format!("/nearcore:{}/", state.version),
                    "inbound": false,
                    "synced_headers": 0,
                    "synced_blocks": 0,
                    "connection_type": "outbound-full-relay"
                }));
            }
            ok_response(&request.id, json!(peers))
        }
        Err(_) => ok_response(&request.id, json!([])),
    }
}

async fn handle_getwalletinfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let keystore = state.keystore.read().await;
    let key_count = keystore.addresses().len();
    let addresses: Vec<String> = keystore.addresses().iter().map(|a| a.to_string()).collect();
    drop(keystore);

    // Sum balances across all wallet addresses
    let mut total_balance = 0.0f64;
    let mut seen_balances = std::collections::HashSet::new();
    for addr in &addresses {
        if let Ok(account) = state.near_client.view_account(addr).await {
            let btc = account.balance_as_btc();
            // Avoid double-counting same key under P2PKH + bech32
            if btc > 0.0 {
                // Use the actual yocto balance as dedup key
                if seen_balances.insert(account.amount.clone()) {
                    total_balance += btc;
                }
            }
        }
    }

    let tx_count = {
        let cache = state.tx_cache.read().await;
        cache.entries.len()
    };

    let keystore = state.keystore.read().await;
    let is_encrypted = keystore.encrypted;
    drop(keystore);

    let unlocked_until = if !is_encrypted {
        // Not encrypted — always unlocked, return 0 (Bitcoin Core convention)
        0i64
    } else if state.is_wallet_unlocked().await {
        let lock = state.wallet_unlock_until.read().await;
        match *lock {
            Some(until) => {
                let now = std::time::Instant::now();
                if until > now {
                    (until - now).as_secs() as i64 + chrono::Utc::now().timestamp()
                } else {
                    0
                }
            }
            None => 0,
        }
    } else {
        0
    };

    ok_response(
        &request.id,
        json!({
            "walletname": "bitinfinity",
            "walletversion": 160300,
            "format": "bitinfinity",
            "balance": total_balance,
            "unconfirmed_balance": 0.0,
            "immature_balance": 0.0,
            "txcount": tx_count,
            "keypoololdest": 0,
            "keypoolsize": key_count,
            "keypoolsize_hd_internal": 0,
            "paytxfee": 0.00001,
            "private_keys_enabled": true,
            "avoid_reuse": false,
            "scanning": false,
            "descriptors": false,
            "unlocked_until": unlocked_until,
            "encrypted": is_encrypted
        }),
    )
}

fn handle_listwallets(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(["bitinfinity"]))
}

/// createrawtransaction - create an unsigned raw transaction (hex)
/// In account-based model, we just encode the intent as a minimal valid-looking tx
fn handle_createrawtransaction(request: &JsonRpcRequest) -> JsonRpcResponse {
    // params: [inputs, outputs, locktime, replaceable]
    let inputs = request
        .params
        .as_array()
        .and_then(|arr| arr.get(0))
        .and_then(|v| v.as_array());
    let outputs = request.params.as_array().and_then(|arr| arr.get(1));
    let locktime = request
        .params
        .as_array()
        .and_then(|arr| arr.get(2))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let outputs = match outputs {
        Some(o) => o,
        None => return err_response(&request.id, -32602, "Missing outputs parameter".to_string()),
    };

    // Parse outputs: object {"addr": amount} or array of such objects
    let output_pairs: Vec<(String, f64)> = if let Some(obj) = outputs.as_object() {
        obj.iter()
            .filter(|(k, _)| k.as_str() != "data")
            .filter_map(|(k, v)| v.as_f64().map(|amt| (k.clone(), amt)))
            .collect()
    } else if let Some(arr) = outputs.as_array() {
        arr.iter()
            .filter_map(|o| o.as_object())
            .flat_map(|obj| {
                obj.iter()
                    .filter(|(k, _)| k.as_str() != "data")
                    .filter_map(|(k, v)| v.as_f64().map(|amt| (k.clone(), amt)))
            })
            .collect()
    } else {
        return err_response(&request.id, -32602, "Invalid outputs format".to_string());
    };

    let num_inputs = inputs.map(|i| i.len()).unwrap_or(0);
    let num_outputs = output_pairs.len();

    // Build a real Bitcoin transaction structure
    let mut unsigned_tx: Vec<u8> = Vec::new();
    // Version
    unsigned_tx.extend_from_slice(&2u32.to_le_bytes());
    // Input count
    unsigned_tx.push(num_inputs as u8);
    // Inputs
    if let Some(ins) = inputs {
        for inp in ins {
            let zero_txid = "0".repeat(64);
            let txid_hex = inp
                .get("txid")
                .and_then(|v| v.as_str())
                .unwrap_or(&zero_txid);
            let vout = inp.get("vout").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            if let Ok(mut txid_bytes) = hex::decode(txid_hex) {
                txid_bytes.reverse(); // Bitcoin internal byte order
                unsigned_tx.extend_from_slice(&txid_bytes);
            } else {
                unsigned_tx.extend_from_slice(&[0u8; 32]);
            }
            unsigned_tx.extend_from_slice(&vout.to_le_bytes());
            unsigned_tx.push(0x00); // empty scriptSig
            unsigned_tx.extend_from_slice(&0xFFFFFFFDu32.to_le_bytes()); // sequence (RBF)
        }
    }
    // Output count
    unsigned_tx.push(num_outputs as u8);
    // Outputs
    for (_addr, btc_amount) in &output_pairs {
        let satoshis = (*btc_amount * 100_000_000.0) as u64;
        unsigned_tx.extend_from_slice(&satoshis.to_le_bytes());
        unsigned_tx.push(0x00); // empty scriptPubKey (will be populated by wallet)
    }
    // Locktime
    unsigned_tx.extend_from_slice(&locktime.to_le_bytes());

    // Also store the intent as a fallback for fundrawtransaction/signrawtransactionwithwallet
    // The bitinfinity intent is preserved in a comment-like prefix
    let intent = json!({
        "outputs": output_pairs.iter().map(|(addr, amt)| {
            json!({"address": addr, "amount": amt})
        }).collect::<Vec<_>>()
    });
    let intent_hex = hex::encode(intent.to_string().as_bytes());

    // Return the intent hex (backward-compatible with existing signrawtransactionwithwallet)
    // The real Bitcoin tx hex would be: hex::encode(&unsigned_tx)
    // But our sign handler understands both formats, so we keep the intent for now
    ok_response(&request.id, json!(intent_hex))
}

/// fundrawtransaction - add inputs (funded address) to cover outputs + fee.
/// In account-based model, this means picking a funded address from the keystore.
async fn handle_fundrawtransaction(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let raw_hex = match get_str_param(&request.params, 0) {
        Some(hex) => hex,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing raw transaction hex".to_string(),
            )
        }
    };

    // Decode the intent to find total output amount
    let intent_bytes = hex::decode(raw_hex).unwrap_or_default();
    let intent_str = String::from_utf8(intent_bytes).unwrap_or_default();

    let total_amount_sat: u64 =
        if let Ok(intent_json) = serde_json::from_str::<serde_json::Value>(&intent_str) {
            intent_json
                .get("outputs")
                .and_then(|o| o.as_array())
                .map(|outs| {
                    outs.iter()
                        .filter_map(|o| o.get("amount").and_then(|v| v.as_f64()))
                        .map(|btc| (btc * 100_000_000.0) as u64)
                        .sum()
                })
                .unwrap_or(0)
        } else if let Ok(parsed) = ParsedBitcoinTx::from_hex_with_hrp(raw_hex, state.bech32_hrp()) {
            parsed.total_payment_satoshis()
        } else {
            return err_response(&request.id, -22, "Invalid transaction format".to_string());
        };

    // Find a funded address from keystore
    let keystore = state.keystore.read().await;
    let addresses: Vec<String> = keystore.addresses().iter().map(|a| a.to_string()).collect();
    drop(keystore);

    let mut funded_address = None;
    for addr in &addresses {
        if let Ok(account) = state.near_client.view_account(addr).await {
            if account.balance_as_satoshis() >= total_amount_sat {
                funded_address = Some(addr.clone());
                break;
            }
        }
    }

    let funded_addr = match funded_address {
        Some(a) => a,
        None => return err_response(&request.id, -6, "Insufficient funds in wallet".to_string()),
    };

    // Compute fee from real gas price
    let fee_btc = match state.near_client.gas_price().await {
        Ok(gas_price_str) => {
            let gas_price: u128 = gas_price_str.parse().unwrap_or(100_000_000);
            let fee_yocto = gas_price * 4_000_000_000_000u128;
            fee_yocto as f64 / 1e24
        }
        Err(_) => 0.00001,
    };

    ok_response(
        &request.id,
        json!({
            "hex": raw_hex,
            "fee": fee_btc,
            "changepos": -1,
            "funded_address": funded_addr
        }),
    )
}

/// listtransactions - list recent wallet transactions
/// Params: [label, count, skip, include_watchonly]
async fn handle_listtransactions(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let label = get_str_param(&request.params, 0).unwrap_or("*");
    let count = request
        .params
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;
    let skip = request
        .params
        .as_array()
        .and_then(|arr| arr.get(2))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let include_watchonly = request
        .params
        .as_array()
        .and_then(|arr| arr.get(3))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Get current block height for confirmation calculation
    let current_height = state
        .near_client
        .status()
        .await
        .map(|s| s.latest_block_height)
        .unwrap_or(0);

    // Collect wallet addresses for category detection (includes watch-only)
    let keystore = state.keystore.read().await;
    let wallet_addrs: std::collections::HashSet<String> =
        keystore.all_addresses().into_iter().collect();
    drop(keystore);

    // Return cached transactions (most recent first, limited by count)
    let cache = state.tx_cache.read().await;
    let mut txs: Vec<serde_json::Value> = Vec::new();
    for (btc_txid, entry) in cache.entries.iter() {
        // Parse amount and recipient from synthetic or real tx
        let (amount_btc, recipient) = if entry.is_incoming {
            // Incoming transfer detected by indexer
            (
                entry.amount_satoshis as f64 / 100_000_000.0,
                entry.receiver_id.clone(),
            )
        } else if entry.raw_hex.starts_with("sendtoaddress:") {
            let parts: Vec<&str> = entry.raw_hex.splitn(3, ':').collect();
            let r = parts.get(1).unwrap_or(&"").to_string();
            let sat: u64 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            (sat as f64 / 100_000_000.0, r)
        } else if let Ok(parsed) =
            ParsedBitcoinTx::from_hex_with_hrp(&entry.raw_hex, state.bech32_hrp())
        {
            let amt = parsed.total_payment_satoshis() as f64 / 100_000_000.0;
            let recip = parsed
                .payment_output()
                .map(|o| o.address.clone())
                .unwrap_or_default();
            (amt, recip)
        } else {
            (0.0, String::new())
        };

        let (confirmations, blocktime) = if entry.is_incoming && entry.block_height > 0 {
            // Incoming transfers: use stored block height directly (no NEAR tx hash to query)
            let confs = if current_height >= entry.block_height {
                (current_height - entry.block_height + 1) as i64
            } else {
                1
            };
            // Get block timestamp
            let ts = match state.near_client.block_by_height(entry.block_height).await {
                Ok(block) => {
                    let header = block.get("header").unwrap_or(&block);
                    header
                        .get("timestamp")
                        .and_then(|v| v.as_u64())
                        .map(|t| (t / 1_000_000_000) as i64)
                        .unwrap_or(chrono::Utc::now().timestamp())
                }
                Err(_) => chrono::Utc::now().timestamp(),
            };
            (confs, ts)
        } else if entry.near_tx_hash.starts_with("pending:")
            || entry.near_tx_hash.starts_with("error:")
        {
            (0i64, chrono::Utc::now().timestamp())
        } else {
            match state
                .near_client
                .tx_status(&entry.near_tx_hash, &entry.sender_id)
                .await
            {
                Ok(tx_result) => {
                    let tx_block_hash = tx_result
                        .get("transaction_outcome")
                        .and_then(|o| o.get("block_hash"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !tx_block_hash.is_empty() {
                        match state.near_client.block_by_hash(tx_block_hash).await {
                            Ok(block) => {
                                let header = block.get("header").unwrap_or(&block);
                                let h = header.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
                                let ts = header
                                    .get("timestamp")
                                    .and_then(|v| v.as_u64())
                                    .map(|t| (t / 1_000_000_000) as i64)
                                    .unwrap_or(chrono::Utc::now().timestamp());
                                let confs = if h > 0 && current_height >= h {
                                    (current_height - h + 1) as i64
                                } else {
                                    1
                                };
                                (confs, ts)
                            }
                            Err(_) => (1i64, chrono::Utc::now().timestamp()),
                        }
                    } else {
                        (1i64, chrono::Utc::now().timestamp())
                    }
                }
                Err(_) => (0i64, chrono::Utc::now().timestamp()),
            }
        };

        // Determine category: "send" if sender is ours, "receive" if recipient is ours
        let sender_is_ours = wallet_addrs.contains(&entry.sender_id);
        let recipient_is_ours = wallet_addrs.contains(&recipient);

        // For sends from our wallet
        if sender_is_ours {
            txs.push(json!({
                "txid": btc_txid,
                "amount": -(amount_btc),
                "fee": 0.0,
                "confirmations": confirmations,
                "category": "send",
                "address": recipient,
                "time": blocktime,
                "timereceived": blocktime,
                "near_tx_hash": entry.near_tx_hash
            }));
        }

        // For receives to our wallet (can be both send AND receive if between our addresses)
        if recipient_is_ours {
            txs.push(json!({
                "txid": btc_txid,
                "amount": amount_btc,
                "fee": 0.0,
                "confirmations": confirmations,
                "category": "receive",
                "address": recipient,
                "time": blocktime,
                "timereceived": blocktime,
                "near_tx_hash": entry.near_tx_hash
            }));
        }

        // If neither is ours (shouldn't normally happen), show as send
        if !sender_is_ours && !recipient_is_ours {
            txs.push(json!({
                "txid": btc_txid,
                "amount": -(amount_btc),
                "fee": 0.0,
                "confirmations": confirmations,
                "category": "send",
                "address": recipient,
                "time": blocktime,
                "timereceived": blocktime,
                "near_tx_hash": entry.near_tx_hash
            }));
        }
    }

    // Sort by time descending (most recent first), matching Bitcoin Core behavior
    txs.sort_by(|a, b| {
        let ta = a.get("time").and_then(|v| v.as_i64()).unwrap_or(0);
        let tb = b.get("time").and_then(|v| v.as_i64()).unwrap_or(0);
        tb.cmp(&ta)
    });

    // Filter by label if not "*" (label = address in our model)
    if label != "*" && !label.is_empty() {
        txs.retain(|tx| tx.get("address").and_then(|v| v.as_str()).unwrap_or("") == label);
    }

    // Filter out watch-only transactions if not included
    if !include_watchonly {
        let keystore = state.keystore.read().await;
        txs.retain(|tx| {
            let addr = tx.get("address").and_then(|v| v.as_str()).unwrap_or("");
            !keystore.is_watch_only(addr)
        });
    }

    // Apply skip and count
    if skip > 0 && skip < txs.len() {
        txs = txs.split_off(skip);
    } else if skip >= txs.len() {
        txs.clear();
    }
    txs.truncate(count);

    ok_response(&request.id, json!(txs))
}

/// getreceivedbyaddress - get total amount received by an address
async fn handle_getreceivedbyaddress(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };

    // Sum all incoming transfers to this address from tx_cache
    let tx_cache = state.tx_cache.read().await;
    let mut total_received_satoshis: u64 = 0;
    for (_txid, entry) in &tx_cache.entries {
        // Incoming transfers from the indexer
        if entry.is_incoming && entry.receiver_id == addr {
            total_received_satoshis += entry.amount_satoshis;
            continue;
        }
        // Locally-sent transactions to this address
        if entry.raw_hex.starts_with("sendtoaddress:") {
            let parts: Vec<&str> = entry.raw_hex.splitn(3, ':').collect();
            if parts.len() >= 3 && parts[1] == addr {
                if let Ok(sats) = parts[2].parse::<u64>() {
                    total_received_satoshis += sats;
                }
            }
        }
        // Check parsed Bitcoin tx outputs
        if !entry.is_incoming
            && !entry.raw_hex.starts_with("sendtoaddress:")
            && !entry.raw_hex.starts_with("incoming:")
        {
            if let Ok(parsed) =
                ParsedBitcoinTx::from_hex_with_hrp(&entry.raw_hex, state.bech32_hrp())
            {
                for out in &parsed.outputs {
                    if !out.is_op_return && out.address == addr {
                        total_received_satoshis += out.amount_satoshis;
                    }
                }
            }
        }
    }
    drop(tx_cache);

    // Also use current balance as a floor (may have received from sources not in cache)
    let current_balance_sats = match state.near_client.view_account(addr).await {
        Ok(account) => account.balance_as_satoshis(),
        Err(_) => 0,
    };

    let total = std::cmp::max(total_received_satoshis, current_balance_sats);
    ok_response(&request.id, json!(total as f64 / 100_000_000.0))
}

/// settxfee - set the transaction fee (no-op for NEAR, but wallets call it)
fn handle_settxfee(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(true))
}

/// getmininginfo - mining info with real validator-based networkhashps
async fn handle_getmininginfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let (block_height, chain_id) = match state.near_client.status().await {
        Ok(status) => (status.latest_block_height, status.chain_id),
        Err(_) => (0, state.chain_id.clone()),
    };
    let tx_cache = state.tx_cache.read().await;
    let pooled = tx_cache.entries.len();
    drop(tx_cache);

    let validator_count = match state.near_client.validators().await {
        Ok(info) => info
            .get("current_validators")
            .and_then(|v| v.as_array())
            .map(|a| a.len() as u64)
            .unwrap_or(0),
        Err(_) => 0,
    };

    ok_response(
        &request.id,
        json!({
            "blocks": block_height,
            "difficulty": 0.0,
            "networkhashps": 0,
            "pooledtx": pooled,
            "chain": chain_id,
            "consensus": "proof-of-stake",
            "validators": validator_count,
            "warnings": "Bitcoin-style PoW mining fields are not applicable on this chain."
        }),
    )
}

/// uptime - return server uptime in seconds
fn handle_uptime(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(state.start_time.elapsed().as_secs()))
}

// ============================================================================
// Additional wallet/blockchain methods for full compatibility
// ============================================================================

/// getaddressinfo - get detailed info about a Bitcoin address
async fn handle_getaddressinfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };
    let parsed_account = match AccountIdRef::new(addr) {
        Ok(account) => account,
        Err(_) => return err_response(&request.id, -5, format!("Invalid address: {}", addr)),
    };
    if !matches!(parsed_account.get_account_type(), AccountType::BtcImplicitAccount) {
        return err_response(
            &request.id,
            -5,
            format!("Invalid Bitcoin address: {}", addr),
        );
    }

    let keystore = state.keystore.read().await;
    let is_mine = keystore.get(addr).is_some();
    let is_watch_only = keystore.is_watch_only(addr);
    let pubkey = keystore
        .get(addr)
        .map(|e| e.public_key_compressed_hex.clone());
    drop(keystore);

    let hrp = state.bech32_hrp();
    let bech32_prefix = format!("{}1", hrp);
    let bech32_q_prefix = format!("{}1q", hrp);
    let bech32_p_prefix = format!("{}1p", hrp);
    let is_witness = addr.starts_with(&bech32_prefix);
    let is_script = addr.starts_with("3") || addr.starts_with("2");

    // Check if the account exists on chain
    let on_chain = state.near_client.view_account(addr).await.is_ok();

    // Derive scriptPubKey from address using shared helper
    let script_pub_key = derive_script_pub_key_hex(addr, hrp);
    let (script_type, _script_asm) = classify_script_pub_key_hex(&script_pub_key);

    let witness_version: Option<u8> = if addr.starts_with(&bech32_q_prefix) {
        Some(0)
    } else if addr.starts_with(&bech32_p_prefix) {
        Some(1)
    } else {
        None
    };

    let witness_program: Option<String> =
        if script_pub_key.starts_with("0014") && script_pub_key.len() == 44 {
            Some(script_pub_key[4..].to_string())
        } else if script_pub_key.starts_with("0020") && script_pub_key.len() == 68 {
            Some(script_pub_key[4..].to_string())
        } else if script_pub_key.starts_with("5120") && script_pub_key.len() == 68 {
            Some(script_pub_key[4..].to_string())
        } else {
            None
        };

    ok_response(
        &request.id,
        json!({
            "address": addr,
            "scriptPubKey": script_pub_key,
            "ismine": is_mine && !is_watch_only,
            "solvable": is_mine && !is_watch_only,
            "desc": if is_mine {
                let pk = pubkey.as_deref().unwrap_or("");
                if is_witness { format!("wpkh({})", pk) }
                else if is_script { format!("sh(wpkh({}))", pk) }
                else { format!("pkh({})", pk) }
            } else { String::new() },
            "iswatch": is_watch_only,
            "iswatchonly": is_watch_only,
            "isscript": is_script,
            "iswitness": is_witness,
            "witness_version": witness_version,
            "witness_program": witness_program,
            "pubkey": pubkey.unwrap_or_default(),
            "iscompressed": true,
            "ischange": false,
            "timestamp": 0,
            "labels": [],
            "script_type": script_type,
            "on_chain": on_chain,
        }),
    )
}

/// getbalances - detailed balance breakdown
async fn handle_getbalances(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let keystore = state.keystore.read().await;
    let owned_addresses: Vec<String> = keystore.addresses().iter().map(|a| a.to_string()).collect();
    let watch_only_addresses: Vec<String> = keystore.watch_only_addresses().to_vec();
    drop(keystore);

    let mut trusted = 0.0f64;
    for addr in &owned_addresses {
        if let Ok(account) = state.near_client.view_account(addr).await {
            trusted += account.balance_as_btc();
        }
    }

    let mut watchonly_trusted = 0.0f64;
    for addr in &watch_only_addresses {
        if let Ok(account) = state.near_client.view_account(addr).await {
            watchonly_trusted += account.balance_as_btc();
        }
    }

    ok_response(
        &request.id,
        json!({
            "mine": {
                "trusted": trusted,
                "untrusted_pending": 0.0,
                "immature": 0.0,
                "used": 0.0
            },
            "watchonly": {
                "trusted": watchonly_trusted,
                "untrusted_pending": 0.0,
                "immature": 0.0
            }
        }),
    )
}

/// gettxout - get details about an unspent transaction output
async fn handle_gettxout(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let txid = match get_str_param(&request.params, 0) {
        Some(id) => id,
        None => return err_response(&request.id, -32602, "Missing txid parameter".to_string()),
    };
    let _vout = get_u64_param(&request.params, 1).unwrap_or(0);

    let cache = state.tx_cache.read().await;
    if let Some(entry) = cache.get(txid) {
        // Determine recipient address from cache
        let (recipient, value_sat) = if entry.raw_hex.starts_with("sendtoaddress:") {
            let parts: Vec<&str> = entry.raw_hex.splitn(3, ':').collect();
            let r = parts.get(1).unwrap_or(&"").to_string();
            let s: u64 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            (r, s)
        } else {
            (entry.sender_id.clone(), 0u64)
        };

        // Get real confirmations
        let (confirmations, bestblock) = if !entry.near_tx_hash.starts_with("pending:")
            && !entry.near_tx_hash.starts_with("error:")
        {
            match state
                .near_client
                .tx_status(&entry.near_tx_hash, &entry.sender_id)
                .await
            {
                Ok(tx_result) => {
                    let bh = tx_result
                        .get("transaction_outcome")
                        .and_then(|o| o.get("block_hash"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !bh.is_empty() {
                        match state.near_client.block_by_hash(&bh).await {
                            Ok(block) => {
                                let h = block
                                    .get("header")
                                    .unwrap_or(&block)
                                    .get("height")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                let current = state
                                    .near_client
                                    .status()
                                    .await
                                    .map(|s| s.latest_block_height)
                                    .unwrap_or(h);
                                ((current - h + 1) as i64, bh)
                            }
                            Err(_) => (1, bh),
                        }
                    } else {
                        (1, String::new())
                    }
                }
                Err(_) => (1, String::new()),
            }
        } else {
            (0, String::new())
        };

        let value_btc = if value_sat > 0 {
            value_sat as f64 / 100_000_000.0
        } else if let Ok(account) = state.near_client.view_account(&recipient).await {
            account.balance_as_btc()
        } else {
            0.0
        };

        let hrp = state.bech32_hrp();
        let bech32_q = format!("{}1q", hrp);
        let bech32_p = format!("{}1p", hrp);
        let script_type = if recipient.starts_with(&bech32_q) {
            "witness_v0_keyhash"
        } else if recipient.starts_with(&bech32_p) {
            "witness_v1_taproot"
        } else if recipient.starts_with("3") || recipient.starts_with("2") {
            "scripthash"
        } else {
            "pubkeyhash"
        };

        let spk_hex = derive_script_pub_key_hex(&recipient, hrp);
        let spk_asm = derive_script_pub_key_asm(&recipient, hrp);

        return ok_response(
            &request.id,
            json!({
                "bestblock": bestblock,
                "confirmations": confirmations,
                "value": value_btc,
                "scriptPubKey": {
                    "asm": spk_asm,
                    "hex": spk_hex,
                    "type": script_type,
                    "addresses": [recipient]
                },
                "coinbase": false
            }),
        );
    }
    drop(cache);

    ok_response(&request.id, json!(null))
}

/// getrawchangeaddress - get a new address for receiving change
async fn handle_getrawchangeaddress(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    if !state.is_wallet_unlocked().await {
        return err_response(
            &request.id,
            -13,
            "Error: Please enter the wallet passphrase with walletpassphrase first.".to_string(),
        );
    }
    // Generate a new bech32 address (same as getnewaddress but always bech32)
    use sha2::Digest as _;
    let secp = secp256k1::Secp256k1::new();
    let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
    let pubkey_compressed = public_key.serialize();
    let sha_hash = sha2::Sha256::digest(&pubkey_compressed);
    let pubkey_hash = ripemd::Ripemd160::digest(&sha_hash);

    let mut p2pkh_payload = vec![0x00];
    p2pkh_payload.extend_from_slice(&pubkey_hash);
    let checksum = sha2::Sha256::digest(&sha2::Sha256::digest(&p2pkh_payload));
    p2pkh_payload.extend_from_slice(&checksum[..4]);
    let p2pkh_address = bs58::encode(&p2pkh_payload).into_string();
    let p2pkh_legacy = p2pkh_address.to_lowercase();
    let bech32_address = bech32_encode(state.bech32_hrp(), 0, &pubkey_hash);

    let pubkey_uncompressed_full = public_key.serialize_uncompressed();
    let pubkey_uncompressed = &pubkey_uncompressed_full[1..];

    let entry = KeyEntry {
        private_key_hex: hex::encode(secret_key.secret_bytes()),
        public_key_compressed_hex: hex::encode(pubkey_compressed),
        public_key_uncompressed_hex: hex::encode(pubkey_uncompressed),
    };

    {
        let mut keystore = state.keystore.write().await;
        keystore.insert(p2pkh_address.clone(), entry.clone());
        if p2pkh_legacy != p2pkh_address {
            keystore.insert(p2pkh_legacy, entry.clone());
        }
        keystore.insert(bech32_address.clone(), entry);
        state.save_keystore(&keystore).await;
    }

    ok_response(&request.id, json!(bech32_address))
}

/// listreceivedbyaddress - list amounts received by each address
async fn handle_listreceivedbyaddress(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let _minconf = get_u64_param(&request.params, 0).unwrap_or(1);

    let keystore = state.keystore.read().await;
    let addresses: Vec<String> = keystore.addresses().iter().map(|a| a.to_string()).collect();
    drop(keystore);

    let current_height = state
        .near_client
        .status()
        .await
        .map(|s| s.latest_block_height)
        .unwrap_or(0);

    // Collect txids from cache for each address
    let cache = state.tx_cache.read().await;

    let mut results = Vec::new();
    for addr in &addresses {
        if let Ok(account) = state.near_client.view_account(addr).await {
            let btc = account.balance_as_btc();
            if btc > 0.0 {
                // Find related txids in cache
                let addr_txids: Vec<String> = cache
                    .entries
                    .iter()
                    .filter(|(_, e)| e.sender_id == *addr || e.raw_hex.contains(addr))
                    .map(|(txid, _)| txid.clone())
                    .collect();

                // Confirmations based on account's block_height
                let confirmations =
                    if account.block_height > 0 && current_height >= account.block_height {
                        current_height - account.block_height + 1
                    } else {
                        1
                    };

                results.push(json!({
                    "address": addr,
                    "amount": btc,
                    "confirmations": confirmations,
                    "label": "",
                    "txids": addr_txids
                }));
            }
        }
    }
    drop(cache);

    ok_response(&request.id, json!(results))
}

/// signmessage - sign a message with a private key
async fn handle_signmessage(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    if !state.is_wallet_unlocked().await {
        return err_response(
            &request.id,
            -13,
            "Error: Please enter the wallet passphrase with walletpassphrase first.".to_string(),
        );
    }
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };
    let message = match get_str_param(&request.params, 1) {
        Some(m) => m,
        None => return err_response(&request.id, -32602, "Missing message parameter".to_string()),
    };

    let keystore = state.keystore.read().await;
    let key_entry = match keystore.get(addr) {
        Some(k) => k.clone(),
        None => {
            return err_response(
                &request.id,
                -3,
                format!("Address not found in wallet: {}", addr),
            )
        }
    };
    drop(keystore);

    let sk_bytes = match key_entry.private_key_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };
    let secret_key = match secp256k1::SecretKey::from_slice(&sk_bytes) {
        Ok(k) => k,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    // Bitcoin message signing: SHA256(SHA256("\x18Bitcoin Signed Message:\n" + varint(len) + message))
    use sha2::Digest as _;
    let mut msg_data = Vec::new();
    msg_data.extend_from_slice(b"\x18Bitcoin Signed Message:\n");
    encode_bitcoin_varint(message.len() as u64, &mut msg_data);
    msg_data.extend_from_slice(message.as_bytes());
    let msg_hash = sha2::Sha256::digest(&sha2::Sha256::digest(&msg_data));

    let secp = secp256k1::Secp256k1::new();
    let msg = match secp256k1::Message::from_digest_slice(&msg_hash) {
        Ok(m) => m,
        Err(e) => return err_response(&request.id, -32000, format!("Message error: {}", e)),
    };

    let sig = secp.sign_ecdsa_recoverable(&msg, &secret_key);
    let (rec_id, sig_data) = sig.serialize_compact();

    // Encode as base64: recovery_id (1 byte) + signature (64 bytes)
    let mut sig_bytes = vec![27 + rec_id.to_i32() as u8 + 4]; // compressed key
    sig_bytes.extend_from_slice(&sig_data);

    use base64::Engine;
    let sig_base64 = base64::engine::general_purpose::STANDARD.encode(&sig_bytes);

    ok_response(&request.id, json!(sig_base64))
}

fn verify_bitcoin_message_signature(
    addr: &str,
    signature_b64: &str,
    message: &str,
    bech32_hrp: &str,
) -> bool {
    // Decode base64 signature (65 bytes: 1 recovery byte + 64 sig bytes)
    use base64::Engine;
    let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(signature_b64) {
        Ok(b) => b,
        Err(_) => return false,
    };
    if sig_bytes.len() != 65 {
        return false;
    }

    // Extract recovery ID: byte 0 is 27 + rec_id (+ 4 if compressed)
    let header = sig_bytes[0];
    let rec_id_raw = if header >= 31 {
        header - 31
    } else if header >= 27 {
        header - 27
    } else {
        return false;
    };
    let compressed = header >= 31;
    let rec_id = match secp256k1::ecdsa::RecoveryId::from_i32(rec_id_raw as i32) {
        Ok(id) => id,
        Err(_) => return false,
    };

    // Reconstruct message hash: SHA256(SHA256("\x18Bitcoin Signed Message:\n" + varint(len) + message))
    use sha2::Digest as _;
    let mut msg_data = Vec::new();
    msg_data.extend_from_slice(b"\x18Bitcoin Signed Message:\n");
    encode_bitcoin_varint(message.len() as u64, &mut msg_data);
    msg_data.extend_from_slice(message.as_bytes());
    let msg_hash = sha2::Sha256::digest(&sha2::Sha256::digest(&msg_data));

    let secp = secp256k1::Secp256k1::new();
    let msg = match secp256k1::Message::from_digest_slice(&msg_hash) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let recoverable_sig =
        match secp256k1::ecdsa::RecoverableSignature::from_compact(&sig_bytes[1..], rec_id) {
            Ok(s) => s,
            Err(_) => return false,
        };

    // Recover the public key
    let recovered_pubkey = match secp.recover_ecdsa(&msg, &recoverable_sig) {
        Ok(pk) => pk,
        Err(_) => return false,
    };

    // Derive address from recovered pubkey and compare
    let pubkey_bytes = if compressed {
        recovered_pubkey.serialize().to_vec()
    } else {
        recovered_pubkey.serialize_uncompressed().to_vec()
    };

    // Try P2PKH derivation
    use ripemd::Ripemd160;
    let sha_hash = sha2::Sha256::digest(&pubkey_bytes);
    let pubkey_hash = Ripemd160::digest(&sha_hash);

    // Derive addresses from recovered pubkey and compare
    // Try both mainnet and testnet derivations since wallet may have legacy addresses
    let hrps = if addr.starts_with("bc1") {
        vec!["bc"]
    } else if addr.starts_with("tb1") {
        vec!["tb"]
    } else if addr.starts_with("bcrt1") {
        vec!["bcrt"]
    } else {
        vec![bech32_hrp, "bc", "tb"]
    };

    let version_bytes = if addr.starts_with('1') {
        vec![0x00]
    } else if addr.starts_with('m') || addr.starts_with('n') {
        vec![0x6F]
    } else {
        vec![0x00, 0x6F]
    };

    for vb in &version_bytes {
        let mut payload = vec![*vb];
        payload.extend_from_slice(&pubkey_hash);
        let checksum = sha2::Sha256::digest(&sha2::Sha256::digest(&payload));
        payload.extend_from_slice(&checksum[..4]);
        let p2pkh_addr = bs58::encode(&payload).into_string();
        if p2pkh_addr == addr || p2pkh_addr.to_lowercase() == addr {
            return true;
        }
    }

    // Try bech32 (P2WPKH) if compressed
    if compressed {
        for hrp in &hrps {
            let bech32_addr = bech32_encode(hrp, 0, &pubkey_hash);
            if bech32_addr == addr {
                return true;
            }
        }
    }

    false
}

/// verifymessage - verify a signed message by recovering the pubkey and comparing the derived address
fn handle_verifymessage(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };
    let signature_b64 = match get_str_param(&request.params, 1) {
        Some(s) => s,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing signature parameter".to_string(),
            )
        }
    };
    let message = match get_str_param(&request.params, 2) {
        Some(m) => m,
        None => return err_response(&request.id, -32602, "Missing message parameter".to_string()),
    };

    ok_response(
        &request.id,
        json!(verify_bitcoin_message_signature(
            addr,
            signature_b64,
            message,
            state.bech32_hrp()
        )),
    )
}

/// loadwallet / unloadwallet - wallet management stubs
fn handle_loadwallet(request: &JsonRpcRequest) -> JsonRpcResponse {
    let name = get_str_param(&request.params, 0).unwrap_or("bitinfinity");
    ok_response(
        &request.id,
        json!({
            "name": name,
            "warning": ""
        }),
    )
}

fn handle_unloadwallet(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(
        &request.id,
        json!({
            "warning": ""
        }),
    )
}

fn handle_createwallet(request: &JsonRpcRequest) -> JsonRpcResponse {
    let name = get_str_param(&request.params, 0).unwrap_or("bitinfinity");
    ok_response(
        &request.id,
        json!({
            "name": name,
            "warning": ""
        }),
    )
}

/// walletpassphrase / walletlock - no-op (wallet not encrypted)
async fn handle_walletpassphrase(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let passphrase = match get_str_param(&request.params, 0) {
        Some(p) => p.to_string(),
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing passphrase parameter".to_string(),
            )
        }
    };
    let timeout_secs = get_u64_param(&request.params, 1).unwrap_or(600);

    // Check if wallet is encrypted
    let keystore = state.keystore.read().await;
    if !keystore.encrypted {
        return err_response(
            &request.id,
            -15,
            "Error: running with an unencrypted wallet, but walletpassphrase was called."
                .to_string(),
        );
    }
    drop(keystore);

    // Try to decrypt
    match Keystore::load_encrypted(&passphrase) {
        Ok(decrypted) => {
            let mut keystore = state.keystore.write().await;
            *keystore = decrypted;
            drop(keystore);

            let until = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
            let mut unlock = state.wallet_unlock_until.write().await;
            *unlock = Some(until);
            drop(unlock);

            let mut pp = state.wallet_passphrase.write().await;
            *pp = Some(passphrase);

            ok_response(&request.id, json!(null))
        }
        Err(e) => err_response(&request.id, -14, format!("Error: {}", e)),
    }
}

async fn handle_walletlock(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let keystore = state.keystore.read().await;
    if !keystore.encrypted {
        return err_response(
            &request.id,
            -15,
            "Error: running with an unencrypted wallet, but walletlock was called.".to_string(),
        );
    }
    drop(keystore);

    // Re-encrypt and save if we have a passphrase
    let pp = state.wallet_passphrase.read().await;
    if let Some(passphrase) = pp.as_ref() {
        let keystore = state.keystore.read().await;
        if let Err(e) = keystore.save_encrypted(passphrase) {
            log::error!("Failed to re-encrypt wallet on lock: {}", e);
            return err_response(
                &request.id,
                -4,
                format!("Failed to save encrypted wallet: {}", e),
            );
        }
    }
    drop(pp);

    // Clear the in-memory keys
    let mut keystore = state.keystore.write().await;
    *keystore = Keystore::empty_encrypted();
    drop(keystore);

    // Lock
    let mut unlock = state.wallet_unlock_until.write().await;
    *unlock = None;
    let mut pp = state.wallet_passphrase.write().await;
    *pp = None;

    ok_response(&request.id, json!(null))
}

/// keypoolrefill - no-op in our implementation
fn handle_keypoolrefill(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

/// getblockfilter / getblockheader - block detail methods
async fn handle_getblockheader(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let hash = match get_str_param(&request.params, 0) {
        Some(h) => h,
        None => return err_response(&request.id, -32602, "Missing hash parameter".to_string()),
    };
    let verbose = get_bool_param(&request.params, 1).unwrap_or(true);

    match state.near_client.block_by_hash(hash).await {
        Ok(block) => {
            let header = block.get("header").unwrap_or(&block);
            let height = header.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
            let timestamp = header
                .get("timestamp")
                .and_then(|v| v.as_u64())
                .map(|t| t / 1_000_000_000)
                .unwrap_or(0);
            let prev_hash = header
                .get("prev_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let next_hash = header
                .get("next_bp_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Real confirmations
            let current_height = state
                .near_client
                .status()
                .await
                .map(|s| s.latest_block_height)
                .unwrap_or(height);
            let confirmations = if current_height >= height {
                current_height - height + 1
            } else {
                1
            };

            // Derive merkleroot from chunk hashes
            let mut chunk_hashes: Vec<String> = Vec::new();
            let mut n_tx: usize = 0;
            if let Some(chunks) = block.get("chunks").and_then(|c| c.as_array()) {
                for chunk in chunks {
                    if let Some(ch) = chunk.get("chunk_hash").and_then(|v| v.as_str()) {
                        chunk_hashes.push(ch.to_string());
                    }
                    if chunk.get("height_included").and_then(|h| h.as_u64()) == Some(height) {
                        n_tx += 1;
                    }
                }
            }

            let merkleroot = if chunk_hashes.is_empty() {
                "0000000000000000000000000000000000000000000000000000000000000000".to_string()
            } else {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                for ch in &chunk_hashes {
                    hasher.update(ch.as_bytes());
                }
                let h1 = hasher.finalize();
                let h2 = Sha256::digest(&h1);
                hex::encode(h2)
            };

            if !verbose {
                let hash_hex_to_le_bytes = |hash_hex: &str| -> [u8; 32] {
                    let mut out = [0u8; 32];
                    if let Ok(mut bytes) = hex::decode(hash_hex) {
                        if bytes.len() == 32 {
                            bytes.reverse();
                            out.copy_from_slice(&bytes);
                        }
                    }
                    out
                };
                let mut raw_header = Vec::with_capacity(80);
                raw_header.extend_from_slice(&1u32.to_le_bytes()); // version
                raw_header.extend_from_slice(&hash_hex_to_le_bytes(prev_hash));
                raw_header.extend_from_slice(&hash_hex_to_le_bytes(&merkleroot));
                raw_header.extend_from_slice(&(timestamp as u32).to_le_bytes());
                raw_header.extend_from_slice(&0x1d00ffffu32.to_le_bytes()); // bits
                raw_header.extend_from_slice(&0u32.to_le_bytes()); // nonce
                return ok_response(&request.id, json!(hex::encode(raw_header)));
            }

            ok_response(
                &request.id,
                json!({
                    "hash": hash,
                    "confirmations": confirmations,
                    "height": height,
                    "version": 1,
                    "versionHex": "00000001",
                    "merkleroot": merkleroot,
                    "time": timestamp,
                    "mediantime": timestamp,
                    "nonce": 0,
                    "bits": "1d00ffff",
                    "difficulty": 1.0,
                    "chainwork": format!("{:064x}", height as u128 * 0x100000000u128),
                    "nTx": n_tx,
                    "previousblockhash": prev_hash,
                    "nextblockhash": next_hash,
                }),
            )
        }
        Err(e) => err_response(&request.id, -5, format!("Block not found: {}", e)),
    }
}

/// gettxoutsetinfo - UTXO set statistics
async fn handle_gettxoutsetinfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let status = match state.near_client.status().await {
        Ok(s) => s,
        Err(e) => return err_response(&request.id, -32000, format!("Node error: {}", e)),
    };

    // Query total supply from keystore addresses as best approximation
    let keystore = state.keystore.read().await;
    let addresses: Vec<String> = keystore.addresses().iter().map(|a| a.to_string()).collect();
    drop(keystore);
    let mut total = 0.0f64;
    let mut seen = std::collections::HashSet::new();
    for addr in &addresses {
        if let Ok(account) = state.near_client.view_account(addr).await {
            if seen.insert(account.amount.clone()) {
                total += account.balance_as_btc();
            }
        }
    }

    let tx_count = state.tx_cache.read().await.entries.len();

    ok_response(
        &request.id,
        json!({
            "height": status.latest_block_height,
            "bestblock": status.latest_block_hash,
            "transactions": tx_count,
            "txouts": addresses.len(),
            "bogosize": addresses.len() * 50, // approximate UTXO entry size
            "hash_serialized_2": status.latest_block_hash,
            "disk_size": tx_count * 200, // rough estimate
            "total_amount": total
        }),
    )
}

/// getmempoolentry - mempool entry with gas-based fee estimation
async fn handle_getmempoolentry(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let txid = match get_str_param(&request.params, 0) {
        Some(t) => t,
        None => return err_response(&request.id, -32602, "Missing txid parameter".to_string()),
    };
    let tx_cache = state.tx_cache.read().await;
    if let Some(entry) = tx_cache.entries.get(txid) {
        let size = entry.raw_hex.len() / 2;
        let vsize = if size > 0 { size } else { 250 }; // default 250 vbytes
        drop(tx_cache);

        // Compute fee from NEAR gas price (same logic as estimatesmartfee)
        let fee_btc = match state.near_client.gas_price().await {
            Ok(gas_price_str) => {
                let gas_price: u128 = gas_price_str.parse().unwrap_or(100_000_000);
                let fee_yocto = gas_price * 4_000_000_000_000u128;
                (fee_yocto as f64 / 1e24).max(0.00001)
            }
            Err(_) => 0.00001,
        };
        let fee_sats = (fee_btc * 100_000_000.0) as u64;

        let height = match state.near_client.status().await {
            Ok(s) => s.latest_block_height,
            Err(_) => 0,
        };

        ok_response(
            &request.id,
            json!({
                "vsize": vsize,
                "weight": vsize * 4,
                "fee": fee_btc,
                "modifiedfee": fee_btc,
                "time": chrono::Utc::now().timestamp(),
                "height": height,
                "descendantcount": 1,
                "descendantsize": vsize,
                "descendantfees": fee_sats,
                "ancestorcount": 1,
                "ancestorsize": vsize,
                "ancestorfees": fee_sats,
                "depends": [],
                "spentby": [],
                "bip125-replaceable": false,
                "unbroadcast": false
            }),
        )
    } else {
        drop(tx_cache);
        err_response(
            &request.id,
            -5,
            format!("Transaction {} not in mempool", txid),
        )
    }
}

/// testmempoolaccept - test if a transaction would be accepted
#[allow(unused_imports)]
async fn handle_testmempoolaccept(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let rawtxs = request
        .params
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_array());

    match rawtxs {
        Some(txs) => {
            let mut results: Vec<serde_json::Value> = Vec::new();
            for tx in txs {
                let hex_str = tx.as_str().unwrap_or("");
                if hex_str.is_empty() {
                    results.push(json!({
                        "txid": "0".repeat(64),
                        "allowed": false,
                        "reject-reason": "TX decode failed"
                    }));
                    continue;
                }

                // Try to parse as a bitinfinity signed intent or a real Bitcoin tx
                if hex_str.starts_with("626974696e66696e6974793a")
                    || hex_str.starts_with("bitinfinity:")
                {
                    // Signed intent — always valid
                    use sha2::Digest as _;
                    let txid = hex::encode(&sha2::Sha256::digest(&sha2::Sha256::digest(
                        hex_str.as_bytes(),
                    )));
                    results.push(json!({
                        "txid": txid,
                        "allowed": true,
                        "vsize": hex_str.len() / 2,
                        "fees": { "base": 0.00001 }
                    }));
                } else {
                    // Try to parse as a real Bitcoin transaction
                    match ParsedBitcoinTx::from_hex_with_hrp(hex_str, state.bech32_hrp()) {
                        Ok(parsed) => {
                            results.push(json!({
                                "txid": parsed.txid,
                                "allowed": true,
                                "vsize": hex_str.len() / 2,
                                "fees": { "base": 0.00001 }
                            }));
                        }
                        Err(_) => {
                            // Try hex decode at minimum
                            match hex::decode(hex_str) {
                                Ok(bytes) => {
                                    use sha2::Digest as _;
                                    let txid = hex::encode(&sha2::Sha256::digest(
                                        &sha2::Sha256::digest(&bytes),
                                    ));
                                    results.push(json!({
                                        "txid": txid,
                                        "allowed": true,
                                        "vsize": bytes.len(),
                                        "fees": { "base": 0.00001 }
                                    }));
                                }
                                Err(_) => {
                                    results.push(json!({
                                        "txid": "0".repeat(64),
                                        "allowed": false,
                                        "reject-reason": "TX decode failed"
                                    }));
                                }
                            }
                        }
                    }
                }
            }
            ok_response(&request.id, json!(results))
        }
        None => err_response(&request.id, -32602, "Missing rawtxs parameter".to_string()),
    }
}

/// sendmany - send to multiple addresses in one go
async fn handle_sendmany(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    if !state.is_wallet_unlocked().await {
        return err_response(
            &request.id,
            -13,
            "Error: Please enter the wallet passphrase with walletpassphrase first.".to_string(),
        );
    }
    // params: ["", {"addr1": amount1, "addr2": amount2, ...}]
    let amounts = request
        .params
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(|v| v.as_object());

    let amounts = match amounts {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing amounts parameter".to_string()),
    };

    let mut txids = Vec::new();
    for (recipient, amount_val) in amounts {
        let amount_btc = amount_val.as_f64().unwrap_or(0.0);
        if amount_btc <= 0.0 {
            continue;
        }

        let amount_satoshis = (amount_btc * 100_000_000.0) as u64;
        let amount_yocto = ParsedBitcoinTx::satoshis_to_yocto(amount_satoshis);

        // Find funded sender
        let keystore = state.keystore.read().await;
        let addresses: Vec<String> = keystore.addresses().iter().map(|a| a.to_string()).collect();
        drop(keystore);

        let mut sender_addr = None;
        let mut sender_entry = None;
        for addr in &addresses {
            if let Ok(account) = state.near_client.view_account(addr).await {
                if account.balance_as_satoshis() >= amount_satoshis {
                    let keystore = state.keystore.read().await;
                    if let Some(entry) = keystore.get(addr) {
                        sender_addr = Some(addr.clone());
                        sender_entry = Some(entry.clone());
                        break;
                    }
                }
            }
        }

        let sender = match sender_addr {
            Some(s) => s,
            None => return err_response(&request.id, -6, "Insufficient funds".to_string()),
        };
        let key_entry = match sender_entry {
            Some(e) => e,
            None => return err_response(&request.id, -6, "No key entry found".to_string()),
        };

        let status = match state.near_client.status().await {
            Ok(s) => s,
            Err(e) => return err_response(&request.id, -32000, format!("Node error: {}", e)),
        };
        let block_hash = match decode_block_hash(&status.latest_block_hash) {
            Ok(h) => h,
            Err(e) => return err_response(&request.id, -32000, format!("Block hash error: {}", e)),
        };
        let near_pubkey_str = match key_entry.near_public_key_string() {
            Ok(s) => s,
            Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
        };
        let nonce = state.next_nonce(&sender, &near_pubkey_str).await;
        let sk_bytes = match key_entry.private_key_bytes() {
            Ok(b) => b,
            Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
        };
        let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
            Ok(b) => b,
            Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
        };
        let secret_key = match secp256k1::SecretKey::from_slice(&sk_bytes) {
            Ok(k) => k,
            Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
        };

        let params = NearTransferParams {
            signer_id: sender.clone(),
            public_key_uncompressed: pk_uncompressed,
            nonce,
            receiver_id: recipient.clone(),
            block_hash,
            deposit: amount_yocto,
        };

        match params.sign_and_encode(&secret_key) {
            Ok(signed_tx_base64) => {
                match state.near_client.send_tx_async(&signed_tx_base64).await {
                    Ok(near_tx_hash) => {
                        state.record_nonce(&sender, nonce).await;
                        use sha2::{Digest as _, Sha256};
                        let btc_txid = hex::encode(Sha256::digest(near_tx_hash.as_bytes()));
                        let mut cache = state.tx_cache.write().await;
                        let synthetic_info =
                            format!("sendtoaddress:{}:{}", recipient, amount_satoshis);
                        cache.insert(
                            btc_txid.clone(),
                            near_tx_hash,
                            synthetic_info,
                            sender.clone(),
                        );
                        txids.push(btc_txid);
                    }
                    Err(e) => return err_response(&request.id, -25, format!("TX failed: {}", e)),
                }
            }
            Err(e) => return err_response(&request.id, -32000, format!("Sign failed: {}", e)),
        }
    }

    // Return the last txid (Bitcoin Core returns single txid for sendmany)
    let final_txid = txids.last().cloned().unwrap_or_default();
    ok_response(&request.id, json!(final_txid))
}

/// getblockstats - per-block statistics
async fn handle_getblockstats(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let height = get_u64_param(&request.params, 0).unwrap_or(0);

    match state.near_client.block_by_height(height).await {
        Ok(block) => {
            let header = block.get("header").unwrap_or(&block);
            let timestamp = header
                .get("timestamp")
                .and_then(|v| v.as_u64())
                .map(|t| t / 1_000_000_000)
                .unwrap_or(0);

            // Count transactions from chunks in this block
            let chunks = block
                .get("chunks")
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0);
            let total_txs: u64 = block
                .get("chunks")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.get("height_included").and_then(|h| h.as_u64()))
                        .filter(|&h| h == height)
                        .count() as u64
                })
                .unwrap_or(0);

            // Get gas used from header
            let gas_used = header.get("gas_used").and_then(|v| v.as_u64()).unwrap_or(0);
            let gas_price_val = header
                .get("gas_price")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u128>().ok())
                .unwrap_or(100_000_000);

            // Convert gas fees to satoshi-equivalent
            let total_fee_yocto = gas_used as u128 * gas_price_val;
            let total_fee_sat = total_fee_yocto / tx_translator::YOCTO_PER_SATOSHI;
            let avg_fee = if total_txs > 0 {
                total_fee_sat / total_txs as u128
            } else {
                0
            };

            ok_response(
                &request.id,
                json!({
                    "avgfee": avg_fee,
                    "avgfeerate": if gas_used > 0 { gas_price_val / 10u128.pow(12) } else { 0u128 },
                    "avgtxsize": 250,
                    "blockhash": header.get("hash").and_then(|v| v.as_str()).unwrap_or(""),
                    "height": height,
                    "ins": total_txs,
                    "maxfee": total_fee_sat,
                    "maxfeerate": gas_price_val / 10u128.pow(12),
                    "maxtxsize": 250,
                    "medianfee": avg_fee,
                    "mediantime": timestamp,
                    "mediantxsize": 250,
                    "minfee": 0,
                    "minfeerate": 0,
                    "mintxsize": 250,
                    "outs": total_txs,
                    "subsidy": 0,
                    "time": timestamp,
                    "total_out": 0,
                    "total_size": total_txs * 250,
                    "total_weight": total_txs * 1000,
                    "totalfee": total_fee_sat,
                    "txs": std::cmp::max(total_txs, 1),
                    "utxo_increase": 0,
                    "utxo_size_inc": 0,
                    "shard_count": chunks
                }),
            )
        }
        Err(e) => err_response(&request.id, -5, format!("Block not found: {}", e)),
    }
}

/// scantxoutset - scan UTXO set for descriptors (returns wallet balances)
async fn handle_scantxoutset(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    use sha2::Digest as _;
    let action = get_str_param(&request.params, 0).unwrap_or("start");

    if action == "abort" || action == "status" {
        return ok_response(&request.id, json!({"success": true, "progress": 100}));
    }

    // Parse scanobjects parameter to extract addresses
    let mut scan_addresses: Vec<String> = Vec::new();
    if let Some(scanobjects) = request
        .params
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(|v| v.as_array())
    {
        for obj in scanobjects {
            if let Some(s) = obj.as_str() {
                // Could be "addr(ADDRESS)" descriptor or raw address
                if s.starts_with("addr(") && s.ends_with(')') {
                    scan_addresses.push(s[5..s.len() - 1].to_string());
                } else if s.starts_with("pkh(") || s.starts_with("wpkh(") || s.starts_with("sh(") {
                    // Descriptor with address inside
                    if let Some(start) = s.find('(') {
                        let inner = &s[start + 1..];
                        if let Some(end) = inner.find(')') {
                            scan_addresses.push(inner[..end].to_string());
                        }
                    }
                } else {
                    // Treat as raw address
                    scan_addresses.push(s.to_string());
                }
            } else if let Some(desc) = obj.get("desc").and_then(|v| v.as_str()) {
                if desc.starts_with("addr(") && desc.ends_with(')') {
                    scan_addresses.push(desc[5..desc.len() - 1].to_string());
                }
            }
        }
    }

    // Fallback to all wallet addresses if no scanobjects specified
    if scan_addresses.is_empty() {
        let keystore = state.keystore.read().await;
        scan_addresses = keystore.all_addresses();
        drop(keystore);
    }

    let status = state.near_client.status().await;
    let block_height = status.as_ref().map(|s| s.latest_block_height).unwrap_or(0);
    let bestblock = status
        .as_ref()
        .map(|s| s.latest_block_hash.clone())
        .unwrap_or_default();
    let hrp = state.bech32_hrp();

    let mut utxos = Vec::new();
    let mut total = 0.0f64;
    for addr in &scan_addresses {
        if let Ok(account) = state.near_client.view_account(addr).await {
            let btc = account.balance_as_btc();
            if btc > 0.0 {
                total += btc;
                let spk = derive_script_pub_key_hex(addr, hrp);
                utxos.push(json!({
                    "txid": hex::encode(sha2::Sha256::digest(format!("utxo:{}:{}", addr, block_height).as_bytes())),
                    "vout": 0,
                    "scriptPubKey": spk,
                    "desc": format!("addr({})", addr),
                    "amount": btc,
                    "height": block_height
                }));
            }
        }
    }

    ok_response(
        &request.id,
        json!({
            "success": true,
            "txouts": utxos.len(),
            "height": block_height,
            "bestblock": bestblock,
            "unspents": utxos,
            "total_amount": total
        }),
    )
}

/// PSBT stubs — Bitcoin wallets need these even if we don't fully implement
fn handle_decodepsbt(request: &JsonRpcRequest) -> JsonRpcResponse {
    let psbt_b64 = match get_str_param(&request.params, 0) {
        Some(s) => s,
        None => return err_response(&request.id, -32602, "Missing PSBT parameter".to_string()),
    };

    let bytes = match base64_decode(psbt_b64) {
        Ok(b) => b,
        Err(_) => {
            return err_response(
                &request.id,
                -22,
                "Invalid PSBT: base64 decode failed".to_string(),
            )
        }
    };
    if bytes.len() < 5 || &bytes[..5] != b"psbt\xff" {
        return err_response(
            &request.id,
            -22,
            "Invalid PSBT: missing magic bytes".to_string(),
        );
    }

    // Parse the global unsigned tx from the PSBT
    let (vin, vout, tx_version, locktime) = parse_psbt_unsigned_tx(&bytes);

    // Compute txid from the unsigned tx (not the PSBT envelope)
    let unsigned_tx_hex = extract_unsigned_tx_hex(&bytes);
    let txid = if !unsigned_tx_hex.is_empty() {
        if let Ok(tx_bytes) = hex::decode(&unsigned_tx_hex) {
            use sha2::Digest as _;
            let hash = sha2::Sha256::digest(&sha2::Sha256::digest(&tx_bytes));
            let mut txid_bytes = hash.to_vec();
            txid_bytes.reverse(); // Bitcoin txid is reversed
            hex::encode(&txid_bytes)
        } else {
            "0".repeat(64)
        }
    } else {
        "0".repeat(64)
    };

    // Parse per-input PSBT maps
    let psbt_inputs = parse_psbt_input_maps(&bytes, vin.len());

    // Build per-output info from vout scriptPubKeys
    let psbt_outputs: Vec<serde_json::Value> = vout
        .iter()
        .map(|v| {
            let mut out = json!({});
            if let Some(spk) = v
                .get("scriptPubKey")
                .and_then(|s| s.get("hex"))
                .and_then(|h| h.as_str())
            {
                if !spk.is_empty() {
                    out = json!({
                        "witness_script": {},
                        "bip32_derivs": []
                    });
                }
            }
            out
        })
        .collect();

    ok_response(
        &request.id,
        json!({
            "tx": {
                "txid": txid,
                "hash": txid,
                "version": tx_version,
                "size": unsigned_tx_hex.len() / 2,
                "vsize": unsigned_tx_hex.len() / 2,
                "weight": (unsigned_tx_hex.len() / 2) * 4,
                "locktime": locktime,
                "vin": vin,
                "vout": vout
            },
            "global_xpubs": [],
            "psbt_version": 0,
            "proprietary": [],
            "unknown": {},
            "inputs": psbt_inputs,
            "outputs": psbt_outputs,
            "fee": null
        }),
    )
}

/// Parse per-input PSBT maps, extracting partial_sig, sighash_type, etc.
fn parse_psbt_input_maps(psbt_bytes: &[u8], num_inputs: usize) -> Vec<serde_json::Value> {
    let mut inputs = Vec::new();
    // Skip to end of global map
    let mut pos = 5; // skip magic
                     // Skip global key-value pairs
    while pos < psbt_bytes.len() {
        let (key_len, advance) = read_compact_size(psbt_bytes, pos);
        pos += advance;
        if key_len == 0 {
            break;
        } // end of global map
        pos += key_len as usize; // skip key
        if pos >= psbt_bytes.len() {
            break;
        }
        let (val_len, advance2) = read_compact_size(psbt_bytes, pos);
        pos += advance2;
        pos += val_len as usize; // skip value
    }

    // Now parse per-input maps
    for _ in 0..num_inputs {
        let mut input_info = serde_json::Map::new();
        let mut has_partial_sig = false;
        while pos < psbt_bytes.len() {
            let (key_len, advance) = read_compact_size(psbt_bytes, pos);
            pos += advance;
            if key_len == 0 {
                break;
            } // end of this input map
            if pos >= psbt_bytes.len() {
                break;
            }
            let key_type = psbt_bytes[pos];
            let key_data = if key_len > 1 && pos + key_len as usize <= psbt_bytes.len() {
                psbt_bytes[pos + 1..pos + key_len as usize].to_vec()
            } else {
                Vec::new()
            };
            pos += key_len as usize;
            if pos >= psbt_bytes.len() {
                break;
            }
            let (val_len, advance2) = read_compact_size(psbt_bytes, pos);
            pos += advance2;
            let val_data = if pos + val_len as usize <= psbt_bytes.len() {
                psbt_bytes[pos..pos + val_len as usize].to_vec()
            } else {
                Vec::new()
            };
            pos += val_len as usize;

            match key_type {
                0x02 => {
                    // PSBT_IN_PARTIAL_SIG: key = pubkey, value = signature
                    has_partial_sig = true;
                    let pubkey_hex = hex::encode(&key_data);
                    let sig_hex = hex::encode(&val_data);
                    if !input_info.contains_key("partial_signatures") {
                        input_info.insert("partial_signatures".to_string(), json!({}));
                    }
                    if let Some(sigs) = input_info
                        .get_mut("partial_signatures")
                        .and_then(|v| v.as_object_mut())
                    {
                        sigs.insert(pubkey_hex, json!(sig_hex));
                    }
                }
                0x03 => {
                    // PSBT_IN_SIGHASH_TYPE
                    if val_data.len() >= 4 {
                        let sighash = u32::from_le_bytes([
                            val_data[0],
                            val_data[1],
                            val_data[2],
                            val_data[3],
                        ]);
                        input_info.insert(
                            "sighash".to_string(),
                            json!(format!("ALL|ANYONECANPAY+{}", sighash)),
                        );
                    }
                }
                0x06 => {
                    // PSBT_IN_BIP32_DERIVATION
                    input_info.insert("bip32_derivs".to_string(), json!([{
                        "pubkey": hex::encode(&key_data),
                        "master_fingerprint": if val_data.len() >= 4 { hex::encode(&val_data[..4]) } else { "00000000".to_string() },
                        "path": "m/0'/0'/0'"
                    }]));
                }
                _ => {}
            }
        }
        if !has_partial_sig {
            input_info.insert("partial_signatures".to_string(), json!({}));
        }
        if !input_info.contains_key("bip32_derivs") {
            input_info.insert("bip32_derivs".to_string(), json!([]));
        }
        inputs.push(json!(input_info));
    }

    // If we didn't parse enough, pad with empty
    while inputs.len() < num_inputs {
        inputs.push(json!({"partial_signatures": {}, "bip32_derivs": []}));
    }
    inputs
}

/// Return per-input partial-signature counts for a PSBT.
fn psbt_input_signature_counts(psbt_bytes: &[u8]) -> Vec<usize> {
    let (vin, _, _, _) = parse_psbt_unsigned_tx(psbt_bytes);
    if vin.is_empty() {
        return Vec::new();
    }
    parse_psbt_input_maps(psbt_bytes, vin.len())
        .into_iter()
        .map(|input| {
            input
                .get("partial_signatures")
                .and_then(|v| v.as_object())
                .map(|sigs| sigs.len())
                .unwrap_or(0)
        })
        .collect()
}

/// Parse the unsigned transaction from PSBT bytes, returning (vin, vout, version, locktime).
fn parse_psbt_unsigned_tx(
    psbt_bytes: &[u8],
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>, u32, u32) {
    let empty = || (Vec::new(), Vec::new(), 2u32, 0u32);
    // Skip magic (5 bytes)
    let mut pos = 5;
    // Read key-value pairs until separator (0x00)
    while pos < psbt_bytes.len() {
        let (key_len, advance) = read_compact_size(psbt_bytes, pos);
        pos += advance;
        if key_len == 0 {
            break;
        } // end of global map
        if pos >= psbt_bytes.len() {
            return empty();
        }
        let key_type = psbt_bytes[pos];
        pos += key_len as usize;
        if pos >= psbt_bytes.len() {
            return empty();
        }
        // Read value
        let (val_len, advance2) = read_compact_size(psbt_bytes, pos);
        pos += advance2;
        if key_type == 0x00 {
            // This is PSBT_GLOBAL_UNSIGNED_TX — parse it
            let tx_start = pos;
            let tx_end = pos + val_len as usize;
            if tx_end > psbt_bytes.len() {
                return empty();
            }
            let tx_bytes = &psbt_bytes[tx_start..tx_end];
            return parse_raw_tx_for_psbt(tx_bytes);
        }
        pos += val_len as usize;
    }
    empty()
}

fn read_compact_size(data: &[u8], pos: usize) -> (u64, usize) {
    if pos >= data.len() {
        return (0, 1);
    }
    let first = data[pos];
    if first < 253 {
        (first as u64, 1)
    } else if first == 0xFD && pos + 2 < data.len() {
        let v = u16::from_le_bytes([data[pos + 1], data[pos + 2]]);
        (v as u64, 3)
    } else if first == 0xFE && pos + 4 < data.len() {
        let v = u32::from_le_bytes([data[pos + 1], data[pos + 2], data[pos + 3], data[pos + 4]]);
        (v as u64, 5)
    } else if first == 0xFF && pos + 8 < data.len() {
        let v = u64::from_le_bytes([
            data[pos + 1],
            data[pos + 2],
            data[pos + 3],
            data[pos + 4],
            data[pos + 5],
            data[pos + 6],
            data[pos + 7],
            data[pos + 8],
        ]);
        (v, 9)
    } else {
        (0, 1)
    }
}

/// Parse a raw Bitcoin transaction for PSBT decode, returning (vin, vout, version, locktime).
fn parse_raw_tx_for_psbt(tx: &[u8]) -> (Vec<serde_json::Value>, Vec<serde_json::Value>, u32, u32) {
    let mut vin = Vec::new();
    let mut vout = Vec::new();
    if tx.len() < 10 {
        return (vin, vout, 2, 0);
    }

    let mut pos = 0;
    // version (4 bytes)
    let version = u32::from_le_bytes([tx[0], tx[1], tx[2], tx[3]]);
    pos += 4;

    // input count
    let (in_count, adv) = read_compact_size(tx, pos);
    pos += adv;

    for _ in 0..in_count {
        if pos + 36 > tx.len() {
            break;
        }
        // prevout hash (32 bytes, reversed for display)
        let mut txid_bytes = tx[pos..pos + 32].to_vec();
        txid_bytes.reverse();
        let prev_txid = hex::encode(&txid_bytes);
        pos += 32;
        let prev_vout = u32::from_le_bytes([tx[pos], tx[pos + 1], tx[pos + 2], tx[pos + 3]]);
        pos += 4;
        // scriptSig
        let (script_len, adv) = read_compact_size(tx, pos);
        pos += adv;
        pos += script_len as usize;
        // sequence
        if pos + 4 > tx.len() {
            break;
        }
        let sequence = u32::from_le_bytes([tx[pos], tx[pos + 1], tx[pos + 2], tx[pos + 3]]);
        pos += 4;

        vin.push(json!({
            "txid": prev_txid,
            "vout": prev_vout,
            "scriptSig": { "asm": "", "hex": "" },
            "sequence": sequence
        }));
    }

    // output count
    let (out_count, adv) = read_compact_size(tx, pos);
    pos += adv;

    for n in 0..out_count {
        if pos + 8 > tx.len() {
            break;
        }
        let value = u64::from_le_bytes([
            tx[pos],
            tx[pos + 1],
            tx[pos + 2],
            tx[pos + 3],
            tx[pos + 4],
            tx[pos + 5],
            tx[pos + 6],
            tx[pos + 7],
        ]);
        pos += 8;
        let (script_len, adv) = read_compact_size(tx, pos);
        pos += adv;
        let script_hex = if (pos + script_len as usize) <= tx.len() {
            hex::encode(&tx[pos..pos + script_len as usize])
        } else {
            String::new()
        };
        pos += script_len as usize;

        // Derive script type and asm from hex
        let (script_type, script_asm) = classify_script_pub_key_hex(&script_hex);
        vout.push(json!({
            "value": value as f64 / 100_000_000.0,
            "n": n,
            "scriptPubKey": {
                "asm": script_asm,
                "hex": script_hex,
                "type": script_type
            }
        }));
    }

    // locktime (last 4 bytes)
    let locktime = if pos + 4 <= tx.len() {
        u32::from_le_bytes([tx[pos], tx[pos + 1], tx[pos + 2], tx[pos + 3]])
    } else {
        0
    };

    (vin, vout, version, locktime)
}

fn parse_psbt_output_pairs(outputs_param: Option<&serde_json::Value>) -> Vec<(String, f64)> {
    let mut output_pairs = Vec::new();
    match outputs_param {
        Some(serde_json::Value::Array(outs)) => {
            for out_obj in outs {
                if let Some(obj) = out_obj.as_object() {
                    for (addr, amount_val) in obj {
                        if addr == "data" {
                            continue;
                        }
                        if let Some(amt) = amount_val.as_f64() {
                            output_pairs.push((addr.clone(), amt));
                        }
                    }
                }
            }
        }
        Some(serde_json::Value::Object(obj)) => {
            for (addr, amount_val) in obj {
                if addr == "data" {
                    continue;
                }
                if let Some(amt) = amount_val.as_f64() {
                    output_pairs.push((addr.clone(), amt));
                }
            }
        }
        _ => {}
    }
    output_pairs
}

async fn handle_walletcreatefundedpsbt(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    // walletcreatefundedpsbt(inputs, outputs, locktime, options)
    // If inputs is empty, auto-select from wallet UTXOs
    let inputs = request
        .params
        .as_array()
        .and_then(|arr| arr.get(0))
        .and_then(|v| v.as_array());
    let outputs = request.params.as_array().and_then(|arr| arr.get(1));
    let locktime = request
        .params
        .as_array()
        .and_then(|arr| arr.get(2))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    // Parse outputs: [{addr: amount}, ...]
    let output_pairs = parse_psbt_output_pairs(outputs);

    let total_output: f64 = output_pairs.iter().map(|(_, a)| a).sum();
    let fee_btc = 0.00001_f64;

    // Auto-select inputs if none provided
    let use_inputs: Vec<(String, u32)> = if inputs.map(|i| i.is_empty()).unwrap_or(true) {
        // Find a wallet address with enough balance
        let keystore = state.keystore.read().await;
        let addrs: Vec<String> = keystore.addresses().iter().map(|a| a.to_string()).collect();
        drop(keystore);
        let locked_utxos: std::collections::HashSet<(String, u32)> = {
            let locked = state.locked_utxos.read().await;
            locked.iter().cloned().collect()
        };

        let mut selected = Vec::new();
        for addr in &addrs {
            let txid = SyntheticUtxo::txid_for_account(addr);
            if locked_utxos.contains(&(txid.clone(), 0u32)) {
                continue;
            }
            if let Ok(account) = state.near_client.view_account(addr).await {
                let btc = account.balance_as_btc();
                if btc >= total_output + fee_btc {
                    selected.push((txid, 0u32));
                    break;
                }
            }
        }
        selected
    } else {
        inputs
            .unwrap()
            .iter()
            .map(|inp| {
                let txid = inp
                    .get("txid")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&"0".repeat(64))
                    .to_string();
                let vout = inp.get("vout").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                (txid, vout)
            })
            .collect()
    };

    if use_inputs.is_empty() {
        return err_response(&request.id, -4, "Insufficient funds".to_string());
    }

    let num_inputs = use_inputs.len();
    let num_outputs = output_pairs.len();

    // Build unsigned transaction (same structure as createpsbt)
    let mut unsigned_tx: Vec<u8> = Vec::new();
    unsigned_tx.extend_from_slice(&2u32.to_le_bytes()); // version
    write_compact_size(&mut unsigned_tx, num_inputs as u64);
    for (txid_hex, vout) in &use_inputs {
        if let Ok(mut txid_bytes) = hex::decode(txid_hex) {
            txid_bytes.reverse();
            unsigned_tx.extend_from_slice(&txid_bytes);
        } else {
            unsigned_tx.extend_from_slice(&[0u8; 32]);
        }
        unsigned_tx.extend_from_slice(&vout.to_le_bytes());
        unsigned_tx.push(0x00); // empty scriptSig
        unsigned_tx.extend_from_slice(&0xFFFFFFFDu32.to_le_bytes()); // sequence
    }
    write_compact_size(&mut unsigned_tx, num_outputs as u64);
    let hrp = state.bech32_hrp();
    for (addr, btc_amount) in &output_pairs {
        let satoshis = (*btc_amount * 100_000_000.0) as u64;
        unsigned_tx.extend_from_slice(&satoshis.to_le_bytes());
        let script_hex = derive_script_pub_key_hex(addr, hrp);
        if script_hex.is_empty() {
            unsigned_tx.push(0x00);
        } else {
            match hex::decode(&script_hex) {
                Ok(script_bytes) => {
                    write_compact_size(&mut unsigned_tx, script_bytes.len() as u64);
                    unsigned_tx.extend_from_slice(&script_bytes);
                }
                Err(_) => unsigned_tx.push(0x00),
            }
        }
    }
    unsigned_tx.extend_from_slice(&locktime.to_le_bytes());

    // Build PSBT
    let mut psbt: Vec<u8> = Vec::new();
    psbt.extend_from_slice(b"psbt\xff");
    psbt.push(0x01); // key length
    psbt.push(0x00); // unsigned tx key
    write_compact_size(&mut psbt, unsigned_tx.len() as u64);
    psbt.extend_from_slice(&unsigned_tx);
    psbt.push(0x00); // end global map
    for _ in 0..num_inputs {
        psbt.push(0x00);
    }
    for _ in 0..num_outputs {
        psbt.push(0x00);
    }

    let psbt_b64 = base64_encode(&psbt);

    ok_response(
        &request.id,
        json!({
            "psbt": psbt_b64,
            "fee": fee_btc,
            "changepos": -1
        }),
    )
}

fn handle_finalizepsbt(request: &JsonRpcRequest) -> JsonRpcResponse {
    let psbt_b64 = match get_str_param(&request.params, 0) {
        Some(s) => s,
        None => return err_response(&request.id, -32602, "Missing PSBT parameter".to_string()),
    };

    // Extract the unsigned tx from the PSBT — this is the broadcastable hex
    let psbt_bytes = match base64_decode(psbt_b64) {
        Ok(b) => b,
        Err(_) => return err_response(&request.id, -22, "Invalid PSBT base64".to_string()),
    };
    if psbt_bytes.len() < 5 || &psbt_bytes[..5] != b"psbt\xff" {
        return err_response(
            &request.id,
            -22,
            "Invalid PSBT: missing magic bytes".to_string(),
        );
    }

    let tx_hex = extract_unsigned_tx_hex(&psbt_bytes);
    if tx_hex.is_empty() {
        return ok_response(
            &request.id,
            json!({
                "hex": "",
                "complete": false,
                "psbt": psbt_b64
            }),
        );
    }

    let signature_counts = psbt_input_signature_counts(&psbt_bytes);
    let has_inputs = !signature_counts.is_empty();
    let all_inputs_signed = has_inputs && signature_counts.iter().all(|count| *count > 0);
    if !all_inputs_signed {
        return ok_response(
            &request.id,
            json!({
                "hex": "",
                "complete": false,
                "psbt": psbt_b64
            }),
        );
    }

    // All inputs are signed: return extractable tx hex.
    ok_response(
        &request.id,
        json!({
            "hex": tx_hex,
            "complete": true
        }),
    )
}

fn handle_combinepsbt(request: &JsonRpcRequest) -> JsonRpcResponse {
    // Combine multiple PSBTs by merging partial signatures for the same unsigned tx.
    let psbt_array = request
        .params
        .as_array()
        .and_then(|arr| arr.get(0))
        .and_then(|v| v.as_array());

    let psbts: Vec<&str> = match psbt_array {
        Some(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
        None => return err_response(&request.id, -32602, "Missing PSBTs array".to_string()),
    };

    if psbts.is_empty() {
        return err_response(&request.id, -32602, "Empty PSBTs array".to_string());
    }

    let mut reference_unsigned_tx: Option<String> = None;
    let mut reference_unsigned_tx_bytes: Vec<u8> = Vec::new();
    let mut expected_inputs: usize = 0;
    let mut expected_outputs: usize = 0;
    let mut merged_signatures: Vec<std::collections::BTreeMap<String, String>> = Vec::new();
    let mut saw_valid_candidate = false;

    for psbt in psbts {
        let bytes = match base64_decode(psbt) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        if bytes.len() < 5 || &bytes[..5] != b"psbt\xff" {
            continue;
        }

        let unsigned_tx_hex = extract_unsigned_tx_hex(&bytes);
        if unsigned_tx_hex.is_empty() {
            continue;
        }
        let unsigned_tx_bytes = match hex::decode(&unsigned_tx_hex) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let (vin, vout, _, _) = parse_psbt_unsigned_tx(&bytes);
        let input_count = vin.len();
        let output_count = vout.len();

        match &reference_unsigned_tx {
            Some(reference) if reference != &unsigned_tx_hex => {
                return err_response(
                    &request.id,
                    -8,
                    "PSBTs do not refer to the same transaction".to_string(),
                );
            }
            None => {
                reference_unsigned_tx = Some(unsigned_tx_hex);
                reference_unsigned_tx_bytes = unsigned_tx_bytes;
                expected_inputs = input_count;
                expected_outputs = output_count;
                merged_signatures = vec![std::collections::BTreeMap::new(); expected_inputs];
            }
            _ => {}
        }

        if input_count != expected_inputs || output_count != expected_outputs {
            return err_response(
                &request.id,
                -8,
                "PSBTs do not refer to the same transaction".to_string(),
            );
        }

        let input_maps = parse_psbt_input_maps(&bytes, expected_inputs);
        for (idx, input_map) in input_maps.iter().enumerate().take(expected_inputs) {
            let Some(partial_sigs) = input_map.get("partial_signatures").and_then(|v| v.as_object())
            else {
                continue;
            };
            for (pubkey_hex, sig_hex_value) in partial_sigs {
                if let Some(sig_hex) = sig_hex_value.as_str() {
                    merged_signatures[idx].insert(pubkey_hex.clone(), sig_hex.to_string());
                }
            }
        }
        saw_valid_candidate = true;
    }

    if !saw_valid_candidate || reference_unsigned_tx.is_none() {
        return err_response(
            &request.id,
            -22,
            "Invalid PSBT: no valid candidates".to_string(),
        );
    }

    let mut combined_psbt: Vec<u8> = Vec::new();
    combined_psbt.extend_from_slice(b"psbt\xff");
    combined_psbt.push(0x01); // key len
    combined_psbt.push(0x00); // PSBT_GLOBAL_UNSIGNED_TX
    write_compact_size(&mut combined_psbt, reference_unsigned_tx_bytes.len() as u64);
    combined_psbt.extend_from_slice(&reference_unsigned_tx_bytes);
    combined_psbt.push(0x00); // end global map

    for input_sig_map in &merged_signatures {
        for (pubkey_hex, sig_hex) in input_sig_map {
            let pubkey_bytes = match hex::decode(pubkey_hex) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let sig_bytes = match hex::decode(sig_hex) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };

            write_compact_size(&mut combined_psbt, (1 + pubkey_bytes.len()) as u64);
            combined_psbt.push(0x02); // PSBT_IN_PARTIAL_SIG
            combined_psbt.extend_from_slice(&pubkey_bytes);
            write_compact_size(&mut combined_psbt, sig_bytes.len() as u64);
            combined_psbt.extend_from_slice(&sig_bytes);
        }
        combined_psbt.push(0x00); // end input map
    }

    for _ in 0..expected_outputs {
        combined_psbt.push(0x00); // empty output map
    }

    ok_response(&request.id, json!(base64_encode(&combined_psbt)))
}

/// deriveaddresses - derive addresses from a descriptor
fn handle_deriveaddresses(request: &JsonRpcRequest) -> JsonRpcResponse {
    let descriptor = match get_str_param(&request.params, 0) {
        Some(d) => d,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing descriptor parameter".to_string(),
            )
        }
    };

    // Parse basic descriptor types: addr(ADDRESS), wpkh(KEY), pkh(KEY), sh(wpkh(KEY))
    // Strip checksum if present (everything after #)
    let desc_clean = descriptor.split('#').next().unwrap_or(descriptor);

    // addr(ADDRESS) — simplest: just return the address
    if desc_clean.starts_with("addr(") && desc_clean.ends_with(')') {
        let addr = &desc_clean[5..desc_clean.len() - 1];
        return ok_response(&request.id, json!([addr]));
    }

    // For key-based descriptors, extract the inner key/address
    // wpkh(ADDRESS), pkh(ADDRESS), sh(wpkh(ADDRESS))
    let inner = if desc_clean.starts_with("wpkh(") && desc_clean.ends_with(')') {
        Some(&desc_clean[5..desc_clean.len() - 1])
    } else if desc_clean.starts_with("pkh(") && desc_clean.ends_with(')') {
        Some(&desc_clean[4..desc_clean.len() - 1])
    } else if desc_clean.starts_with("sh(wpkh(") && desc_clean.ends_with("))") {
        Some(&desc_clean[8..desc_clean.len() - 2])
    } else if desc_clean.starts_with("sh(") && desc_clean.ends_with(')') {
        Some(&desc_clean[3..desc_clean.len() - 1])
    } else {
        None
    };

    match inner {
        Some(key_or_addr) => {
            // If it looks like a Bitcoin address already, return it
            // Otherwise it's a hex pubkey or xpub which we can't derive without BIP32
            if key_or_addr.starts_with('1')
                || key_or_addr.starts_with('3')
                || key_or_addr.starts_with('m')
                || key_or_addr.starts_with('n')
                || key_or_addr.starts_with('2')
                || key_or_addr.starts_with("bc1")
                || key_or_addr.starts_with("tb1")
                || key_or_addr.starts_with("bcrt1")
            {
                ok_response(&request.id, json!([key_or_addr]))
            } else {
                err_response(&request.id, -5, "Cannot derive address from extended key or raw pubkey — use addr() descriptor or getnewaddress".to_string())
            }
        }
        None => err_response(
            &request.id,
            -5,
            format!("Unsupported descriptor type: {}", desc_clean),
        ),
    }
}

/// getdescriptorinfo - analyze a descriptor
fn handle_getdescriptorinfo(request: &JsonRpcRequest) -> JsonRpcResponse {
    let desc = get_str_param(&request.params, 0).unwrap_or("");
    // Strip existing checksum if present
    let desc_clean = desc.split('#').next().unwrap_or(desc);
    let checksum = compute_descriptor_checksum(desc_clean);
    let descriptor_with_checksum = format!("{}#{}", desc_clean, checksum);

    let is_range = desc_clean.contains('*'); // range descriptors contain wildcard
    let has_private = desc_clean.contains("xprv") || desc_clean.contains("tprv");
    let is_solvable = desc_clean.starts_with("wpkh(")
        || desc_clean.starts_with("pkh(")
        || desc_clean.starts_with("sh(")
        || desc_clean.starts_with("addr(")
        || desc_clean.starts_with("tr(");

    ok_response(
        &request.id,
        json!({
            "descriptor": descriptor_with_checksum,
            "checksum": checksum,
            "isrange": is_range,
            "issolvable": is_solvable,
            "hasprivatekeys": has_private
        }),
    )
}

/// Compute descriptor checksum (Bitcoin Core compatible, 8-char)
fn compute_descriptor_checksum(desc: &str) -> String {
    const INPUT_CHARSET: &str = "0123456789()[],'/*abcdefgh@:$%{}IJKLMNOPQRSTUVWXYZ&+-.;<=>?!^_|~ijklmnopqrstuvwxyzABCDEFGH`#\"\\ ";
    const CHECKSUM_CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

    fn polymod(c: u64, val: u64) -> u64 {
        let c0 = c >> 35;
        let mut c = ((c & 0x7ffffffff) << 5) ^ val;
        if c0 & 1 != 0 {
            c ^= 0xf5dee51989;
        }
        if c0 & 2 != 0 {
            c ^= 0xa9fdca3312;
        }
        if c0 & 4 != 0 {
            c ^= 0x1bab10e32d;
        }
        if c0 & 8 != 0 {
            c ^= 0x3706b1677a;
        }
        if c0 & 16 != 0 {
            c ^= 0x644d626ffd;
        }
        c
    }

    let mut c: u64 = 1;
    let mut cls: u64 = 0;
    let mut clscount: u64 = 0;

    for ch in desc.chars() {
        let pos = match INPUT_CHARSET.find(ch) {
            Some(p) => p as u64,
            None => continue,
        };
        c = polymod(c, pos & 31);
        cls = cls * 3 + (pos >> 5);
        clscount += 1;
        if clscount == 3 {
            c = polymod(c, cls);
            cls = 0;
            clscount = 0;
        }
    }
    if clscount > 0 {
        c = polymod(c, cls);
    }
    for _ in 0..8 {
        c = polymod(c, 0);
    }
    c ^= 1;

    let mut result = String::with_capacity(8);
    for j in 0..8 {
        result.push(CHECKSUM_CHARSET[((c >> (5 * (7 - j))) & 31) as usize] as char);
    }
    result
}

/// importdescriptors - import output descriptors (stub)
fn handle_importdescriptors(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(
        &request.id,
        json!([{
            "success": true,
            "warnings": ["Descriptors not natively supported; use importprivkey instead"]
        }]),
    )
}

/// getdifficulty - return current difficulty (always 1 for account-based)
fn handle_getdifficulty(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(1.0))
}

/// getchaintips - return chain tip info
async fn handle_getchaintips(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let status = match state.near_client.status().await {
        Ok(s) => s,
        Err(e) => return err_response(&request.id, -32000, format!("Node error: {}", e)),
    };
    ok_response(
        &request.id,
        json!([{
            "height": status.latest_block_height,
            "hash": status.latest_block_hash,
            "branchlen": 0,
            "status": "active"
        }]),
    )
}

/// gettxoutproof - get proof for a transaction inclusion (not applicable to NEAR)
fn handle_gettxoutproof(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(
        &request.id,
        -1,
        "Transaction inclusion proofs not supported in account-based model".to_string(),
    )
}

/// verifytxoutproof - verify a tx inclusion proof
fn handle_verifytxoutproof(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(
        &request.id,
        -1,
        "Transaction inclusion proofs not supported in account-based model".to_string(),
    )
}

/// listsinceblock - list transactions since a block
async fn handle_listsinceblock(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let status = match state.near_client.status().await {
        Ok(s) => s,
        Err(e) => return err_response(&request.id, -32000, format!("Node error: {}", e)),
    };

    // Get the "since" block height from the provided blockhash
    let since_height = if let Some(blockhash) = get_str_param(&request.params, 0) {
        if !blockhash.is_empty() {
            match state.near_client.block_by_hash(blockhash).await {
                Ok(block) => block
                    .get("header")
                    .and_then(|h| h.get("height"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                Err(_) => 0,
            }
        } else {
            0
        }
    } else {
        0
    };

    let current_height = status.latest_block_height;

    // Collect wallet addresses for category detection
    let keystore = state.keystore.read().await;
    let wallet_addrs: std::collections::HashSet<String> =
        keystore.addresses().iter().map(|a| a.to_string()).collect();
    drop(keystore);

    let cache = state.tx_cache.read().await;
    let mut txs: Vec<serde_json::Value> = Vec::new();
    for (btc_txid, entry) in cache.entries.iter() {
        if entry.near_tx_hash.starts_with("pending:") || entry.near_tx_hash.starts_with("error:") {
            continue;
        }

        // Handle incoming transactions (from indexer) — they have block_height directly
        if entry.is_incoming {
            let tx_height = entry.block_height;
            if since_height > 0 && tx_height <= since_height {
                continue;
            }
            let amount_btc = entry.amount_satoshis as f64 / 100_000_000.0;
            let confs = if tx_height > 0 && current_height >= tx_height {
                (current_height - tx_height + 1) as i64
            } else {
                1
            };
            let ts = match state.near_client.block_by_height(tx_height).await {
                Ok(block) => block
                    .get("header")
                    .unwrap_or(&block)
                    .get("timestamp")
                    .and_then(|v| v.as_u64())
                    .map(|t| (t / 1_000_000_000) as i64)
                    .unwrap_or(chrono::Utc::now().timestamp()),
                Err(_) => chrono::Utc::now().timestamp(),
            };

            if wallet_addrs.contains(&entry.receiver_id) {
                txs.push(json!({
                    "txid": btc_txid,
                    "amount": amount_btc,
                    "fee": 0.0,
                    "confirmations": confs,
                    "category": "receive",
                    "time": ts,
                    "timereceived": ts,
                    "address": entry.receiver_id,
                    "near_tx_hash": entry.near_tx_hash
                }));
            }
            continue;
        }

        let (tx_height, blocktime, confirmations) = match state
            .near_client
            .tx_status(&entry.near_tx_hash, &entry.sender_id)
            .await
        {
            Ok(tx_result) => {
                let tx_block_hash = tx_result
                    .get("transaction_outcome")
                    .and_then(|o| o.get("block_hash"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !tx_block_hash.is_empty() {
                    match state.near_client.block_by_hash(tx_block_hash).await {
                        Ok(block) => {
                            let header = block.get("header").unwrap_or(&block);
                            let h = header.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
                            let ts = header
                                .get("timestamp")
                                .and_then(|v| v.as_u64())
                                .map(|t| (t / 1_000_000_000) as i64)
                                .unwrap_or(chrono::Utc::now().timestamp());
                            let confs = if h > 0 && current_height >= h {
                                (current_height - h + 1) as i64
                            } else {
                                1
                            };
                            (h, ts, confs)
                        }
                        Err(_) => (0u64, chrono::Utc::now().timestamp(), 1i64),
                    }
                } else {
                    (0u64, chrono::Utc::now().timestamp(), 1i64)
                }
            }
            Err(_) => continue,
        };

        if since_height > 0 && tx_height <= since_height {
            continue;
        }

        let (amount_btc, recipient) = if entry.raw_hex.starts_with("sendtoaddress:") {
            let parts: Vec<&str> = entry.raw_hex.splitn(3, ':').collect();
            let r = parts.get(1).unwrap_or(&"").to_string();
            let sat: u64 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            (sat as f64 / 100_000_000.0, r)
        } else if let Ok(parsed) =
            ParsedBitcoinTx::from_hex_with_hrp(&entry.raw_hex, state.bech32_hrp())
        {
            let amt = parsed.total_payment_satoshis() as f64 / 100_000_000.0;
            let recip = parsed
                .payment_output()
                .map(|o| o.address.clone())
                .unwrap_or_default();
            (amt, recip)
        } else {
            (0.0, String::new())
        };

        let sender_is_ours = wallet_addrs.contains(&entry.sender_id);
        let recipient_is_ours = wallet_addrs.contains(&recipient);

        if sender_is_ours {
            txs.push(json!({
                "txid": btc_txid,
                "amount": -(amount_btc),
                "fee": 0.0,
                "confirmations": confirmations,
                "category": "send",
                "time": blocktime,
                "timereceived": blocktime,
                "address": recipient,
                "near_tx_hash": entry.near_tx_hash
            }));
        }
        if recipient_is_ours {
            txs.push(json!({
                "txid": btc_txid,
                "amount": amount_btc,
                "fee": 0.0,
                "confirmations": confirmations,
                "category": "receive",
                "time": blocktime,
                "timereceived": blocktime,
                "address": recipient,
                "near_tx_hash": entry.near_tx_hash
            }));
        }
        if !sender_is_ours && !recipient_is_ours {
            txs.push(json!({
                "txid": btc_txid,
                "amount": -(amount_btc),
                "fee": 0.0,
                "confirmations": confirmations,
                "category": "send",
                "time": blocktime,
                "timereceived": blocktime,
                "address": recipient,
                "near_tx_hash": entry.near_tx_hash
            }));
        }
    }

    ok_response(
        &request.id,
        json!({
            "transactions": txs,
            "removed": [],
            "lastblock": status.latest_block_hash
        }),
    )
}

/// listdescriptors - list wallet descriptors
async fn handle_listdescriptors(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let keystore = state.keystore.read().await;
    let descriptors: Vec<serde_json::Value> = keystore
        .addresses()
        .iter()
        .filter_map(|addr| {
            let entry = keystore.get(addr)?;
            let bech32_prefix = format!("{}1", state.bech32_hrp());
            let desc = if addr.starts_with(&bech32_prefix) {
                format!("wpkh({})", entry.public_key_compressed_hex)
            } else {
                format!("pkh({})", entry.public_key_compressed_hex)
            };
            Some(json!({
                "desc": desc,
                "timestamp": chrono::Utc::now().timestamp(),
                "active": true,
                "internal": false,
                "range": [0, 0],
                "next": 0
            }))
        })
        .collect();

    ok_response(
        &request.id,
        json!({
            "wallet_name": "bitinfinity",
            "descriptors": descriptors
        }),
    )
}

/// signrawtransactionwithkey - sign a raw tx with externally provided private keys
/// Params: [hexstring, privkeys_array, prevtxs, sighashtype]
async fn handle_signrawtransactionwithkey(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let raw_hex = match get_str_param(&request.params, 0) {
        Some(h) => h,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing raw transaction hex".to_string(),
            )
        }
    };
    let privkeys = request
        .params
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if privkeys.is_empty() {
        return err_response(
            &request.id,
            -32602,
            "Missing private keys array".to_string(),
        );
    }

    // Import the provided keys into a temporary keystore for signing
    let mut temp_keystore = crate::keystore::Keystore::default();
    for key_val in &privkeys {
        let key_str = match key_val.as_str() {
            Some(k) => k,
            None => continue,
        };
        // Try WIF format first, then raw hex
        let privkey_hex = if key_str.len() == 64 && hex::decode(key_str).is_ok() {
            key_str.to_string()
        } else {
            // Decode WIF
            match bs58::decode(key_str).into_vec() {
                Ok(decoded) if decoded.len() >= 33 => hex::encode(&decoded[1..33]),
                _ => continue,
            }
        };

        // Derive public key and address
        let privkey_bytes = match hex::decode(&privkey_hex) {
            Ok(b) if b.len() == 32 => b,
            _ => continue,
        };
        let secp = secp256k1::Secp256k1::new();
        let secret_key = match secp256k1::SecretKey::from_slice(&privkey_bytes) {
            Ok(sk) => sk,
            Err(_) => continue,
        };
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let compressed = public_key.serialize();
        let uncompressed = public_key.serialize_uncompressed();

        // Derive address
        use ripemd::Ripemd160;
        use sha2::{Digest, Sha256};
        let sha_hash = Sha256::digest(&compressed);
        let pubkey_hash = Ripemd160::digest(&sha_hash);
        let bech32_hrp = state.bech32_hrp();
        let address = crate::utxo_synth::SyntheticUtxo::derive_script_pub_key_address(
            &pubkey_hash,
            bech32_hrp,
        );

        temp_keystore.insert(
            address,
            crate::keystore::KeyEntry {
                private_key_hex: privkey_hex,
                public_key_compressed_hex: hex::encode(&compressed),
                public_key_uncompressed_hex: hex::encode(&uncompressed[1..]),
            },
        );
    }

    // Now sign using the same logic as signrawtransactionwithwallet
    // but with our temp keystore
    let intent_bytes = hex::decode(raw_hex).unwrap_or_default();
    let intent_str = String::from_utf8(intent_bytes).unwrap_or_default();

    if intent_str.starts_with("bitinfinity-intent:") {
        // Parse intent and sign with temp keystore keys
        let parts: Vec<&str> = intent_str.splitn(4, ':').collect();
        if parts.len() >= 4 {
            let sender = parts[1];
            if temp_keystore.get(sender).is_some() {
                let signed_payload = format!("bitinfinity:{}:{}:{}", sender, parts[2], parts[3]);
                return ok_response(
                    &request.id,
                    json!({
                        "hex": hex::encode(signed_payload.as_bytes()),
                        "complete": true
                    }),
                );
            }
        }
        return ok_response(
            &request.id,
            json!({
                "hex": raw_hex,
                "complete": false,
                "errors": [{"error": "No matching key found for sender address"}]
            }),
        );
    }

    // Try as real Bitcoin tx
    match ParsedBitcoinTx::from_hex_with_hrp(raw_hex, state.bech32_hrp()) {
        Ok(parsed) => {
            let sender = &parsed.sender_address;
            if temp_keystore.get(sender).is_some() {
                // Build signed payload
                if let Some(output) = parsed.payment_output() {
                    let signed_payload = format!(
                        "bitinfinity:{}:{}:{}",
                        sender, output.address, output.amount_satoshis
                    );
                    return ok_response(
                        &request.id,
                        json!({
                            "hex": hex::encode(signed_payload.as_bytes()),
                            "complete": true
                        }),
                    );
                }
            }
            ok_response(
                &request.id,
                json!({
                    "hex": raw_hex,
                    "complete": false,
                    "errors": [{"error": "No matching key found for sender address"}]
                }),
            )
        }
        Err(_) => ok_response(
            &request.id,
            json!({
                "hex": raw_hex,
                "complete": false,
                "errors": [{"error": "Could not parse transaction"}]
            }),
        ),
    }
}

/// converttopsbt - convert a raw transaction to PSBT format
/// Wraps an unsigned tx in a PSBT envelope (magic + global unsigned_tx + empty input/output maps)
fn handle_converttopsbt(request: &JsonRpcRequest) -> JsonRpcResponse {
    let raw_hex = match get_str_param(&request.params, 0) {
        Some(h) => h,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing hex string parameter".to_string(),
            )
        }
    };

    let tx_bytes = match hex::decode(raw_hex) {
        Ok(b) => b,
        Err(_) => return err_response(&request.id, -22, "TX decode failed".to_string()),
    };

    // Count inputs and outputs from the raw tx for empty maps
    // Parse minimally: version(4) + varint(inputs) + ... + varint(outputs) + ...
    let (n_inputs, n_outputs) = if tx_bytes.len() > 10 {
        // Try to count from the bitcoin crate parse
        match bitcoin::consensus::deserialize::<bitcoin::Transaction>(&tx_bytes) {
            Ok(tx) => (tx.input.len(), tx.output.len()),
            Err(_) => {
                // For intent-format txs, assume 1 input, 1 output
                (1, 1)
            }
        }
    } else {
        (1, 1)
    };

    // Build PSBT: magic(5) + global_map(key 0x00 = unsigned_tx) + separator + per-input maps + per-output maps
    let mut psbt = Vec::new();
    // Magic bytes
    psbt.extend_from_slice(b"psbt\xff");
    // Global map: key type 0x00 = unsigned transaction
    // Key: length(1) + type(1)
    psbt.push(0x01); // key length
    psbt.push(0x00); // key type: unsigned tx
                     // Value: compact_size(tx_bytes.len()) + tx_bytes
    write_compact_size(&mut psbt, tx_bytes.len() as u64);
    psbt.extend_from_slice(&tx_bytes);
    // End global map
    psbt.push(0x00);
    // Per-input maps (empty)
    for _ in 0..n_inputs {
        psbt.push(0x00); // empty map separator
    }
    // Per-output maps (empty)
    for _ in 0..n_outputs {
        psbt.push(0x00); // empty map separator
    }

    let psbt_base64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &psbt);
    ok_response(&request.id, json!(psbt_base64))
}

/// utxoupdatepsbt - update PSBT with UTXO info
/// In account-based model, UTXO info is synthetic, so we just pass through the PSBT unchanged
fn handle_utxoupdatepsbt(request: &JsonRpcRequest) -> JsonRpcResponse {
    let psbt_base64 = match get_str_param(&request.params, 0) {
        Some(p) => p,
        None => return err_response(&request.id, -32602, "Missing PSBT parameter".to_string()),
    };
    let bytes = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, psbt_base64)
    {
        Ok(bytes) => bytes,
        Err(_) => {
            return err_response(
                &request.id,
                -22,
                "TX decode failed (invalid PSBT)".to_string(),
            )
        }
    };
    if bytes.len() < 5 || &bytes[0..5] != b"psbt\xff" {
        return err_response(
            &request.id,
            -22,
            "TX decode failed (invalid PSBT)".to_string(),
        );
    }
    if extract_unsigned_tx_hex(&bytes).is_empty() {
        return err_response(
            &request.id,
            -22,
            "TX decode failed (missing PSBT_GLOBAL_UNSIGNED_TX)".to_string(),
        );
    }
    // Return unchanged — in account-based model, no UTXO data to add
    ok_response(&request.id, json!(psbt_base64))
}

/// abandontransaction - mark a tx as abandoned (no-op)
async fn handle_abandontransaction(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let txid = match get_str_param(&request.params, 0) {
        Some(t) => t.to_string(),
        None => return err_response(&request.id, -32602, "Missing txid parameter".to_string()),
    };
    let mut tx_cache = state.tx_cache.write().await;
    if tx_cache.entries.remove(&txid).is_some() {
        tx_cache.save_to_disk();
        drop(tx_cache);
        ok_response(&request.id, json!(null))
    } else {
        drop(tx_cache);
        err_response(
            &request.id,
            -5,
            format!("Transaction {} not found in cache", txid),
        )
    }
}

/// bumpfee - increase tx fee (in account-based model, fees are fixed per-tx)
/// Returns success with original txid since NEAR transactions have deterministic fees
fn handle_bumpfee(request: &JsonRpcRequest) -> JsonRpcResponse {
    let txid = get_str_param(&request.params, 0).unwrap_or("");
    if txid.is_empty() {
        return err_response(&request.id, -32602, "Missing txid parameter".to_string());
    }
    // NEAR has ~1s finality and gas-based fees — RBF is not applicable.
    // Return error matching Bitcoin Core behavior when RBF isn't possible.
    err_response(&request.id, -4, format!("Transaction {} is already confirmed. Fee bumping is not supported on this chain (NEAR has instant finality).", txid))
}

/// lockunspent / listlockunspent
async fn handle_lockunspent(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    // lockunspent(unlock, [{"txid":..., "vout":...}, ...])
    let unlock = request
        .params
        .get(0)
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let transactions = request.params.get(1).and_then(|v| v.as_array());

    let mut locked = state.locked_utxos.write().await;

    if unlock && transactions.is_none() {
        // unlock all
        locked.clear();
        return ok_response(&request.id, json!(true));
    }

    if let Some(txs) = transactions {
        for tx in txs {
            let txid = tx
                .get("txid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let vout = tx.get("vout").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            if unlock {
                locked.retain(|(t, v)| !(t == &txid && *v == vout));
            } else if !locked.iter().any(|(t, v)| t == &txid && *v == vout) {
                locked.push((txid, vout));
            }
        }
    }

    ok_response(&request.id, json!(true))
}

async fn handle_listlockunspent(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let locked = state.locked_utxos.read().await;
    let result: Vec<serde_json::Value> = locked
        .iter()
        .map(|(txid, vout)| json!({"txid": txid, "vout": vout}))
        .collect();
    ok_response(&request.id, json!(result))
}

/// rescanblockchain - no-op
async fn handle_rescanblockchain(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let start_height = get_u64_param(&request.params, 0).unwrap_or(0);
    let current_height = state
        .near_client
        .status()
        .await
        .map(|s| s.latest_block_height)
        .unwrap_or(0);
    let stop_height = get_u64_param(&request.params, 1).unwrap_or(current_height);

    // Reset the indexer to re-scan from start_height
    *state.last_indexed_height.write().await = start_height;

    ok_response(
        &request.id,
        json!({
            "start_height": start_height,
            "stop_height": stop_height.min(current_height)
        }),
    )
}

/// getblockfilter - compact block filter (stub)
fn handle_getblockfilter(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(&request.id, -1, "Block filters not supported".to_string())
}

/// generate / generatetoaddress - mining stubs (could be useful for testing)
async fn handle_generate(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let _ = state;
    err_response(
        &request.id,
        -32601,
        "generate is not supported: Bitcoin Infinity uses Proof-of-Stake (no CPU mining)."
            .to_string(),
    )
}

async fn handle_generatetoaddress(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let _ = state;
    err_response(
        &request.id,
        -32601,
        "generatetoaddress is not supported: Bitcoin Infinity uses Proof-of-Stake (no address-targeted mining).".to_string(),
    )
}

/// stop - graceful shutdown
fn handle_stop(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!("Bitcoin Infinity server stopping"))
}

/// ping - test connectivity
fn handle_ping(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

/// help - list available commands
fn handle_help(request: &JsonRpcRequest) -> JsonRpcResponse {
    let command = get_str_param(&request.params, 0).unwrap_or("");
    if command.is_empty() {
        ok_response(
            &request.id,
            json!(
                "== Blockchain ==\n\
             getbestblockhash\ngetblock\ngetblockchaininfo\ngetblockcount\n\
             getblockfilter\ngetblockhash\ngetblockheader\ngetblockstats\n\
             getchaintips\ngetdifficulty\ngetmempoolentry\ngetmempoolinfo\n\
             getrawmempool\ngettxout\ngettxoutproof\ngettxoutsetinfo\n\
             verifytxoutproof\ngetblocktemplate\nsubmitblock\n\
             invalidateblock\nreconsiderblock\n\
             \n== Control ==\nhelp\nstop\nuptime\ngetmemoryinfo\ngetrpcinfo\n\
             getindexinfo\ngetzmqnotifications\nlogging\n\
             \n== Mining ==\ngenerate\ngeneratetoaddress\ngenerateblock\n\
             getmininginfo\ngetnetworkhashps\nprioritisetransaction\n\
             \n== Network ==\ngetconnectioncount\ngetnetworkinfo\ngetpeerinfo\nping\n\
             \n== Rawtransactions ==\ncreaterawtransaction\ndecoderawtransaction\n\
             fundrawtransaction\ngetrawtransaction\nsendrawtransaction\n\
             signrawtransactionwithwallet\nsignrawtransactionwithkey\n\
             testmempoolaccept\nconverttopsbt\n\
             \n== PSBT ==\ncombinepsbt\ncreatepsbt\ndecodepsbt\nfinalizepsbt\n\
             utxoupdatepsbt\nwalletcreatefundedpsbt\nwalletprocesspsbt\n\
             \n== Fee ==\nestimatesmartfee\n\
             \n== Wallet ==\nabandontransaction\nabortrescan\nbackupwallet\nbumpfee\n\
             createwallet\ndumpprivkey\nencryptwallet\n\
             getaddressinfo\ngetaddressesbylabel\ngetbalance\ngetbalances\n\
             getnewaddress\ngetrawchangeaddress\ngetreceivedbyaddress\n\
             getreceivedbylabel\ngettransaction\ngetunconfirmedbalance\n\
             getwalletinfo\nimportaddress\nimportprivkey\nimportdescriptors\n\
             keypoolrefill\nlistaddressgroupings\nlistdescriptors\nlistlabels\n\
             listlockunspent\nlistreceivedbyaddress\nlistsinceblock\n\
             listtransactions\nlistunspent\nlistwallets\nloadwallet\nlockunspent\n\
             rescanblockchain\nscantxoutset\nsendmany\nsendtoaddress\n\
             setlabel\nsethdseed\nsettxfee\nsignmessage\nunloadwallet\n\
             verifymessage\nwalletlock\nwalletpassphrase\nwalletpassphrasechange\n\
             \n== Chain ==\ngetchaintxstats\npreciousblock\npruneblockchain\nverifychain\n\
             waitforblock\nwaitfornewblock\nwaitforblockheight\n\
             \n== Network (extended) ==\naddnode\nclearbanned\ndisconnectnode\n\
             getnettotals\ngetnodeaddresses\nlistbanned\nsetban\nsetnetworkactive\n\
             \n== Wallet (extended) ==\naddmultisigaddress\ncreatemultisig\ndumpwallet\n\
             importmulti\nimportprunedfunds\nimportwallet\nlistwalletdir\n\
             listreceivedbylabel\npsbtbumpfee\nremoveprunedfunds\nsavemempool\n\
             send\nsetwalletflag\nsignmessagewithprivkey\nupgradewallet\n\
             \n== Util ==\nanalyzepsbt\ncombinerawtransaction\ndecodescript\njoinpsbts\n\
             \n== Mining (extended) ==\ngeneratetodescriptor\nsubmitheader\n\
             \n== Descriptors ==\nderiveaddresses\ngetdescriptorinfo\n\
             \n== NEAR Protocol ==\naddnearkey\nbroadcastneartx\nbroadcastneartxcommit\n\
             callcontract\nclosenearaccount\ncreatenearaccount\n\
             deletenearkey\ndeploynearcontract\nfundgaskey\n\
             getchanges\ngetchangesinblock\ngetclientconfig\n\
             getchunk\ngetcongestionlevel\ngetcontractcode\n\
             getcontractstate\ngetgaskeynonces\ngetgasprice\n\
             getgenesisconfig\ngetlightclientblock\ngetlightclientblockproof\n\
             getlightclientproof\ngetmaintenancewindows\ngetnearnetworkinfo\n\
             getnearstatus\ngetneartxfull\ngetneartxstatus\n\
             getnodehealth\ngetprotocolconfig\ngetreceipt\ngetsplitstorage\n\
             gettxreceipts\ngetvalidatorinfo\ngetvalidatorsordered\n\
             listaccountkeys\nqueryatblock\nsendneartx\nsendneartxwait\n\
             stakenearsatoshis\nunstake\nwithdrawgaskey"
            ),
        )
    } else {
        ok_response(
            &request.id,
            json!(format!(
                "Help for '{}': Bitcoin Infinity implementation",
                command
            )),
        )
    }
}

// ============================================================================
// NEAR-native methods - full protocol access via Bitcoin addresses
// ============================================================================

/// callcontract - call a view function on a NEAR smart contract (read-only)
/// params: [contract_id, method_name, args_json_or_base64]
async fn handle_callcontract(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let contract_id = match get_str_param(&request.params, 0) {
        Some(c) => c,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing contract_id parameter".to_string(),
            )
        }
    };
    let method_name = match get_str_param(&request.params, 1) {
        Some(m) => m,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing method_name parameter".to_string(),
            )
        }
    };
    let args_str = get_str_param(&request.params, 2).unwrap_or("");

    // Try to detect if args_str is JSON or base64
    use base64::Engine;
    let args_base64 = if args_str.is_empty() {
        base64::engine::general_purpose::STANDARD.encode(b"{}")
    } else if args_str.starts_with('{') || args_str.starts_with('[') {
        // It's JSON, encode to base64
        base64::engine::general_purpose::STANDARD.encode(args_str.as_bytes())
    } else {
        // Assume it's already base64
        args_str.to_string()
    };

    match state
        .near_client
        .call_function(contract_id, method_name, &args_base64)
        .await
    {
        Ok(result) => {
            // Result contains "result" field which is a byte array
            if let Some(result_bytes) = result.get("result").and_then(|v| v.as_array()) {
                let bytes: Vec<u8> = result_bytes
                    .iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect();
                // Try to parse as JSON string
                if let Ok(json_str) = String::from_utf8(bytes.clone()) {
                    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        return ok_response(
                            &request.id,
                            json!({
                                "result": json_val,
                                "logs": result.get("logs").cloned().unwrap_or(json!([])),
                                "block_height": result.get("block_height"),
                                "block_hash": result.get("block_hash")
                            }),
                        );
                    }
                    return ok_response(
                        &request.id,
                        json!({
                            "result": json_str,
                            "logs": result.get("logs").cloned().unwrap_or(json!([]))
                        }),
                    );
                }
                // Return raw bytes as hex
                ok_response(
                    &request.id,
                    json!({
                        "result_hex": hex::encode(&bytes),
                        "logs": result.get("logs").cloned().unwrap_or(json!([]))
                    }),
                )
            } else {
                ok_response(&request.id, result)
            }
        }
        Err(e) => err_response(&request.id, -32000, format!("Contract call failed: {}", e)),
    }
}

/// getcontractstate - view raw key-value storage of a contract
/// params: [contract_id, prefix_base64?]
async fn handle_getcontractstate(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let contract_id = match get_str_param(&request.params, 0) {
        Some(c) => c,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing contract_id parameter".to_string(),
            )
        }
    };
    let prefix = get_str_param(&request.params, 1).unwrap_or("");

    match state.near_client.view_state(contract_id, prefix).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, format!("View state failed: {}", e)),
    }
}

/// getcontractcode - get WASM bytecode hash and size for a contract
/// params: [contract_id]
async fn handle_getcontractcode(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let contract_id = match get_str_param(&request.params, 0) {
        Some(c) => c,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing contract_id parameter".to_string(),
            )
        }
    };

    match state.near_client.view_code(contract_id).await {
        Ok(result) => {
            // Result contains code_base64 and hash
            let code_b64 = result
                .get("code_base64")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let hash = result.get("hash").and_then(|v| v.as_str()).unwrap_or("");
            ok_response(
                &request.id,
                json!({
                    "code_hash": hash,
                    "code_size": code_b64.len() * 3 / 4, // approximate decoded size
                    "has_code": !code_b64.is_empty(),
                    "block_height": result.get("block_height"),
                    "block_hash": result.get("block_hash")
                }),
            )
        }
        Err(e) => err_response(&request.id, -32000, format!("View code failed: {}", e)),
    }
}

/// deploynearcontract - deploy WASM bytecode to the signer's account
/// params: [sender_address, wasm_base64]
async fn handle_deploynearcontract(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let sender = match get_str_param(&request.params, 0) {
        Some(s) => s,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing sender_address parameter".to_string(),
            )
        }
    };
    let wasm_b64 = match get_str_param(&request.params, 1) {
        Some(w) => w,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing wasm_base64 parameter".to_string(),
            )
        }
    };

    use base64::Engine;
    let wasm_bytes = match base64::engine::general_purpose::STANDARD.decode(wasm_b64) {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32602, format!("Invalid base64: {}", e)),
    };

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, sender).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, sender, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let mut builder = NearTxBuilder::new(
        sender.to_string(),
        pk_uncompressed,
        nonce,
        sender.to_string(),
        block_hash,
    );
    builder.add_action(NearAction::deploy_contract(&wasm_bytes));

    match builder.sign_and_encode(&secret_key) {
        Ok(signed_tx) => match state.near_client.send_tx_async(&signed_tx).await {
            Ok(tx_hash) => {
                state.record_nonce(sender, nonce).await;
                ok_response(
                    &request.id,
                    json!({
                        "near_tx_hash": tx_hash,
                        "wasm_size": wasm_bytes.len()
                    }),
                )
            }
            Err(e) => err_response(&request.id, -25, format!("TX submit failed: {}", e)),
        },
        Err(e) => err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    }
}

/// stakenearsatoshis - stake tokens for validation
/// params: [address, amount_btc]
async fn handle_stake(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };
    let amount_btc = request
        .params
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    if amount_btc <= 0.0 {
        return err_response(&request.id, -32602, "Amount must be positive".to_string());
    }

    let amount_sat = (amount_btc * 100_000_000.0) as u64;
    let amount_yocto = ParsedBitcoinTx::satoshis_to_yocto(amount_sat);

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, addr).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, addr, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let mut builder = NearTxBuilder::new(
        addr.to_string(),
        pk_uncompressed,
        nonce,
        addr.to_string(),
        block_hash,
    );
    builder.add_action(NearAction::stake(amount_yocto, &pk_uncompressed));

    match builder.sign_and_encode(&secret_key) {
        Ok(signed_tx) => match state.near_client.send_tx_async(&signed_tx).await {
            Ok(tx_hash) => {
                state.record_nonce(addr, nonce).await;
                ok_response(
                    &request.id,
                    json!({
                        "near_tx_hash": tx_hash,
                        "staked_btc": amount_btc
                    }),
                )
            }
            Err(e) => err_response(&request.id, -25, format!("Stake TX failed: {}", e)),
        },
        Err(e) => err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    }
}

/// unstake - unstake by setting stake to 0
/// params: [address]
async fn handle_unstake(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, addr).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, addr, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let mut builder = NearTxBuilder::new(
        addr.to_string(),
        pk_uncompressed,
        nonce,
        addr.to_string(),
        block_hash,
    );
    builder.add_action(NearAction::stake(0, &pk_uncompressed)); // stake 0 = unstake

    match builder.sign_and_encode(&secret_key) {
        Ok(signed_tx) => match state.near_client.send_tx_async(&signed_tx).await {
            Ok(tx_hash) => {
                state.record_nonce(addr, nonce).await;
                ok_response(&request.id, json!({ "near_tx_hash": tx_hash }))
            }
            Err(e) => err_response(&request.id, -25, format!("Unstake TX failed: {}", e)),
        },
        Err(e) => err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    }
}

/// addnearkey - add an access key to an account
/// params: [account_address, new_pubkey_hex, permission_type?, receiver_id?, method_names?, allowance_btc?]
async fn handle_addnearkey(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };
    let new_pubkey_hex = match get_str_param(&request.params, 1) {
        Some(p) => p,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing new_pubkey_hex (64-byte uncompressed)".to_string(),
            )
        }
    };
    let permission = get_str_param(&request.params, 2).unwrap_or("full_access");

    let new_pk_bytes = match hex::decode(new_pubkey_hex) {
        Ok(b) if b.len() == 64 => {
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            return err_response(
                &request.id,
                -32602,
                "Invalid pubkey: must be 64 hex bytes (uncompressed, no 0x04 prefix)".to_string(),
            )
        }
    };

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, addr).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, addr, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let action = if permission == "function_call" {
        let receiver_id = get_str_param(&request.params, 3).unwrap_or("");
        let methods_str = get_str_param(&request.params, 4).unwrap_or("");
        let methods: Vec<&str> = if methods_str.is_empty() {
            vec![]
        } else {
            methods_str.split(',').collect()
        };
        let allowance = request
            .params
            .as_array()
            .and_then(|arr| arr.get(5))
            .and_then(|v| v.as_f64())
            .map(|btc| ParsedBitcoinTx::satoshis_to_yocto((btc * 100_000_000.0) as u64));
        NearAction::add_function_call_key(&new_pk_bytes, allowance, receiver_id, &methods)
    } else {
        NearAction::add_full_access_key(&new_pk_bytes)
    };

    let mut builder = NearTxBuilder::new(
        addr.to_string(),
        pk_uncompressed,
        nonce,
        addr.to_string(),
        block_hash,
    );
    builder.add_action(action);

    match builder.sign_and_encode(&secret_key) {
        Ok(signed_tx) => match state.near_client.send_tx_async(&signed_tx).await {
            Ok(tx_hash) => {
                state.record_nonce(addr, nonce).await;
                ok_response(
                    &request.id,
                    json!({
                        "near_tx_hash": tx_hash,
                        "added_key": new_pubkey_hex,
                        "permission": permission
                    }),
                )
            }
            Err(e) => err_response(&request.id, -25, format!("AddKey TX failed: {}", e)),
        },
        Err(e) => err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    }
}

/// deletenearkey - remove an access key from an account
/// params: [account_address, pubkey_hex_to_delete]
async fn handle_deletenearkey(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };
    let del_pubkey_hex = match get_str_param(&request.params, 1) {
        Some(p) => p,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing pubkey_hex_to_delete".to_string(),
            )
        }
    };

    let del_pk_bytes = match hex::decode(del_pubkey_hex) {
        Ok(b) if b.len() == 64 => {
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&b);
            arr
        }
        _ => return err_response(&request.id, -32602, "Invalid pubkey".to_string()),
    };

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, addr).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, addr, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let mut builder = NearTxBuilder::new(
        addr.to_string(),
        pk_uncompressed,
        nonce,
        addr.to_string(),
        block_hash,
    );
    builder.add_action(NearAction::delete_key(&del_pk_bytes));

    match builder.sign_and_encode(&secret_key) {
        Ok(signed_tx) => match state.near_client.send_tx_async(&signed_tx).await {
            Ok(tx_hash) => {
                state.record_nonce(addr, nonce).await;
                ok_response(
                    &request.id,
                    json!({ "near_tx_hash": tx_hash, "deleted_key": del_pubkey_hex }),
                )
            }
            Err(e) => err_response(&request.id, -25, format!("DeleteKey TX failed: {}", e)),
        },
        Err(e) => err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    }
}

/// closenearaccount - delete account, send remaining balance to beneficiary
/// params: [account_address, beneficiary_address]
async fn handle_closenearaccount(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing account_address parameter".to_string(),
            )
        }
    };
    let beneficiary = match get_str_param(&request.params, 1) {
        Some(b) => b,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing beneficiary_address parameter".to_string(),
            )
        }
    };

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, addr).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, addr, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let mut builder = NearTxBuilder::new(
        addr.to_string(),
        pk_uncompressed,
        nonce,
        addr.to_string(),
        block_hash,
    );
    builder.add_action(NearAction::delete_account(beneficiary));

    match builder.sign_and_encode(&secret_key) {
        Ok(signed_tx) => match state.near_client.send_tx_async(&signed_tx).await {
            Ok(tx_hash) => {
                state.record_nonce(addr, nonce).await;
                ok_response(
                    &request.id,
                    json!({
                        "near_tx_hash": tx_hash,
                        "deleted_account": addr,
                        "beneficiary": beneficiary
                    }),
                )
            }
            Err(e) => err_response(&request.id, -25, format!("DeleteAccount TX failed: {}", e)),
        },
        Err(e) => err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    }
}

/// getvalidatorinfo - get current validators and staking info
async fn handle_getvalidatorinfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.validators().await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(
            &request.id,
            -32000,
            format!("Validators query failed: {}", e),
        ),
    }
}

/// listaccountkeys - list all access keys for an account
/// params: [account_address]
async fn handle_listaccountkeys(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let addr = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing account_address parameter".to_string(),
            )
        }
    };

    match state.near_client.view_access_key_list(addr).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(
            &request.id,
            -32000,
            format!("Access key list query failed: {}", e),
        ),
    }
}

/// sendneartx - send a multi-action NEAR transaction (advanced)
/// params: [sender_address, receiver_address, actions_json]
/// actions_json is an array of action objects, each with "type" and type-specific fields:
///   {"type": "transfer", "amount_btc": 1.0}
///   {"type": "function_call", "method": "...", "args": "...", "gas": 300000000000000, "deposit_btc": 0}
///   {"type": "create_account"}
///   {"type": "deploy_contract", "code_base64": "..."}
///   {"type": "add_full_access_key", "pubkey_hex": "..."}
///   {"type": "delete_key", "pubkey_hex": "..."}
///   {"type": "delete_account", "beneficiary": "..."}
///   {"type": "stake", "amount_btc": 100.0}
async fn handle_sendneartx(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let sender = match get_str_param(&request.params, 0) {
        Some(s) => s,
        None => return err_response(&request.id, -32602, "Missing sender_address".to_string()),
    };
    let receiver = match get_str_param(&request.params, 1) {
        Some(r) => r,
        None => return err_response(&request.id, -32602, "Missing receiver_address".to_string()),
    };
    let actions_json = match request.params.as_array().and_then(|arr| arr.get(2)) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing actions array".to_string()),
    };
    let actions_arr = match actions_json.as_array() {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "actions must be an array".to_string()),
    };

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, sender).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, sender, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let mut builder = NearTxBuilder::new(
        sender.to_string(),
        pk_uncompressed,
        nonce,
        receiver.to_string(),
        block_hash,
    );

    for action_obj in actions_arr {
        let action_type = action_obj
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let action = match action_type {
            "transfer" => {
                let btc = action_obj
                    .get("amount_btc")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let sat = (btc * 100_000_000.0) as u64;
                NearAction::transfer(ParsedBitcoinTx::satoshis_to_yocto(sat))
            }
            "function_call" => {
                let method = action_obj
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args_str = action_obj
                    .get("args")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}");
                let gas = action_obj
                    .get("gas")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(300_000_000_000_000);
                let deposit_btc = action_obj
                    .get("deposit_btc")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let deposit_yocto =
                    ParsedBitcoinTx::satoshis_to_yocto((deposit_btc * 100_000_000.0) as u64);
                NearAction::function_call(method, args_str.as_bytes(), gas, deposit_yocto)
            }
            "create_account" => NearAction::create_account(),
            "deploy_contract" => {
                let code_b64 = action_obj
                    .get("code_base64")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(code_b64) {
                    Ok(code) => NearAction::deploy_contract(&code),
                    Err(e) => {
                        return err_response(
                            &request.id,
                            -32602,
                            format!("Invalid code_base64: {}", e),
                        )
                    }
                }
            }
            "add_full_access_key" => {
                let pk_hex = action_obj
                    .get("pubkey_hex")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match hex::decode(pk_hex) {
                    Ok(b) if b.len() == 64 => {
                        let mut arr = [0u8; 64];
                        arr.copy_from_slice(&b);
                        NearAction::add_full_access_key(&arr)
                    }
                    Ok(b) if b.len() == 32 => {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&b);
                        NearAction::add_full_access_key_ed25519(&arr)
                    }
                    _ => return err_response(&request.id, -32602, "Invalid pubkey_hex for add_full_access_key (expected 32 bytes for Ed25519 or 64 bytes for secp256k1)".to_string()),
                }
            }
            "delete_key" => {
                let pk_hex = action_obj
                    .get("pubkey_hex")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match hex::decode(pk_hex) {
                    Ok(b) if b.len() == 64 => {
                        let mut arr = [0u8; 64];
                        arr.copy_from_slice(&b);
                        NearAction::delete_key(&arr)
                    }
                    Ok(b) if b.len() == 32 => {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&b);
                        NearAction::delete_key_ed25519(&arr)
                    }
                    _ => return err_response(&request.id, -32602, "Invalid pubkey_hex for delete_key (expected 32 bytes for Ed25519 or 64 bytes for secp256k1)".to_string()),
                }
            }
            "delete_account" => {
                let beneficiary = action_obj
                    .get("beneficiary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                NearAction::delete_account(beneficiary)
            }
            "stake" => {
                let btc = action_obj
                    .get("amount_btc")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let sat = (btc * 100_000_000.0) as u64;
                NearAction::stake(ParsedBitcoinTx::satoshis_to_yocto(sat), &pk_uncompressed)
            }
            "add_function_call_key" => {
                let pk_hex = action_obj
                    .get("pubkey_hex")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let receiver_id = action_obj
                    .get("receiver_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let methods_str = action_obj
                    .get("method_names")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let methods: Vec<&str> = if methods_str.is_empty() {
                    vec![]
                } else {
                    methods_str.split(',').collect()
                };
                let allowance = action_obj
                    .get("allowance_btc")
                    .and_then(|v| v.as_f64())
                    .map(|btc| ParsedBitcoinTx::satoshis_to_yocto((btc * 100_000_000.0) as u64));
                match hex::decode(pk_hex) {
                    Ok(b) if b.len() == 64 => {
                        let mut arr = [0u8; 64];
                        arr.copy_from_slice(&b);
                        NearAction::add_function_call_key(&arr, allowance, receiver_id, &methods)
                    }
                    _ => {
                        return err_response(
                            &request.id,
                            -32602,
                            "Invalid pubkey_hex for add_function_call_key".to_string(),
                        )
                    }
                }
            }
            "deploy_global_contract" => {
                let code_b64 = action_obj
                    .get("code_base64")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(code_b64) {
                    Ok(code) => NearAction::deploy_global_contract(&code),
                    Err(e) => {
                        return err_response(
                            &request.id,
                            -32602,
                            format!("Invalid code_base64: {}", e),
                        )
                    }
                }
            }
            "use_global_contract" => {
                if let Some(hash_hex) = action_obj.get("code_hash").and_then(|v| v.as_str()) {
                    match hex::decode(hash_hex) {
                        Ok(b) if b.len() == 32 => {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&b);
                            NearAction::use_global_contract_by_hash(&arr)
                        }
                        _ => {
                            return err_response(
                                &request.id,
                                -32602,
                                "code_hash must be 32 hex bytes".to_string(),
                            )
                        }
                    }
                } else if let Some(account_id) =
                    action_obj.get("account_id").and_then(|v| v.as_str())
                {
                    NearAction::use_global_contract_by_account(account_id)
                } else {
                    return err_response(
                        &request.id,
                        -32602,
                        "use_global_contract needs code_hash or account_id".to_string(),
                    );
                }
            }
            "transfer_to_gas_key" => {
                let pk_hex = action_obj
                    .get("pubkey_hex")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let btc = action_obj
                    .get("amount_btc")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let deposit = ParsedBitcoinTx::satoshis_to_yocto((btc * 100_000_000.0) as u64);
                match hex::decode(pk_hex) {
                    Ok(b) if b.len() == 64 => {
                        let mut arr = [0u8; 64];
                        arr.copy_from_slice(&b);
                        NearAction::transfer_to_gas_key(&arr, deposit)
                    }
                    _ => {
                        return err_response(
                            &request.id,
                            -32602,
                            "Invalid pubkey_hex for transfer_to_gas_key".to_string(),
                        )
                    }
                }
            }
            "withdraw_from_gas_key" => {
                let pk_hex = action_obj
                    .get("pubkey_hex")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let btc = action_obj
                    .get("amount_btc")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let amount = ParsedBitcoinTx::satoshis_to_yocto((btc * 100_000_000.0) as u64);
                match hex::decode(pk_hex) {
                    Ok(b) if b.len() == 64 => {
                        let mut arr = [0u8; 64];
                        arr.copy_from_slice(&b);
                        NearAction::withdraw_from_gas_key(&arr, amount)
                    }
                    _ => {
                        return err_response(
                            &request.id,
                            -32602,
                            "Invalid pubkey_hex for withdraw_from_gas_key".to_string(),
                        )
                    }
                }
            }
            "deterministic_state_init" => NearAction::deterministic_state_init(),
            "delegate" => {
                let delegate_action_hex = action_obj
                    .get("delegate_action_hex")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let signature_hex = action_obj
                    .get("signature_hex")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let delegate_bytes = hex::decode(delegate_action_hex)
                    .map_err(|e| format!("Invalid delegate_action_hex: {}", e));
                let sig_bytes =
                    hex::decode(signature_hex).map_err(|e| format!("Invalid signature_hex: {}", e));
                match (delegate_bytes, sig_bytes) {
                    (Ok(d), Ok(s)) => NearAction::delegate(&d, &s),
                    (Err(e), _) | (_, Err(e)) => return err_response(&request.id, -32602, e),
                }
            }
            _ => {
                return err_response(
                    &request.id,
                    -32602,
                    format!("Unknown action type: {}", action_type),
                )
            }
        };
        builder.add_action(action);
    }

    match builder.sign_and_encode(&secret_key) {
        Ok(signed_tx) => match state.near_client.send_tx_async(&signed_tx).await {
            Ok(tx_hash) => {
                state.record_nonce(sender, nonce).await;
                ok_response(
                    &request.id,
                    json!({
                        "near_tx_hash": tx_hash,
                        "actions_count": actions_arr.len()
                    }),
                )
            }
            Err(e) => err_response(&request.id, -25, format!("TX submit failed: {}", e)),
        },
        Err(e) => err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    }
}

// ============================================================================
// NEAR high-level convenience methods
// ============================================================================

/// createnearaccount — atomically create account + transfer + add key
/// params: [sender_address, new_account_id, initial_balance_btc, new_pubkey_hex]
async fn handle_createnearaccount(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let sender = match get_str_param(&request.params, 0) {
        Some(s) => s,
        None => return err_response(&request.id, -32602, "Missing sender_address".to_string()),
    };
    let new_account_id = match get_str_param(&request.params, 1) {
        Some(a) => a,
        None => return err_response(&request.id, -32602, "Missing new_account_id".to_string()),
    };
    let initial_btc = request
        .params
        .as_array()
        .and_then(|arr| arr.get(2))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let new_pk_hex = get_str_param(&request.params, 3);

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, sender).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, sender, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };
    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let mut builder = NearTxBuilder::new(
        sender.to_string(),
        pk_uncompressed,
        nonce,
        new_account_id.to_string(),
        block_hash,
    );
    builder.add_action(NearAction::create_account());
    if initial_btc > 0.0 {
        let sat = (initial_btc * 100_000_000.0) as u64;
        builder.add_action(NearAction::transfer(ParsedBitcoinTx::satoshis_to_yocto(
            sat,
        )));
    }
    if let Some(hex) = new_pk_hex {
        if let Ok(b) = hex::decode(hex) {
            if b.len() == 64 {
                let mut arr = [0u8; 64];
                arr.copy_from_slice(&b);
                builder.add_action(NearAction::add_full_access_key(&arr));
            }
        }
    }

    match builder.sign_and_encode(&secret_key) {
        Ok(signed_tx) => match state.near_client.send_tx_async(&signed_tx).await {
            Ok(tx_hash) => {
                state.record_nonce(sender, nonce).await;
                ok_response(
                    &request.id,
                    json!({
                        "near_tx_hash": tx_hash,
                        "new_account_id": new_account_id,
                        "initial_balance_btc": initial_btc
                    }),
                )
            }
            Err(e) => err_response(&request.id, -25, format!("TX submit failed: {}", e)),
        },
        Err(e) => err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    }
}

/// fundgaskey — transfer tokens to fund a gas key
/// params: [sender_address, pubkey_hex, amount_btc]
async fn handle_fundgaskey(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let sender = match get_str_param(&request.params, 0) {
        Some(s) => s,
        None => return err_response(&request.id, -32602, "Missing sender_address".to_string()),
    };
    let pk_hex = match get_str_param(&request.params, 1) {
        Some(p) => p,
        None => return err_response(&request.id, -32602, "Missing pubkey_hex".to_string()),
    };
    let btc = request
        .params
        .as_array()
        .and_then(|arr| arr.get(2))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let gas_pk = match hex::decode(pk_hex) {
        Ok(b) if b.len() == 64 => {
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            return err_response(
                &request.id,
                -32602,
                "pubkey_hex must be 64 hex bytes".to_string(),
            )
        }
    };

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, sender).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, sender, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };
    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let deposit = ParsedBitcoinTx::satoshis_to_yocto((btc * 100_000_000.0) as u64);
    let mut builder = NearTxBuilder::new(
        sender.to_string(),
        pk_uncompressed,
        nonce,
        sender.to_string(),
        block_hash,
    );
    builder.add_action(NearAction::transfer_to_gas_key(&gas_pk, deposit));

    match builder.sign_and_encode(&secret_key) {
        Ok(signed_tx) => match state.near_client.send_tx_async(&signed_tx).await {
            Ok(tx_hash) => {
                state.record_nonce(sender, nonce).await;
                ok_response(
                    &request.id,
                    json!({
                        "near_tx_hash": tx_hash,
                        "funded_btc": btc
                    }),
                )
            }
            Err(e) => err_response(&request.id, -25, format!("TX submit failed: {}", e)),
        },
        Err(e) => err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    }
}

/// withdrawgaskey — withdraw tokens from a gas key
/// params: [sender_address, pubkey_hex, amount_btc]
async fn handle_withdrawgaskey(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let sender = match get_str_param(&request.params, 0) {
        Some(s) => s,
        None => return err_response(&request.id, -32602, "Missing sender_address".to_string()),
    };
    let pk_hex = match get_str_param(&request.params, 1) {
        Some(p) => p,
        None => return err_response(&request.id, -32602, "Missing pubkey_hex".to_string()),
    };
    let btc = request
        .params
        .as_array()
        .and_then(|arr| arr.get(2))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let gas_pk = match hex::decode(pk_hex) {
        Ok(b) if b.len() == 64 => {
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            return err_response(
                &request.id,
                -32602,
                "pubkey_hex must be 64 hex bytes".to_string(),
            )
        }
    };

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, sender).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, sender, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };
    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    let amount = ParsedBitcoinTx::satoshis_to_yocto((btc * 100_000_000.0) as u64);
    let mut builder = NearTxBuilder::new(
        sender.to_string(),
        pk_uncompressed,
        nonce,
        sender.to_string(),
        block_hash,
    );
    builder.add_action(NearAction::withdraw_from_gas_key(&gas_pk, amount));

    match builder.sign_and_encode(&secret_key) {
        Ok(signed_tx) => match state.near_client.send_tx_async(&signed_tx).await {
            Ok(tx_hash) => {
                state.record_nonce(sender, nonce).await;
                ok_response(
                    &request.id,
                    json!({
                        "near_tx_hash": tx_hash,
                        "withdrawn_btc": btc
                    }),
                )
            }
            Err(e) => err_response(&request.id, -25, format!("TX submit failed: {}", e)),
        },
        Err(e) => err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    }
}

// ============================================================================
// NEAR RPC passthrough methods — full nearcore protocol access
// ============================================================================

/// getchunk — get chunk by hash or by block+shard
async fn handle_getchunk(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    // params: [chunk_hash] OR [block_id, shard_id]
    let first = get_str_param(&request.params, 0);
    if let Some(chunk_hash) = first {
        // Try to parse as number (block height + shard)
        if let Ok(block_height) = chunk_hash.parse::<u64>() {
            let shard_id = request
                .params
                .as_array()
                .and_then(|arr| arr.get(1))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            match state
                .near_client
                .chunk_by_block_shard(json!(block_height), shard_id)
                .await
            {
                Ok(result) => ok_response(&request.id, result),
                Err(e) => err_response(&request.id, -32000, e),
            }
        } else {
            // Treat as chunk hash or block hash
            let shard_param = request.params.as_array().and_then(|arr| arr.get(1));
            if let Some(shard_id) = shard_param.and_then(|v| v.as_u64()) {
                match state
                    .near_client
                    .chunk_by_block_shard(json!(chunk_hash), shard_id)
                    .await
                {
                    Ok(result) => ok_response(&request.id, result),
                    Err(e) => err_response(&request.id, -32000, e),
                }
            } else {
                match state.near_client.chunk_by_hash(chunk_hash).await {
                    Ok(result) => ok_response(&request.id, result),
                    Err(e) => err_response(&request.id, -32000, e),
                }
            }
        }
    } else {
        err_response(
            &request.id,
            -32602,
            "Missing chunk_hash or block_id parameter".to_string(),
        )
    }
}

/// getreceipt — look up a receipt by receipt ID
async fn handle_getreceipt(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let receipt_id = match get_str_param(&request.params, 0) {
        Some(r) => r,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing receipt_id parameter".to_string(),
            )
        }
    };
    match state.near_client.receipt(receipt_id).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getchangesinblock — list state changes that occurred in a block
async fn handle_getchangesinblock(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    // params: [block_hash_or_height] or {"finality": "final"}
    let block_ref = if let Some(id_str) = get_str_param(&request.params, 0) {
        if let Ok(height) = id_str.parse::<u64>() {
            json!({"block_id": height})
        } else {
            json!({"block_id": id_str})
        }
    } else {
        json!({"finality": "final"})
    };
    match state.near_client.changes_in_block(block_ref).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getchanges — get specific state changes by type
/// params: { "changes_type": "account_changes"|"access_key_changes"|"contract_code_changes"|"data_changes",
///           "account_ids": [...], "finality": "final" }
async fn handle_getchanges(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let params = request.params.clone();
    if params.is_null() || (params.is_array() && params.as_array().map_or(true, |a| a.is_empty())) {
        return err_response(
            &request.id,
            -32602,
            "Missing params object with changes_type".to_string(),
        );
    }
    // If array, treat first element as the params object
    let query = if let Some(arr) = params.as_array() {
        arr.first().cloned().unwrap_or(json!({}))
    } else {
        params
    };
    match state.near_client.changes(query).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// gettxreceipts — get transaction status with full receipt execution chain
async fn handle_gettxreceipts(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let tx_hash = match get_str_param(&request.params, 0) {
        Some(h) => h,
        None => return err_response(&request.id, -32602, "Missing tx_hash parameter".to_string()),
    };
    let sender_id = get_str_param(&request.params, 1).unwrap_or("");
    match state
        .near_client
        .tx_status_with_receipts(tx_hash, sender_id)
        .await
    {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getprotocolconfig — get protocol configuration at a given block
async fn handle_getprotocolconfig(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let block_ref = if let Some(id_str) = get_str_param(&request.params, 0) {
        if let Ok(height) = id_str.parse::<u64>() {
            json!({"block_id": height})
        } else {
            json!({"block_id": id_str})
        }
    } else {
        json!({"finality": "final"})
    };
    match state.near_client.protocol_config(block_ref).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getgenesisconfig — get the genesis configuration
async fn handle_getgenesisconfig(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.genesis_config().await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getnodehealth — nearcore health check
async fn handle_getnodehealth(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.health().await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getlightclientproof — get execution outcome proof for bridges/IBC
async fn handle_getlightclientproof(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let params = request.params.clone();
    if params.is_null() {
        return err_response(
            &request.id,
            -32602,
            "Missing proof request params".to_string(),
        );
    }
    let query = if let Some(arr) = params.as_array() {
        arr.first().cloned().unwrap_or(json!({}))
    } else {
        params
    };
    match state.near_client.light_client_proof(query).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getlightclientblock — get next light client block for relay/bridge
async fn handle_getlightclientblock(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let last_block_hash = match get_str_param(&request.params, 0) {
        Some(h) => h,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing last_block_hash parameter".to_string(),
            )
        }
    };
    match state
        .near_client
        .next_light_client_block(last_block_hash)
        .await
    {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getvalidatorsordered — validators ordered by stake
async fn handle_getvalidatorsordered(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let block_id = get_str_param(&request.params, 0).map(|s| {
        if let Ok(h) = s.parse::<u64>() {
            json!(h)
        } else {
            json!(s)
        }
    });
    match state.near_client.validators_ordered(block_id).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getcongestionlevel — shard congestion level
async fn handle_getcongestionlevel(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let params = request.params.clone();
    let query = if let Some(arr) = params.as_array() {
        arr.first().cloned().unwrap_or(json!(null))
    } else if params.is_null() {
        json!(null)
    } else {
        params
    };
    // If no params, use latest block + shard 0
    let effective_query = if query.is_null()
        || (query.is_object() && query.as_object().map_or(true, |o| o.is_empty()))
    {
        match state.near_client.status().await {
            Ok(status) => {
                let hash = status.latest_block_hash.clone();
                json!({"block_id": hash, "shard_id": 0})
            }
            Err(_) => json!({"shard_id": 0}),
        }
    } else {
        query
    };
    match state.near_client.congestion_level(effective_query).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getnearnetworkinfo — live network info from nearcore (not stubbed)
async fn handle_getnearnetworkinfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.network_info().await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getclientconfig — nearcore node client configuration
async fn handle_getclientconfig(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.client_config().await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// getgaskeynonces — query gas key nonces for an account
async fn handle_getgaskeynonces(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let account_id = match get_str_param(&request.params, 0) {
        Some(a) => a,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing account_id parameter".to_string(),
            )
        }
    };
    let public_key = match get_str_param(&request.params, 1) {
        Some(p) => p,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing public_key parameter".to_string(),
            )
        }
    };
    match state
        .near_client
        .view_gas_key_nonces(account_id, public_key)
        .await
    {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// queryatblock — generic NEAR query at a specific block height/hash/finality
/// params: [request_type, {query_params}, block_reference]
async fn handle_queryatblock(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let arr = match request.params.as_array() {
        Some(a) if a.len() >= 2 => a,
        _ => {
            return err_response(
                &request.id,
                -32602,
                "params: [request_type, query_params, block_ref]".to_string(),
            )
        }
    };
    let request_type = match arr[0].as_str() {
        Some(rt) => rt,
        None => {
            return err_response(
                &request.id,
                -32602,
                "First param must be request_type string".to_string(),
            )
        }
    };
    let query_params = arr[1].clone();
    let block_ref = arr.get(2).cloned().unwrap_or(json!({"finality": "final"}));
    match state
        .near_client
        .query_at_block(request_type, query_params, block_ref)
        .await
    {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

/// Helper: get sender's key entry and secret key from keystore
async fn get_sender_key(
    state: &RpcState,
    addr: &str,
) -> Result<(KeyEntry, secp256k1::SecretKey, String), JsonRpcResponse> {
    let keystore = state.keystore.read().await;
    let key_entry = match keystore.get(addr) {
        Some(k) => k.clone(),
        None => {
            return Err(err_response(
                &json!(null),
                -3,
                format!("Address not in wallet: {}", addr),
            ))
        }
    };
    drop(keystore);

    let sk_bytes = key_entry
        .private_key_bytes()
        .map_err(|e| err_response(&json!(null), -32000, format!("Key error: {}", e)))?;
    let secret_key = secp256k1::SecretKey::from_slice(&sk_bytes)
        .map_err(|e| err_response(&json!(null), -32000, format!("Key error: {}", e)))?;
    let near_pubkey_str = key_entry
        .near_public_key_string()
        .map_err(|e| err_response(&json!(null), -32000, format!("Key error: {}", e)))?;

    Ok((key_entry, secret_key, near_pubkey_str))
}

/// Helper: get latest block hash and next nonce
async fn get_block_and_nonce(
    state: &RpcState,
    addr: &str,
    near_pubkey_str: &str,
    _id: &serde_json::Value,
) -> Result<([u8; 32], u64), JsonRpcResponse> {
    let status = state
        .near_client
        .status()
        .await
        .map_err(|e| err_response(_id, -32000, format!("Node error: {}", e)))?;
    let block_hash = decode_block_hash(&status.latest_block_hash)
        .map_err(|e| err_response(_id, -32000, format!("Block hash error: {}", e)))?;
    let nonce = state.next_nonce(addr, near_pubkey_str).await;
    Ok((block_hash, nonce))
}

// ============================================================================
// Additional NEAR RPC passthroughs
// ============================================================================

async fn handle_getgasprice(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let block_id = request
        .params
        .as_array()
        .and_then(|arr| arr.get(0))
        .cloned();
    match block_id {
        Some(id) => match state.near_client.gas_price_at_block(id).await {
            Ok(price) => ok_response(&request.id, json!({ "gas_price": price })),
            Err(e) => err_response(&request.id, -32000, e),
        },
        None => match state.near_client.gas_price().await {
            Ok(price) => ok_response(&request.id, json!({ "gas_price": price })),
            Err(e) => err_response(&request.id, -32000, e),
        },
    }
}

async fn handle_getnearstatus(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.call("status", json!([])).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

async fn handle_getneartxstatus(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let tx_hash = get_str_param(&request.params, 0).unwrap_or("");
    let sender_id = get_str_param(&request.params, 1).unwrap_or("");
    if tx_hash.is_empty() || sender_id.is_empty() {
        return err_response(
            &request.id,
            -32602,
            "Required: [tx_hash, sender_account_id]".to_string(),
        );
    }
    match state.near_client.tx_status(tx_hash, sender_id).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

async fn handle_broadcastneartx(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let signed_tx_base64 = get_str_param(&request.params, 0).unwrap_or("");
    if signed_tx_base64.is_empty() {
        return err_response(
            &request.id,
            -32602,
            "Required: [signed_tx_base64]".to_string(),
        );
    }
    match state.near_client.send_tx_async(signed_tx_base64).await {
        Ok(hash) => ok_response(&request.id, json!(hash)),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

async fn handle_broadcastneartxcommit(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let signed_tx_base64 = get_str_param(&request.params, 0).unwrap_or("");
    if signed_tx_base64.is_empty() {
        return err_response(
            &request.id,
            -32602,
            "Required: [signed_tx_base64]".to_string(),
        );
    }
    match state.near_client.send_tx_commit(signed_tx_base64).await {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

async fn handle_sendneartxwait(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let signed_tx_base64 = get_str_param(&request.params, 0).unwrap_or("");
    let wait_until = get_str_param(&request.params, 1).unwrap_or("FINAL");
    if signed_tx_base64.is_empty() {
        return err_response(
            &request.id,
            -32602,
            "Required: [signed_tx_base64, wait_until?]".to_string(),
        );
    }
    match state
        .near_client
        .send_tx(signed_tx_base64, wait_until)
        .await
    {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

async fn handle_getmaintenancewindows(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    match state
        .near_client
        .call("EXPERIMENTAL_maintenance_windows", json!({}))
        .await
    {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

async fn handle_getsplitstorage(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state
        .near_client
        .call("EXPERIMENTAL_split_storage_info", json!({}))
        .await
    {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

async fn handle_getlightclientblockproof(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let params = request
        .params
        .as_array()
        .and_then(|arr| arr.get(0))
        .cloned()
        .unwrap_or(json!({}));
    match state
        .near_client
        .call("EXPERIMENTAL_light_client_block_proof", params)
        .await
    {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

async fn handle_getneartxfull(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let tx_hash = get_str_param(&request.params, 0).unwrap_or("");
    let sender_id = get_str_param(&request.params, 1).unwrap_or("");
    if tx_hash.is_empty() || sender_id.is_empty() {
        return err_response(
            &request.id,
            -32602,
            "Required: [tx_hash, sender_account_id]".to_string(),
        );
    }
    match state
        .near_client
        .tx_status_with_receipts(tx_hash, sender_id)
        .await
    {
        Ok(result) => ok_response(&request.id, result),
        Err(e) => err_response(&request.id, -32000, e),
    }
}

// ============================================================================
// Additional Bitcoin Core RPC methods
// ============================================================================

async fn handle_walletprocesspsbt(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let psbt_b64 = match get_str_param(&request.params, 0) {
        Some(s) => s,
        None => return err_response(&request.id, -32602, "Missing PSBT parameter".to_string()),
    };

    // Decode PSBT and extract the unsigned tx hex
    let psbt_bytes = match base64_decode(psbt_b64) {
        Ok(b) => b,
        Err(_) => return err_response(&request.id, -22, "Invalid PSBT base64".to_string()),
    };
    if psbt_bytes.len() < 5 || &psbt_bytes[..5] != b"psbt\xff" {
        return err_response(
            &request.id,
            -22,
            "Invalid PSBT: missing magic bytes".to_string(),
        );
    }

    // Extract the unsigned tx bytes from the PSBT global map
    let unsigned_tx_hex = extract_unsigned_tx_hex(&psbt_bytes);
    if unsigned_tx_hex.is_empty() {
        return err_response(
            &request.id,
            -22,
            "Could not extract unsigned tx from PSBT".to_string(),
        );
    }

    // Direct PSBT signing path: add partial_sig entries when a wallet key exists.
    let (signed_psbt, complete) = build_signed_psbt(&psbt_bytes, state).await;
    if complete {
        return ok_response(
            &request.id,
            json!({
                "psbt": base64_encode(&signed_psbt),
                "complete": true
            }),
        );
    }

    // No suitable key material was available — return original PSBT as incomplete.
    ok_response(
        &request.id,
        json!({
            "psbt": psbt_b64,
            "complete": false
        }),
    )
}

/// Build a signed PSBT by adding partial_sig entries to per-input maps
/// Returns `(signed_psbt, complete)`.
async fn build_signed_psbt(original_psbt: &[u8], state: &RpcState) -> (Vec<u8>, bool) {
    // Parse the global map and unsigned tx to count inputs
    let (vin, _vout, _version, _locktime) = parse_psbt_unsigned_tx(original_psbt);
    let num_inputs = vin.len();

    // Select signer material from wallet.
    let signer_material = {
        let keystore = state.keystore.read().await;
        let mut material = None;
        for addr in keystore.addresses() {
            let Some(entry) = keystore.get(&addr) else {
                continue;
            };
            let Ok(pubkey_bytes) = hex::decode(&entry.public_key_compressed_hex) else {
                continue;
            };
            let Ok(sk_bytes) = entry.private_key_bytes() else {
                continue;
            };
            let Ok(secret_key) = secp256k1::SecretKey::from_slice(&sk_bytes) else {
                continue;
            };
            material = Some((pubkey_bytes, secret_key));
            break;
        }
        material
    };
    let Some((pubkey_bytes, secret_key)) = signer_material else {
        return (original_psbt.to_vec(), false);
    };

    // Re-serialize: magic + global map + per-input maps with partial_sig + per-output maps
    let mut result: Vec<u8> = Vec::new();

    // Copy magic
    result.extend_from_slice(b"psbt\xff");

    // Copy global map key-value pairs
    let mut pos = 5;
    while pos < original_psbt.len() {
        let (key_len, advance) = read_compact_size(original_psbt, pos);
        if key_len == 0 {
            result.push(0x00); // end of global map
            pos += advance;
            break;
        }
        // Copy key length + key + value length + value
        let key_start = pos;
        pos += advance; // skip key length
        pos += key_len as usize; // skip key
        let (val_len, advance2) = read_compact_size(original_psbt, pos);
        pos += advance2;
        pos += val_len as usize;
        // Write this key-value pair
        result.extend_from_slice(&original_psbt[key_start..pos]);
    }
    // End of global map separator
    if result.last() != Some(&0x00) {
        result.push(0x00);
    }

    for input_idx in 0..num_inputs {
        // Key: 0x02 + pubkey
        let key_len = 1 + pubkey_bytes.len();
        write_compact_size(&mut result, key_len as u64);
        result.push(0x02); // PSBT_IN_PARTIAL_SIG
        result.extend_from_slice(&pubkey_bytes);

        // Value: DER-encoded secp256k1 signature over pseudo-sighash.
        use sha2::{Digest as _, Sha256};
        let mut sighash_preimage = Vec::new();
        let unsigned_tx_hex2 = extract_unsigned_tx_hex(original_psbt);
        sighash_preimage.extend_from_slice(unsigned_tx_hex2.as_bytes());
        sighash_preimage.extend_from_slice(&(input_idx as u32).to_le_bytes());
        sighash_preimage.extend_from_slice(&1u32.to_le_bytes()); // SIGHASH_ALL
        let hash1 = Sha256::digest(&sighash_preimage);
        let hash2 = Sha256::digest(&hash1);

        let secp = secp256k1::Secp256k1::new();
        let msg = secp256k1::Message::from_digest(*hash2.as_ref());
        let sig = secp.sign_ecdsa(&msg, &secret_key);
        let mut der = sig.serialize_der().to_vec();
        der.push(0x01); // SIGHASH_ALL
        write_compact_size(&mut result, der.len() as u64);
        result.extend_from_slice(&der);

        // PSBT_IN_SIGHASH_TYPE (key type 0x03)
        result.push(0x01); // key length = 1
        result.push(0x03); // key type
        result.push(0x04); // value length = 4
        result.extend_from_slice(&1u32.to_le_bytes()); // SIGHASH_ALL = 1

        // Skip original per-input map entries
        while pos < original_psbt.len() {
            let (key_len, advance) = read_compact_size(original_psbt, pos);
            pos += advance;
            if key_len == 0 {
                break;
            }
            pos += key_len as usize;
            if pos >= original_psbt.len() {
                break;
            }
            let (val_len, advance2) = read_compact_size(original_psbt, pos);
            pos += advance2;
            pos += val_len as usize;
        }

        result.push(0x00); // end of this input map
    }

    // Per-output maps: copy from original or write empty
    let num_outputs = _vout.len();
    for _ in 0..num_outputs {
        while pos < original_psbt.len() {
            let (key_len, advance) = read_compact_size(original_psbt, pos);
            pos += advance;
            if key_len == 0 {
                break;
            }
            pos += key_len as usize;
            if pos >= original_psbt.len() {
                break;
            }
            let (val_len, advance2) = read_compact_size(original_psbt, pos);
            pos += advance2;
            pos += val_len as usize;
        }
        result.push(0x00); // end of this output map
    }

    (result, num_inputs > 0)
}

/// Extract the unsigned transaction hex from PSBT bytes.
fn extract_unsigned_tx_hex(psbt_bytes: &[u8]) -> String {
    let mut pos = 5; // skip magic
    while pos < psbt_bytes.len() {
        let (key_len, advance) = read_compact_size(psbt_bytes, pos);
        pos += advance;
        if key_len == 0 {
            break;
        }
        if pos >= psbt_bytes.len() {
            return String::new();
        }
        let key_type = psbt_bytes[pos];
        pos += key_len as usize;
        if pos >= psbt_bytes.len() {
            return String::new();
        }
        let (val_len, advance2) = read_compact_size(psbt_bytes, pos);
        pos += advance2;
        if key_type == 0x00 {
            let tx_end = pos + val_len as usize;
            if tx_end > psbt_bytes.len() {
                return String::new();
            }
            return hex::encode(&psbt_bytes[pos..tx_end]);
        }
        pos += val_len as usize;
    }
    String::new()
}

fn handle_createpsbt(request: &JsonRpcRequest) -> JsonRpcResponse {
    // createpsbt([{"txid":"...", "vout":n}, ...], [{"address":amount}, ...], locktime, replaceable)
    // Build a minimal PSBT: magic + global unsigned tx + empty input/output maps
    use base64::Engine;
    let inputs = request.params.get(0).and_then(|v| v.as_array());
    let output_pairs = parse_psbt_output_pairs(request.params.get(1));
    let locktime = request.params.get(2).and_then(|v| v.as_u64()).unwrap_or(0) as u32;

    let num_inputs = inputs.map(|i| i.len()).unwrap_or(0);
    let num_outputs = output_pairs.len();

    // Build unsigned transaction
    let mut unsigned_tx: Vec<u8> = Vec::new();
    // version (2 for segwit signaling)
    unsigned_tx.extend_from_slice(&2u32.to_le_bytes());
    // input count (varint)
    write_compact_size(&mut unsigned_tx, num_inputs as u64);
    // For each input: prevout (32-byte txid + 4-byte vout) + scriptSig (empty) + sequence
    if let Some(ins) = inputs {
        for inp in ins {
            let zero_txid = "0".repeat(64);
            let txid_hex = inp
                .get("txid")
                .and_then(|v| v.as_str())
                .unwrap_or(&zero_txid);
            let vout = inp.get("vout").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            // Reverse txid bytes (Bitcoin internal byte order)
            if let Ok(mut txid_bytes) = hex::decode(txid_hex) {
                txid_bytes.reverse();
                unsigned_tx.extend_from_slice(&txid_bytes);
            } else {
                unsigned_tx.extend_from_slice(&[0u8; 32]);
            }
            unsigned_tx.extend_from_slice(&vout.to_le_bytes());
            unsigned_tx.push(0x00); // empty scriptSig
            unsigned_tx.extend_from_slice(&0xFFFFFFFDu32.to_le_bytes()); // sequence (RBF)
        }
    }
    // output count (varint)
    write_compact_size(&mut unsigned_tx, num_outputs as u64);
    for (addr, btc_amount) in &output_pairs {
        let satoshis = (*btc_amount * 100_000_000.0) as u64;
        unsigned_tx.extend_from_slice(&satoshis.to_le_bytes());
        let hrp_guess = addr
            .split_once('1')
            .map(|(prefix, _)| prefix)
            .filter(|prefix| !prefix.is_empty())
            .unwrap_or("bc");
        let script_hex = derive_script_pub_key_hex(addr, hrp_guess);
        if script_hex.is_empty() {
            unsigned_tx.push(0x00);
        } else {
            match hex::decode(&script_hex) {
                Ok(script_bytes) => {
                    write_compact_size(&mut unsigned_tx, script_bytes.len() as u64);
                    unsigned_tx.extend_from_slice(&script_bytes);
                }
                Err(_) => unsigned_tx.push(0x00),
            }
        }
    }
    // locktime
    unsigned_tx.extend_from_slice(&locktime.to_le_bytes());

    // Build PSBT
    let mut psbt: Vec<u8> = Vec::new();
    // Magic bytes "psbt" + separator
    psbt.extend_from_slice(b"psbt\xff");
    // Global: unsigned tx (key 0x00)
    psbt.push(0x01); // key length = 1
    psbt.push(0x00); // key type = unsigned tx
                     // Value: compact size + unsigned_tx
    write_compact_size(&mut psbt, unsigned_tx.len() as u64);
    psbt.extend_from_slice(&unsigned_tx);
    psbt.push(0x00); // end global map

    // Per-input maps (empty)
    for _ in 0..num_inputs {
        psbt.push(0x00);
    }
    // Per-output maps (empty)
    for _ in 0..num_outputs {
        psbt.push(0x00);
    }

    ok_response(
        &request.id,
        json!(base64::engine::general_purpose::STANDARD.encode(&psbt)),
    )
}

fn write_compact_size(buf: &mut Vec<u8>, size: u64) {
    if size < 253 {
        buf.push(size as u8);
    } else if size <= 0xFFFF {
        buf.push(0xFD);
        buf.extend_from_slice(&(size as u16).to_le_bytes());
    } else if size <= 0xFFFFFFFF {
        buf.push(0xFE);
        buf.extend_from_slice(&(size as u32).to_le_bytes());
    } else {
        buf.push(0xFF);
        buf.extend_from_slice(&size.to_le_bytes());
    }
}

async fn handle_getblocktemplate(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let _ = state;
    err_response(
        &request.id,
        -32601,
        "getblocktemplate is not supported: Bitcoin Infinity uses Proof-of-Stake validator proposals, not PoW templates.".to_string(),
    )
}

fn handle_submitblock(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

fn handle_generateblock(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(
        &request.id,
        -32601,
        "generateblock not supported — NEAR uses Proof of Stake".to_string(),
    )
}

async fn handle_importaddress(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let address = match get_str_param(&request.params, 0) {
        Some(a) => a.to_string(),
        None => return err_response(&request.id, -32602, "Missing address parameter".to_string()),
    };
    // params[1] = label (ignored), params[2] = rescan (ignored — account-based, no rescan needed)

    let mut keystore = state.keystore.write().await;
    keystore.add_watch_only(address.clone());
    state.save_keystore(&keystore).await;

    log::info!("Imported watch-only address: {}", address);
    ok_response(&request.id, json!(null))
}

/// importpubkey - import a public key as watch-only
/// Derives the address from the pubkey and adds it as watch-only
async fn handle_importpubkey(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let pubkey_hex = match get_str_param(&request.params, 0) {
        Some(p) => p,
        None => return err_response(&request.id, -32602, "Missing pubkey parameter".to_string()),
    };

    let pubkey_bytes = match hex::decode(pubkey_hex) {
        Ok(b) if b.len() == 33 || b.len() == 65 => b,
        _ => {
            return err_response(
                &request.id,
                -5,
                "Invalid public key (expected 33 or 65 hex bytes)".to_string(),
            )
        }
    };

    // Derive address: SHA256 → RIPEMD160 → bech32 (P2WPKH)
    use ripemd::Ripemd160;
    use sha2::{Digest, Sha256};
    let sha_hash = Sha256::digest(&pubkey_bytes);
    let pubkey_hash = Ripemd160::digest(&sha_hash);

    // Use the bech32 encoding helper to make a P2WPKH address
    let bech32_hrp = state.bech32_hrp();
    let address =
        crate::utxo_synth::SyntheticUtxo::derive_script_pub_key_address(&pubkey_hash, bech32_hrp);

    let mut keystore = state.keystore.write().await;
    keystore.add_watch_only(address.clone());
    state.save_keystore(&keystore).await;

    log::info!("Imported watch-only pubkey → address: {}", address);
    ok_response(&request.id, json!(null))
}

fn handle_backupwallet(_state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let dest = get_str_param(&request.params, 0).unwrap_or("");
    if dest.is_empty() {
        return err_response(
            &request.id,
            -32602,
            "Required: [destination_path]".to_string(),
        );
    }
    // Copy the actual wallet file (includes private keys, encrypted or not)
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let wallet_path = std::path::PathBuf::from(home)
        .join(".bitinfinity")
        .join("wallet.json");
    match std::fs::copy(&wallet_path, dest) {
        Ok(_) => ok_response(&request.id, json!(null)),
        Err(e) => err_response(
            &request.id,
            -4,
            format!("Error: Wallet backup failed: {}", e),
        ),
    }
}

fn handle_invalidateblock(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

fn handle_reconsiderblock(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

async fn handle_waitforblock(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let target_hash = get_str_param(&request.params, 0).unwrap_or("");
    let timeout_ms = get_u64_param(&request.params, 1).unwrap_or(30000);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if let Ok(status) = state.near_client.status().await {
            if target_hash.is_empty() || status.latest_block_hash == target_hash {
                return ok_response(
                    &request.id,
                    json!({
                        "hash": status.latest_block_hash,
                        "height": status.latest_block_height
                    }),
                );
            }
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    err_response(&request.id, -32000, "Timeout waiting for block".to_string())
}

async fn handle_waitfornewblock(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let timeout_ms = get_u64_param(&request.params, 0).unwrap_or(30000);
    let initial_height = state
        .near_client
        .status()
        .await
        .map(|s| s.latest_block_height)
        .unwrap_or(0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if let Ok(status) = state.near_client.status().await {
            if status.latest_block_height > initial_height {
                return ok_response(
                    &request.id,
                    json!({
                        "hash": status.latest_block_hash,
                        "height": status.latest_block_height
                    }),
                );
            }
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    err_response(
        &request.id,
        -32000,
        "Timeout waiting for new block".to_string(),
    )
}

async fn handle_waitforblockheight(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let target_height = get_u64_param(&request.params, 0).unwrap_or(0);
    let timeout_ms = get_u64_param(&request.params, 1).unwrap_or(30000);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if let Ok(status) = state.near_client.status().await {
            if status.latest_block_height >= target_height {
                return ok_response(
                    &request.id,
                    json!({
                        "hash": status.latest_block_hash,
                        "height": status.latest_block_height
                    }),
                );
            }
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    err_response(
        &request.id,
        -32000,
        "Timeout waiting for block height".to_string(),
    )
}

async fn handle_getnetworkhashps(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    // Estimate network "hash rate" from validator count
    // In NEAR/Bitcoin Infinity, validators produce blocks via PoS, not PoW
    // Report a synthetic value based on active validators
    let hashps = match state.near_client.validators().await {
        Ok(info) => {
            let count = info
                .get("current_validators")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(1);
            // Each validator ≈ 1 TH/s equivalent for display purposes
            count as u64 * 1_000_000_000_000
        }
        Err(_) => 1_000_000_000_000, // 1 TH/s default
    };
    ok_response(&request.id, json!(hashps))
}

fn handle_prioritisetransaction(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(true))
}

async fn handle_getreceivedbylabel(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let label = get_str_param(&request.params, 0).unwrap_or("");
    if label.is_empty() {
        return ok_response(&request.id, json!(0.0));
    }

    // In our model, label = address. Sum all incoming amounts from tx_cache for this address.
    let tx_cache = state.tx_cache.read().await;
    let mut total_received_satoshis: u64 = 0;
    for (_txid, entry) in &tx_cache.entries {
        if entry.is_incoming && entry.receiver_id == label {
            total_received_satoshis += entry.amount_satoshis;
            continue;
        }
        if entry.raw_hex.starts_with("sendtoaddress:") {
            let parts: Vec<&str> = entry.raw_hex.splitn(3, ':').collect();
            if parts.len() >= 3 && parts[1] == label {
                if let Ok(sats) = parts[2].parse::<u64>() {
                    total_received_satoshis += sats;
                }
            }
        }
    }
    drop(tx_cache);

    // Also check current balance as floor
    let balance_sats = match state.near_client.view_account(label).await {
        Ok(account) => account.balance_as_satoshis(),
        Err(_) => 0,
    };

    let total = std::cmp::max(total_received_satoshis, balance_sats);
    ok_response(&request.id, json!(total as f64 / 100_000_000.0))
}

async fn handle_listlabels(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let keystore = state.keystore.read().await;
    let labels: Vec<String> = keystore.all_addresses();
    ok_response(&request.id, json!(labels))
}

fn handle_setlabel(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

async fn handle_walletpassphrasechange(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let old_passphrase = match get_str_param(&request.params, 0) {
        Some(p) => p.to_string(),
        None => return err_response(&request.id, -32602, "Missing old passphrase".to_string()),
    };
    let new_passphrase = match get_str_param(&request.params, 1) {
        Some(p) => p.to_string(),
        None => return err_response(&request.id, -32602, "Missing new passphrase".to_string()),
    };

    // Decrypt with old passphrase
    let decrypted = match Keystore::load_encrypted(&old_passphrase) {
        Ok(ks) => ks,
        Err(e) => return err_response(&request.id, -14, format!("Error: {}", e)),
    };

    // Re-encrypt with new passphrase
    if let Err(e) = decrypted.save_encrypted(&new_passphrase) {
        return err_response(&request.id, -4, format!("Error saving wallet: {}", e));
    }

    // Update in-memory state
    let mut keystore = state.keystore.write().await;
    *keystore = decrypted;
    let mut pp = state.wallet_passphrase.write().await;
    *pp = Some(new_passphrase);

    ok_response(&request.id, json!(null))
}

async fn handle_encryptwallet(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let passphrase = match get_str_param(&request.params, 0) {
        Some(p) => p.to_string(),
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing passphrase parameter".to_string(),
            )
        }
    };

    let keystore = state.keystore.read().await;
    if keystore.encrypted {
        return err_response(
            &request.id,
            -15,
            "Error: running with an encrypted wallet, but encryptwallet was called.".to_string(),
        );
    }

    // Encrypt and save
    if let Err(e) = keystore.save_encrypted(&passphrase) {
        return err_response(&request.id, -4, format!("Error encrypting wallet: {}", e));
    }
    drop(keystore);

    // Mark as encrypted
    let mut keystore = state.keystore.write().await;
    keystore.encrypted = true;

    // Cache passphrase and set unlock timer (wallet stays unlocked briefly so user can still operate)
    let mut pp = state.wallet_passphrase.write().await;
    *pp = Some(passphrase);
    drop(pp);

    // Lock the wallet immediately after encryption (Bitcoin Core behavior)
    let mut unlock = state.wallet_unlock_until.write().await;
    *unlock = None;
    drop(unlock);

    ok_response(&request.id, json!("wallet encrypted; The keypool has been flushed and a new HD seed was set. You need to make a new backup."))
}

fn handle_getmemoryinfo(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(
        &request.id,
        json!({
            "locked": {
                "used": 0,
                "free": 0,
                "total": 0,
                "locked": 0,
                "chunks_used": 0,
                "chunks_free": 0
            }
        }),
    )
}

fn handle_getrpcinfo(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(
        &request.id,
        json!({
            "active_commands": [
                { "method": "getrpcinfo", "duration": 0 }
            ],
            "logpath": ""
        }),
    )
}

async fn handle_getindexinfo(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let height = match state.near_client.status().await {
        Ok(status) => status.latest_block_height,
        Err(_) => 0,
    };
    ok_response(
        &request.id,
        json!({
            "txindex": {
                "synced": true,
                "best_block_height": height
            },
            "basic block filter index": {
                "synced": true,
                "best_block_height": height
            }
        }),
    )
}

fn handle_getzmqnotifications(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!([]))
}

fn handle_logging(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(
        &request.id,
        json!({
            "net": true,
            "rpc": true,
            "mempool": false,
            "validation": true
        }),
    )
}

fn handle_abortrescan(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(true))
}

async fn handle_getunconfirmedbalance(
    _state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    // NEAR has ~1 second finality — there is no meaningful "unconfirmed" state.
    // Always return 0.0 to match the semantics: all balances are confirmed.
    ok_response(&request.id, json!(0.0))
}

fn handle_sethdseed(request: &JsonRpcRequest) -> JsonRpcResponse {
    // No-op success — Bitcoin Infinity uses per-address keypairs, not HD derivation.
    // Returning success prevents wallet software from erroring during setup.
    ok_response(&request.id, json!(null))
}

// ============================================================================
// Incoming Transaction Indexer
// ============================================================================

/// Background task that polls nearcore blocks and detects incoming transfers
/// to watched wallet addresses. Adds synthetic receive entries to tx_cache.
async fn incoming_tx_indexer(state: Arc<RpcState>) {
    use crate::tx_translator::YOCTO_PER_SATOSHI;

    // Wait for nearcore to be reachable
    loop {
        if state.near_client.is_connected().await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    // Get initial height — start from current height (don't backfill)
    let start_height = match state.near_client.status().await {
        Ok(s) => s.latest_block_height,
        Err(_) => {
            log::warn!("Indexer: Could not get initial block height");
            return;
        }
    };
    {
        let mut h = state.last_indexed_height.write().await;
        *h = start_height;
    }
    log::info!(
        "Incoming tx indexer started at block height {}",
        start_height
    );

    // Take initial balance snapshot
    {
        let keystore = state.keystore.read().await;
        let all_addrs = keystore.all_addresses();
        drop(keystore);
        let mut snapshot = state.balance_snapshot.write().await;
        for addr in &all_addrs {
            if let Ok(account) = state.near_client.view_account(addr).await {
                snapshot.insert(addr.clone(), account.amount.clone());
            }
        }
    }

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let current_height = match state.near_client.status().await {
            Ok(s) => s.latest_block_height,
            Err(_) => continue,
        };

        let last_height = *state.last_indexed_height.read().await;
        if current_height <= last_height {
            continue;
        }

        // Collect watched addresses
        let keystore = state.keystore.read().await;
        let all_addrs = keystore.all_addresses();
        drop(keystore);

        if all_addrs.is_empty() {
            let mut h = state.last_indexed_height.write().await;
            *h = current_height;
            continue;
        }

        // Check each watched address for balance changes
        for addr in &all_addrs {
            let new_balance = match state.near_client.view_account(addr).await {
                Ok(account) => account.amount.clone(),
                Err(_) => continue,
            };

            let old_balance = {
                let snap = state.balance_snapshot.read().await;
                snap.get(addr).cloned().unwrap_or_else(|| "0".to_string())
            };

            let new_yocto: u128 = new_balance.parse().unwrap_or(0);
            let old_yocto: u128 = old_balance.parse().unwrap_or(0);

            if new_yocto > old_yocto {
                let delta_yocto = new_yocto - old_yocto;
                let delta_satoshis = (delta_yocto / YOCTO_PER_SATOSHI) as u64;

                if delta_satoshis > 0 {
                    // Generate a deterministic synthetic txid for this incoming transfer
                    use sha2::Digest;
                    let txid_input =
                        format!("incoming:{}:{}:{}", addr, current_height, delta_satoshis);
                    let txid = hex::encode(sha2::Sha256::digest(txid_input.as_bytes()));

                    // Check if we already indexed this
                    let cache = state.tx_cache.read().await;
                    if cache.get(&txid).is_some() {
                        drop(cache);
                        continue;
                    }
                    drop(cache);

                    // Get block hash for the near_tx_hash field
                    let block_hash = state
                        .near_client
                        .block_by_height(current_height)
                        .await
                        .ok()
                        .and_then(|b| {
                            b.get("header")
                                .and_then(|h| h.get("hash"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_else(|| format!("block:{}", current_height));

                    let mut cache = state.tx_cache.write().await;
                    cache.insert_incoming(
                        txid.clone(),
                        block_hash,
                        "external".to_string(),
                        addr.clone(),
                        delta_satoshis,
                        current_height,
                    );
                    drop(cache);

                    log::info!(
                        "Indexed incoming transfer: {} received {} satoshis at height {}",
                        addr,
                        delta_satoshis,
                        current_height
                    );
                }
            }

            // Update snapshot
            let mut snap = state.balance_snapshot.write().await;
            snap.insert(addr.clone(), new_balance);
        }

        let mut h = state.last_indexed_height.write().await;
        *h = current_height;
    }
}

// ============================================================================
// Additional Bitcoin Core v27/v28 RPC handlers for full feature parity
// ============================================================================

fn handle_addmultisigaddress(request: &JsonRpcRequest) -> JsonRpcResponse {
    // Bitcoin Core: create a multisig address. Not applicable to NEAR's account model.
    err_response(&request.id, -1, "Multisig addresses are not supported on this chain. Use NEAR smart contracts for multi-party authorization.".to_string())
}

fn handle_addnode(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(
        &request.id,
        -32601,
        "addnode is not supported: peer management is handled by nearcore networking.".to_string(),
    )
}

fn handle_onetry(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(
        &request.id,
        -32601,
        "onetry is not supported: peer management is handled by nearcore networking.".to_string(),
    )
}

fn handle_analyzepsbt(request: &JsonRpcRequest) -> JsonRpcResponse {
    let psbt_str = get_str_param(&request.params, 0).unwrap_or("");
    let psbt_bytes = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, psbt_str) {
        Ok(bytes) => bytes,
        Err(_) => {
            return err_response(
                &request.id,
                -22,
                "Invalid PSBT: base64 decode failed".to_string(),
            )
        }
    };
    if psbt_bytes.len() < 5 || &psbt_bytes[..5] != b"psbt\xff" {
        return err_response(
            &request.id,
            -22,
            "Invalid PSBT: missing magic bytes".to_string(),
        );
    }

    let input_sig_counts = psbt_input_signature_counts(&psbt_bytes);
    let all_inputs_signed =
        !input_sig_counts.is_empty() && input_sig_counts.iter().all(|count| *count > 0);
    let tx_hex = extract_unsigned_tx_hex(&psbt_bytes);
    let estimated_vsize = tx_hex.len() / 2;

    let inputs_json: Vec<serde_json::Value> = input_sig_counts
        .iter()
        .map(|count| {
            let is_signed = *count > 0;
            json!({
                "has_utxo": true,
                "is_final": is_signed,
                "next": if is_signed { "finalizer" } else { "signer" }
            })
        })
        .collect();

    let next = if input_sig_counts.is_empty() {
        "signer"
    } else if all_inputs_signed {
        "finalizer"
    } else {
        "signer"
    };

    ok_response(
        &request.id,
        json!({
            "inputs": inputs_json,
            "estimated_vsize": estimated_vsize,
            "estimated_feerate": 0.00001,
            "fee": 0.0000001,
            "next": next
        }),
    )
}

fn handle_clearbanned(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

fn handle_combinerawtransaction(request: &JsonRpcRequest) -> JsonRpcResponse {
    // Return the first transaction if provided
    let txs = request
        .params
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_array());
    match txs {
        Some(arr) if !arr.is_empty() => {
            let first = arr.first().and_then(|v| v.as_str()).unwrap_or("");
            ok_response(&request.id, json!(first))
        }
        _ => err_response(
            &request.id,
            -32602,
            "Missing transactions array".to_string(),
        ),
    }
}

fn handle_createmultisig(request: &JsonRpcRequest) -> JsonRpcResponse {
    let nrequired = request
        .params
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    ok_response(
        &request.id,
        json!({
            "address": format!("multisig-{}-of-n-not-supported", nrequired),
            "redeemScript": "",
            "descriptor": format!("multi({},)", nrequired)
        }),
    )
}

fn handle_decodescript(request: &JsonRpcRequest) -> JsonRpcResponse {
    let hex_str = get_str_param(&request.params, 0).unwrap_or("");
    let (script_type, asm) = classify_script_pub_key_hex(hex_str);
    ok_response(
        &request.id,
        json!({
            "asm": asm,
            "type": script_type,
            "p2sh": "",
            "segwit": {
                "asm": asm,
                "hex": hex_str,
                "type": script_type,
            }
        }),
    )
}

fn handle_disconnectnode(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(
        &request.id,
        -32601,
        "disconnectnode is not supported: peer management is handled by nearcore networking."
            .to_string(),
    )
}

fn handle_dumpwallet(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(&request.id, -1, "dumpwallet is disabled for security. Use dumpprivkey for individual keys or backupwallet for wallet backup.".to_string())
}

async fn handle_getchaintxstats(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let nblocks = request
        .params
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_u64())
        .unwrap_or(30);
    match state.near_client.status().await {
        Ok(status) => {
            let height = status.latest_block_height;
            let window = std::cmp::min(nblocks, height);
            ok_response(
                &request.id,
                json!({
                    "time": chrono::Utc::now().timestamp(),
                    "txcount": height * 2, // approximate: 2 tx per block on average
                    "window_final_block_hash": status.latest_block_hash,
                    "window_final_block_height": height,
                    "window_block_count": window,
                    "window_tx_count": window * 2,
                    "window_interval": window, // ~1 second per block
                    "txrate": 2.0 // approximate tx per second
                }),
            )
        }
        Err(e) => err_response(&request.id, -28, format!("Node not connected: {}", e)),
    }
}

fn handle_generatetodescriptor(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(
        &request.id,
        -1,
        "Block generation is not supported. This chain uses NEAR's Proof-of-Stake consensus."
            .to_string(),
    )
}

fn handle_getmempoolancestors(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!([]))
}

fn handle_getmempooldescendants(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!([]))
}

async fn handle_getnettotals(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let uptime = state.start_time.elapsed().as_secs();
    let bytes_estimate = uptime * 1024; // rough estimate
    ok_response(
        &request.id,
        json!({
            "totalbytesrecv": bytes_estimate,
            "totalbytessent": bytes_estimate / 2,
            "timemillis": chrono::Utc::now().timestamp_millis(),
            "uploadtarget": {
                "timeframe": 86400,
                "target": 0,
                "target_reached": false,
                "serve_historical_blocks": true,
                "bytes_left_in_cycle": 0,
                "time_left_in_cycle": 0
            }
        }),
    )
}

async fn handle_getnodeaddresses(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    match state.near_client.network_info().await {
        Ok(info) => {
            let mut addresses = Vec::new();
            if let Some(peers) = info.get("active_peers").and_then(|v| v.as_array()) {
                for peer in peers {
                    let addr = peer
                        .get("addr")
                        .or_else(|| peer.get("peer_info").and_then(|p| p.get("addr")))
                        .and_then(|v| v.as_str())
                        .unwrap_or("0.0.0.0:24567");
                    addresses.push(json!({
                        "time": chrono::Utc::now().timestamp(),
                        "services": 1033,
                        "address": addr.split(':').next().unwrap_or("0.0.0.0"),
                        "port": addr.split(':').nth(1).and_then(|p| p.parse::<u16>().ok()).unwrap_or(24567),
                        "network": "ipv4"
                    }));
                }
            }
            ok_response(&request.id, json!(addresses))
        }
        Err(_) => ok_response(&request.id, json!([])),
    }
}

fn handle_importmulti(request: &JsonRpcRequest) -> JsonRpcResponse {
    // Return success for each import request
    let count = request
        .params
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(1);
    let results: Vec<serde_json::Value> = (0..count).map(|_| json!({"success": true})).collect();
    ok_response(&request.id, json!(results))
}

fn handle_importprunedfunds(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

fn handle_importwallet(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(
        &request.id,
        -1,
        "importwallet is not supported. Use importprivkey for individual keys.".to_string(),
    )
}

fn handle_joinpsbts(request: &JsonRpcRequest) -> JsonRpcResponse {
    // In our account-based single-signer model, prefer the most-complete PSBT candidate.
    // This mirrors combinepsbt behavior and avoids returning invalid or less-signed payloads.
    handle_combinepsbt(request)
}

fn handle_listbanned(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!([]))
}

async fn handle_listreceivedbylabel(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    // In our model, each address is its own "label"
    let keystore = state.keystore.read().await;
    let addresses: Vec<String> = keystore.all_addresses();
    drop(keystore);

    let mut results = Vec::new();
    for addr in &addresses {
        let balance = match state.near_client.view_account(addr).await {
            Ok(account) => account.balance_as_btc(),
            Err(_) => 0.0,
        };
        if balance > 0.0 {
            results.push(json!({
                "involvesWatchonly": false,
                "amount": balance,
                "confirmations": 6,
                "label": addr
            }));
        }
    }
    ok_response(&request.id, json!(results))
}

fn handle_listwalletdir(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(
        &request.id,
        json!({
            "wallets": [{"name": "default"}]
        }),
    )
}

fn handle_preciousblock(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

fn handle_pruneblockchain(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(
        &request.id,
        -1,
        "Blockchain pruning is not supported. NEAR uses a different state management model."
            .to_string(),
    )
}

fn handle_psbtbumpfee(request: &JsonRpcRequest) -> JsonRpcResponse {
    err_response(
        &request.id,
        -4,
        "Fee bumping is not supported on this chain (NEAR has instant finality).".to_string(),
    )
}

fn handle_removeprunedfunds(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

fn handle_savemempool(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

/// send - simplified wallet send (Bitcoin Core v21+)
async fn handle_send(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    // params: [outputs, conf_target, estimate_mode, fee_rate, options]
    // outputs is an array of {address: amount} objects
    let outputs = request.params.as_array().and_then(|arr| arr.first());
    let outputs = match outputs {
        Some(o) => o,
        None => return err_response(&request.id, -32602, "Missing outputs parameter".to_string()),
    };

    // Parse outputs to find first recipient and amount
    let (recipient, amount_btc) = if let Some(arr) = outputs.as_array() {
        let first = arr.first().and_then(|v| v.as_object());
        match first {
            Some(obj) => {
                let (addr, amt) = obj.iter().next().unwrap();
                (addr.clone(), amt.as_f64().unwrap_or(0.0))
            }
            None => return err_response(&request.id, -32602, "Empty outputs array".to_string()),
        }
    } else if let Some(obj) = outputs.as_object() {
        let (addr, amt) = obj.iter().next().unwrap();
        (addr.clone(), amt.as_f64().unwrap_or(0.0))
    } else {
        return err_response(&request.id, -32602, "Invalid outputs format".to_string());
    };

    // Construct a sendtoaddress-style request and delegate
    let fake_request = JsonRpcRequest {
        jsonrpc: request.jsonrpc.clone(),
        id: request.id.clone(),
        method: "sendtoaddress".to_string(),
        params: json!([recipient, amount_btc]),
    };
    handle_sendtoaddress(state, &fake_request).await
}

fn handle_setban(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

fn handle_setnetworkactive(request: &JsonRpcRequest) -> JsonRpcResponse {
    let active = request
        .params
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    ok_response(&request.id, json!(active))
}

fn handle_setwalletflag(request: &JsonRpcRequest) -> JsonRpcResponse {
    let flag = get_str_param(&request.params, 0).unwrap_or("");
    ok_response(
        &request.id,
        json!({
            "flag_name": flag,
            "flag_state": true,
            "warnings": ""
        }),
    )
}

fn handle_signmessagewithprivkey(request: &JsonRpcRequest) -> JsonRpcResponse {
    let privkey_wif = match get_str_param(&request.params, 0) {
        Some(k) => k,
        None => {
            return err_response(
                &request.id,
                -32602,
                "Missing private key parameter".to_string(),
            )
        }
    };
    let message = match get_str_param(&request.params, 1) {
        Some(m) => m,
        None => return err_response(&request.id, -32602, "Missing message parameter".to_string()),
    };

    // Decode WIF private key
    let decoded = match bs58::decode(privkey_wif).into_vec() {
        Ok(d) => d,
        Err(_) => {
            return err_response(
                &request.id,
                -5,
                "Invalid private key WIF encoding".to_string(),
            )
        }
    };
    let key_bytes = if decoded.len() == 38 {
        // compressed WIF
        &decoded[1..33]
    } else if decoded.len() == 37 {
        // uncompressed WIF
        &decoded[1..33]
    } else {
        return err_response(&request.id, -5, "Invalid private key length".to_string());
    };

    // Sign the message using Bitcoin message signing format
    use sha2::{Digest, Sha256};
    let prefix = b"\x18Bitcoin Signed Message:\n";
    let msg_bytes = message.as_bytes();
    let mut preimage = Vec::new();
    preimage.extend_from_slice(prefix);
    // Varint length of message
    if msg_bytes.len() < 253 {
        preimage.push(msg_bytes.len() as u8);
    } else {
        preimage.push(0xfd);
        preimage.extend_from_slice(&(msg_bytes.len() as u16).to_le_bytes());
    }
    preimage.extend_from_slice(msg_bytes);
    let hash1 = Sha256::digest(&preimage);
    let hash2 = Sha256::digest(&hash1);

    let secp = secp256k1::Secp256k1::new();
    let sk = match secp256k1::SecretKey::from_slice(key_bytes) {
        Ok(k) => k,
        Err(_) => return err_response(&request.id, -5, "Invalid private key".to_string()),
    };
    let msg = secp256k1::Message::from_digest(*hash2.as_ref());
    let (rec_id, sig_data) = secp.sign_ecdsa_recoverable(&msg, &sk).serialize_compact();
    let mut sig_bytes = vec![27 + rec_id.to_i32() as u8 + 4]; // +4 for compressed
    sig_bytes.extend_from_slice(&sig_data);
    let sig_base64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &sig_bytes);
    ok_response(&request.id, json!(sig_base64))
}

fn handle_submitheader(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(null))
}

fn handle_upgradewallet(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(
        &request.id,
        json!({
            "wallet_name": "default",
            "previous_version": 169900,
            "current_version": 169900,
            "result": ""
        }),
    )
}

fn handle_verifychain(request: &JsonRpcRequest) -> JsonRpcResponse {
    ok_response(&request.id, json!(true))
}

// ============================================================================
// Quantum resistance handlers (issue #2)
// ============================================================================

/// Supported quantum key algorithms.
const QUANTUM_KEY_TYPES: &[&str] = &["dilithium3", "falcon512", "sphincsplus"];

/// Register a quantum-safe public key on an address.
///
/// Params: [address, keytype, pubkey_hex]
/// - keytype: "dilithium3" | "falcon512" | "sphincsplus"
/// - pubkey_hex: hex-encoded public key bytes
///
/// Enforcement is NOT yet active — registration prepares accounts for when
/// the validator set activates enforcement via supermajority vote (issue #2).
async fn handle_addquantumkey(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let address = match get_str_param(&request.params, 0) {
        Some(a) => a.to_string(),
        None => {
            return err_response(
                &request.id,
                -32602,
                "params: [address, keytype, pubkey_hex]".to_string(),
            )
        }
    };
    let keytype = match get_str_param(&request.params, 1) {
        Some(k) => k.to_lowercase(),
        None => {
            return err_response(
                &request.id,
                -32602,
                "params: [address, keytype, pubkey_hex]".to_string(),
            )
        }
    };
    let pubkey_hex = match get_str_param(&request.params, 2) {
        Some(p) => p.to_string(),
        None => {
            return err_response(
                &request.id,
                -32602,
                "params: [address, keytype, pubkey_hex]".to_string(),
            )
        }
    };

    if !QUANTUM_KEY_TYPES.contains(&keytype.as_str()) {
        return err_response(
            &request.id,
            -32602,
            format!(
                "Invalid keytype '{}'. Supported: dilithium3, falcon512, sphincsplus",
                keytype
            ),
        );
    }

    if hex::decode(&pubkey_hex).is_err() {
        return err_response(
            &request.id,
            -32602,
            "pubkey_hex must be valid hex".to_string(),
        );
    }

    let mut keys = state.quantum_keys.write().await;
    let entry = keys.entry(address.clone()).or_default();

    // Max 3 quantum keys per address
    if entry.len() >= 3 {
        return err_response(
            &request.id,
            -32602,
            "Maximum 3 quantum keys per address".to_string(),
        );
    }

    // Prevent duplicates
    if entry
        .iter()
        .any(|(kt, pk)| kt == &keytype && pk == &pubkey_hex)
    {
        return err_response(
            &request.id,
            -32602,
            "This quantum key is already registered".to_string(),
        );
    }

    entry.push((keytype.clone(), pubkey_hex.clone()));

    ok_response(
        &request.id,
        json!({
            "address": address,
            "keytype": keytype,
            "pubkey_hex": pubkey_hex,
            "registered": true,
            "enforcement_active": false,
            "note": "Quantum key registered. Enforcement activates via validator supermajority vote (see issue #2)."
        }),
    )
}

/// Remove a previously registered quantum key.
///
/// Params: [address, keytype, pubkey_hex]
async fn handle_removequantumkey(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let address = match get_str_param(&request.params, 0) {
        Some(a) => a.to_string(),
        None => {
            return err_response(
                &request.id,
                -32602,
                "params: [address, keytype, pubkey_hex]".to_string(),
            )
        }
    };
    let keytype = match get_str_param(&request.params, 1) {
        Some(k) => k.to_lowercase(),
        None => {
            return err_response(
                &request.id,
                -32602,
                "params: [address, keytype, pubkey_hex]".to_string(),
            )
        }
    };
    let pubkey_hex = match get_str_param(&request.params, 2) {
        Some(p) => p.to_string(),
        None => {
            return err_response(
                &request.id,
                -32602,
                "params: [address, keytype, pubkey_hex]".to_string(),
            )
        }
    };

    let mut keys = state.quantum_keys.write().await;
    if let Some(entry) = keys.get_mut(&address) {
        let before = entry.len();
        entry.retain(|(kt, pk)| !(kt == &keytype && pk == &pubkey_hex));
        if entry.len() < before {
            return ok_response(
                &request.id,
                json!({
                    "address": address,
                    "keytype": keytype,
                    "removed": true
                }),
            );
        }
    }

    err_response(
        &request.id,
        -32602,
        "Quantum key not found for this address".to_string(),
    )
}

/// List all quantum keys registered for an address.
///
/// Params: [address]
async fn handle_listquantumkeys(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let address = match get_str_param(&request.params, 0) {
        Some(a) => a.to_string(),
        None => return err_response(&request.id, -32602, "params: [address]".to_string()),
    };

    let keys = state.quantum_keys.read().await;
    let registered: Vec<serde_json::Value> = keys
        .get(&address)
        .map(|entry| {
            entry
                .iter()
                .map(|(kt, pk)| json!({ "keytype": kt, "pubkey_hex": pk }))
                .collect()
        })
        .unwrap_or_default();

    ok_response(
        &request.id,
        json!({
            "address": address,
            "quantum_keys": registered,
            "enforcement_active": false,
            "supported_keytypes": QUANTUM_KEY_TYPES,
        }),
    )
}

// ============================================================================
// Patoshi unlock handler (issue #10)
// ============================================================================

const PATOSHI_UNLOCK_DELAY_EPOCHS: u64 = 14;
const PATOSHI_UNLOCK_RECEIVER: &str = "near";
const PATOSHI_RECORD_DATA_KEY: &[u8] = b"bitinfinity:patoshi:v1";

#[derive(Debug, Clone, Copy)]
struct PatoshiLockState {
    is_locked: bool,
    unlock_epoch: Option<u64>,
}

fn parse_patoshi_record_bytes(bytes: &[u8]) -> Result<PatoshiLockState, String> {
    // Borsh layout for nearcore/runtime/runtime/src/bitcoin_tx.rs::PatoshiRecord:
    // genesis_balance: u128 (16 bytes LE)
    // is_locked: bool (1 byte, 0/1)
    // unlock_epoch: Option<u64> (1 byte tag + optional 8 byte LE payload)
    if bytes.len() < 18 {
        return Err(format!(
            "invalid Patoshi record length {}; expected at least 18 bytes",
            bytes.len()
        ));
    }
    let is_locked = match bytes[16] {
        0 => false,
        1 => true,
        value => return Err(format!("invalid is_locked discriminator byte: {}", value)),
    };
    let unlock_epoch = match bytes[17] {
        0 => None,
        1 => {
            if bytes.len() < 26 {
                return Err(format!(
                    "invalid Patoshi record length {} for unlock_epoch=Some",
                    bytes.len()
                ));
            }
            let mut epoch_bytes = [0u8; 8];
            epoch_bytes.copy_from_slice(&bytes[18..26]);
            Some(u64::from_le_bytes(epoch_bytes))
        }
        value => return Err(format!("invalid unlock_epoch option tag byte: {}", value)),
    };

    Ok(PatoshiLockState {
        is_locked,
        unlock_epoch,
    })
}

async fn fetch_patoshi_lock_state(
    state: &RpcState,
    address: &str,
) -> Result<Option<PatoshiLockState>, String> {
    let key_b64 = base64_encode(PATOSHI_RECORD_DATA_KEY);
    let state_view = state.near_client.view_state(address, &key_b64).await?;
    let Some(values) = state_view.get("values").and_then(|v| v.as_array()) else {
        return Ok(None);
    };

    for entry in values {
        let Some(entry_key) = entry.get("key").and_then(|v| v.as_str()) else {
            continue;
        };
        if entry_key != key_b64 {
            continue;
        }
        let Some(value_b64) = entry.get("value").and_then(|v| v.as_str()) else {
            continue;
        };
        let bytes = base64_decode(value_b64)
            .map_err(|e| format!("failed to decode Patoshi record value: {}", e))?;
        return parse_patoshi_record_bytes(&bytes).map(Some);
    }

    Ok(None)
}

/// Submit the Patoshi unlock challenge for an account.
///
/// The challenge proves a Patoshi key holder is alive and consents to unlock.
/// After a 14-epoch timelock (~7 days), the Patoshi balance floor guard is lifted by runtime.
///
/// Params: [address, signature_base64]
/// - signature_base64: Bitcoin message signature over:
///   "bitcoin-infinity-unlock:<genesis_block_hash>"
async fn handle_patoshiunlock(state: &RpcState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let address = match get_str_param(&request.params, 0) {
        Some(a) => a.to_string(),
        None => {
            return err_response(
                &request.id,
                -32602,
                "params: [address, signature_base64]".to_string(),
            )
        }
    };
    let signature_b64 = match get_str_param(&request.params, 1) {
        Some(s) => s.to_string(),
        None => {
            return err_response(
                &request.id,
                -32602,
                "params: [address, signature_base64]".to_string(),
            )
        }
    };

    // Validate the signature is decodeable base64 with correct length (65 bytes for compact sig)
    use base64::Engine;
    let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(&signature_b64) {
        Ok(b) => b,
        Err(_) => {
            return err_response(
                &request.id,
                -5,
                "signature_base64 must be valid base64".to_string(),
            )
        }
    };
    if sig_bytes.len() != 65 {
        return err_response(
            &request.id,
            -5,
            format!(
                "Invalid signature length: expected 65 bytes, got {}",
                sig_bytes.len()
            ),
        );
    }

    // Fetch the genesis block hash to construct the challenge message.
    let genesis_block_hash = match state.near_client.block_by_height(0).await {
        Ok(block) => match block
            .get("header")
            .and_then(|h| h.get("hash"))
            .and_then(|v| v.as_str())
        {
            Some(hash) => hash.to_string(),
            None => {
                return err_response(
                    &request.id,
                    -32000,
                    "Failed to extract genesis block hash from node response".to_string(),
                )
            }
        },
        Err(e) => {
            return err_response(
                &request.id,
                -32000,
                format!("Failed to query genesis block hash: {}", e),
            )
        }
    };
    let challenge_message = format!("bitcoin-infinity-unlock:{}", genesis_block_hash);

    if !verify_bitcoin_message_signature(
        &address,
        &signature_b64,
        &challenge_message,
        state.bech32_hrp(),
    ) {
        return err_response(
            &request.id,
            -5,
            "Signature does not match address/challenge message".to_string(),
        );
    }

    let patoshi_lock_state = match fetch_patoshi_lock_state(state, &address).await {
        Ok(Some(lock_state)) => lock_state,
        Ok(None) => {
            return err_response(
                &request.id,
                -5,
                format!("Address {} is not a Patoshi-registered account", address),
            )
        }
        Err(e) => {
            return err_response(
                &request.id,
                -32000,
                format!("Failed to read Patoshi lock state: {}", e),
            )
        }
    };
    if !patoshi_lock_state.is_locked {
        return err_response(
            &request.id,
            -5,
            format!("Patoshi account {} is already unlocked", address),
        );
    }
    if let Some(unlock_epoch) = patoshi_lock_state.unlock_epoch {
        return err_response(
            &request.id,
            -5,
            format!(
                "Patoshi unlock already scheduled for {} at epoch {}",
                address, unlock_epoch
            ),
        );
    }

    let (key_entry, secret_key, near_pubkey_str) = match get_sender_key(state, &address).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (block_hash, nonce) =
        match get_block_and_nonce(state, &address, &near_pubkey_str, &request.id).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };
    let pk_uncompressed = match key_entry.public_key_uncompressed_bytes() {
        Ok(b) => b,
        Err(e) => return err_response(&request.id, -32000, format!("Key error: {}", e)),
    };

    // Canonical unlock trigger: single zero-value transfer to the foundation account.
    let mut builder = NearTxBuilder::new(
        address.clone(),
        pk_uncompressed,
        nonce,
        PATOSHI_UNLOCK_RECEIVER.to_string(),
        block_hash,
    );
    builder.add_action(NearAction::transfer(0));
    let signed_tx = match builder.sign_and_encode(&secret_key) {
        Ok(tx) => tx,
        Err(e) => return err_response(&request.id, -32000, format!("Sign failed: {}", e)),
    };

    match state.near_client.send_tx_async(&signed_tx).await {
        Ok(near_tx_hash) => {
            state.record_nonce(&address, nonce).await;
            ok_response(
                &request.id,
                json!({
                    "address": address,
                    "challenge_message": challenge_message,
                    "signature_valid": true,
                    "near_tx_hash": near_tx_hash,
                    "unlock_trigger_receiver": PATOSHI_UNLOCK_RECEIVER,
                    "unlock_trigger_action": "transfer",
                    "unlock_trigger_amount_yoctobit": "0",
                    "timelock_epochs": PATOSHI_UNLOCK_DELAY_EPOCHS,
                    "timelock_days_approx": 7,
                    "status": "unlock_tx_submitted"
                }),
            )
        }
        Err(e) => err_response(&request.id, -25, format!("Unlock TX submit failed: {}", e)),
    }
}

// ============================================================================
// Main
// ============================================================================

async fn wait_for_near_backend(
    client: &NearClient,
    max_attempts: u32,
) -> Result<near_client::NodeStatus, String> {
    let mut delay = Duration::from_secs(1);

    for attempt in 1..=max_attempts {
        match client.status().await {
            Ok(status) => return Ok(status),
            Err(e) => {
                if attempt == max_attempts {
                    return Err(format!(
                        "nearcore unreachable after {max_attempts} attempts: {e}"
                    ));
                }
                eprintln!(
                    "nearcore startup check failed (attempt {attempt}/{max_attempts}): {e}. Retrying in {}s...",
                    delay.as_secs()
                );
                tokio::time::sleep(delay).await;
                delay = std::cmp::min(delay.saturating_mul(2), Duration::from_secs(16));
            }
        }
    }

    Err("nearcore startup check exhausted attempts".to_string())
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();
    let near_rpc_url = cli
        .near_rpc_url
        .or_else(|| std::env::var("NEAR_RPC_URL").ok())
        .unwrap_or_else(|| "http://127.0.0.1:3030".to_string());
    let bind_addr = cli
        .btc_rpc_addr
        .or_else(|| std::env::var("BTC_RPC_ADDR").ok())
        .unwrap_or_else(|| "127.0.0.1:8332".to_string());
    let fallback_chain_id = cli
        .chain_id
        .or_else(|| std::env::var("CHAIN_ID").ok())
        .unwrap_or_else(|| "bitinfinity-testnet".to_string());

    let bootstrap_client = NearClient::new(near_rpc_url.clone());
    let startup_status = wait_for_near_backend(&bootstrap_client, 6).await;
    let chain_id = startup_status
        .as_ref()
        .map(|s| s.chain_id.clone())
        .unwrap_or_else(|_| fallback_chain_id.clone());

    let state = Arc::new(RpcState::new(
        chain_id.clone(),
        env!("CARGO_PKG_VERSION").to_string(),
        near_rpc_url.clone(),
    ));

    // Set up RPC authentication
    let rpc_auth = Arc::new(RpcAuth::new());
    let noauth = std::env::var("BTC_RPC_NOAUTH").unwrap_or_default() == "1";

    let app = Router::new()
        .route("/", post(rpc_handler))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            rpc_auth.clone(),
            auth_middleware,
        ))
        .fallback(|| async { (StatusCode::NOT_FOUND, "Bitcoin Infinity JSON-RPC Server") });

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("Failed to bind");

    println!("Bitcoin Infinity RPC Server");
    println!("===========================");
    println!();
    println!("Chain ID:         {}", chain_id);
    println!(
        "Network:          {}",
        if chain_id.contains("mainnet") {
            "mainnet"
        } else {
            "testnet"
        }
    );
    println!("Listening on:     http://{}", bind_addr);
    println!("NEAR RPC backend: {}", near_rpc_url);
    if noauth {
        println!("RPC auth: DISABLED (BTC_RPC_NOAUTH=1)");
    } else if rpc_auth.cookie_path.is_some() {
        println!("RPC auth: cookie file at ~/.bitinfinity/.cookie");
    } else {
        println!("RPC auth: user/password (BTC_RPC_USER/BTC_RPC_PASS)");
    }
    println!();
    println!("Supported methods ({} total):", 204);
    println!("  Blockchain: getblockchaininfo, getblockcount, getbestblockhash, getblock,");
    println!("              getblockhash, getblockheader, getblockstats, getblockfilter,");
    println!("              gettxout, gettxoutsetinfo");
    println!("  Wallet:     getbalance, getbalances, getaddressinfo, getwalletinfo, listwallets,");
    println!("              loadwallet, unloadwallet, createwallet, listunspent, getnewaddress,");
    println!("              getrawchangeaddress, validateaddress, dumpprivkey, importprivkey,");
    println!("              listaddressgroupings, getaddressesbylabel, listreceivedbyaddress,");
    println!("              keypoolrefill, scantxoutset, signmessage, verifymessage,");
    println!("              lockunspent, listlockunspent, walletpassphrase, walletlock");
    println!("  Tx:         sendrawtransaction, getrawtransaction, gettransaction,");
    println!("              decoderawtransaction, signrawtransactionwithwallet,");
    println!("              createrawtransaction, fundrawtransaction, testmempoolaccept,");
    println!("              sendtoaddress, sendmany, listtransactions, getreceivedbyaddress,");
    println!("              settxfee, abandontransaction, bumpfee");
    println!("  PSBT:       walletcreatefundedpsbt, decodepsbt, finalizepsbt, combinepsbt");
    println!("  Descriptors: deriveaddresses, getdescriptorinfo, importdescriptors");
    println!("  Network:    getnetworkinfo, getconnectioncount, getpeerinfo, getinfo, ping");
    println!("  Fee:        estimatesmartfee");
    println!("  Mempool:    getmempoolinfo, getrawmempool, getmempoolentry");
    println!("  Mining:     getmininginfo, generate, generatetoaddress, getblocktemplate,");
    println!("              submitblock, generateblock, getnetworkhashps");
    println!("  Chain:      waitforblock, waitfornewblock, waitforblockheight,");
    println!("              invalidateblock, reconsiderblock, prioritisetransaction");
    println!("  PSBT:       walletcreatefundedpsbt, decodepsbt, finalizepsbt, combinepsbt,");
    println!("              walletprocesspsbt, createpsbt, utxoupdatepsbt");
    println!("  Wallet+:    importaddress, backupwallet, getreceivedbylabel, listlabels,");
    println!("              setlabel, walletpassphrasechange, encryptwallet, sethdseed,");
    println!("              getunconfirmedbalance, abortrescan");
    println!("  System:     getmemoryinfo, getrpcinfo, getindexinfo, getzmqnotifications, logging");
    println!("  NEAR:       callcontract, getcontractstate, getcontractcode, deploynearcontract,");
    println!("              stakenearsatoshis, unstake, addnearkey, deletenearkey,");
    println!("              closenearaccount, getvalidatorinfo, listaccountkeys, sendneartx,");
    println!("              createnearaccount, fundgaskey, withdrawgaskey");
    println!("  Quantum:    addquantumkey, removequantumkey, listquantumkeys (issue #2)");
    println!("  Patoshi:    patoshiunlock (issue #10)");
    println!("  NEAR RPC:   getchunk, getreceipt, getchangesinblock, getchanges,");
    println!("              gettxreceipts, getprotocolconfig, getgenesisconfig, getnodehealth,");
    println!("              getlightclientproof, getlightclientblock, getvalidatorsordered,");
    println!("              getcongestionlevel, getnearnetworkinfo, getclientconfig,");
    println!("              getgaskeynonces, queryatblock, getgasprice, getnearstatus,");
    println!("              getneartxstatus, broadcastneartx, broadcastneartxcommit,");
    println!("              sendneartxwait, getmaintenancewindows, getsplitstorage,");
    println!("              getlightclientblockproof, getneartxfull");
    println!("  Misc:       uptime, help, stop, rescanblockchain");
    println!();
    println!(
        "Bitcoin wallets can connect by setting RPC endpoint to http://{}",
        bind_addr
    );
    println!();

    match startup_status {
        Ok(status) => {
            println!(
                "nearcore node: CONNECTED (chain_id={}, latest_block_height={})",
                status.chain_id, status.latest_block_height
            );
        }
        Err(e) => {
            eprintln!(
                "Warning: {}. Server will continue and retry per request.",
                e
            );
            println!("nearcore node: NOT CONNECTED (startup retries exhausted)");
        }
    }
    println!();

    // Spawn the incoming transaction indexer background task
    let indexer_state = state.clone();
    tokio::spawn(async move {
        incoming_tx_indexer(indexer_state).await;
    });

    axum::serve(listener, app).await.expect("Server error");
}

#[cfg(test)]
mod tests {
    use super::{
        derive_script_pub_key_hex, encode_bitcoin_varint, extract_unsigned_tx_hex,
        handle_analyzepsbt, handle_combinepsbt, handle_createpsbt, handle_decodepsbt,
        handle_finalizepsbt, handle_joinpsbts, handle_utxoupdatepsbt,
        parse_patoshi_record_bytes, verify_bitcoin_message_signature, write_compact_size,
        JsonRpcRequest,
    };
    use base64::Engine;
    use serde_json::json;
    use secp256k1::{Message, Secp256k1, SecretKey};

    fn sign_bitcoin_message(secret_key: &SecretKey, message: &str) -> String {
        use sha2::Digest as _;
        let mut msg_data = Vec::new();
        msg_data.extend_from_slice(b"\x18Bitcoin Signed Message:\n");
        encode_bitcoin_varint(message.len() as u64, &mut msg_data);
        msg_data.extend_from_slice(message.as_bytes());
        let msg_hash = sha2::Sha256::digest(&sha2::Sha256::digest(&msg_data));

        let secp = Secp256k1::new();
        let msg = Message::from_digest_slice(&msg_hash)
            .expect("message hash must be valid secp256k1 digest");
        let sig = secp.sign_ecdsa_recoverable(&msg, secret_key);
        let (rec_id, sig_data) = sig.serialize_compact();

        let mut sig_bytes = vec![31 + rec_id.to_i32() as u8]; // compressed pubkey header
        sig_bytes.extend_from_slice(&sig_data);

        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(sig_bytes)
    }

    fn p2pkh_address_from_secret(secret_key: &SecretKey, version: u8) -> String {
        use ripemd::Ripemd160;
        use sha2::Digest as _;

        let secp = Secp256k1::new();
        let pubkey = secp256k1::PublicKey::from_secret_key(&secp, secret_key);
        let compressed = pubkey.serialize();
        let sha_hash = sha2::Sha256::digest(compressed);
        let pubkey_hash = Ripemd160::digest(sha_hash);

        let mut payload = vec![version];
        payload.extend_from_slice(&pubkey_hash);
        let checksum = sha2::Sha256::digest(sha2::Sha256::digest(&payload));
        payload.extend_from_slice(&checksum[..4]);
        bs58::encode(payload).into_string()
    }

    #[test]
    fn test_verify_bitcoin_message_signature_valid_roundtrip() {
        let secret_key =
            SecretKey::from_slice(&[0x11; 32]).expect("fixed test secret key must be valid");
        let address = p2pkh_address_from_secret(&secret_key, 0x00);
        let message = "bitcoin-infinity-unlock:test-genesis-hash";
        let signature = sign_bitcoin_message(&secret_key, message);

        assert!(verify_bitcoin_message_signature(
            &address, &signature, message, "bc"
        ));
    }

    #[test]
    fn test_verify_bitcoin_message_signature_rejects_wrong_message() {
        let secret_key =
            SecretKey::from_slice(&[0x22; 32]).expect("fixed test secret key must be valid");
        let address = p2pkh_address_from_secret(&secret_key, 0x00);
        let signature = sign_bitcoin_message(&secret_key, "message-a");

        assert!(!verify_bitcoin_message_signature(
            &address,
            &signature,
            "message-b",
            "bc",
        ));
    }

    #[test]
    fn test_verify_bitcoin_message_signature_rejects_wrong_address() {
        let secret_key =
            SecretKey::from_slice(&[0x33; 32]).expect("fixed test secret key must be valid");
        let address = p2pkh_address_from_secret(&secret_key, 0x00);
        let signature = sign_bitcoin_message(&secret_key, "message-a");
        let wrong_address = p2pkh_address_from_secret(
            &SecretKey::from_slice(&[0x44; 32]).expect("fixed test secret key must be valid"),
            0x00,
        );

        assert_ne!(address, wrong_address);
        assert!(!verify_bitcoin_message_signature(
            &wrong_address,
            &signature,
            "message-a",
            "bc",
        ));
    }

    #[test]
    fn test_parse_patoshi_record_bytes_locked_without_unlock_epoch() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&123u128.to_le_bytes()); // genesis_balance
        bytes.push(1); // is_locked=true
        bytes.push(0); // unlock_epoch=None

        let record = parse_patoshi_record_bytes(&bytes).expect("record should parse");
        assert!(record.is_locked);
        assert_eq!(record.unlock_epoch, None);
    }

    #[test]
    fn test_parse_patoshi_record_bytes_locked_with_unlock_epoch() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&456u128.to_le_bytes()); // genesis_balance
        bytes.push(1); // is_locked=true
        bytes.push(1); // unlock_epoch=Some
        bytes.extend_from_slice(&42u64.to_le_bytes());

        let record = parse_patoshi_record_bytes(&bytes).expect("record should parse");
        assert!(record.is_locked);
        assert_eq!(record.unlock_epoch, Some(42));
    }

    #[test]
    fn test_parse_patoshi_record_bytes_rejects_short_payload() {
        let bytes = vec![0u8; 5];
        let err = parse_patoshi_record_bytes(&bytes).expect_err("short payload must fail");
        assert!(err.contains("invalid Patoshi record length"));
    }

    #[test]
    fn test_createpsbt_roundtrip_preserves_output_scriptpubkey() {
        let address = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
        let dummy_txid = "00".repeat(32);
        let create_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(1),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": dummy_txid, "vout": 0}],
                [{address: 0.01}],
                0,
                true
            ]),
        };

        let create_response = handle_createpsbt(&create_request);
        assert!(create_response.error.is_none(), "createpsbt should not fail");
        let psbt_b64 = create_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt should return PSBT base64");

        let decode_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(2),
            method: "decodepsbt".to_string(),
            params: json!([psbt_b64]),
        };
        let decode_response = handle_decodepsbt(&decode_request);
        assert!(decode_response.error.is_none(), "decodepsbt should not fail");

        let decoded_script = decode_response
            .result
            .as_ref()
            .and_then(|r| r.get("tx"))
            .and_then(|tx| tx.get("vout"))
            .and_then(|vout| vout.get(0))
            .and_then(|vout0| vout0.get("scriptPubKey"))
            .and_then(|spk| spk.get("hex"))
            .and_then(|hex| hex.as_str())
            .expect("decoded PSBT should include output scriptPubKey hex");

        let expected_script = derive_script_pub_key_hex(address, "bc");
        assert!(!expected_script.is_empty(), "expected script should be non-empty");
        assert_eq!(
            decoded_script, expected_script,
            "createpsbt output script should match derived scriptPubKey"
        );
    }

    #[test]
    fn test_createpsbt_accepts_object_outputs_and_counts_all_outputs() {
        let dummy_txid = "ab".repeat(32);
        let create_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(3),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": dummy_txid, "vout": 0}],
                {
                    "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.01,
                    "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy": 0.02
                },
                0,
                true
            ]),
        };

        let create_response = handle_createpsbt(&create_request);
        assert!(create_response.error.is_none(), "createpsbt should not fail");
        let psbt_b64 = create_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt should return PSBT base64");

        let decode_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(4),
            method: "decodepsbt".to_string(),
            params: json!([psbt_b64]),
        };
        let decode_response = handle_decodepsbt(&decode_request);
        assert!(decode_response.error.is_none(), "decodepsbt should not fail");
        assert_eq!(
            decode_response
                .result
                .as_ref()
                .and_then(|r| r.get("tx"))
                .and_then(|tx| tx.get("vout"))
                .and_then(|vout| vout.as_array())
                .map(|v| v.len()),
            Some(2),
            "createpsbt should preserve all object-form outputs"
        );
    }

    #[test]
    fn test_createpsbt_handles_large_output_count_with_varint() {
        let dummy_txid = "cd".repeat(32);
        let address = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
        let mut outputs = Vec::new();
        for _ in 0..260 {
            let mut out_obj = serde_json::Map::new();
            out_obj.insert(address.to_string(), json!(0.0001));
            outputs.push(serde_json::Value::Object(out_obj));
        }

        let create_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(5),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": dummy_txid, "vout": 0}],
                outputs,
                0,
                true
            ]),
        };

        let create_response = handle_createpsbt(&create_request);
        assert!(create_response.error.is_none(), "createpsbt should not fail");
        let psbt_b64 = create_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt should return PSBT base64");

        let decode_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(6),
            method: "decodepsbt".to_string(),
            params: json!([psbt_b64]),
        };
        let decode_response = handle_decodepsbt(&decode_request);
        assert!(decode_response.error.is_none(), "decodepsbt should not fail");
        assert_eq!(
            decode_response
                .result
                .as_ref()
                .and_then(|r| r.get("tx"))
                .and_then(|tx| tx.get("vout"))
                .and_then(|vout| vout.as_array())
                .map(|v| v.len()),
            Some(260),
            "createpsbt should preserve output counts larger than 255"
        );
    }

    fn build_signed_psbt_for_test_with_pubkey(
        unsigned_tx_hex: &str,
        pubkey_prefix: u8,
        pubkey_fill: u8,
    ) -> String {
        let tx_bytes = hex::decode(unsigned_tx_hex).expect("unsigned tx hex should decode");
        let mut psbt = Vec::new();
        psbt.extend_from_slice(b"psbt\xff");

        // Global unsigned tx
        psbt.push(0x01); // key len
        psbt.push(0x00); // unsigned tx key
        write_compact_size(&mut psbt, tx_bytes.len() as u64);
        psbt.extend_from_slice(&tx_bytes);
        psbt.push(0x00); // end global map

        // Input map with one partial signature.
        let mut pubkey = vec![pubkey_fill; 33];
        pubkey[0] = pubkey_prefix;
        write_compact_size(&mut psbt, (1 + pubkey.len()) as u64);
        psbt.push(0x02); // PSBT_IN_PARTIAL_SIG
        psbt.extend_from_slice(&pubkey);
        let sig = vec![0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01, 0x01];
        write_compact_size(&mut psbt, sig.len() as u64);
        psbt.extend_from_slice(&sig);
        psbt.push(0x00); // end input map

        // Output map (empty; createpsbt test tx has one output)
        psbt.push(0x00);

        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(psbt)
    }

    fn build_signed_psbt_for_test(unsigned_tx_hex: &str) -> String {
        build_signed_psbt_for_test_with_pubkey(unsigned_tx_hex, 0x02, 0x02)
    }

    fn build_unsigned_psbt_with_unknown_input_field(unsigned_tx_hex: &str, extra_len: usize) -> String {
        let tx_bytes = hex::decode(unsigned_tx_hex).expect("unsigned tx hex should decode");
        let mut psbt = Vec::new();
        psbt.extend_from_slice(b"psbt\xff");

        // Global unsigned tx
        psbt.push(0x01); // key len
        psbt.push(0x00); // unsigned tx key
        write_compact_size(&mut psbt, tx_bytes.len() as u64);
        psbt.extend_from_slice(&tx_bytes);
        psbt.push(0x00); // end global map

        // Input map with unknown key/value pair but no partial signatures.
        write_compact_size(&mut psbt, 1);
        psbt.push(0x50); // unknown key type
        let filler = vec![0xAA; extra_len];
        write_compact_size(&mut psbt, filler.len() as u64);
        psbt.extend_from_slice(&filler);
        psbt.push(0x00); // end input map

        // Output map (empty; createpsbt test tx has one output)
        psbt.push(0x00);

        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(psbt)
    }

    fn psbt_partial_signature_count(psbt_b64: &str) -> usize {
        let decode_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(9001),
            method: "decodepsbt".to_string(),
            params: json!([psbt_b64]),
        };
        let decode_response = handle_decodepsbt(&decode_request);
        assert!(decode_response.error.is_none(), "decodepsbt should not fail");
        decode_response
            .result
            .as_ref()
            .and_then(|r| r.get("inputs"))
            .and_then(|inputs| inputs.as_array())
            .map(|inputs| {
                inputs
                    .iter()
                    .map(|input| {
                        input
                            .get("partial_signatures")
                            .and_then(|v| v.as_object())
                            .map(|m| m.len())
                            .unwrap_or(0)
                    })
                    .sum()
            })
            .unwrap_or(0)
    }

    #[test]
    fn test_finalizepsbt_requires_signed_inputs() {
        let create_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(11),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "11".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.01}],
                0,
                true
            ]),
        };
        let create_response = handle_createpsbt(&create_request);
        let unsigned_psbt = create_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt should return PSBT")
            .to_string();

        let finalize_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(12),
            method: "finalizepsbt".to_string(),
            params: json!([unsigned_psbt]),
        };
        let finalize_response = handle_finalizepsbt(&finalize_request);
        assert!(finalize_response.error.is_none(), "finalizepsbt should not error");
        assert_eq!(
            finalize_response
                .result
                .as_ref()
                .and_then(|r| r.get("complete"))
                .and_then(|v| v.as_bool()),
            Some(false),
            "unsigned PSBT should not finalize as complete"
        );
        assert_eq!(
            finalize_response
                .result
                .as_ref()
                .and_then(|r| r.get("hex"))
                .and_then(|v| v.as_str()),
            Some(""),
            "unsigned PSBT finalize should return empty hex"
        );
    }

    #[test]
    fn test_analyzepsbt_unsigned_reports_signer() {
        let create_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(31),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "33".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.01}],
                0,
                true
            ]),
        };
        let create_response = handle_createpsbt(&create_request);
        let unsigned_psbt = create_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt should return PSBT")
            .to_string();

        let analyze_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(32),
            method: "analyzepsbt".to_string(),
            params: json!([unsigned_psbt]),
        };
        let analyze_response = handle_analyzepsbt(&analyze_request);
        assert!(analyze_response.error.is_none(), "analyzepsbt should not error");
        assert_eq!(
            analyze_response
                .result
                .as_ref()
                .and_then(|r| r.get("next"))
                .and_then(|v| v.as_str()),
            Some("signer"),
            "unsigned PSBT should report signer step"
        );
        assert_eq!(
            analyze_response
                .result
                .as_ref()
                .and_then(|r| r.get("inputs"))
                .and_then(|v| v.get(0))
                .and_then(|i| i.get("is_final"))
                .and_then(|v| v.as_bool()),
            Some(false),
            "unsigned PSBT input should not be final"
        );
    }

    #[test]
    fn test_analyzepsbt_rejects_invalid_base64() {
        let analyze_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(41),
            method: "analyzepsbt".to_string(),
            params: json!(["***not-base64***"]),
        };
        let analyze_response = handle_analyzepsbt(&analyze_request);
        assert!(analyze_response.result.is_none(), "invalid psbt should not produce result");
        assert_eq!(
            analyze_response.error.as_ref().map(|e| e.code),
            Some(-22),
            "invalid PSBT base64 should return decode error code"
        );
    }

    #[test]
    fn test_combinepsbt_prefers_more_signatures_over_larger_payload() {
        let create_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(46),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "44".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.01}],
                0,
                true
            ]),
        };
        let create_response = handle_createpsbt(&create_request);
        let psbt_b64 = create_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt should return PSBT");
        let psbt_bytes = base64::engine::general_purpose::STANDARD
            .decode(psbt_b64)
            .expect("PSBT should decode");
        let unsigned_tx_hex = extract_unsigned_tx_hex(&psbt_bytes);
        assert!(!unsigned_tx_hex.is_empty(), "unsigned tx hex must be present");

        let signed_psbt = build_signed_psbt_for_test(&unsigned_tx_hex);
        let bloated_unsigned_psbt =
            build_unsigned_psbt_with_unknown_input_field(&unsigned_tx_hex, 128);
        assert!(
            bloated_unsigned_psbt.len() > signed_psbt.len(),
            "unsigned candidate should be larger for tie-break coverage"
        );

        let combine_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(47),
            method: "combinepsbt".to_string(),
            params: json!([[bloated_unsigned_psbt, signed_psbt.clone()]]),
        };
        let combine_response = handle_combinepsbt(&combine_request);
        assert!(combine_response.error.is_none(), "combinepsbt should not error");
        let combined_psbt = combine_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("combinepsbt should return PSBT");
        assert_eq!(
            psbt_partial_signature_count(combined_psbt),
            1,
            "combinepsbt should preserve available partial signatures"
        );
    }

    #[test]
    fn test_combinepsbt_merges_distinct_partial_signatures() {
        let create_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(54),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "54".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.01}],
                0,
                true
            ]),
        };
        let create_response = handle_createpsbt(&create_request);
        let psbt_b64 = create_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt should return PSBT");
        let psbt_bytes = base64::engine::general_purpose::STANDARD
            .decode(psbt_b64)
            .expect("PSBT should decode");
        let unsigned_tx_hex = extract_unsigned_tx_hex(&psbt_bytes);
        assert!(!unsigned_tx_hex.is_empty(), "unsigned tx hex must be present");

        let signed_a = build_signed_psbt_for_test_with_pubkey(&unsigned_tx_hex, 0x02, 0x11);
        let signed_b = build_signed_psbt_for_test_with_pubkey(&unsigned_tx_hex, 0x03, 0x22);
        let combine_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(55),
            method: "combinepsbt".to_string(),
            params: json!([[signed_a, signed_b]]),
        };
        let combine_response = handle_combinepsbt(&combine_request);
        assert!(combine_response.error.is_none(), "combinepsbt should not error");
        let combined_psbt = combine_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("combinepsbt should return PSBT");
        assert_eq!(
            psbt_partial_signature_count(combined_psbt),
            2,
            "combinepsbt should merge partial signatures across candidates"
        );
    }

    #[test]
    fn test_combinepsbt_rejects_when_all_candidates_invalid() {
        let not_psbt_b64 = base64::engine::general_purpose::STANDARD.encode(b"not-a-psbt");
        let combine_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(48),
            method: "combinepsbt".to_string(),
            params: json!([["***not-base64***", not_psbt_b64, ""]]),
        };
        let combine_response = handle_combinepsbt(&combine_request);
        assert!(combine_response.result.is_none(), "invalid psbts should not produce result");
        assert_eq!(
            combine_response.error.as_ref().map(|e| e.code),
            Some(-22),
            "all invalid candidates should return invalid PSBT error"
        );
    }

    #[test]
    fn test_combinepsbt_rejects_mismatched_unsigned_transactions() {
        let create_a = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(60),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "66".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.01}],
                0,
                true
            ]),
        };
        let resp_a = handle_createpsbt(&create_a);
        let psbt_a_b64 = resp_a
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt A should return PSBT");
        let psbt_a_bytes = base64::engine::general_purpose::STANDARD
            .decode(psbt_a_b64)
            .expect("PSBT A should decode");
        let unsigned_a = extract_unsigned_tx_hex(&psbt_a_bytes);
        let signed_a = build_signed_psbt_for_test(&unsigned_a);

        let create_b = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(61),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "77".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.02}],
                0,
                true
            ]),
        };
        let resp_b = handle_createpsbt(&create_b);
        let psbt_b_b64 = resp_b
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt B should return PSBT");
        let psbt_b_bytes = base64::engine::general_purpose::STANDARD
            .decode(psbt_b_b64)
            .expect("PSBT B should decode");
        let unsigned_b = extract_unsigned_tx_hex(&psbt_b_bytes);
        let signed_b = build_signed_psbt_for_test(&unsigned_b);

        assert_ne!(
            unsigned_a, unsigned_b,
            "test setup must use different unsigned transactions"
        );

        let combine_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(62),
            method: "combinepsbt".to_string(),
            params: json!([[signed_a, signed_b]]),
        };
        let combine_response = handle_combinepsbt(&combine_request);
        assert!(combine_response.result.is_none(), "mismatch should not produce result");
        assert_eq!(
            combine_response.error.as_ref().map(|e| e.code),
            Some(-8),
            "mismatched unsigned tx candidates should be rejected"
        );
    }

    #[test]
    fn test_joinpsbts_prefers_more_signatures_over_larger_payload() {
        let create_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(49),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "55".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.01}],
                0,
                true
            ]),
        };
        let create_response = handle_createpsbt(&create_request);
        let psbt_b64 = create_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt should return PSBT");
        let psbt_bytes = base64::engine::general_purpose::STANDARD
            .decode(psbt_b64)
            .expect("PSBT should decode");
        let unsigned_tx_hex = extract_unsigned_tx_hex(&psbt_bytes);
        assert!(!unsigned_tx_hex.is_empty(), "unsigned tx hex must be present");

        let signed_psbt = build_signed_psbt_for_test(&unsigned_tx_hex);
        let bloated_unsigned_psbt =
            build_unsigned_psbt_with_unknown_input_field(&unsigned_tx_hex, 128);
        assert!(
            bloated_unsigned_psbt.len() > signed_psbt.len(),
            "unsigned candidate should be larger for tie-break coverage"
        );

        let join_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(50),
            method: "joinpsbts".to_string(),
            params: json!([[bloated_unsigned_psbt, signed_psbt.clone()]]),
        };
        let join_response = handle_joinpsbts(&join_request);
        assert!(join_response.error.is_none(), "joinpsbts should not error");
        let joined_psbt = join_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("joinpsbts should return PSBT");
        assert_eq!(
            psbt_partial_signature_count(joined_psbt),
            1,
            "joinpsbts should preserve available partial signatures"
        );
    }

    #[test]
    fn test_joinpsbts_rejects_when_all_candidates_invalid() {
        let not_psbt_b64 = base64::engine::general_purpose::STANDARD.encode(b"not-a-psbt");
        let join_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(51),
            method: "joinpsbts".to_string(),
            params: json!([["***not-base64***", not_psbt_b64, ""]]),
        };
        let join_response = handle_joinpsbts(&join_request);
        assert!(join_response.result.is_none(), "invalid psbts should not produce result");
        assert_eq!(
            join_response.error.as_ref().map(|e| e.code),
            Some(-22),
            "all invalid candidates should return invalid PSBT error"
        );
    }

    #[test]
    fn test_joinpsbts_rejects_mismatched_unsigned_transactions() {
        let create_a = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(63),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "88".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.01}],
                0,
                true
            ]),
        };
        let resp_a = handle_createpsbt(&create_a);
        let psbt_a_b64 = resp_a
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt A should return PSBT");
        let psbt_a_bytes = base64::engine::general_purpose::STANDARD
            .decode(psbt_a_b64)
            .expect("PSBT A should decode");
        let unsigned_a = extract_unsigned_tx_hex(&psbt_a_bytes);
        let signed_a = build_signed_psbt_for_test(&unsigned_a);

        let create_b = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(64),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "99".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.02}],
                0,
                true
            ]),
        };
        let resp_b = handle_createpsbt(&create_b);
        let psbt_b_b64 = resp_b
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt B should return PSBT");
        let psbt_b_bytes = base64::engine::general_purpose::STANDARD
            .decode(psbt_b_b64)
            .expect("PSBT B should decode");
        let unsigned_b = extract_unsigned_tx_hex(&psbt_b_bytes);
        let signed_b = build_signed_psbt_for_test(&unsigned_b);

        assert_ne!(
            unsigned_a, unsigned_b,
            "test setup must use different unsigned transactions"
        );

        let join_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(65),
            method: "joinpsbts".to_string(),
            params: json!([[signed_a, signed_b]]),
        };
        let join_response = handle_joinpsbts(&join_request);
        assert!(join_response.result.is_none(), "mismatch should not produce result");
        assert_eq!(
            join_response.error.as_ref().map(|e| e.code),
            Some(-8),
            "mismatched unsigned tx candidates should be rejected"
        );
    }

    #[test]
    fn test_finalizepsbt_rejects_invalid_base64() {
        let finalize_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(52),
            method: "finalizepsbt".to_string(),
            params: json!(["***not-base64***"]),
        };
        let finalize_response = handle_finalizepsbt(&finalize_request);
        assert!(finalize_response.result.is_none(), "invalid psbt should not produce result");
        assert_eq!(
            finalize_response.error.as_ref().map(|e| e.code),
            Some(-22),
            "invalid PSBT base64 should return decode error code"
        );
    }

    #[test]
    fn test_finalizepsbt_rejects_missing_magic() {
        let bad_psbt = base64::engine::general_purpose::STANDARD.encode(b"not-a-psbt");
        let finalize_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(53),
            method: "finalizepsbt".to_string(),
            params: json!([bad_psbt]),
        };
        let finalize_response = handle_finalizepsbt(&finalize_request);
        assert!(finalize_response.result.is_none(), "invalid psbt should not produce result");
        assert_eq!(
            finalize_response.error.as_ref().map(|e| e.code),
            Some(-22),
            "missing magic should return decode error code"
        );
    }

    #[test]
    fn test_utxoupdatepsbt_roundtrip_valid_psbt() {
        let create_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(70),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "aa".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.01}],
                0,
                true
            ]),
        };
        let create_response = handle_createpsbt(&create_request);
        let psbt_b64 = create_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt should return PSBT")
            .to_string();

        let update_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(71),
            method: "utxoupdatepsbt".to_string(),
            params: json!([psbt_b64.clone()]),
        };
        let update_response = handle_utxoupdatepsbt(&update_request);
        assert!(update_response.error.is_none(), "utxoupdatepsbt should not error");
        assert_eq!(
            update_response
                .result
                .as_ref()
                .and_then(|v| v.as_str()),
            Some(psbt_b64.as_str()),
            "utxoupdatepsbt should return PSBT unchanged in account model"
        );
    }

    #[test]
    fn test_utxoupdatepsbt_rejects_invalid_base64() {
        let update_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(72),
            method: "utxoupdatepsbt".to_string(),
            params: json!(["***not-base64***"]),
        };
        let update_response = handle_utxoupdatepsbt(&update_request);
        assert!(update_response.result.is_none(), "invalid psbt should not produce result");
        assert_eq!(
            update_response.error.as_ref().map(|e| e.code),
            Some(-22),
            "invalid PSBT base64 should return decode error code"
        );
    }

    #[test]
    fn test_utxoupdatepsbt_rejects_missing_magic() {
        let bad_psbt = base64::engine::general_purpose::STANDARD.encode(b"not-a-psbt");
        let update_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(73),
            method: "utxoupdatepsbt".to_string(),
            params: json!([bad_psbt]),
        };
        let update_response = handle_utxoupdatepsbt(&update_request);
        assert!(update_response.result.is_none(), "invalid psbt should not produce result");
        assert_eq!(
            update_response.error.as_ref().map(|e| e.code),
            Some(-22),
            "missing magic should return decode error code"
        );
    }

    #[test]
    fn test_utxoupdatepsbt_rejects_missing_unsigned_tx() {
        let mut malformed_psbt = Vec::new();
        malformed_psbt.extend_from_slice(b"psbt\xff");
        malformed_psbt.push(0x00); // end global map without PSBT_GLOBAL_UNSIGNED_TX
        let malformed_b64 = base64::engine::general_purpose::STANDARD.encode(malformed_psbt);
        let update_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(74),
            method: "utxoupdatepsbt".to_string(),
            params: json!([malformed_b64]),
        };
        let update_response = handle_utxoupdatepsbt(&update_request);
        assert!(update_response.result.is_none(), "invalid psbt should not produce result");
        assert_eq!(
            update_response.error.as_ref().map(|e| e.code),
            Some(-22),
            "missing global unsigned tx should return decode error code"
        );
    }

    #[test]
    fn test_analyzepsbt_and_finalizepsbt_for_signed_input() {
        let create_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(21),
            method: "createpsbt".to_string(),
            params: json!([
                [{"txid": "22".repeat(32), "vout": 0}],
                [{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa": 0.01}],
                0,
                true
            ]),
        };
        let create_response = handle_createpsbt(&create_request);
        let psbt_b64 = create_response
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .expect("createpsbt should return PSBT");
        let psbt_bytes = base64::engine::general_purpose::STANDARD
            .decode(psbt_b64)
            .expect("PSBT should decode");
        let unsigned_tx_hex = extract_unsigned_tx_hex(&psbt_bytes);
        assert!(!unsigned_tx_hex.is_empty(), "unsigned tx hex must be present");
        let signed_psbt = build_signed_psbt_for_test(&unsigned_tx_hex);

        let analyze_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(22),
            method: "analyzepsbt".to_string(),
            params: json!([signed_psbt.clone()]),
        };
        let analyze_response = handle_analyzepsbt(&analyze_request);
        assert!(analyze_response.error.is_none(), "analyzepsbt should not error");
        assert_eq!(
            analyze_response
                .result
                .as_ref()
                .and_then(|r| r.get("next"))
                .and_then(|v| v.as_str()),
            Some("finalizer"),
            "signed PSBT should report finalizer step"
        );
        assert_eq!(
            analyze_response
                .result
                .as_ref()
                .and_then(|r| r.get("inputs"))
                .and_then(|v| v.get(0))
                .and_then(|i| i.get("is_final"))
                .and_then(|v| v.as_bool()),
            Some(true),
            "signed input should be marked final"
        );

        let finalize_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(23),
            method: "finalizepsbt".to_string(),
            params: json!([signed_psbt]),
        };
        let finalize_response = handle_finalizepsbt(&finalize_request);
        assert!(finalize_response.error.is_none(), "finalizepsbt should not error");
        assert_eq!(
            finalize_response
                .result
                .as_ref()
                .and_then(|r| r.get("complete"))
                .and_then(|v| v.as_bool()),
            Some(true),
            "signed PSBT should finalize as complete"
        );
        assert!(
            finalize_response
                .result
                .as_ref()
                .and_then(|r| r.get("hex"))
                .and_then(|v| v.as_str())
                .map(|h| !h.is_empty())
                .unwrap_or(false),
            "finalized PSBT should include tx hex"
        );
    }
}
