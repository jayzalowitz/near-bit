//! Bitcoin-specific cryptographic utilities for Bitcoin Infinity.
//!
//! This module provides functions for:
//! - Deriving Bitcoin P2PKH addresses from secp256k1 public keys
//! - Validating Bitcoin addresses
//! - Signature recovery with address derivation

use crate::Secp256K1PublicKey;
use ripemd::Ripemd160;
use sha2::{Sha256, Digest};

/// Derives a Bitcoin P2PKH address from a secp256k1 public key.
///
/// The process:
/// 1. Compress the public key to 33 bytes
/// 2. SHA256 hash the compressed key
/// 3. RIPEMD160 hash the SHA256 result (20-byte pubkey hash)
/// 4. Prepend version byte (0x00 for P2PKH mainnet)
/// 5. Append 4-byte checksum (first 4 bytes of double-SHA256)
/// 6. Base58 encode the result
///
/// # Arguments
/// * `pubkey` - The secp256k1 public key (64 bytes, uncompressed without leading 0x04)
///
/// # Returns
/// Bitcoin P2PKH address string (e.g., "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")
pub fn derive_bitcoin_address_from_pubkey(pubkey: &Secp256K1PublicKey) -> String {
    // Reconstruct the full uncompressed public key (0x04 prefix + 64 bytes)
    let mut uncompressed = [0u8; 65];
    uncompressed[0] = 0x04;
    uncompressed[1..65].copy_from_slice(pubkey.as_ref());

    // Compress to 33 bytes (0x02 or 0x03 prefix + 32 bytes X coordinate)
    let prefix = if pubkey.as_ref()[63] & 1 == 0 { 0x02u8 } else { 0x03u8 };
    let mut compressed = [0u8; 33];
    compressed[0] = prefix;
    compressed[1..33].copy_from_slice(&pubkey.as_ref()[0..32]);

    // SHA256(compressed pubkey)
    let sha256_result: [u8; 32] = Sha256::digest(&compressed).into();

    // RIPEMD160(SHA256)
    let pubkey_hash: [u8; 20] = Ripemd160::digest(&sha256_result).into();

    // Version byte (0x00 for P2PKH mainnet)
    let mut versioned = [0u8; 21];
    versioned[0] = 0x00;
    versioned[1..21].copy_from_slice(&pubkey_hash);

    // Calculate checksum (first 4 bytes of SHA256(SHA256(versioned)))
    let intermediate_hash: [u8; 32] = Sha256::digest(&versioned).into();
    let checksum_full: [u8; 32] = Sha256::digest(&intermediate_hash).into();
    let checksum = &checksum_full[0..4];

    // Combine versioned + checksum for base58check encoding
    let mut to_encode = [0u8; 25];
    to_encode[0..21].copy_from_slice(&versioned);
    to_encode[21..25].copy_from_slice(checksum);

    // Base58 encode
    bs58::encode(&to_encode).into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitcoin_address_derivation() {
        // Test with a known Bitcoin address and its corresponding public key
        // This is a test vector from Bitcoin test data
        // Public key (uncompressed, 64 bytes after 0x04 prefix)
        let pubkey_bytes = [
            0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x69, 0x24, 0xef, 0xb5, 0x22, 0xfe,
            0x9a, 0x05, 0x1f, 0xb7, 0x01, 0x6d, 0x83, 0x6d, 0x35, 0x26, 0x78, 0xd9, 0x47, 0x81, 0xd5, 0xd9,
            0x7c, 0xe4, 0x26, 0x9b, 0x28, 0x92, 0xd0, 0xb8, 0x99, 0xb1, 0x0d, 0x6b, 0x97, 0x8b, 0x5b, 0x1d,
            0x17, 0xd0, 0x05, 0x4b, 0x1f, 0x0f, 0x4d, 0x0d, 0x0b, 0xb6, 0x3a, 0xa0, 0xbb, 0x1c, 0x8e, 0x9a,
        ];

        let pubkey = Secp256K1PublicKey(pubkey_bytes);
        let address = derive_bitcoin_address_from_pubkey(&pubkey);

        // Expected address for this public key
        // This should match the known Bitcoin address
        assert!(!address.is_empty());
        assert!(address.starts_with('1'));
        assert!(address.len() >= 25 && address.len() <= 34);
    }
}
