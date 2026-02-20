//! Parse Bitcoin UTXO snapshot from dumptxoutset binary format
//!
//! Uses the `txoutset` crate to stream-parse the binary dump produced by
//! Bitcoin Core's `dumptxoutset` RPC command.

use std::collections::BTreeMap;
use std::path::Path;

pub struct UtxoParser {
    path: std::path::PathBuf,
}

impl UtxoParser {
    pub fn new(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if !path.exists() {
            return Err(format!("UTXO snapshot not found: {}", path.display()).into());
        }
        let metadata = std::fs::metadata(path)?;
        println!(
            "UTXO snapshot: {} ({:.2} GB)",
            path.display(),
            metadata.len() as f64 / 1_073_741_824.0
        );
        Ok(UtxoParser {
            path: path.to_path_buf(),
        })
    }

    /// Stream-parse all UTXOs, aggregate balances by address.
    /// Returns a map of Bitcoin address -> total satoshis.
    pub fn parse_and_aggregate(
        &mut self,
    ) -> Result<BTreeMap<String, u64>, Box<dyn std::error::Error>> {
        use txoutset::{ComputeAddresses, Dump};

        let dump = Dump::new(
            self.path.to_str().unwrap(),
            ComputeAddresses::Yes(txoutset::Network::Bitcoin),
        )
        .map_err(|e| format!("Failed to open UTXO dump: {}", e))?;

        let mut address_balances: BTreeMap<String, u64> = BTreeMap::new();
        let mut total_utxos: u64 = 0;
        let mut skipped_no_address: u64 = 0;
        let mut total_satoshis: u64 = 0;

        for item in dump {
            total_utxos += 1;

            if total_utxos % 5_000_000 == 0 {
                println!(
                    "  Processed {} UTXOs ({} addresses, {} skipped)...",
                    total_utxos,
                    address_balances.len(),
                    skipped_no_address
                );
            }

            let sats: u64 = u64::from(item.amount);
            total_satoshis += sats;

            match item.address {
                Some(addr) => {
                    let addr_str = addr.to_string();
                    *address_balances.entry(addr_str).or_insert(0) += sats;
                }
                None => {
                    // OP_RETURN, bare multisig, or other non-standard scripts
                    skipped_no_address += 1;
                }
            }
        }

        let total_btc = total_satoshis as f64 / 100_000_000.0;
        println!("✓ UTXO parsing complete:");
        println!("  Total UTXOs: {}", total_utxos);
        println!("  Unique addresses: {}", address_balances.len());
        println!(
            "  Total satoshis: {} ({:.8} BTC)",
            total_satoshis, total_btc
        );
        println!("  Skipped (no address): {}", skipped_no_address);

        Ok(address_balances)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_rejects_missing_file() {
        let result = UtxoParser::new(Path::new("/nonexistent/utxo.dat"));
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // Only run manually with real UTXO snapshot
    fn test_parse_real_snapshot() {
        let path = Path::new("/tmp/utxo-snapshot.dat");
        if !path.exists() {
            eprintln!("Skipping: no UTXO snapshot at /tmp/utxo-snapshot.dat");
            return;
        }

        let mut parser = UtxoParser::new(path).unwrap();
        let balances = parser.parse_and_aggregate().unwrap();

        // Basic sanity checks
        assert!(
            balances.len() > 1_000_000,
            "Should have >1M unique addresses"
        );

        let total_sats: u64 = balances.values().sum();
        let total_btc = total_sats as f64 / 100_000_000.0;
        // Total BTC should be close to 21M (minus lost coins + unmined)
        assert!(
            total_btc > 19_000_000.0 && total_btc < 21_000_001.0,
            "Total BTC {} out of expected range",
            total_btc
        );

        // Satoshi's genesis address should exist
        assert!(
            balances.contains_key("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"),
            "Satoshi's genesis address should be in UTXO set"
        );
    }
}
