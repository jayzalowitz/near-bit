//! Manual NEAR transaction construction and signing.
//!
//! Constructs borsh-encoded NEAR SignedTransaction without importing
//! near-primitives, keeping the dependency tree small.
//!
//! Supports TransactionV0 format with secp256k1 signatures.
//! All NEAR action types: CreateAccount, DeployContract, FunctionCall,
//! Transfer, Stake, AddKey, DeleteKey, DeleteAccount.

use secp256k1::{Message, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};

// ============================================================================
// NEAR Action Discriminants
// ============================================================================

/// All NEAR action type discriminant bytes (TransactionV0 format).
pub mod action_kind {
    pub const CREATE_ACCOUNT: u8 = 0x00;
    pub const DEPLOY_CONTRACT: u8 = 0x01;
    pub const FUNCTION_CALL: u8 = 0x02;
    pub const TRANSFER: u8 = 0x03;
    pub const STAKE: u8 = 0x04;
    pub const ADD_KEY: u8 = 0x05;
    pub const DELETE_KEY: u8 = 0x06;
    pub const DELETE_ACCOUNT: u8 = 0x07;
    pub const DELEGATE: u8 = 0x08;
    pub const DEPLOY_GLOBAL_CONTRACT: u8 = 0x09;
    pub const USE_GLOBAL_CONTRACT: u8 = 0x0A;
    pub const DETERMINISTIC_STATE_INIT: u8 = 0x0B;
    pub const TRANSFER_TO_GAS_KEY: u8 = 0x0C;
    pub const WITHDRAW_FROM_GAS_KEY: u8 = 0x0D;
}

// ============================================================================
// Borsh-serialized NEAR Actions
// ============================================================================

/// A pre-serialized NEAR action (borsh bytes).
pub struct NearAction(pub Vec<u8>);

impl NearAction {
    /// CreateAccount — empty action, just the discriminant.
    pub fn create_account() -> Self {
        NearAction(vec![action_kind::CREATE_ACCOUNT])
    }

    /// DeployContract — deploys WASM code to the account.
    pub fn deploy_contract(code: &[u8]) -> Self {
        let mut buf = Vec::with_capacity(1 + 4 + code.len());
        buf.push(action_kind::DEPLOY_CONTRACT);
        buf.extend_from_slice(&(code.len() as u32).to_le_bytes());
        buf.extend_from_slice(code);
        NearAction(buf)
    }

    /// FunctionCall — calls a method on a contract.
    pub fn function_call(method_name: &str, args: &[u8], gas: u64, deposit: u128) -> Self {
        let mut buf = Vec::with_capacity(128);
        buf.push(action_kind::FUNCTION_CALL);
        borsh_write_string(&mut buf, method_name);
        buf.extend_from_slice(&(args.len() as u32).to_le_bytes());
        buf.extend_from_slice(args);
        buf.extend_from_slice(&gas.to_le_bytes());
        buf.extend_from_slice(&deposit.to_le_bytes());
        NearAction(buf)
    }

    /// Transfer — sends tokens.
    pub fn transfer(deposit: u128) -> Self {
        let mut buf = Vec::with_capacity(17);
        buf.push(action_kind::TRANSFER);
        buf.extend_from_slice(&deposit.to_le_bytes());
        NearAction(buf)
    }

    /// Stake — locks tokens for validation. public_key is 64-byte uncompressed secp256k1.
    pub fn stake(stake_amount: u128, public_key_uncompressed: &[u8; 64]) -> Self {
        let mut buf = Vec::with_capacity(1 + 16 + 1 + 64);
        buf.push(action_kind::STAKE);
        buf.extend_from_slice(&stake_amount.to_le_bytes());
        buf.push(0x01); // PublicKey::SECP256K1
        buf.extend_from_slice(public_key_uncompressed);
        NearAction(buf)
    }

    /// Stake with ed25519 key.
    pub fn stake_ed25519(stake_amount: u128, public_key_ed25519: &[u8; 32]) -> Self {
        let mut buf = Vec::with_capacity(1 + 16 + 1 + 32);
        buf.push(action_kind::STAKE);
        buf.extend_from_slice(&stake_amount.to_le_bytes());
        buf.push(0x00); // PublicKey::ED25519
        buf.extend_from_slice(public_key_ed25519);
        NearAction(buf)
    }

