use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::PathBuf;

mod account_manager;
mod genesis_builder;
mod keygen;
mod patoshi;
mod signature_recovery;
mod testnet;
mod transaction;
mod utxo_parser;

#[derive(Parser)]
#[command(name = "bitinfinity-tools")]
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

        /// Validator account ID
        #[arg(long, default_value = "validator.bitinfinity")]
        validator_account: String,

        /// Validator ed25519 public key
        #[arg(long)]
        validator_key: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::GenerateGenesis {
            utxo_snapshot,
            patoshi_csv,
            output_dir,
            testnet,
            num_accounts,
            chain_id,
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
                    let patoshi_keypair = keygen::generate_keypair()?;
                    let reassignment = patoshi::reassign_patoshi(
                        &mut utxos,
                        &patoshi_addrs,
                        &patoshi_keypair.bitcoin_address,
                    );

                    if reassignment.total_satoshis > 0 {
                        let genesis_floor_yocto =
                            reassignment.total_satoshis as u128 * 10u128.pow(16);
                        patoshi_registry
                            .insert(reassignment.target_address.clone(), genesis_floor_yocto);

                        std::fs::create_dir_all(&output_dir)?;
                        let key_path = output_dir.join("patoshi-keypair.txt");
                        let key_file_contents = format!(
                            "address={}\nprivate_key_wif={}\n",
                            patoshi_keypair.bitcoin_address, patoshi_keypair.private_key_wif
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
                        println!("  Address: {}", patoshi_keypair.bitcoin_address);
                        println!("  Key file: {}", key_path.display());
                        println!("  WARNING: Keep patoshi-keypair.txt secure and offline.");
                    }
                }

                utxos
            };

            // Build nearcore-compatible genesis
            let builder =
                genesis_builder::GenesisBuilder::new(chain_id.clone(), output_dir.clone());
            builder.build(&utxos, &validator, &patoshi_registry)?;
            println!();
            println!("✓ Genesis written to {}", output_dir.display());
        }
    }

    Ok(())
}
