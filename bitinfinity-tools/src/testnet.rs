//! Testnet utilities: generate synthetic UTXO data for development

use std::collections::BTreeMap;

/// Generate synthetic Bitcoin addresses with test balances for development
pub fn generate_synthetic_utxos(
    count: usize,
) -> Result<BTreeMap<String, u64>, Box<dyn std::error::Error>> {
    let mut utxos = BTreeMap::new();

    // Well-known Bitcoin addresses for testing
    let test_addresses = vec![
        // Satoshi's genesis address
        ("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", 50_000_000_000_000u64),
        // Early Bitcoin addresses
        ("1dice8EMCQAqQSN3r3EKqyJi2rK5JSFD", 100_000_000_000u64),
        ("1JfmMwGBVbKYxTshbAJbPqjfaHeHNFELPJ", 50_000_000_000u64),
        ("1FP5gk4z7mDdSb3m3YvUwFb1BDUvcLYe1F", 25_000_000_000u64),
        ("1LCBvP2zP7VqcHeKz1U7TZ8VpGJTfvKbvf", 10_000_000_000u64),
        // Bech32 SegWit address
        ("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4", 5_000_000_000u64),
        // P2SH multisig
        ("3J98t1WpEZ73CNmYviecrnyiWrnqRhWNLy", 2_000_000_000u64),
    ];

    for (i, (address, balance)) in test_addresses.iter().enumerate() {
        if i >= count {
            break;
        }
        utxos.insert(address.to_string(), *balance);
    }

    // If more accounts needed, generate synthetic ones
    if count > test_addresses.len() {
        for i in 0..(count - test_addresses.len()) {
            let synthetic_address = format!("1SyntheticAccount{:0width$}jdx", i, width = 20);
            let balance = (100_000_000u64) * ((i % 10 + 1) as u64); // 1-10 BTC in satoshis
            utxos.insert(synthetic_address, balance);
        }
    }

    // Total supply check
    let total: u64 = utxos.values().sum();
    eprintln!("Generated {} test accounts with total {} satoshis", utxos.len(), total);

    Ok(utxos)
}