    /// AddKey with FullAccess permission (secp256k1 key).
    pub fn add_full_access_key(public_key_uncompressed: &[u8; 64]) -> Self {
        let mut buf = Vec::with_capacity(1 + 1 + 64 + 8 + 1);
        buf.push(action_kind::ADD_KEY);
        // PublicKey
        buf.push(0x01); // SECP256K1
        buf.extend_from_slice(public_key_uncompressed);
        // AccessKey: nonce (u64) + permission
        buf.extend_from_slice(&0u64.to_le_bytes()); // nonce = 0
        buf.push(0x01); // AccessKeyPermission::FullAccess
        NearAction(buf)
    }

    /// AddKey with FunctionCall permission (secp256k1 key).
    pub fn add_function_call_key(
        public_key_uncompressed: &[u8; 64],
        allowance: Option<u128>,
        receiver_id: &str,
        method_names: &[&str],
    ) -> Self {
        let mut buf = Vec::with_capacity(256);
        buf.push(action_kind::ADD_KEY);
        // PublicKey
        buf.push(0x01); // SECP256K1
        buf.extend_from_slice(public_key_uncompressed);
        // AccessKey: nonce (u64) + permission
        buf.extend_from_slice(&0u64.to_le_bytes()); // nonce = 0
        buf.push(0x00); // AccessKeyPermission::FunctionCall
                        // FunctionCallPermission:
                        //   allowance: Option<u128>
        match allowance {
            Some(a) => {
                buf.push(0x01); // Some
                buf.extend_from_slice(&a.to_le_bytes());
            }
            None => {
                buf.push(0x00); // None
            }
        }
        //   receiver_id: String
        borsh_write_string(&mut buf, receiver_id);
        //   method_names: Vec<String>
        buf.extend_from_slice(&(method_names.len() as u32).to_le_bytes());
        for name in method_names {
            borsh_write_string(&mut buf, name);
        }
        NearAction(buf)
    }

    /// DeleteKey (secp256k1 key).
    pub fn delete_key(public_key_uncompressed: &[u8; 64]) -> Self {
        let mut buf = Vec::with_capacity(1 + 1 + 64);
        buf.push(action_kind::DELETE_KEY);
        buf.push(0x01); // SECP256K1
        buf.extend_from_slice(public_key_uncompressed);
        NearAction(buf)
    }

    /// AddKey with FullAccess permission (Ed25519 key, 32 bytes).
    pub fn add_full_access_key_ed25519(public_key: &[u8; 32]) -> Self {
        let mut buf = Vec::with_capacity(1 + 1 + 32 + 8 + 1);
        buf.push(action_kind::ADD_KEY);
        buf.push(0x00); // ED25519
        buf.extend_from_slice(public_key);
        buf.extend_from_slice(&0u64.to_le_bytes()); // nonce = 0
        buf.push(0x01); // AccessKeyPermission::FullAccess
        NearAction(buf)
    }

    /// DeleteKey (Ed25519 key, 32 bytes).
    pub fn delete_key_ed25519(public_key: &[u8; 32]) -> Self {
        let mut buf = Vec::with_capacity(1 + 1 + 32);
        buf.push(action_kind::DELETE_KEY);
        buf.push(0x00); // ED25519
        buf.extend_from_slice(public_key);
        NearAction(buf)
    }

    /// DeleteAccount — closes account and sends balance to beneficiary.
    pub fn delete_account(beneficiary_id: &str) -> Self {
        let mut buf = Vec::with_capacity(1 + 4 + beneficiary_id.len());
        buf.push(action_kind::DELETE_ACCOUNT);
        borsh_write_string(&mut buf, beneficiary_id);
        NearAction(buf)
    }

    /// DeployGlobalContract — deploy a shared WASM contract by code hash.
    /// `code` is the WASM bytecode.
    pub fn deploy_global_contract(code: &[u8]) -> Self {
        let mut buf = Vec::with_capacity(1 + 4 + code.len());
        buf.push(action_kind::DEPLOY_GLOBAL_CONTRACT);
        // DeployGlobalContractAction: deploy_by_code variant (0x00) + code bytes
        buf.push(0x00); // ByCode variant
        buf.extend_from_slice(&(code.len() as u32).to_le_bytes());
        buf.extend_from_slice(code);
        NearAction(buf)
    }

