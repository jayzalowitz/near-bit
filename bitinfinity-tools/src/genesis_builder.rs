//! Convert aggregated UTXO balances into nearcore-compatible genesis format
//!
//! nearcore has very specific requirements for its genesis JSON. This module
//! produces a single genesis.json file (with embedded records) that passes
//! nearcore's genesis validation.

use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;
use std::fs;
use chrono::Utc;

// ============================================================================
// nearcore-compatible genesis types
// ============================================================================

/// Complete genesis file structure matching nearcore's expected format.
/// All fields are at the top level (GenesisConfig fields + records).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Genesis {
    pub protocol_version: u32,
    pub genesis_time: String,
    pub chain_id: String,
    pub genesis_height: u64,
    pub num_block_producer_seats: u64,
    pub num_block_producer_seats_per_shard: Vec<u64>,
    pub avg_hidden_validator_seats_per_shard: Vec<u64>,
    pub dynamic_resharding: bool,
    pub protocol_upgrade_stake_threshold: [i32; 2],
    pub epoch_length: u64,
    pub gas_limit: u64,
    pub min_gas_price: String,
    pub max_gas_price: String,
    pub block_producer_kickout_threshold: u8,
    pub chunk_producer_kickout_threshold: u8,
    pub chunk_validator_only_kickout_threshold: u8,
    pub target_validator_mandates_per_shard: u64,
    pub online_min_threshold: [i32; 2],
    pub online_max_threshold: [i32; 2],
    pub gas_price_adjustment_rate: [i32; 2],
    pub validators: Vec<ValidatorInfo>,
    pub transaction_validity_period: u64,
    pub protocol_reward_rate: [i32; 2],
    pub max_inflation_rate: [i32; 2],
    pub total_supply: String,
    pub num_blocks_per_year: u64,
    pub protocol_treasury_account: String,
    pub fishermen_threshold: String,
    pub minimum_stake_divisor: u64,
    pub shard_layout: ShardLayout,
    pub num_chunk_only_producer_seats: u64,
    pub minimum_validators_per_shard: u64,
    pub max_kickout_stake_perc: u8,
    pub minimum_stake_ratio: [i32; 2],
    pub use_production_config: bool,
    pub records: Vec<StateRecord>,
}

/// Validator entry in genesis (matches nearcore's AccountInfo)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub account_id: String,
    pub public_key: String,
    pub amount: String,
}

/// Shard layout — using V0 for simplicity (single shard)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShardLayout {
    V0 { num_shards: u64, version: u64 },
}

/// State record — nearcore uses externally-tagged enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateRecord {
    Account {
        account_id: String,
        account: AccountData,
    },
    AccessKey {
        account_id: String,
        public_key: String,
        access_key: AccessKeyData,
    },
}

/// Account data matching nearcore's SerdeAccount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    pub amount: String,
    pub locked: String,
    pub code_hash: String,
    pub storage_usage: u64,
    #[serde(default = "default_account_version")]
    pub version: String,
}

fn default_account_version() -> String {
    "V1".to_string()
}

/// Access key data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessKeyData {
    pub nonce: u64,
    pub permission: AccessKeyPermission,
}

/// Access key permission — "FullAccess" or FunctionCall object
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AccessKeyPermission {
    FullAccess(String),
}

/// No-code hash: 32 zero bytes encoded as base58
const NO_CODE_HASH: &str = "11111111111111111111111111111111";

// ============================================================================
// Builder
// ============================================================================

pub struct GenesisBuilder {
    chain_id: String,
    output_dir: std::path::PathBuf,
}

/// Configuration for the validator that will produce blocks
pub struct ValidatorConfig {
    pub account_id: String,
    pub public_key_ed25519: String,
    pub stake_yocto: u128,
    /// Extra free balance for the validator account (for gas, etc.)
    pub balance_yocto: u128,
}

impl GenesisBuilder {
    pub fn new(chain_id: String, output_dir: std::path::PathBuf) -> Self {
        GenesisBuilder { chain_id, output_dir }
    }

