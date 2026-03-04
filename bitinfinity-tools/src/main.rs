use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[allow(dead_code)]
mod account_manager;
mod genesis_builder;
mod keygen;
mod patoshi;
#[allow(dead_code)]
mod signature_recovery;
mod testnet;
#[allow(dead_code)]
mod transaction;
mod utxo_parser;

#[derive(Parser)]
#[command(name = "bitinfinity-tools")]
#[command(version)]
#[command(about = "Genesis and configuration tools for Bitcoin Infinity chain")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    GenerateGenesis {
        /// Path to Bitcoin UTXO snapshot (dumptxoutset output)
        #[arg(long)]
        utxo_snapshot: Option<PathBuf>,

        /// Path to Patoshi addresses CSV
        #[arg(long)]
        patoshi_csv: Option<PathBuf>,

        /// Explicit target Bitcoin address to receive reassigned Patoshi balances
        #[arg(long)]
        satoshi_address: Option<String>,

        /// Output directory for genesis files
        #[arg(long, default_value = "./genesis")]
        output_dir: PathBuf,

        /// Use synthetic test data instead of real UTXO snapshot
        #[arg(long)]
        testnet: bool,

        /// Number of test accounts (with --testnet)
        #[arg(long, default_value = "100")]
        num_accounts: usize,

        /// Chain ID
        #[arg(long, default_value = "bitinfinity-mainnet")]
        chain_id: String,

        /// Explicit genesis time (RFC3339). If omitted, current UTC time is used.
        #[arg(long)]
        genesis_time: Option<String>,

        /// Validator account ID
        #[arg(long, default_value = "validator.bitinfinity")]
        validator_account: String,

        /// Validator ed25519 public key
        #[arg(long)]
        validator_key: Option<String>,
    },
    GenerateKeypair {
        /// Output file (defaults to stdout)
        #[arg(long)]
        output: Option<PathBuf>,
    },
    VerifyGenesis {
        /// Path to generated genesis.json
        #[arg(long)]
        genesis: PathBuf,

        /// Optional path to write machine-readable summary JSON
        #[arg(long)]
        json_out: Option<PathBuf>,
    },
    VerifySnapshotSupply {
        /// Path to generated genesis.json
        #[arg(long)]
        genesis: PathBuf,

        /// Path to `bitcoin-cli gettxoutsetinfo` JSON output
        #[arg(long)]
        txoutsetinfo: PathBuf,

        /// Allowed absolute difference in satoshis. Default: 1 satoshi.
        #[arg(long, default_value = "1")]
        tolerance_sats: u64,

        /// Optional path to write machine-readable summary JSON
        #[arg(long)]
        json_out: Option<PathBuf>,
    },
}

const SATOSHIS_PER_BTC: u128 = 100_000_000;
const YOCTO_PER_SATOSHI: u128 = 10_000_000_000_000_000;

struct GenesisSupplySummary {
    chain_id: String,
    genesis_time: String,
    declared_total_yocto: u128,
    computed_total_yocto: u128,
    reconciled: bool,
    total_records: usize,
    account_records: usize,
    access_key_records: usize,
    data_records: usize,
}

struct TxoutsetSupplySummary {
    height: Option<u64>,
    txouts: Option<u64>,
    total_amount_btc: String,
    total_satoshis: u128,
}

fn summarize_genesis(path: &Path) -> Result<GenesisSupplySummary, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let parsed: genesis_builder::Genesis = serde_json::from_str(&content)?;

    let declared_total = parsed
        .total_supply
        .parse::<u128>()
        .map_err(|e| format!("Invalid genesis total_supply: {}", e))?;

    let mut computed_total: u128 = 0;
    let mut account_records: usize = 0;
    let mut access_key_records: usize = 0;
    let mut data_records: usize = 0;

    for record in &parsed.records {
        match record {
            genesis_builder::StateRecord::Account { account, .. } => {
                let amount = account
                    .amount
                    .parse::<u128>()
                    .map_err(|e| format!("Invalid account.amount: {}", e))?;
                let locked = account
                    .locked
                    .parse::<u128>()
                    .map_err(|e| format!("Invalid account.locked: {}", e))?;
                computed_total = computed_total
                    .checked_add(amount)
                    .and_then(|x| x.checked_add(locked))
                    .ok_or("Computed account total overflowed u128")?;
                account_records += 1;
            }
            genesis_builder::StateRecord::AccessKey { .. } => {
                access_key_records += 1;
            }
            genesis_builder::StateRecord::Data { .. } => {
                data_records += 1;
            }
        }
    }

    Ok(GenesisSupplySummary {
        chain_id: parsed.chain_id,
        genesis_time: parsed.genesis_time,
        declared_total_yocto: declared_total,
        computed_total_yocto: computed_total,
        reconciled: declared_total == computed_total,
        total_records: parsed.records.len(),
        account_records,
        access_key_records,
        data_records,
    })
}

