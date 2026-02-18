//! Generate nearcore-compatible key files (node_key.json, validator_key.json).
//!
//! nearcore key files have this format:
//! {
//!   "account_id": "validator.bitinfinity",
//!   "public_key": "ed25519:BASE58_PUBKEY",
//!   "secret_key": "ed25519:BASE58_SECRET"
//! }

use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

/// nearcore-compatible key file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFile {
    pub account_id: String,
    pub public_key: String,
    pub secret_key: String,
}

/// Generate a new ed25519 key file for the given account ID.
pub fn generate_key_file(account_id: &str) -> KeyFile {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    let public_key = format!("ed25519:{}", bs58::encode(verifying_key.as_bytes()).into_string());
    let secret_key = format!(
        "ed25519:{}",
        bs58::encode(signing_key.to_keypair_bytes()).into_string()
    );

    KeyFile {
        account_id: account_id.to_string(),
        public_key,
        secret_key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_key_file() {
        let kf = generate_key_file("test.near");
        assert_eq!(kf.account_id, "test.near");
        assert!(kf.public_key.starts_with("ed25519:"));
        assert!(kf.secret_key.starts_with("ed25519:"));

        // Public key should be 32 bytes = ~44 base58 chars
        let pub_b58 = kf.public_key.strip_prefix("ed25519:").unwrap();
        let pub_bytes = bs58::decode(pub_b58).into_vec().unwrap();
        assert_eq!(pub_bytes.len(), 32);

        // Secret key should be 64 bytes (keypair) = ~88 base58 chars
        let sec_b58 = kf.secret_key.strip_prefix("ed25519:").unwrap();
        let sec_bytes = bs58::decode(sec_b58).into_vec().unwrap();
        assert_eq!(sec_bytes.len(), 64);
    }

    #[test]
    fn test_key_file_serialization() {
        let kf = generate_key_file("validator.bitinfinity");
        let json = serde_json::to_string_pretty(&kf).unwrap();
        let parsed: KeyFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.account_id, "validator.bitinfinity");
        assert_eq!(parsed.public_key, kf.public_key);
        assert_eq!(parsed.secret_key, kf.secret_key);
    }
}