    /// Build a complete nearcore-compatible genesis.json from UTXO data + validator config.
    pub fn build(
        &self,
        utxo_map: &BTreeMap<String, u64>,
        validator: &ValidatorConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(&self.output_dir)?;

        let mut records = Vec::new();
        let mut total_supply: u128 = 0;

        // 1. Add validator account (uses ed25519 key, has stake)
        let validator_amount = validator.balance_yocto;
        let validator_locked = validator.stake_yocto;
        total_supply += validator_amount + validator_locked;

        records.push(StateRecord::Account {
            account_id: validator.account_id.clone(),
            account: AccountData {
                amount: validator_amount.to_string(),
                locked: validator_locked.to_string(),
                code_hash: NO_CODE_HASH.to_string(),
                storage_usage: 0,
                version: "V1".to_string(),
            },
        });
        records.push(StateRecord::AccessKey {
            account_id: validator.account_id.clone(),
            public_key: validator.public_key_ed25519.clone(),
            access_key: AccessKeyData {
                nonce: 0,
                permission: AccessKeyPermission::FullAccess("FullAccess".to_string()),
            },
        });

        // 2. Add protocol treasury account if different from validator
        let treasury_account = "near".to_string();
        if treasury_account != validator.account_id {
            let treasury_balance: u128 = 1_000_000_000_000_000_000_000_000; // 1 BIT
            total_supply += treasury_balance;

            records.push(StateRecord::Account {
                account_id: treasury_account.clone(),
                account: AccountData {
                    amount: treasury_balance.to_string(),
                    locked: "0".to_string(),
                    code_hash: NO_CODE_HASH.to_string(),
                    storage_usage: 0,
                    version: "V1".to_string(),
                },
            });
        }

        // 3. Add all Bitcoin address accounts from UTXO data
        // NEAR AccountId only allows lowercase — lowercase all Bitcoin addresses.
        // P2PKH/P2SH use base58check (mixed case) but bech32 is already lowercase.
        for (addr, satoshis) in utxo_map {
            let addr_lower = addr.to_lowercase();
            if addr_lower == validator.account_id || addr_lower == treasury_account {
                continue;
            }

            let yocto = *satoshis as u128 * 10u128.pow(16);
            total_supply = total_supply.checked_add(yocto)
                .ok_or("Total supply overflow")?;

            records.push(StateRecord::Account {
                account_id: addr_lower,
                account: AccountData {
                    amount: yocto.to_string(),
                    locked: "0".to_string(),
                    code_hash: NO_CODE_HASH.to_string(),
                    storage_usage: 0,
                    version: "V1".to_string(),
                },
            });
            // No AccessKey records for Bitcoin accounts — auto-registered on first tx.
        }

        // 4. Build the complete genesis
        let genesis = Genesis {
            protocol_version: 84,
            genesis_time: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            chain_id: self.chain_id.clone(),
            genesis_height: 0,
            num_block_producer_seats: 1,
            num_block_producer_seats_per_shard: vec![1],
            avg_hidden_validator_seats_per_shard: vec![0],
            dynamic_resharding: false,
            protocol_upgrade_stake_threshold: [4, 5],
            epoch_length: 500,
            gas_limit: 1_000_000_000_000_000,
            min_gas_price: "100000000".to_string(),
            max_gas_price: "10000000000000000000000".to_string(),
            block_producer_kickout_threshold: 90,
            chunk_producer_kickout_threshold: 90,
            chunk_validator_only_kickout_threshold: 80,
            target_validator_mandates_per_shard: 68,
            online_min_threshold: [9, 10],
            online_max_threshold: [99, 100],
            gas_price_adjustment_rate: [1, 100],
            validators: vec![ValidatorInfo {
                account_id: validator.account_id.clone(),
                public_key: validator.public_key_ed25519.clone(),
                amount: validator.stake_yocto.to_string(),
            }],
            transaction_validity_period: 100,
            protocol_reward_rate: [0, 1],
            max_inflation_rate: [0, 1],
            total_supply: total_supply.to_string(),
            num_blocks_per_year: 31_536_000,
            protocol_treasury_account: treasury_account,
            fishermen_threshold: "10000000000000000000000000".to_string(),
            minimum_stake_divisor: 10,
            shard_layout: ShardLayout::V0 { num_shards: 1, version: 0 },
            num_chunk_only_producer_seats: 300,
            minimum_validators_per_shard: 1,
            max_kickout_stake_perc: 100,
            minimum_stake_ratio: [1, 6250],
            use_production_config: false,
            records,
        };

        let genesis_path = self.output_dir.join("genesis.json");
        let genesis_json = serde_json::to_string_pretty(&genesis)?;
        fs::write(&genesis_path, &genesis_json)?;
        println!("✓ Wrote genesis to {}", genesis_path.display());
        println!("  Chain ID: {}", self.chain_id);
        println!("  Total supply: {} yoctoBIT", total_supply);
        println!("  Validator: {} (stake: {} yoctoBIT)", validator.account_id, validator.stake_yocto);
        println!("  Bitcoin address accounts: {}", utxo_map.len());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_validator() -> ValidatorConfig {
        ValidatorConfig {
            account_id: "validator.bitinfinity".to_string(),
            public_key_ed25519: "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp".to_string(),
            stake_yocto: 50_000_000_000_000_000_000_000_000_000_000, // 50,000 BIT
            balance_yocto: 1_000_000_000_000_000_000_000_000_000_000, // 1,000,000 BIT
        }
    }

    #[test]
    fn test_genesis_builder_produces_valid_format() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::path::PathBuf::from("/tmp/test_genesis_nearcore");
        let _ = fs::remove_dir_all(&temp_dir);

        let builder = GenesisBuilder::new("bitinfinity-testnet".to_string(), temp_dir.clone());

        let mut utxos = BTreeMap::new();
        utxos.insert("1a1zp1ep5qgefi2dmptftl5slmv7divfna".to_string(), 5_000_000_000);
        utxos.insert("3j98t1wpez73cnmyviecrnyiwrnqrhwnly".to_string(), 1_000_000_000);

        builder.build(&utxos, &test_validator())?;

        let genesis_path = temp_dir.join("genesis.json");
        assert!(genesis_path.exists());

        let content = fs::read_to_string(&genesis_path)?;
        let genesis: Genesis = serde_json::from_str(&content)?;

        assert_eq!(genesis.chain_id, "bitinfinity-testnet");
        assert_eq!(genesis.protocol_version, 84);
        assert_eq!(genesis.genesis_height, 0);
        assert_eq!(genesis.epoch_length, 500);
        assert_eq!(genesis.validators.len(), 1);
        assert_eq!(genesis.validators[0].account_id, "validator.bitinfinity");

        // validator account + validator access key + treasury + 2 BTC accounts = 5 records
        assert_eq!(genesis.records.len(), 5);

        // Verify total_supply matches sum of all account (amount + locked)
        let mut computed_total: u128 = 0;
        for record in &genesis.records {
            if let StateRecord::Account { account, .. } = record {
                computed_total += account.amount.parse::<u128>().unwrap();
                computed_total += account.locked.parse::<u128>().unwrap();
            }
        }
        assert_eq!(genesis.total_supply, computed_total.to_string());

        // Verify validator locked matches validators[].amount
        let validator_record = genesis.records.iter().find(|r| {
            matches!(r, StateRecord::Account { account_id, .. } if account_id == "validator.bitinfinity")
        });
        if let Some(StateRecord::Account { account, .. }) = validator_record {
            assert_eq!(account.locked, genesis.validators[0].amount);
        } else {
            panic!("Validator account record not found");
        }

        // No AccessKey records for Bitcoin addresses
        let btc_access_keys: Vec<_> = genesis.records.iter().filter(|r| {
            matches!(r, StateRecord::AccessKey { account_id, .. }
                if account_id.starts_with('1') || account_id.starts_with('3') || account_id.starts_with("bc1"))
        }).collect();
        assert_eq!(btc_access_keys.len(), 0);

        fs::remove_dir_all(&temp_dir)?;
        Ok(())
    }

