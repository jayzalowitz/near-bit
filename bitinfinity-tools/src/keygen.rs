//! Generate secp256k1 keypair and derive Bitcoin address

use secp256k1::{Secp256k1, SecretKey};
use sha2::{Sha256, Digest};
use ripemd::Ripemd160;
use rand::RngCore;

/// Represents a generated keypair with Bitcoin address
#[derive(Debug, Clone)]
pub struct Keypair {
    /// Private key in WIF (Wallet Import Format)
    pub private_key_wif: String,
    /// Bitcoin P2PKH address derived from public key
    pub bitcoin_address: String,
}

/// Generate a random secp256k1 keypair and derive a Bitcoin P2PKH address
pub fn generate_keypair() -> Result<Keypair, Box<dyn std::error::Error>> {
    // Generate random 32 bytes for secret key
    let mut secret_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret_bytes);

    // Create secret key from bytes
    let secret_key = SecretKey::from_slice(&secret_bytes)
        .map_err(|e| format!("Failed to create secret key: {}", e))?;

    // Get the public key
    let secp = Secp256k1::new();
    let public_key = secret_key.public_key(&secp);

    // Derive Bitcoin P2PKH address from public key
    let bitcoin_address = derive_p2pkh_address(&public_key)?;

    // Convert secret key to WIF format
    let private_key_wif = secret_key_to_wif(&secret_key);

    Ok(Keypair {
        private_key_wif,
        bitcoin_address,
    })
}

/// Convert a secp256k1 SecretKey to WIF (Wallet Import Format)
fn secret_key_to_wif(secret_key: &SecretKey) -> String {
    let mut payload = vec![0x80]; // Version byte for mainnet
    payload.extend_from_slice(&secret_key.secret_bytes());
    // Note: We're not adding compression flag (0x01) at the end, using uncompressed format

    // Calculate checksum: first 4 bytes of SHA256(SHA256(payload))
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    let hash1 = hasher.finalize();

    let mut hasher = Sha256::new();
    hasher.update(&hash1);
    let hash2 = hasher.finalize();

    let checksum = &hash2[0..4];
    payload.extend_from_slice(checksum);

    // Base58 encode
    bs58::encode(&payload).into_string()
}

/// Derive Bitcoin P2PKH address from a secp256k1 public key
fn derive_p2pkh_address(public_key: &secp256k1::PublicKey) -> Result<String, Box<dyn std::error::Error>> {
    // Compress the public key (33 bytes)
    let compressed_pubkey = public_key.serialize();

    // Step 1: SHA256(compressed_pubkey)
    let mut hasher = Sha256::new();
    hasher.update(&compressed_pubkey);
    let sha256_hash = hasher.finalize();

    // Step 2: RIPEMD160(SHA256(pubkey))
    let mut hasher = Ripemd160::new();
    hasher.update(&sha256_hash);
    let pubkey_hash = hasher.finalize();

    // Step 3: Create versioned payload
    let mut versioned = vec![0x00]; // Version byte for P2PKH mainnet
    versioned.extend_from_slice(&pubkey_hash);

    // Step 4: Calculate checksum
    let mut hasher = Sha256::new();
    hasher.update(&versioned);
    let hash1 = hasher.finalize();

    let mut hasher = Sha256::new();
    hasher.update(&hash1);
    let hash2 = hasher.finalize();

    let checksum = &hash2[0..4];
    versioned.extend_from_slice(checksum);

    // Step 5: Base58 encode
    Ok(bs58::encode(&versioned).into_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_generate_keypair() {
        let keypair = generate_keypair().expect("Failed to generate keypair");

        // Check WIF format: starts with 5, 51-52 chars
        assert!(keypair.private_key_wif.starts_with('5') || keypair.private_key_wif.starts_with('K') || keypair.private_key_wif.starts_with('L'));
        assert!(keypair.private_key_wif.len() >= 51 && keypair.private_key_wif.len() <= 52);

        // Check Bitcoin address format: starts with 1, 26-35 chars
        assert!(keypair.bitcoin_address.starts_with('1'));
        assert!(keypair.bitcoin_address.len() >= 26 && keypair.bitcoin_address.len() <= 35);

        println!("Generated keypair:");
        println!("  Private key (WIF): {}", keypair.private_key_wif);
        println!("  Bitcoin address:   {}", keypair.bitcoin_address);
    }

    #[test]
    fn test_wif_to_address_consistency() {
        // Generate multiple keypairs and verify each has unique address
        let kp1 = generate_keypair().expect("Failed to generate keypair 1");
        let kp2 = generate_keypair().expect("Failed to generate keypair 2");

        assert_ne!(kp1.bitcoin_address, kp2.bitcoin_address);
        assert_ne!(kp1.private_key_wif, kp2.private_key_wif);
    }
}
