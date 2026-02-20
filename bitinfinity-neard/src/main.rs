use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

mod keys;

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
        #[arg(long, default_value = "bitinfinity-testnet")]
        chain_id: String,

        /// Validator account ID
        #[arg(long, default_value = "validator.bitinfinity")]
        account_id: String,

        /// Path to a pre-built genesis.json (from bitinfinity-tools)
        #[arg(long)]
        genesis: Option<PathBuf>,

        /// Path to the neard binary (from nearcore fork)
        #[arg(long, default_value = "neard")]
        neard_bin: String,
    },

    /// Run a Bitcoin Infinity node
    Run {
        /// Home directory for node data
        #[arg(long, default_value = "~/.bitinfinity")]
        home: PathBuf,

        /// Path to the neard binary (from nearcore fork)
        #[arg(long, default_value = "neard")]
        neard_bin: String,
    },

    /// Show node configuration
    Config {
        /// Home directory for node data
        #[arg(long, default_value = "~/.bitinfinity")]
        home: PathBuf,
    },

    /// Generate ed25519 key files for the node
    Keygen {
        /// Home directory for node data
        #[arg(long, default_value = "~/.bitinfinity")]
        home: PathBuf,

        /// Validator account ID
        #[arg(long, default_value = "validator.bitinfinity")]
        account_id: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            home,
            chain_id,
            account_id,
            genesis,
            neard_bin,
        } => {
            cmd_init(
                &expand_home(&home),
                &chain_id,
                &account_id,
                genesis.as_deref(),
                &neard_bin,
            )?;
        }

        Commands::Run { home, neard_bin } => {
            cmd_run(&expand_home(&home), &neard_bin)?;
        }

        Commands::Config { home } => {
            cmd_config(&expand_home(&home))?;
        }

        Commands::Keygen { home, account_id } => {
            cmd_keygen(&expand_home(&home), &account_id)?;
        }
    }

    Ok(())
}

// ============================================================================
// Init command
// ============================================================================

fn cmd_init(
    home: &Path,
    chain_id: &str,
    account_id: &str,
    genesis_path: Option<&Path>,
    neard_bin: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Bitcoin Infinity Node Initialization");
    println!("====================================");
    println!();

    std::fs::create_dir_all(home)?;

    // Step 1: Generate key files if they don't exist
    let node_key_path = home.join("node_key.json");
    let validator_key_path = home.join("validator_key.json");

    if !node_key_path.exists() {
        println!("Generating node key...");
        let node_key = keys::generate_key_file("node");
        let json = serde_json::to_string_pretty(&node_key)?;
        std::fs::write(&node_key_path, json)?;
        println!("  Wrote {}", node_key_path.display());
    } else {
        println!("  Node key exists: {}", node_key_path.display());
    }

    if !validator_key_path.exists() {
        println!("Generating validator key...");
        let validator_key = keys::generate_key_file(account_id);
        let json = serde_json::to_string_pretty(&validator_key)?;
        std::fs::write(&validator_key_path, json)?;
        println!("  Wrote {}", validator_key_path.display());
        println!("  Validator account: {}", account_id);
        println!("  Public key: {}", validator_key.public_key);
    } else {
        println!("  Validator key exists: {}", validator_key_path.display());
    }

    // Step 2: Write config.json
    let config_path = home.join("config.json");
    if !config_path.exists() {
        println!("Writing config.json...");
        let config = create_config(chain_id);
        let json = serde_json::to_string_pretty(&config)?;
        std::fs::write(&config_path, json)?;
        println!("  Wrote {}", config_path.display());
    } else {
        println!("  Config exists: {}", config_path.display());
    }

    // Step 3: Handle genesis.json
    let genesis_dest = home.join("genesis.json");
    if let Some(src) = genesis_path {
        println!("Copying genesis from {}...", src.display());
        if !src.exists() {
            return Err(format!("Genesis file not found: {}", src.display()).into());
        }
        std::fs::copy(src, &genesis_dest)?;
        println!("  Wrote {}", genesis_dest.display());
    } else if !genesis_dest.exists() {
        // Try to use neard init to generate a default genesis
        println!("No genesis provided. Generating default genesis via neard init...");
        let status = Command::new(neard_bin)
            .args([
                "--home",
                home.to_str().unwrap(),
                "init",
                "--chain-id",
                chain_id,
                "--account-id",
                account_id,
            ])
            .status();

        match status {
            Ok(s) if s.success() => {
                println!("  neard init completed successfully");
            }
            Ok(s) => {
                eprintln!("  Warning: neard init exited with status {}", s);
                eprintln!("  You may need to provide a genesis.json manually with --genesis");
            }
            Err(e) => {
                eprintln!("  Warning: Could not run neard: {}", e);
                eprintln!("  You can provide a genesis.json manually with --genesis");
                eprintln!("  Or build neard: cd nearcore && cargo build -p neard --release");
            }
        }
    } else {
        println!("  Genesis exists: {}", genesis_dest.display());
    }

    // Step 4: Create data directory
    let data_dir = home.join("data");
    std::fs::create_dir_all(&data_dir)?;

    println!();
    println!("Node initialized at {}", home.display());
    println!();
    println!("To start the node:");
    println!("  bitinfinity-neard run --home {}", home.display());
    println!();
    println!("To use a custom genesis from bitinfinity-tools:");
    println!(
        "  bitinfinity-neard init --home {} --genesis /path/to/genesis.json",
        home.display()
    );

    Ok(())
}