    #[test]
    fn test_total_supply_consistency() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::path::PathBuf::from("/tmp/test_genesis_supply");
        let _ = fs::remove_dir_all(&temp_dir);

        let builder = GenesisBuilder::new("test-supply".to_string(), temp_dir.clone());

        let mut utxos = BTreeMap::new();
        utxos.insert("1addr1xxxxxxxxxxxxxxxxxxxxxxx9Cjx9".to_string(), 100_000_000);
        utxos.insert("1addr2xxxxxxxxxxxxxxxxxxxxxxx9Cjx9".to_string(), 200_000_000);
        utxos.insert("1addr3xxxxxxxxxxxxxxxxxxxxxxx9Cjx9".to_string(), 50_000_000);

        let validator = test_validator();
        builder.build(&utxos, &validator)?;

        let content = fs::read_to_string(temp_dir.join("genesis.json"))?;
        let genesis: Genesis = serde_json::from_str(&content)?;

        let validator_total = validator.balance_yocto + validator.stake_yocto;
        let treasury: u128 = 1_000_000_000_000_000_000_000_000;
        let utxo_total: u128 = (100_000_000u128 + 200_000_000 + 50_000_000) * 10u128.pow(16);
        let expected = validator_total + treasury + utxo_total;

        assert_eq!(genesis.total_supply, expected.to_string());

        fs::remove_dir_all(&temp_dir)?;
        Ok(())
    }
}