fn extract_json_key_literal(input: &str, key: &str) -> Result<String, Box<dyn std::error::Error>> {
    let needle = format!("\"{}\"", key);
    let key_pos = input
        .find(&needle)
        .ok_or_else(|| format!("Missing JSON key: {}", key))?;
    let after_key = &input[key_pos + needle.len()..];
    let colon_rel = after_key
        .find(':')
        .ok_or_else(|| format!("Missing ':' after JSON key: {}", key))?;
    let mut idx = key_pos + needle.len() + colon_rel + 1;

    let bytes = input.as_bytes();
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx >= bytes.len() {
        return Err(format!("Missing JSON value for key: {}", key).into());
    }

    if bytes[idx] == b'"' {
        idx += 1;
        let mut out = String::new();
        let mut escaped = false;
        while idx < bytes.len() {
            let ch = bytes[idx] as char;
            if escaped {
                out.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                return Ok(out);
            } else {
                out.push(ch);
            }
            idx += 1;
        }
        return Err(format!("Unterminated string literal for key: {}", key).into());
    }

    let start = idx;
    while idx < bytes.len() {
        let ch = bytes[idx] as char;
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E' {
            idx += 1;
        } else {
            break;
        }
    }
    if start == idx {
        return Err(format!("Unsupported literal type for key: {}", key).into());
    }
    Ok(input[start..idx].to_string())
}

fn parse_btc_amount_to_satoshis(raw: &str) -> Result<u128, Box<dyn std::error::Error>> {
    let mut value = raw.trim();
    if value.is_empty() {
        return Err("BTC amount is empty".into());
    }
    if value.starts_with('+') {
        value = &value[1..];
    }
    if value.starts_with('-') {
        return Err(format!("BTC amount cannot be negative: {}", raw).into());
    }
    if value.contains('e') || value.contains('E') {
        return Err(format!("BTC amount scientific notation is not supported: {}", raw).into());
    }

    let mut parts = value.split('.');
    let whole_part = parts.next().unwrap_or_default();
    let frac_part = parts.next();
    if parts.next().is_some() {
        return Err(format!("Invalid BTC amount format: {}", raw).into());
    }

    let whole_digits = if whole_part.is_empty() {
        "0"
    } else {
        whole_part
    };
    if !whole_digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("Invalid BTC whole-part digits: {}", raw).into());
    }

    let frac_digits = frac_part.unwrap_or("");
    if !frac_digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("Invalid BTC fractional digits: {}", raw).into());
    }
    if frac_digits.len() > 8 {
        return Err(format!("BTC amount has more than 8 decimal places: {}", raw).into());
    }

    let whole = whole_digits.parse::<u128>()?;
    let frac_padded = format!("{:0<8}", frac_digits);
    let frac = frac_padded.parse::<u128>()?;
    let satoshis = whole
        .checked_mul(SATOSHIS_PER_BTC)
        .and_then(|x| x.checked_add(frac))
        .ok_or("BTC amount conversion overflowed u128")?;
    Ok(satoshis)
}

fn yocto_to_satoshis_exact(yocto: u128) -> Result<u128, Box<dyn std::error::Error>> {
    if !yocto.is_multiple_of(YOCTO_PER_SATOSHI) {
        return Err(format!("Yocto amount is not an exact satoshi multiple: {}", yocto).into());
    }
    Ok(yocto / YOCTO_PER_SATOSHI)
}

