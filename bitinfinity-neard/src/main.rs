use clap::{Parser, Subcommand};
use near_account_id::AccountId;
use serde_json::Value;
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
        #[arg(long = "genesis-config", alias = "genesis")]
        genesis_config: Option<PathBuf>,

        /// Path to a JSON array/object containing genesis records to merge
        #[arg(long = "genesis-records")]
        genesis_records: Option<PathBuf>,

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
            genesis_config,
            genesis_records,
            neard_bin,
        } => {
            cmd_init(
                &expand_home(&home),
                &chain_id,
                &account_id,
                genesis_config.as_deref(),
                genesis_records.as_deref(),
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
    genesis_config_path: Option<&Path>,
    genesis_records_path: Option<&Path>,
    neard_bin: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Bitcoin Infinity Node Initialization");
    println!("====================================");
    println!();

    std::fs::create_dir_all(home)?;

    ensure_neard_initialized(home, chain_id, account_id, neard_bin)?;
    sync_key_files(home, account_id)?;
    patch_config_paths(home)?;

    if let Some(src) = genesis_config_path {
        println!("Copying genesis config from {}...", src.display());
        if !src.exists() {
            return Err(format!("Genesis config not found: {}", src.display()).into());
        }
        std::fs::copy(src, home.join("genesis.json"))?;
        println!("  Wrote {}", home.join("genesis.json").display());
    }

    if let Some(records_path) = genesis_records_path {
        merge_genesis_records(home, records_path)?;
    }

    // Keep data directory explicit for parity with previous behavior.
    std::fs::create_dir_all(home.join("data"))?;

    println!();
    println!("Node initialized at {}", home.display());
    println!();
    println!("To start the node:");
    println!("  bitinfinity-neard run --home {}", home.display());
    println!();
    println!("To use custom genesis files:");
    println!(
        "  bitinfinity-neard init --home {} --genesis-config /path/to/genesis.json --genesis-records /path/to/records.json",
        home.display()
    );

    Ok(())
}

fn ensure_neard_initialized(
    home: &Path,
    chain_id: &str,
    account_id: &str,
    neard_bin: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = home.join("config.json");
    let genesis_path = home.join("genesis.json");

    if config_path.exists() && genesis_path.exists() {
        println!("  Existing config/genesis found; skipping neard init.");
        return Ok(());
    }

    println!("Running neard init to generate base config...");
    let status = Command::new(neard_bin)
        .args([
            "--home",
            home.to_str().ok_or("Invalid home path")?,
            "init",
            "--chain-id",
            chain_id,
            "--account-id",
            account_id,
        ])
        .status()?;

    if !status.success() {
        return Err(format!("neard init failed with status {status}").into());
    }

    println!("  neard init completed");
    Ok(())
}

fn sync_key_files(home: &Path, account_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let keys_dir = home.join("keys");
    std::fs::create_dir_all(&keys_dir)?;

    let root_node = home.join("node_key.json");
    let root_validator = home.join("validator_key.json");
    let key_node = keys_dir.join("node_key.json");
    let key_validator = keys_dir.join("validator_key.json");

    ensure_key_file(&root_node, &key_node, "node")?;
    ensure_key_file(&root_validator, &key_validator, account_id)?;

    Ok(())
}

fn ensure_key_file(
    root_path: &Path,
    key_path: &Path,
    account_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !key_path.exists() {
        if root_path.exists() {
            std::fs::copy(root_path, key_path)?;
        } else {
            let generated = keys::generate_key_file(account_id);
            let json = serde_json::to_string_pretty(&generated)?;
            std::fs::write(key_path, json)?;
        }
    }

    // Keep root-level copies for compatibility with existing tooling.
    if !root_path.exists() {
        std::fs::copy(key_path, root_path)?;
    }

    Ok(())
}

fn patch_config_paths(home: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = home.join("config.json");
    let mut config: Value = serde_json::from_str(&std::fs::read_to_string(&config_path)?)?;
    let Some(obj) = config.as_object_mut() else {
        return Err(format!("Invalid config format: {}", config_path.display()).into());
    };

    obj.insert(
        "validator_key_file".to_string(),
        Value::String("keys/validator_key.json".to_string()),
    );
    obj.insert(
        "node_key_file".to_string(),
        Value::String("keys/node_key.json".to_string()),
    );
    obj.insert(
        "genesis_file".to_string(),
        Value::String("genesis.json".to_string()),
    );

    // Single-validator local bootstrap must not wait for external peers.
    if let Some(consensus) = obj.get_mut("consensus").and_then(Value::as_object_mut) {
        consensus.insert("min_num_peers".to_string(), Value::from(0u64));
    }

    std::fs::write(config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

fn merge_genesis_records(
    home: &Path,
    records_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Merging genesis records from {}...", records_path.display());

    if !records_path.exists() {
        return Err(format!("Genesis records file not found: {}", records_path.display()).into());
    }

    let mut genesis: Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("genesis.json"))?)?;
    let Some(genesis_obj) = genesis.as_object_mut() else {
        return Err("Invalid genesis.json: expected object".into());
    };

    let mut incoming = load_records_payload(records_path)?;
    validate_record_account_ids(&incoming)?;
    let added_supply = incoming.iter().try_fold(0u128, |acc, record| {
        let delta = account_record_supply(record)?;
        acc.checked_add(delta)
            .ok_or_else(|| "total supply overflow while merging records".to_string())
    })?;

    let final_count = {
        let records_value = genesis_obj
            .entry("records".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        let Some(records_array) = records_value.as_array_mut() else {
            return Err("Invalid genesis.json: records must be an array".into());
        };

        let added = incoming.len();
        records_array.append(&mut incoming);
        println!("  Added {} record(s)", added);
        records_array.len()
    };

    if added_supply > 0 {
        let current_total = genesis_obj
            .get("total_supply")
            .and_then(Value::as_str)
            .ok_or("Invalid genesis.json: total_supply must be a string")?
            .parse::<u128>()
            .map_err(|e| format!("Invalid genesis.json total_supply value: {e}"))?;
        let new_total = current_total
            .checked_add(added_supply)
            .ok_or("total_supply overflow while merging records")?;
        genesis_obj.insert(
            "total_supply".to_string(),
            Value::String(new_total.to_string()),
        );
        println!("  Updated total_supply: {} -> {}", current_total, new_total);
    }

    std::fs::write(
        home.join("genesis.json"),
        serde_json::to_string_pretty(&genesis)?,
    )?;

    println!("  Total records: {}", final_count);

    Ok(())
}

fn load_records_payload(path: &Path) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    match value {
        Value::Array(records) => Ok(records),
        Value::Object(mut obj) => {
            let Some(records) = obj.remove("records") else {
                return Err(
                    "Invalid genesis records payload: expected array or object with records".into(),
                );
            };
            let Some(records_array) = records.as_array() else {
                return Err("Invalid genesis records payload: records must be an array".into());
            };
            Ok(records_array.clone())
        }
        _ => Err("Invalid genesis records payload: expected JSON array/object".into()),
    }
}

fn validate_record_account_ids(records: &[Value]) -> Result<(), Box<dyn std::error::Error>> {
    for (idx, record) in records.iter().enumerate() {
        if let Some(account_id) = extract_record_account_id(record) {
            AccountId::validate(account_id)
                .map_err(|e| format!("Invalid account_id at record {idx}: {account_id} ({e})"))?;
        }
    }
    Ok(())
}

fn extract_record_account_id(record: &Value) -> Option<&str> {
    let obj = record.as_object()?;

    for variant in [
        "Account",
        "AccessKey",
        "Data",
        "Contract",
        "ReceivedData",
        "GasKeyNonce",
    ] {
        if let Some(id) = obj
            .get(variant)
            .and_then(|v| v.get("account_id"))
            .and_then(Value::as_str)
        {
            return Some(id);
        }
    }

    None
}

fn account_record_supply(record: &Value) -> Result<u128, String> {
    let Some(account) = record
        .get("Account")
        .and_then(|v| v.get("account"))
        .and_then(Value::as_object)
    else {
        return Ok(0);
    };

    let amount_str = account
        .get("amount")
        .and_then(Value::as_str)
        .ok_or("Account record missing amount string")?;
    let locked_str = account
        .get("locked")
        .and_then(Value::as_str)
        .ok_or("Account record missing locked string")?;

    let amount = amount_str
        .parse::<u128>()
        .map_err(|e| format!("Invalid Account amount {amount_str}: {e}"))?;
    let locked = locked_str
        .parse::<u128>()
        .map_err(|e| format!("Invalid Account locked {locked_str}: {e}"))?;

    amount
        .checked_add(locked)
        .ok_or_else(|| "overflow computing Account amount + locked".to_string())
}

// ============================================================================
// Run command
// ============================================================================

fn cmd_run(home: &Path, neard_bin: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = home.join("config.json");
    if !config_path.exists() {
        return Err(format!("Missing required file: {}", config_path.display()).into());
    }

    let config: Value = serde_json::from_str(&std::fs::read_to_string(&config_path)?)?;

    let genesis_path = resolve_config_path(home, &config, "genesis_file", "genesis.json");
    let validator_key_path = resolve_config_path(
        home,
        &config,
        "validator_key_file",
        "keys/validator_key.json",
    );
    let node_key_path = resolve_config_path(home, &config, "node_key_file", "keys/node_key.json");

    for path in [&genesis_path, &validator_key_path, &node_key_path] {
        if !path.exists() {
            return Err(format!("Missing required file: {}", path.display()).into());
        }
    }

    let vk: keys::KeyFile = serde_json::from_str(&std::fs::read_to_string(&validator_key_path)?)?;

    println!("Starting Bitcoin Infinity Node");
    println!("=============================");
    println!();
    println!("  Home: {}", home.display());
    println!("  Validator: {} ({})", vk.account_id, vk.public_key);
    println!(
        "  RPC: {}",
        get_nested_str(&config, &["rpc", "addr"]).unwrap_or("disabled")
    );
    println!(
        "  Network: {}",
        get_nested_str(&config, &["network", "addr"]).unwrap_or("unknown")
    );
    println!();

    // Exec neard run, replacing this process
    let err = exec_neard(neard_bin, home);
    Err(format!("Failed to exec neard: {err}").into())
}

/// On Unix, replace the current process with neard. On other platforms, spawn.
fn exec_neard(neard_bin: &str, home: &Path) -> std::io::Error {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Command::new(neard_bin)
            .args(["--home", home.to_str().unwrap_or_default(), "run"])
            .exec()
    }
    #[cfg(not(unix))]
    {
        match Command::new(neard_bin)
            .args(["--home", home.to_str().unwrap_or_default(), "run"])
            .status()
        {
            Ok(status) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("neard exited with: {status}"),
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
    if !config_path.exists() {
        println!("config.json: not found");
        return Ok(());
    }

    let config: Value = serde_json::from_str(&std::fs::read_to_string(&config_path)?)?;
    println!(
        "Network address: {}",
        get_nested_str(&config, &["network", "addr"]).unwrap_or("unknown")
    );
    println!(
        "RPC address: {}",
        get_nested_str(&config, &["rpc", "addr"]).unwrap_or("disabled")
    );

    let genesis_path = resolve_config_path(home, &config, "genesis_file", "genesis.json");
    if genesis_path.exists() {
        let genesis: Value = serde_json::from_str(&std::fs::read_to_string(&genesis_path)?)?;
        if let Some(chain_id) = genesis.get("chain_id").and_then(Value::as_str) {
            println!("Chain ID: {}", chain_id);
        }
        if let Some(validators) = genesis.get("validators").and_then(Value::as_array) {
            println!("Validators: {}", validators.len());
            for v in validators {
                if let Some(id) = v.get("account_id").and_then(Value::as_str) {
                    println!("  - {}", id);
                }
            }
        }
        if let Some(total) = genesis.get("total_supply").and_then(Value::as_str) {
            println!("Total supply: {} yoctoBIT", total);
        }
        if let Some(records) = genesis.get("records").and_then(Value::as_array) {
            println!("Genesis records: {}", records.len());
        }
    } else {
        println!("genesis.json: not found ({})", genesis_path.display());
    }

    let validator_key_path = resolve_config_path(
        home,
        &config,
        "validator_key_file",
        "keys/validator_key.json",
    );
    if validator_key_path.exists() {
        let vk: keys::KeyFile =
            serde_json::from_str(&std::fs::read_to_string(&validator_key_path)?)?;
        println!("Validator account: {}", vk.account_id);
        println!("Validator key: {}", vk.public_key);
    }

    let node_key_path = resolve_config_path(home, &config, "node_key_file", "keys/node_key.json");
    if node_key_path.exists() {
        let nk: keys::KeyFile = serde_json::from_str(&std::fs::read_to_string(&node_key_path)?)?;
        println!("Node key: {}", nk.public_key);
    }

    Ok(())
}

// ============================================================================
// Keygen command
// ============================================================================

fn cmd_keygen(home: &Path, account_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let keys_dir = home.join("keys");
    std::fs::create_dir_all(&keys_dir)?;

    println!("Generating keys for Bitcoin Infinity node");
    println!();

    let node_key = keys::generate_key_file("node");
    let node_key_path = keys_dir.join("node_key.json");
    std::fs::write(&node_key_path, serde_json::to_string_pretty(&node_key)?)?;
    // Compatibility copy for tools expecting root-level key files.
    std::fs::write(
        home.join("node_key.json"),
        serde_json::to_string_pretty(&node_key)?,
    )?;
    println!("Node key: {}", node_key.public_key);
    println!("  Wrote {}", node_key_path.display());

    let validator_key = keys::generate_key_file(account_id);
    let vk_path = keys_dir.join("validator_key.json");
    std::fs::write(&vk_path, serde_json::to_string_pretty(&validator_key)?)?;
    std::fs::write(
        home.join("validator_key.json"),
        serde_json::to_string_pretty(&validator_key)?,
    )?;
    println!("Validator key: {}", validator_key.public_key);
    println!("  Account: {}", account_id);
    println!("  Wrote {}", vk_path.display());

    Ok(())
}

fn resolve_config_path(home: &Path, config: &Value, key: &str, default_rel: &str) -> PathBuf {
    let configured = config
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default_rel);
    let configured_path = PathBuf::from(configured);
    if configured_path.is_absolute() {
        configured_path
    } else {
        home.join(configured_path)
    }
}

fn get_nested_str<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_record_account_id() {
        let account = serde_json::json!({
            "Account": {
                "account_id": "validator.bitinfinity",
                "account": {
                    "amount": "1",
                    "locked": "0",
                    "code_hash": "11111111111111111111111111111111",
                    "storage_usage": 0
                }
            }
        });
        let access_key = serde_json::json!({
            "AccessKey": {
                "account_id": "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
                "public_key": "ed25519:11111111111111111111111111111111",
                "access_key": { "nonce": 0, "permission": "FullAccess" }
            }
        });

        assert_eq!(
            extract_record_account_id(&account),
            Some("validator.bitinfinity")
        );
        assert_eq!(
            extract_record_account_id(&access_key),
            Some("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")
        );
    }

    #[test]
    fn test_load_records_payload_from_array_and_object() {
        let records = vec![serde_json::json!({
            "Account": {
                "account_id": "near",
                "account": {
                    "amount": "1",
                    "locked": "0",
                    "code_hash": "11111111111111111111111111111111",
                    "storage_usage": 0
                }
            }
        })];

        let arr = Value::Array(records.clone());
        let obj = serde_json::json!({"records": records});

        let arr_path = std::path::PathBuf::from(format!(
            "/tmp/bitinfinity-records-array-{}.json",
            std::process::id()
        ));
        let obj_path = std::path::PathBuf::from(format!(
            "/tmp/bitinfinity-records-obj-{}.json",
            std::process::id()
        ));

        std::fs::write(&arr_path, serde_json::to_string(&arr).unwrap()).unwrap();
        std::fs::write(&obj_path, serde_json::to_string(&obj).unwrap()).unwrap();

        let from_arr = load_records_payload(&arr_path).unwrap();
        let from_obj = load_records_payload(&obj_path).unwrap();

        assert_eq!(from_arr.len(), 1);
        assert_eq!(from_obj.len(), 1);

        let _ = std::fs::remove_file(arr_path);
        let _ = std::fs::remove_file(obj_path);
    }

    #[test]
    fn test_validate_record_account_ids_accepts_bitcoin_and_near() {
        let records = vec![
            serde_json::json!({
                "Account": {
                    "account_id": "near",
                    "account": {
                        "amount": "1",
                        "locked": "0",
                        "code_hash": "11111111111111111111111111111111",
                        "storage_usage": 0
                    }
                }
            }),
            serde_json::json!({
                "Account": {
                    "account_id": "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
                    "account": {
                        "amount": "1",
                        "locked": "0",
                        "code_hash": "11111111111111111111111111111111",
                        "storage_usage": 0
                    }
                }
            }),
        ];

        validate_record_account_ids(&records).unwrap();
    }
}