// ============================================================================
// Run command
// ============================================================================

fn cmd_run(home: &Path, neard_bin: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Verify required files exist
    let required_files = [
        "config.json",
        "genesis.json",
        "node_key.json",
        "validator_key.json",
    ];
    for file in &required_files {
        let path = home.join(file);
        if !path.exists() {
            eprintln!("Error: Missing required file: {}", path.display());
            eprintln!("Run: bitinfinity-neard init --home {}", home.display());
            std::process::exit(1);
        }
    }

    // Read config to show info
    let config_path = home.join("config.json");
    let config_str = std::fs::read_to_string(&config_path)?;
    let config: NodeConfig = serde_json::from_str(&config_str)?;

    let validator_key_path = home.join("validator_key.json");
    let vk_str = std::fs::read_to_string(&validator_key_path)?;
    let vk: keys::KeyFile = serde_json::from_str(&vk_str)?;

    println!("Starting Bitcoin Infinity Node");
    println!("=============================");
    println!();
    println!("  Home: {}", home.display());
    println!("  Validator: {} ({})", vk.account_id, vk.public_key);
    println!(
        "  RPC: {}",
        config
            .rpc
            .as_ref()
            .map(|r| r.addr.as_str())
            .unwrap_or("disabled")
    );
    println!("  Network: {}", config.network.addr);
    println!();

    // Exec neard run, replacing this process
    let err = exec_neard(neard_bin, home);
    Err(format!("Failed to exec neard: {}", err).into())
}

/// On Unix, replace the current process with neard. On other platforms, spawn.
fn exec_neard(neard_bin: &str, home: &Path) -> std::io::Error {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Command::new(neard_bin)
            .args(["--home", home.to_str().unwrap(), "run"])
            .exec()
    }
    #[cfg(not(unix))]
    {
        match Command::new(neard_bin)
            .args(["--home", home.to_str().unwrap(), "run"])
            .status()
        {
            Ok(status) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("neard exited with: {}", status),
            ),
            Err(e) => e,
        }
    }
}

// ============================================================================
// Config command
// ============================================================================