fn summarize_txoutsetinfo(
    path: &Path,
) -> Result<TxoutsetSupplySummary, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let total_amount_btc = extract_json_key_literal(&content, "total_amount")?;
    let total_satoshis = parse_btc_amount_to_satoshis(&total_amount_btc)?;

    let parsed: serde_json::Value = serde_json::from_str(&content)?;
    let height = parsed.get("height").and_then(|v| v.as_u64());
    let txouts = parsed.get("txouts").and_then(|v| v.as_u64());

    Ok(TxoutsetSupplySummary {
        height,
        txouts,
        total_amount_btc,
        total_satoshis,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::GenerateGenesis {
            utxo_snapshot,
            patoshi_csv,
            satoshi_address,
            output_dir,
            testnet,
            num_accounts,
            chain_id,
            genesis_time,
            validator_account,
            validator_key,
        } => {
            println!("Bitcoin Infinity Genesis Generator");
            println!("==============================");
            println!();

            // Use provided validator key or generate a placeholder
            let validator_pubkey = validator_key.unwrap_or_else(|| {
                "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp".to_string()
            });

            let validator = genesis_builder::ValidatorConfig {
                account_id: validator_account,
                public_key_ed25519: validator_pubkey,
                stake_yocto: 50_000_000_000_000_000_000_000_000_000_000, // 50,000 BIT
                balance_yocto: 1_000_000_000_000_000_000_000_000_000_000, // 1,000,000 BIT
            };

            let mut patoshi_registry: BTreeMap<String, u128> = BTreeMap::new();
            let utxos = if testnet {
                println!("Mode: TESTNET (synthetic data)");
                println!("Accounts: {}", num_accounts);

                let utxos = testnet::generate_synthetic_utxos(num_accounts)?;
                println!("✓ Generated {} test accounts", utxos.len());
                println!();
                for (addr, balance) in utxos.iter().take(5) {
                    println!("  {} : {} satoshis", addr, balance);
                }
                if utxos.len() > 5 {
                    println!("  ... and {} more", utxos.len() - 5);
                }
                println!();
                utxos
            } else {
                println!("Mode: MAINNET (real Bitcoin UTXO snapshot)");
                let snapshot_path = match utxo_snapshot {
                    Some(path) => {
                        println!("UTXO Snapshot: {}", path.display());
                        path
                    }
                    None => {
                        eprintln!("Error: --utxo-snapshot required for mainnet mode");
                        std::process::exit(1);
                    }
                };
                if let Some(path) = &patoshi_csv {
                    println!("Patoshi CSV: {}", path.display());
                }

                // Parse real UTXO snapshot
                let mut parser = utxo_parser::UtxoParser::new(&snapshot_path)?;
                let mut utxos = parser.parse_and_aggregate()?;

                // Apply Patoshi reassignment if CSV provided
                if let Some(csv_path) = patoshi_csv {
                    let patoshi_addrs = patoshi::load_patoshi_addresses(&csv_path)?;
                    let mut generated_keypair: Option<keygen::Keypair> = None;
                    let target_address = if let Some(addr) = satoshi_address {
                        let parsed = bitcoin::Address::from_str(&addr)
                            .map_err(|e| format!("Invalid --satoshi-address: {}", e))?
                            .require_network(bitcoin::Network::Bitcoin)
                            .map_err(|e| format!("--satoshi-address must be mainnet: {}", e))?;
                        parsed.to_string()
                    } else {
                        let kp = keygen::generate_keypair()?;
                        let addr = kp.bitcoin_address.clone();
                        generated_keypair = Some(kp);
                        addr
                    };

                    let reassignment =
                        patoshi::reassign_patoshi(&mut utxos, &patoshi_addrs, &target_address);

                    if reassignment.total_satoshis > 0 {
                        println!(
                            "  Patoshi addresses removed: {}",
                            reassignment.addresses_removed
                        );
                        println!(
                            "  Patoshi satoshis reassigned: {} -> {}",
                            reassignment.total_satoshis, reassignment.target_address
                        );
                        let genesis_floor_yocto =
                            reassignment.total_satoshis as u128 * 10u128.pow(16);
                        patoshi_registry
                            .insert(reassignment.target_address.clone(), genesis_floor_yocto);

                        if let Some(kp) = generated_keypair {
                            std::fs::create_dir_all(&output_dir)?;
                            let key_path = output_dir.join("patoshi-keypair.txt");
                            let key_file_contents = format!(
                                "address={}\nprivate_key_wif={}\n",
                                kp.bitcoin_address, kp.private_key_wif
                            );
                            std::fs::write(&key_path, key_file_contents)?;
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let _ = std::fs::set_permissions(
                                    &key_path,
                                    std::fs::Permissions::from_mode(0o600),
                                );
                            }
                            println!("✓ Patoshi reassigned to generated Bitcoin address");
                            println!("  Address: {}", kp.bitcoin_address);
                            println!("  Key file: {}", key_path.display());
                            println!("  WARNING: Keep patoshi-keypair.txt secure and offline.");
                        } else {
                            println!("✓ Patoshi reassigned to provided --satoshi-address");
                            println!("  Address: {}", target_address);
                        }
                    }
                }

                utxos
            };

            // Build nearcore-compatible genesis
            let builder =
                genesis_builder::GenesisBuilder::new(chain_id.clone(), output_dir.clone())
                    .with_genesis_time(genesis_time)?;
            builder.build(&utxos, &validator, &patoshi_registry)?;
            println!();
            println!("✓ Genesis written to {}", output_dir.display());
        }
        Commands::GenerateKeypair { output } => {
            let kp = keygen::generate_keypair()?;
            let payload = serde_json::json!({
                "bitcoin_address": kp.bitcoin_address,
                "private_key_wif": kp.private_key_wif,
            });

            let rendered = serde_json::to_string_pretty(&payload)?;
            if let Some(path) = output {
                std::fs::write(&path, &rendered)?;
                println!("Wrote keypair to {}", path.display());
            } else {
                println!("{}", rendered);
            }
        }
        Commands::VerifyGenesis { genesis, json_out } => {
            let summary = summarize_genesis(&genesis)?;

            println!("Genesis verification summary");
            println!("  file: {}", genesis.display());
            println!("  chain_id: {}", summary.chain_id);
            println!("  genesis_time: {}", summary.genesis_time);
            println!("  declared_total_supply: {}", summary.declared_total_yocto);
            println!("  computed_total_supply: {}", summary.computed_total_yocto);
            println!("  reconciled: {}", summary.reconciled);
            println!("  records: {}", summary.total_records);
            println!("    account: {}", summary.account_records);
            println!("    access_key: {}", summary.access_key_records);
            println!("    data: {}", summary.data_records);

            if let Some(path) = json_out {
                let payload = serde_json::json!({
                    "file": genesis.display().to_string(),
                    "chain_id": summary.chain_id,
                    "genesis_time": summary.genesis_time,
                    "declared_total_supply": summary.declared_total_yocto.to_string(),
                    "computed_total_supply": summary.computed_total_yocto.to_string(),
                    "reconciled": summary.reconciled,
                    "records": {
                        "total": summary.total_records,
                        "account": summary.account_records,
                        "access_key": summary.access_key_records,
                        "data": summary.data_records
                    }
                });
                let rendered = serde_json::to_string_pretty(&payload)?;
                std::fs::write(&path, rendered)?;
                println!("  summary_json: {}", path.display());
            }

            if !summary.reconciled {
                return Err("Genesis total supply reconciliation failed".into());
            }
        }
        Commands::VerifySnapshotSupply {
            genesis,
            txoutsetinfo,
            tolerance_sats,
            json_out,
        } => {
            let genesis_summary = summarize_genesis(&genesis)?;
            if !genesis_summary.reconciled {
                return Err(
                    "Genesis must reconcile internally before snapshot supply comparison".into(),
                );
            }

            let txoutset_summary = summarize_txoutsetinfo(&txoutsetinfo)?;
            let genesis_total_satoshis =
                yocto_to_satoshis_exact(genesis_summary.computed_total_yocto)?;
            let snapshot_total_satoshis = txoutset_summary.total_satoshis;
            let diff_satoshis = genesis_total_satoshis.abs_diff(snapshot_total_satoshis);
            let within_tolerance = diff_satoshis <= tolerance_sats as u128;

            println!("Snapshot supply reconciliation summary");
            println!("  genesis_file: {}", genesis.display());
            println!("  txoutsetinfo_file: {}", txoutsetinfo.display());
            if let Some(height) = txoutset_summary.height {
                println!("  snapshot_height: {}", height);
            }
            if let Some(txouts) = txoutset_summary.txouts {
                println!("  snapshot_txouts: {}", txouts);
            }
            println!(
                "  snapshot_total_amount_btc: {}",
                txoutset_summary.total_amount_btc
            );
            println!("  snapshot_total_satoshis: {}", snapshot_total_satoshis);
            println!("  genesis_total_satoshis: {}", genesis_total_satoshis);
            println!("  difference_satoshis: {}", diff_satoshis);
            println!("  tolerance_satoshis: {}", tolerance_sats);
            println!("  within_tolerance: {}", within_tolerance);

            if let Some(path) = json_out {
                let payload = serde_json::json!({
                    "genesis_file": genesis.display().to_string(),
                    "txoutsetinfo_file": txoutsetinfo.display().to_string(),
                    "snapshot": {
                        "height": txoutset_summary.height,
                        "txouts": txoutset_summary.txouts,
                        "total_amount_btc": txoutset_summary.total_amount_btc,
                        "total_satoshis": snapshot_total_satoshis.to_string()
                    },
                    "genesis": {
                        "chain_id": genesis_summary.chain_id,
                        "genesis_time": genesis_summary.genesis_time,
                        "total_satoshis": genesis_total_satoshis.to_string()
                    },
                    "difference_satoshis": diff_satoshis.to_string(),
                    "tolerance_satoshis": tolerance_sats,
                    "within_tolerance": within_tolerance
                });
                let rendered = serde_json::to_string_pretty(&payload)?;
                std::fs::write(&path, rendered)?;
                println!("  summary_json: {}", path.display());
            }

            if !within_tolerance {
                return Err(format!(
                    "Snapshot supply reconciliation failed: diff {} sats exceeds tolerance {} sats",
                    diff_satoshis, tolerance_sats
                )
                .into());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_btc_amount_to_satoshis_valid() {
        assert_eq!(parse_btc_amount_to_satoshis("1").unwrap(), 100_000_000);
        assert_eq!(
            parse_btc_amount_to_satoshis("1.00000000").unwrap(),
            100_000_000
        );
        assert_eq!(parse_btc_amount_to_satoshis("0.00000001").unwrap(), 1);
        assert_eq!(
            parse_btc_amount_to_satoshis("19893654.12345678").unwrap(),
            1_989_365_412_345_678
        );
    }

    #[test]
    fn test_parse_btc_amount_to_satoshis_rejects_invalid() {
        assert!(parse_btc_amount_to_satoshis("-1").is_err());
        assert!(parse_btc_amount_to_satoshis("1.123456789").is_err());
        assert!(parse_btc_amount_to_satoshis("1e-8").is_err());
        assert!(parse_btc_amount_to_satoshis("abc").is_err());
    }

    #[test]
    fn test_yocto_to_satoshis_exact() {
        assert_eq!(
            yocto_to_satoshis_exact(5_000_000_000_000_000_000_000_000).unwrap(),
            500_000_000
        );
        assert!(yocto_to_satoshis_exact(5_000_000_000_000_000_000_000_001).is_err());
    }

    #[test]
    fn test_extract_json_key_literal_number_and_string() {
        let json_number = r#"{"height":123,"total_amount":19893654.12345678}"#;
        let json_string = r#"{"height":123,"total_amount":"19893654.12345678"}"#;

        assert_eq!(
            extract_json_key_literal(json_number, "total_amount").unwrap(),
            "19893654.12345678"
        );
        assert_eq!(
            extract_json_key_literal(json_string, "total_amount").unwrap(),
            "19893654.12345678"
        );
    }
}