    /// DeployGlobalContract by code hash (32-byte SHA256 hash of the WASM).
    pub fn deploy_global_contract_by_hash(code_hash: &[u8; 32]) -> Self {
        let mut buf = Vec::with_capacity(1 + 1 + 32);
        buf.push(action_kind::DEPLOY_GLOBAL_CONTRACT);
        buf.push(0x01); // ByHash variant
        buf.extend_from_slice(code_hash);
        NearAction(buf)
    }

    /// UseGlobalContract — attach a global contract to an account.
    pub fn use_global_contract_by_hash(code_hash: &[u8; 32]) -> Self {
        let mut buf = Vec::with_capacity(1 + 1 + 32);
        buf.push(action_kind::USE_GLOBAL_CONTRACT);
        buf.push(0x00); // ByHash variant
        buf.extend_from_slice(code_hash);
        NearAction(buf)
    }

    /// UseGlobalContract by account ID.
    pub fn use_global_contract_by_account(account_id: &str) -> Self {
        let mut buf = Vec::with_capacity(1 + 1 + 4 + account_id.len());
        buf.push(action_kind::USE_GLOBAL_CONTRACT);
        buf.push(0x01); // ByAccountId variant
        borsh_write_string(&mut buf, account_id);
        NearAction(buf)
    }

    /// TransferToGasKey — fund a gas key with NEAR tokens.
    pub fn transfer_to_gas_key(public_key_uncompressed: &[u8; 64], deposit: u128) -> Self {
        let mut buf = Vec::with_capacity(1 + 1 + 64 + 16);
        buf.push(action_kind::TRANSFER_TO_GAS_KEY);
        buf.push(0x01); // SECP256K1
        buf.extend_from_slice(public_key_uncompressed);
        buf.extend_from_slice(&deposit.to_le_bytes());
        NearAction(buf)
    }

    /// WithdrawFromGasKey — withdraw NEAR from a gas key.
    pub fn withdraw_from_gas_key(public_key_uncompressed: &[u8; 64], amount: u128) -> Self {
        let mut buf = Vec::with_capacity(1 + 1 + 64 + 16);
        buf.push(action_kind::WITHDRAW_FROM_GAS_KEY);
        buf.push(0x01); // SECP256K1
        buf.extend_from_slice(public_key_uncompressed);
        buf.extend_from_slice(&amount.to_le_bytes());
        NearAction(buf)
    }

    /// DeterministicStateInit — initialize state deterministically.
    pub fn deterministic_state_init() -> Self {
        NearAction(vec![action_kind::DETERMINISTIC_STATE_INIT])
    }