fn cmd_config(home: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Bitcoin Infinity Node Configuration");
    println!("===================================");
    println!();
    println!("Home: {}", home.display());
    println!();

    let config_path = home.join("config.json");
    if config_path.exists() {
        let config_str = std::fs::read_to_string(&config_path)?;
        let config: NodeConfig = serde_json::from_str(&config_str)?;
        println!("Network address: {}", config.network.addr);
        if let Some(rpc) = &config.rpc {
            println!("RPC address: {}", rpc.addr);
        }
    } else {
        println!("config.json: not found");
    }

    let genesis_path = home.join("genesis.json");
    if genesis_path.exists() {
        let genesis_str = std::fs::read_to_string(&genesis_path)?;
        let genesis: serde_json::Value = serde_json::from_str(&genesis_str)?;
        if let Some(chain_id) = genesis.get("chain_id").and_then(|v| v.as_str()) {
            println!("Chain ID: {}", chain_id);
        }
        if let Some(validators) = genesis.get("validators").and_then(|v| v.as_array()) {
            println!("Validators: {}", validators.len());
            for v in validators {
                if let Some(id) = v.get("account_id").and_then(|x| x.as_str()) {
                    println!("  - {}", id);
                }
            }
        }
        if let Some(total) = genesis.get("total_supply").and_then(|v| v.as_str()) {
            println!("Total supply: {} yoctoBIT", total);
        }
        if let Some(records) = genesis.get("records").and_then(|v| v.as_array()) {
            println!("Genesis records: {}", records.len());
        }
    } else {
        println!("genesis.json: not found");
    }

    let vk_path = home.join("validator_key.json");
    if vk_path.exists() {
        let vk_str = std::fs::read_to_string(&vk_path)?;
        let vk: keys::KeyFile = serde_json::from_str(&vk_str)?;
        println!("Validator account: {}", vk.account_id);
        println!("Validator key: {}", vk.public_key);
    }

    let nk_path = home.join("node_key.json");
    if nk_path.exists() {
        let nk_str = std::fs::read_to_string(&nk_path)?;
        let nk: keys::KeyFile = serde_json::from_str(&nk_str)?;
        println!("Node key: {}", nk.public_key);
    }

    Ok(())
}

// ============================================================================
// Keygen command
// ============================================================================

fn cmd_keygen(home: &Path, account_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(home)?;

    println!("Generating keys for Bitcoin Infinity node");
    println!();

    let node_key = keys::generate_key_file("node");
    let node_key_path = home.join("node_key.json");
    std::fs::write(&node_key_path, serde_json::to_string_pretty(&node_key)?)?;
    println!("Node key: {}", node_key.public_key);
    println!("  Wrote {}", node_key_path.display());

    let validator_key = keys::generate_key_file(account_id);
    let vk_path = home.join("validator_key.json");
    std::fs::write(&vk_path, serde_json::to_string_pretty(&validator_key)?)?;
    println!("Validator key: {}", validator_key.public_key);
    println!("  Account: {}", account_id);
    println!("  Wrote {}", vk_path.display());

    Ok(())
}

// ============================================================================
// Config generation — produces a config.json compatible with nearcore
// ============================================================================

/// Minimal nearcore-compatible config.json structure.
/// Only the fields we need; nearcore fills in defaults for missing fields.
#[derive(Debug, Serialize, Deserialize)]
struct NodeConfig {
    genesis_file: String,
    validator_key_file: String,
    node_key_file: String,
    #[serde(default)]
    network: NetworkConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    rpc: Option<RpcConfig>,
    #[serde(default)]
    consensus: ConsensusConfig,
    #[serde(default)]
    tracked_shards: Vec<u64>,
    #[serde(default = "default_true")]
    archive: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
struct NetworkConfig {
    #[serde(default = "default_network_addr")]
    addr: String,
    boot_nodes: String,
}

fn default_network_addr() -> String {
    "0.0.0.0:24567".to_string()
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            addr: default_network_addr(),
            boot_nodes: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RpcConfig {
    addr: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConsensusConfig {
    #[serde(default)]
    min_block_production_delay: Option<serde_json::Value>,
    #[serde(default)]
    max_block_production_delay: Option<serde_json::Value>,
}

fn create_config(_chain_id: &str) -> NodeConfig {
    NodeConfig {
        genesis_file: "genesis.json".to_string(),
        validator_key_file: "validator_key.json".to_string(),
        node_key_file: "node_key.json".to_string(),
        network: NetworkConfig {
            addr: "0.0.0.0:24567".to_string(),
            boot_nodes: String::new(),
        },
        rpc: Some(RpcConfig {
            addr: "0.0.0.0:3030".to_string(),
        }),
        consensus: ConsensusConfig {
            min_block_production_delay: None,
            max_block_production_delay: None,
        },
        tracked_shards: vec![0],
        archive: false,
    }
}

/// Expand ~ to home directory
fn expand_home(path: &Path) -> PathBuf {
    if path.starts_with("~") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let path_str = path.to_string_lossy();
        let expanded = path_str.replacen('~', &home, 1);
        PathBuf::from(expanded)
    } else {
        path.to_path_buf()
    }
}
