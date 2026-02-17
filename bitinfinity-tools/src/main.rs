use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod utxo_parser;
mod patoshi;
mod genesis_builder;
mod keygen;
mod testnet;

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
        #[arg(long, default_value = "sydney-mainnet")]
        chain_id: String,
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
        } => {
            println!("Sydney Chain Genesis Generator");
            println!("==============================");
            println!();

            if testnet {
                println!("Mode: TESTNET (synthetic data)");
                println!("Accounts: {}", num_accounts);

                // Generate synthetic UTXO data
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

                // Build genesis from synthetic UTXOs
                let builder = genesis_builder::GenesisBuilder::new(
                    chain_id.clone(),
                    output_dir.clone(),
                );
                builder.build(&utxos)?;
                println!();
                println!("✓ Genesis files written to {}", output_dir.display());
            } else {
                println!("Mode: MAINNET (real Bitcoin UTXO snapshot)");
                match utxo_snapshot {
                    Some(path) => println!("UTXO Snapshot: {}", path.display()),
                    None => {
                        eprintln!("Error: --utxo-snapshot required for mainnet mode");
                        std::process::exit(1);
                    }
                }
                if let Some(path) = patoshi_csv {
                    println!("Patoshi CSV: {}", path.display());
                }
                // TODO: implement mainnet mode
                println!("TODO: Parse UTXO snapshot and build genesis");
            }

            println!("Output Directory: {}", output_dir.display());
            println!("Chain ID: {}", chain_id);
            println!();
            println!("WIP: Implementation in progress...");
        }
    }

    Ok(())
}