    /// Delegate (meta-transaction wrapper). Takes pre-serialized DelegateAction bytes.
    /// The DelegateAction is typically signed by the actual user, and the wrapping
    /// transaction is signed by a relayer.
    pub fn delegate(delegate_action_bytes: &[u8], signature_bytes: &[u8]) -> Self {
        let mut buf =
            Vec::with_capacity(1 + 4 + delegate_action_bytes.len() + 1 + signature_bytes.len());
        buf.push(action_kind::DELEGATE);
        // SignedDelegateAction: delegate_action bytes + signature
        buf.extend_from_slice(&(delegate_action_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(delegate_action_bytes);
        buf.extend_from_slice(&(signature_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(signature_bytes);
        NearAction(buf)
    }
}

// ============================================================================
// Generic Transaction Builder (supports multi-action)
// ============================================================================

/// Generic NEAR transaction builder supporting any combination of actions.
pub struct NearTxBuilder {
    pub signer_id: String,
    pub public_key_uncompressed: [u8; 64],
    pub nonce: u64,
    pub receiver_id: String,
    pub block_hash: [u8; 32],
    pub actions: Vec<NearAction>,
}

impl NearTxBuilder {
    pub fn new(
        signer_id: String,
        public_key_uncompressed: [u8; 64],
        nonce: u64,
        receiver_id: String,
        block_hash: [u8; 32],
    ) -> Self {
        NearTxBuilder {
            signer_id,
            public_key_uncompressed,
            nonce,
            receiver_id,
            block_hash,
            actions: Vec::new(),
        }
    }

    pub fn add_action(&mut self, action: NearAction) -> &mut Self {
        self.actions.push(action);
        self
    }

    /// Build, sign, and base64-encode the signed transaction.
    pub fn sign_and_encode(&self, secret_key: &SecretKey) -> Result<String, String> {
        if self.actions.is_empty() {
            return Err("Transaction must have at least one action".to_string());
        }

        let tx_bytes = self.borsh_serialize_tx();
        let tx_hash = Sha256::digest(&tx_bytes);

        let secp = Secp256k1::new();
        let msg = Message::from_digest_slice(&tx_hash)
            .map_err(|e| format!("Failed to create message: {}", e))?;
        let sig = secp.sign_ecdsa_recoverable(&msg, secret_key);
        let (rec_id, sig_compact) = sig.serialize_compact();

        let mut signed_tx = tx_bytes;
        signed_tx.push(0x01); // Signature::SECP256K1
        signed_tx.extend_from_slice(&sig_compact); // r[32] + s[32]
        signed_tx.push(rec_id.to_i32() as u8); // v

        use base64::Engine;
        Ok(base64::engine::general_purpose::STANDARD.encode(&signed_tx))
    }

    fn borsh_serialize_tx(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(512);

        borsh_write_string(&mut buf, &self.signer_id);
        buf.push(0x01); // PublicKey::SECP256K1
        buf.extend_from_slice(&self.public_key_uncompressed);
        buf.extend_from_slice(&self.nonce.to_le_bytes());
        borsh_write_string(&mut buf, &self.receiver_id);
        buf.extend_from_slice(&self.block_hash);

        // actions: Vec<Action>
        buf.extend_from_slice(&(self.actions.len() as u32).to_le_bytes());
        for action in &self.actions {
            buf.extend_from_slice(&action.0);
        }

        buf
    }
}

// ============================================================================
// Legacy convenience wrappers (backward compatible)
// ============================================================================

/// Parameters for a NEAR transfer transaction.
pub struct NearTransferParams {
    pub signer_id: String,
    pub public_key_uncompressed: [u8; 64],
    pub nonce: u64,
    pub receiver_id: String,
    pub block_hash: [u8; 32],
    pub deposit: u128,
}

impl NearTransferParams {
    pub fn sign_and_encode(&self, secret_key: &SecretKey) -> Result<String, String> {
        let mut builder = NearTxBuilder::new(
            self.signer_id.clone(),
            self.public_key_uncompressed,
            self.nonce,
            self.receiver_id.clone(),
            self.block_hash,
        );
        builder.add_action(NearAction::transfer(self.deposit));
        builder.sign_and_encode(secret_key)
    }

    /// Borsh-serialize just the Transaction (V0 format) — kept for test compat.
    fn borsh_serialize_tx(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        borsh_write_string(&mut buf, &self.signer_id);
        buf.push(0x01);
        buf.extend_from_slice(&self.public_key_uncompressed);
        buf.extend_from_slice(&self.nonce.to_le_bytes());
        borsh_write_string(&mut buf, &self.receiver_id);
        buf.extend_from_slice(&self.block_hash);
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.push(0x03);
        buf.extend_from_slice(&self.deposit.to_le_bytes());
        buf
    }
}

/// Parameters for a NEAR function call transaction.
pub struct NearFunctionCallParams {
    pub signer_id: String,
    pub public_key_uncompressed: [u8; 64],
    pub nonce: u64,
    pub receiver_id: String,
    pub block_hash: [u8; 32],
    pub method_name: String,
    pub args: Vec<u8>,
    pub gas: u64,
    pub deposit: u128,
}

impl NearFunctionCallParams {
    pub fn sign_and_encode(&self, secret_key: &SecretKey) -> Result<String, String> {
        let mut builder = NearTxBuilder::new(
            self.signer_id.clone(),
            self.public_key_uncompressed,
            self.nonce,
            self.receiver_id.clone(),
            self.block_hash,
        );
        builder.add_action(NearAction::function_call(
            &self.method_name,
            &self.args,
            self.gas,
            self.deposit,
        ));
        builder.sign_and_encode(secret_key)
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Borsh-write a string: u32 LE length + UTF-8 bytes.
fn borsh_write_string(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

/// Decode a base58-encoded NEAR block hash to 32 bytes.
pub fn decode_block_hash(hash_str: &str) -> Result<[u8; 32], String> {
    let bytes = bs58::decode(hash_str)
        .into_vec()
        .map_err(|e| format!("Invalid base58 block hash: {}", e))?;
    if bytes.len() != 32 {
        return Err(format!("Block hash must be 32 bytes, got {}", bytes.len()));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borsh_write_string() {
        let mut buf = Vec::new();
        borsh_write_string(&mut buf, "hello");
        assert_eq!(buf.len(), 4 + 5);
        assert_eq!(&buf[..4], &5u32.to_le_bytes());
        assert_eq!(&buf[4..], b"hello");
    }

    #[test]
    fn test_borsh_serialize_transfer_tx() {
        let params = NearTransferParams {
            signer_id: "alice".to_string(),
            public_key_uncompressed: [0x01; 64],
            nonce: 1,
            receiver_id: "bob".to_string(),
            block_hash: [0xAA; 32],
            deposit: 1_000_000,
        };
        let tx_bytes = params.borsh_serialize_tx();
        // signer_id: 4+5=9, pubkey: 1+64=65, nonce: 8, receiver: 4+3=7,
        // block_hash: 32, vec len: 4, Transfer: 1+16=17 = 142
        assert_eq!(tx_bytes.len(), 142);
    }

    #[test]
    fn test_sign_and_encode() {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let uncompressed = public_key.serialize_uncompressed();
        let mut pk_64 = [0u8; 64];
        pk_64.copy_from_slice(&uncompressed[1..]);

        let params = NearTransferParams {
            signer_id: "1abc123def456".to_string(),
            public_key_uncompressed: pk_64,
            nonce: 1,
            receiver_id: "1xyz789ghi012".to_string(),
            block_hash: [0xBB; 32],
            deposit: 10_000_000_000_000_000,
        };

        let result = params.sign_and_encode(&secret_key);
        assert!(result.is_ok());

        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD.decode(result.unwrap());
        assert!(decoded.is_ok());
    }

    #[test]
    fn test_decode_block_hash() {
        let hash_bytes = [0xAA; 32];
        let encoded = bs58::encode(&hash_bytes).into_string();
        let decoded = decode_block_hash(&encoded).unwrap();
        assert_eq!(decoded, hash_bytes);
    }

    #[test]
    fn test_multi_action_tx() {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let uncompressed = public_key.serialize_uncompressed();
        let mut pk_64 = [0u8; 64];
        pk_64.copy_from_slice(&uncompressed[1..]);

        let mut builder = NearTxBuilder::new(
            "sender".to_string(),
            pk_64,
            1,
            "receiver".to_string(),
            [0xCC; 32],
        );
        builder.add_action(NearAction::create_account());
        builder.add_action(NearAction::transfer(1_000_000));
        builder.add_action(NearAction::add_full_access_key(&pk_64));

        let result = builder.sign_and_encode(&secret_key);
        assert!(result.is_ok());
    }

    #[test]
    fn test_all_action_types_serialize() {
        // Just verify they don't panic
        let pk = [0x01u8; 64];
        let _ = NearAction::create_account();
        let _ = NearAction::deploy_contract(b"fake wasm");
        let _ = NearAction::function_call("method", b"{}", 300_000_000_000_000, 0);
        let _ = NearAction::transfer(1_000_000);
        let _ = NearAction::stake(1_000_000, &pk);
        let _ = NearAction::add_full_access_key(&pk);
        let _ = NearAction::add_function_call_key(
            &pk,
            Some(1_000_000),
            "contract.near",
            &["method1", "method2"],
        );
        let _ = NearAction::delete_key(&pk);
        let _ = NearAction::delete_account("beneficiary.near");
    }
}
