//! Parse Bitcoin UTXO snapshot from dumptxoutset binary format
//!
//! Uses the `txoutset` crate to stream-parse the binary dump produced by
//! Bitcoin Core's `dumptxoutset` RPC command.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

pub struct UtxoParser {
    path: std::path::PathBuf,
    expected_utxo_count: u64,
    tip_block_hash: String,
}

impl UtxoParser {
    pub fn new(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        use txoutset::{ComputeAddresses, Dump};

        if !path.exists() {
            return Err(format!("UTXO snapshot not found: {}", path.display()).into());
        }
        let metadata = std::fs::metadata(path)?;
        if metadata.len() < 40 {
            return Err(format!(
                "UTXO snapshot too small to contain required header: {} bytes",
                metadata.len()
            )
            .into());
        }

        // Header sanity check: decode chain tip hash + total coin count.
        let dump = Dump::new(path, ComputeAddresses::No)
            .map_err(|e| format!("Failed to decode UTXO dump header: {}", e))?;
        if dump.utxo_set_size == 0 {
            return Err("UTXO snapshot header reports zero coins".into());
        }

        println!(
            "UTXO snapshot: {} ({:.2} GB)",
            path.display(),
            metadata.len() as f64 / 1_073_741_824.0
        );
        println!("  Header tip hash: {}", dump.block_hash);
        println!("  Header coin count: {}", dump.utxo_set_size);

        Ok(UtxoParser {
            path: path.to_path_buf(),
            expected_utxo_count: dump.utxo_set_size,
            tip_block_hash: dump.block_hash.to_string(),
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
        let started = Instant::now();

        for item in dump {
            total_utxos += 1;

            if total_utxos % 1_000_000 == 0 {
                println!(
                    "  Processed {} UTXOs ({} addresses, {} skipped, elapsed: {:.1}s)...",
                    total_utxos,
                    address_balances.len(),
                    skipped_no_address,
                    started.elapsed().as_secs_f64(),
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

        if total_utxos != self.expected_utxo_count {
            return Err(format!(
                "UTXO count mismatch: parsed {} records, header reported {}",
                total_utxos, self.expected_utxo_count
            )
            .into());
        }

        let elapsed = started.elapsed();
        let total_btc = total_satoshis as f64 / 100_000_000.0;
        println!("✓ UTXO parsing complete:");
        println!("  Header tip hash: {}", self.tip_block_hash);
        println!("  Total UTXOs: {}", total_utxos);
        println!("  Unique addresses: {}", address_balances.len());
        println!(
            "  Total satoshis: {} ({:.8} BTC)",
            total_satoshis, total_btc
        );
        println!("  Skipped (no address): {}", skipped_no_address);
        println!("  Elapsed: {:.2}s", elapsed.as_secs_f64());
        if elapsed > Duration::from_secs(600) {
            println!("  WARNING: Parsing exceeded 10 minutes benchmark target");
        }

        Ok(address_balances)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::consensus::Encodable;
    use bitcoin::hashes::Hash;
    use bitcoin::{BlockHash, Network, OutPoint, PubkeyHash, Txid};
    use std::fs::File;
    use std::io::Write;

    fn write_compact_size(writer: &mut impl Write, n: u64) -> Result<(), std::io::Error> {
        match n {
            0..=252 => writer.write_all(&[n as u8]),
            253..=0xFFFF => {
                writer.write_all(&[0xFD])?;
                writer.write_all(&(n as u16).to_le_bytes())
            }
            0x1_0000..=0xFFFF_FFFF => {
                writer.write_all(&[0xFE])?;
                writer.write_all(&(n as u32).to_le_bytes())
            }
            _ => {
                writer.write_all(&[0xFF])?;
                writer.write_all(&n.to_le_bytes())
            }
        }
    }

    fn compress_amount(amount: u64) -> u64 {
        if amount == 0 {
            return 0;
        }

        let mut n = amount;
        let mut e = 0;
        while (n % 10) == 0 && e < 9 {
            n /= 10;
            e += 1;
        }

        if e < 9 {
            let d = n % 10;
            debug_assert!((1..=9).contains(&d));
            n /= 10;
            1 + (n * 9 + d - 1) * 10 + e
        } else {
            1 + (n - 1) * 10 + 9
        }
    }

    #[test]
    fn test_parser_rejects_missing_file() {
        let result = UtxoParser::new(Path::new("/nonexistent/utxo.dat"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_small_synthetic_snapshot() -> Result<(), Box<dyn std::error::Error>> {
        let path =
            std::path::PathBuf::from(format!("/tmp/utxo-parser-test-{}.dat", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let mut file = File::create(&path)?;

        // Header: chain tip hash + coin count.
        BlockHash::all_zeros().consensus_encode(&mut file)?;
        1u64.consensus_encode(&mut file)?;

        // Single UTXO entry.
        let txid = Txid::from_slice(&[7u8; 32])?;
        let outpoint = OutPoint { txid, vout: 0 };
        outpoint.consensus_encode(&mut file)?;

        // code = height * 2 + is_coinbase
        write_compact_size(&mut file, 2u64)?; // height=1, coinbase=false
        write_compact_size(&mut file, compress_amount(50_000_000))?; // 0.5 BTC

        // Script compression encoding for P2PKH: tag 0x00 + 20-byte hash160.
        let hash20 = [0x11u8; 20];
        write_compact_size(&mut file, 0u64)?;
        file.write_all(&hash20)?;
        file.flush()?;

        let mut parser = UtxoParser::new(&path)?;
        let balances = parser.parse_and_aggregate()?;

        let expected_addr =
            bitcoin::Address::p2pkh(PubkeyHash::from_slice(&hash20)?, Network::Bitcoin).to_string();
        assert_eq!(balances.get(&expected_addr), Some(&50_000_000));
        assert_eq!(balances.len(), 1);

        let _ = std::fs::remove_file(&path);
        Ok(())
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
