//! Synthesize UTXOs from NEAR account balances for Bitcoin wallet compatibility.
//!
//! Since Bitcoin Infinity is account-based (NEAR), we synthesize a single
//! UTXO per address from the account balance. This allows Bitcoin wallets
//! to see spendable coins via the `listunspent` RPC method.

use sha2::{Digest, Sha256};

/// A synthetic UTXO generated from an account balance.
pub struct SyntheticUtxo {
    /// Deterministic txid derived from account_id.
    ///
    /// This must remain stable across blocks so wallet coin-control locks
    /// (`lockunspent`) remain effective.
    pub txid: String,
    /// Always 0 (single output per synthetic tx)
    pub vout: u32,
    /// The account/address this UTXO belongs to
    pub address: String,
    /// Amount in BTC
    pub amount_btc: f64,
    /// Amount in satoshis
    pub amount_satoshis: u64,
    /// Always 6 (account-based = always confirmed)
    pub confirmations: u64,
    /// Whether this UTXO is spendable
    pub spendable: bool,
    /// The scriptPubKey hex (synthetic P2PKH)
    pub script_pub_key: String,
}

impl SyntheticUtxo {
    /// Create a synthetic UTXO from an account balance.
    ///
    /// The txid is deterministic based on account ID only, so wallets can
    /// reliably reference and lock individual synthetic outputs.
    pub fn from_account(account_id: &str, balance_satoshis: u64) -> Self {
        let txid = Self::deterministic_txid(account_id);
        let amount_btc = balance_satoshis as f64 / 100_000_000.0;

        // Derive scriptPubKey from address format
        let script_pub_key = Self::derive_script_pub_key(account_id);

        SyntheticUtxo {
            txid,
            vout: 0,
            address: account_id.to_string(),
            amount_btc,
            amount_satoshis: balance_satoshis,
            confirmations: 6,
            spendable: true,
            script_pub_key,
        }
    }

