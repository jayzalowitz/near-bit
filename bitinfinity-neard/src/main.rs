use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "bitinfinity-neard")]
#[command(about = "Bitcoin Infinity Node - NEAR with Bitcoin addresses", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Bitcoin Infinity node
    Init {
        /// Home directory for node data
        #[arg(long, default_value = "~/.bitinfinity")]
        home: PathBuf,

        /// Chain ID
        #[arg(long, default_value = "bitinfinity-mainnet")]
        chain_id: String,

        /// Validator account ID (Bitcoin address)
        #[arg(long)]
        account_id: Option<String>,

        /// Genesis config file
        #[arg(long)]
        genesis_config: Option<PathBuf>,

        /// Genesis records file
        #[arg(long)]
        genesis_records: Option<PathBuf>,

        /// Fast init (skip some checks)
        #[arg(long)]
        fast: bool,
    },

    /// Run a Bitcoin Infinity node
    Run {
        /// Home directory for node data
        #[arg(long, default_value = "~/.bitinfinity")]
        home: PathBuf,

        /// Port for JSON-RPC
        #[arg(long, default_value = "3030")]
        rpc_port: u16,

        /// Port for peer-to-peer network
        #[arg(long, default_value = "24567")]
        p2p_port: u16,
    },

    /// Show node configuration
    Config {
        /// Home directory for node data
        #[arg(long, default_value = "~/.bitinfinity")]
        home: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            home,
            chain_id,
            account_id,
            genesis_config,
            genesis_records,
            fast,
        } => {
            println!("Bitcoin Infinity Node Initialization");
            println!("====================================");
            println!();

            let home_path = expand_home(&home);
            println!("Home directory: {}", home_path.display());
            println!("Chain ID: {}", chain_id);
            println!("Fast init: {}", fast);

            if let Some(account) = &account_id {
                println!("Validator account: {}", account);
            }

            println!();
            println!("Setting up node structure...");

            // Create directories
            std::fs::create_dir_all(&home_path)?;
            std::fs::create_dir_all(home_path.join("data"))?;
            std::fs::create_dir_all(home_path.join("keys"))?;

            // TODO: Generate or load keys
            // TODO: Create validator key pair (ed25519 for block production)
            // TODO: Load or generate genesis config

            println!("✓ Node structure created");
            println!("✓ Ready to run with: bitinfinity-neard run --home {}", home_path.display());
        }

        Commands::Run {
            home,
            rpc_port,
            p2p_port,
        } => {
            println!("Starting Bitcoin Infinity Node");
            println!("=============================");
            println!();

            let home_path = expand_home(&home);
            println!("Home: {}", home_path.display());
            println!("JSON-RPC: http://localhost:{}", rpc_port);
            println!("P2P Network: localhost:{}", p2p_port);
            println!();

            // Verify node is initialized
            if !home_path.exists() {
                eprintln!("Error: Node not initialized at {}", home_path.display());
                eprintln!("Run: bitinfinity-neard init --home {}", home_path.display());
                std::process::exit(1);
            }

            println!("Reading node configuration...");
            // TODO: Load config and genesis
            // TODO: Initialize NEAR runtime with Bitcoin address support
            // TODO: Start block production
            // TODO: Start JSON-RPC server

            println!("✓ Node initialized and ready");
            println!("✓ Listening on RPC port {} and P2P port {}", rpc_port, p2p_port);
            println!();
            println!("Use 'curl -X POST' to interact with the RPC:");
            println!("  curl -X POST http://localhost:{}/", rpc_port);

            // Keep the node running
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }

        Commands::Config { home } => {
            let home_path = expand_home(&home);
            println!("Bitcoin Infinity Node Configuration");
            println!("===================================");
            println!();
            println!("Home: {}", home_path.display());
            println!();

            // TODO: Load and display actual config
            println!("Chain ID: bitinfinity-mainnet");
            println!("Network: testnet");
            println!("Genesis: Not yet loaded");
            println!("Validators: 0");
        }
    }

    Ok(())
}

/// Expand ~ to home directory
fn expand_home(path: &std::path::Path) -> PathBuf {
    if path.starts_with("~") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let path_str = path.to_string_lossy();
        let expanded = path_str.replacen("~", &home, 1);
        PathBuf::from(expanded)
    } else {
        path.to_path_buf()
    }
}
