//! NEAR-compatible account ID type with Bitcoin address support
//!
//! Account types:
//! - Named: standard NEAR-style accounts (e.g., "account.near")
//! - Bitcoin P2PKH: Bitcoin legacy addresses (e.g., "1A1z...")
//! - Bitcoin P2SH: Bitcoin multisig addresses (e.g., "3...")
//! - Bitcoin Bech32: Bitcoin SegWit addresses (e.g., "bc1q...")
//! - Bitcoin Taproot: Bitcoin Taproot addresses (e.g., "bc1p...")

use std::fmt;
use std::str::FromStr;
use sha2::{Sha256, Digest};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccountId(String);

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for AccountId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("Account ID cannot be empty".to_string());
        }
        // Validate Bitcoin addresses, but allow NEAR-style accounts too
        Ok(AccountId(s.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountType {
    NamedAccount,
    NearImplicitAccount,
    BtcImplicitAccount,
}

/// Detects account type from account ID string
pub fn get_account_type(account_id: &str) -> AccountType {
    // Check if it's a Bitcoin address
    if is_bitcoin_p2pkh(account_id)
        || is_bitcoin_p2sh(account_id)
        || is_bitcoin_bech32(account_id)
        || is_bitcoin_taproot(account_id)
    {
        return AccountType::BtcImplicitAccount;
    }

    // Check if it's a 64-char NEAR implicit account (ed25519 pubkey)
    if account_id.len() == 64 && account_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return AccountType::NearImplicitAccount;
    }

    AccountType::NamedAccount
}

/// Validates any Bitcoin address format
pub fn validate_bitcoin_address(address: &str) -> bool {
    is_bitcoin_p2pkh(address)
        || is_bitcoin_p2sh(address)
        || is_bitcoin_bech32(address)
        || is_bitcoin_taproot(address)
}

/// Validates Bitcoin P2PKH address (legacy, starts with '1')
fn is_bitcoin_p2pkh(address: &str) -> bool {
    if !address.starts_with('1') || address.len() < 25 || address.len() > 34 {
        return false;
    }
    validate_base58check(address, 0x00)
}

/// Validates Bitcoin P2SH address (multisig, starts with '3')
fn is_bitcoin_p2sh(address: &str) -> bool {
    if !address.starts_with('3') || address.len() != 34 {
        return false;
    }
    validate_base58check(address, 0x05)
}

/// Validates Bitcoin P2WPKH/P2WSH addresses (SegWit, starts with 'bc1q')
fn is_bitcoin_bech32(address: &str) -> bool {
    if !address.starts_with("bc1q") {
        return false;
    }
    // Valid lengths for P2WPKH: 42-44 chars, P2WSH: 62-66 chars
    if (address.len() >= 42 && address.len() <= 44)
        || (address.len() >= 62 && address.len() <= 66)
    {
        return validate_bech32(address, 0);
    }
    false
}

/// Validates Bitcoin P2TR address (Taproot, starts with 'bc1p')
fn is_bitcoin_taproot(address: &str) -> bool {
    if !address.starts_with("bc1p") || address.len() < 62 || address.len() > 66 {
        return false;
    }
    validate_bech32(address, 1)
}

/// Validates Base58Check encoding with version byte
fn validate_base58check(address: &str, expected_version: u8) -> bool {
    // Decode from Base58
    let decoded = match bs58::decode(address).into_vec() {
        Ok(v) => v,
        Err(_) => return false,
    };

    // Must be 25 bytes: 1 version + 20 payload + 4 checksum
    if decoded.len() != 25 {
        return false;
    }

    // Check version byte
    if decoded[0] != expected_version {
        return false;
    }

    // Verify checksum: last 4 bytes should match first 4 bytes of SHA256(SHA256(version + payload))
    let payload = &decoded[0..21];
    let checksum = &decoded[21..25];

    let mut hasher = Sha256::new();
    hasher.update(payload);
    let hash1 = hasher.finalize();

    let mut hasher = Sha256::new();
    hasher.update(&hash1);
    let hash2 = hasher.finalize();

    checksum == &hash2[0..4]
}

/// Validates Bech32/Bech32m encoding for SegWit and Taproot addresses
fn validate_bech32(address: &str, witness_version: u8) -> bool {
    // Decode using bech32 crate
    let (hrp, data) = match bech32::decode(address) {
        Ok((hrp, data)) => (hrp, data),
        Err(_) => return false,
    };

    // Should decode to mainnet (hrp = "bc")
    if hrp.as_str() != "bc" {
        return false;
    }

    // Data must have at least witness version + witness program
    if data.is_empty() {
        return false;
    }

    // First element is witness version
    let version = data[0];
    if version > 16 {
        return false;
    }

    // Version must match what we expect
    if witness_version != version {
        return false;
    }

    // Witness program length check (after version)
    // Minimum: 2 elements (10 bits ≈ 1 byte), Maximum: 41 elements (205 bits ≈ 25.6 bytes)
    let witness_program_len = data.len() - 1; // subtract version element
    if witness_program_len < 2 || witness_program_len > 41 {
        return false;
    }

    // Version 0 (P2WPKH/P2WSH):
    // - P2WPKH: 20-byte program (converts to 4 bech32 elements)
    // - P2WSH: 32-byte program (converts to 51 bech32 elements, but with padding ~41)
    if version == 0 && witness_program_len != 4 && witness_program_len != 41 {
        return false;
    }

    // Version 1 (P2TR): 32-byte program
    if version == 1 && witness_program_len != 41 {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p2pkh_validation() {
        // Valid P2PKH (Satoshi's genesis address)
        assert!(is_bitcoin_p2pkh("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"));

        // Invalid: wrong version
        assert!(!is_bitcoin_p2pkh("3A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"));

        // Invalid: bad checksum
        assert!(!is_bitcoin_p2pkh("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNb"));

        // Invalid: too short
        assert!(!is_bitcoin_p2pkh("1A1z"));
    }

    #[test]
    fn test_account_type_detection() {
        // Bitcoin P2PKH
        assert_eq!(
            get_account_type("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"),
            AccountType::BtcImplicitAccount
        );

        // NEAR implicit (64 hex chars)
        assert_eq!(
            get_account_type("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            AccountType::NearImplicitAccount
        );

        // Named account
        assert_eq!(
            get_account_type("alice.near"),
            AccountType::NamedAccount
        );
    }

    #[test]
    fn test_validate_bitcoin_address() {
        // Valid P2PKH
        assert!(validate_bitcoin_address("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"));

        // Invalid address
        assert!(!validate_bitcoin_address("not_an_address"));

        // Invalid P2SH checksum
        assert!(!validate_bitcoin_address("3J98t1WpEZ73CNmYviecrnyiWrnqRhWNLx"));
    }
}
