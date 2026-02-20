//! Testnet utilities: generate synthetic UTXO data for development

use bitcoin::secp256k1::{PublicKey as SecpPublicKey, Secp256k1, SecretKey};
use bitcoin::{Address, Network, PublicKey};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const TESTNET_SEED_DOMAIN: &[u8] = b"bitinfinity:testnet:address:v1";

fn deterministic_mainnet_p2pkh(index: usize) -> Result<String, Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let mut salt: u32 = 0;

    loop {
        let mut hasher = Sha256::new();
        hasher.update(TESTNET_SEED_DOMAIN);
        hasher.update(index.to_le_bytes());
        hasher.update(salt.to_le_bytes());
        let seed = hasher.finalize();

        if let Ok(secret) = SecretKey::from_slice(&seed) {
            let secp_pk = SecpPublicKey::from_secret_key(&secp, &secret);
            let pk = PublicKey::new(secp_pk);
            let addr = Address::p2pkh(pk, Network::Bitcoin);
            return Ok(addr.to_string());
        }

        salt = salt
            .checked_add(1)
            .ok_or("failed to derive deterministic secret key for testnet address")?;
    }
}

/// Generate synthetic Bitcoin addresses with test balances for development
pub fn generate_synthetic_utxos(
    count: usize,
) -> Result<BTreeMap<String, u64>, Box<dyn std::error::Error>> {
    let mut utxos = BTreeMap::new();

    if count > 0 {
        // Always include Satoshi's genesis address so testnet queries can check a known constant.
        utxos.insert(
            "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string(),
            50_000_000_000_000u64,
        );
    }

    // Fill the remaining accounts with deterministic, canonical mainnet P2PKH addresses.
    for i in 0..count.saturating_sub(1) {
        let address = deterministic_mainnet_p2pkh(i)?;
        let balance = (100_000_000u64) * ((i % 10 + 1) as u64); // 1-10 BTC in satoshis
        utxos.entry(address).or_insert(balance);
    }

    // Total supply check
    let total: u64 = utxos.values().sum();
    eprintln!(
        "Generated {} test accounts with total {} satoshis",
        utxos.len(),
        total
    );

    Ok(utxos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_deterministic_generator_is_stable() -> Result<(), Box<dyn std::error::Error>> {
        let a0 = deterministic_mainnet_p2pkh(0)?;
        let a1 = deterministic_mainnet_p2pkh(1)?;
        assert_eq!(a0, deterministic_mainnet_p2pkh(0)?);
        assert_eq!(a1, deterministic_mainnet_p2pkh(1)?);
        assert_ne!(a0, a1);
        Ok(())
    }

    #[test]
    fn test_generated_utxos_are_valid_bitcoin_addresses() -> Result<(), Box<dyn std::error::Error>>
    {
        let utxos = generate_synthetic_utxos(12)?;
        assert_eq!(utxos.len(), 12);

        for addr in utxos.keys() {
            let parsed = Address::from_str(addr)?;
            let _checked = parsed.require_network(Network::Bitcoin)?;
        }

        Ok(())
    }
}
