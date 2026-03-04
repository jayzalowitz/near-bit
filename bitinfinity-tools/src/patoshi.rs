//! Identify and reassign Satoshi Nakamoto's Patoshi pattern coins
//!
//! The "Patoshi pattern" refers to blocks mined by a single entity (likely Satoshi)
//! in Bitcoin's early days, identifiable by a distinctive nonce pattern.
//! These addresses hold ~1.1M BTC that have never moved.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Parse Patoshi addresses from a CSV reader.
/// The CSV should have at least one column with Bitcoin addresses.
pub fn load_patoshi_addresses_from_reader<R: std::io::Read>(
    reader: R,
) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let mut addresses = HashSet::new();
    let mut reader = csv::Reader::from_reader(reader);

    for result in reader.records() {
        let record = result?;
        if let Some(addr) = record.get(0) {
            let addr = addr.trim().to_string();
            if !addr.is_empty() {
                addresses.insert(addr);
            }
        }
    }

    Ok(addresses)
}

/// Load Patoshi addresses from a CSV file.
/// The CSV should have at least one column with Bitcoin addresses.
pub fn load_patoshi_addresses(
    csv_path: &Path,
) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(csv_path)?;
    let addresses = load_patoshi_addresses_from_reader(file)?;

    println!(
        "✓ Loaded {} Patoshi addresses from {}",
        addresses.len(),
        csv_path.display()
    );
    Ok(addresses)
}

/// Remove Patoshi addresses from the UTXO map and assign their combined balance
/// to a target address.
pub fn reassign_patoshi(
    utxo_map: &mut BTreeMap<String, u64>,
    patoshi_addresses: &HashSet<String>,
    target_address: &str,
) -> PatoshiReassignment {
    let mut total_removed: u64 = 0;
    let mut count_removed: usize = 0;

    for addr in patoshi_addresses {
        if let Some(balance) = utxo_map.remove(addr) {
            total_removed += balance;
            count_removed += 1;
        }
    }

    // Add combined balance to target address
    if total_removed > 0 {
        *utxo_map.entry(target_address.to_string()).or_insert(0) += total_removed;
    }

    PatoshiReassignment {
        total_satoshis: total_removed,
        addresses_removed: count_removed,
        target_address: target_address.to_string(),
    }
}

pub struct PatoshiReassignment {
    pub total_satoshis: u64,
    pub addresses_removed: usize,
    pub target_address: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_patoshi_addresses_from_reader_trims_and_deduplicates() {
        let csv_data = b"address\n 1PatoshiAddr1 \n1PatoshiAddr2\n1PatoshiAddr1\n\n";
        let parsed = load_patoshi_addresses_from_reader(&csv_data[..]).expect("csv should parse");

        assert_eq!(parsed.len(), 2);
        assert!(parsed.contains("1PatoshiAddr1"));
        assert!(parsed.contains("1PatoshiAddr2"));
    }

    #[test]
    fn test_reassign_patoshi() {
        let mut utxos = BTreeMap::new();
        utxos.insert("1PatoshiAddr1".to_string(), 5_000_000_000);
        utxos.insert("1PatoshiAddr2".to_string(), 3_000_000_000);
        utxos.insert("1NormalAddr".to_string(), 1_000_000_000);

        let mut patoshi = HashSet::new();
        patoshi.insert("1PatoshiAddr1".to_string());
        patoshi.insert("1PatoshiAddr2".to_string());

        let result = reassign_patoshi(&mut utxos, &patoshi, "1TargetAddr");

        assert_eq!(result.total_satoshis, 8_000_000_000);
        assert_eq!(result.addresses_removed, 2);
        assert_eq!(utxos.get("1TargetAddr"), Some(&8_000_000_000));
        assert_eq!(utxos.get("1NormalAddr"), Some(&1_000_000_000));
        assert!(!utxos.contains_key("1PatoshiAddr1"));
        assert!(!utxos.contains_key("1PatoshiAddr2"));
    }
}