    /// Generate a deterministic txid from account_id.
    fn deterministic_txid(account_id: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"bitinfinity-utxo:");
        hasher.update(account_id.as_bytes());
        let hash1 = hasher.finalize();
        // Double SHA256 like Bitcoin
        let hash2 = Sha256::digest(&hash1);
        hex::encode(hash2)
    }

    /// Derive a synthetic 20-byte pubkey hash from account_id.
    fn account_to_pubkey_hash(account_id: &str) -> [u8; 20] {
        let hash = Sha256::digest(account_id.as_bytes());
        let mut result = [0u8; 20];
        result.copy_from_slice(&hash[..20]);
        result
    }

    /// Derive scriptPubKey from address format.
    /// For bech32 addresses: decode witness program from bech32 encoding.
    /// For base58 addresses: decode pubkey/script hash from base58check.
    /// Falls back to synthetic P2PKH for unrecognized formats.
    fn derive_script_pub_key(account_id: &str) -> String {
        // Bech32 SegWit (P2WPKH): bc1q... or tb1q...
        if account_id.starts_with("bc1q")
            || account_id.starts_with("tb1q")
            || account_id.starts_with("bcrt1q")
        {
            if let Some(program) = Self::decode_bech32_program(account_id) {
                if program.len() == 20 {
                    return format!("0014{}", hex::encode(&program));
                }
            }
        }
        // Bech32m Taproot (P2TR): bc1p... or tb1p...
        if account_id.starts_with("bc1p")
            || account_id.starts_with("tb1p")
            || account_id.starts_with("bcrt1p")
        {
            if let Some(program) = Self::decode_bech32_program(account_id) {
                if program.len() == 32 {
                    return format!("5120{}", hex::encode(&program));
                }
            }
        }
        // P2SH: starts with 3 (mainnet) or 2 (testnet)
        if account_id.starts_with('3') || account_id.starts_with('2') {
            if let Ok(decoded) = bs58::decode(account_id).into_vec() {
                if decoded.len() >= 25 {
                    return format!("a914{}87", hex::encode(&decoded[1..21]));
                }
            }
        }
        // P2PKH: starts with 1 (mainnet), m/n (testnet)
        if account_id.starts_with('1') || account_id.starts_with('m') || account_id.starts_with('n')
        {
            if let Ok(decoded) = bs58::decode(account_id).into_vec() {
                if decoded.len() >= 25 {
                    return format!("76a914{}88ac", hex::encode(&decoded[1..21]));
                }
            }
        }
        // Fallback: synthetic P2PKH from hash of account_id
        let pubkey_hash = Self::account_to_pubkey_hash(account_id);
        format!("76a914{}88ac", hex::encode(&pubkey_hash))
    }

    /// Decode bech32/bech32m address to extract the witness program bytes.
    fn decode_bech32_program(addr: &str) -> Option<Vec<u8>> {
        // Find the separator '1' (last occurrence)
        let sep_pos = addr.rfind('1')?;
        let data_part = &addr[sep_pos + 1..];
        // Skip first char (witness version), decode 5-bit to 8-bit
        const BECH32_CHARSET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
        let data5: Vec<u8> = data_part
            .chars()
            .filter_map(|c| BECH32_CHARSET.find(c).map(|i| i as u8))
            .collect();
        if data5.len() <= 7 {
            return None;
        } // too short (version + min data + 6 checksum)
          // Skip version byte (index 0), remove 6 checksum bytes
        let payload5 = &data5[1..data5.len() - 6];
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
        Some(program)
    }

    /// Derive a P2WPKH bech32 address from a 20-byte pubkey hash.
    /// Used by importpubkey to derive watch-only address from a public key.
    pub fn derive_script_pub_key_address(pubkey_hash: &[u8], bech32_hrp: &str) -> String {
        if pubkey_hash.len() != 20 {
            return String::new();
        }
        // Bech32 encode witness version 0 + 20-byte program
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

        let mut data5 = vec![0u8]; // witness version 0
        let mut acc: u32 = 0;
        let mut bits: u32 = 0;
        for &byte in pubkey_hash {
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

        let mut values = hrp_expand(bech32_hrp);
        values.extend_from_slice(&data5);
        values.extend_from_slice(&[0u8; 6]);
        let polymod_val = polymod(&values) ^ 1;
        let checksum: Vec<u8> = (0..6)
            .map(|i| ((polymod_val >> (5 * (5 - i))) & 31) as u8)
            .collect();

        let mut result = String::from(bech32_hrp);
        result.push('1');
        for &d in data5.iter().chain(checksum.iter()) {
            result.push(CHARSET[d as usize] as char);
        }
        result
    }

    /// Convert to JSON for the listunspent response.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "txid": self.txid,
            "vout": self.vout,
            "address": self.address,
            "scriptPubKey": self.script_pub_key,
            "amount": self.amount_btc,
            "confirmations": self.confirmations,
            "spendable": self.spendable,
            "solvable": true,
            "safe": true
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_txid() {
        let txid1 = SyntheticUtxo::deterministic_txid("1abc123");
        let txid2 = SyntheticUtxo::deterministic_txid("1abc123");
        let txid3 = SyntheticUtxo::deterministic_txid("1def456");

        // Same inputs = same txid
        assert_eq!(txid1, txid2);
        // Different addresses = different txid
        assert_ne!(txid1, txid3);
        // Should be 64 hex chars (32 bytes)
        assert_eq!(txid1.len(), 64);
    }

    #[test]
    fn test_from_account() {
        let utxo = SyntheticUtxo::from_account("1abc123", 50_000_000);
        assert_eq!(utxo.vout, 0);
        assert_eq!(utxo.amount_btc, 0.5);
        assert_eq!(utxo.amount_satoshis, 50_000_000);
        assert_eq!(utxo.confirmations, 6);
        assert!(utxo.spendable);
        assert!(utxo.script_pub_key.starts_with("76a914"));
        assert!(utxo.script_pub_key.ends_with("88ac"));
    }
}
