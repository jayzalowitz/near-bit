//! Convert aggregated UTXO balances into NEAR genesis format

use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use chrono::Utc;

/// Genesis configuration for Bitcoin Infinity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisConfig {
    pub chain_id: String,
    pub protocol_version: u32,
    pub genesis_height: u64,
    pub genesis_time: String,
    pub total_supply: String, // in yoctoBIT
    pub validators: Vec<ValidatorConfig>,
    pub sharding_config: ShardingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorConfig {
    pub account_id: String,
    pub stake: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardingConfig {
    pub num_shards: u32,
}

/// Account state record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StateRecord {
    #[serde(rename = "Account")]
    Account {
        account_id: String,
        balance: String, // in yoctoBIT
        nonce: u64,
    },
}

pub struct GenesisBuilder {
    chain_id: String,
    output_dir: std::path::PathBuf,
}

impl GenesisBuilder {
    pub fn new(chain_id: String, output_dir: std::path::PathBuf) -> Self {
        GenesisBuilder { chain_id, output_dir }
    }

    /// Build genesis files from UTXO data
    pub fn build(
        &self,
        utxo_map: &BTreeMap<String, u64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create output directory if it doesn't exist
        fs::create_dir_all(&self.output_dir)?;

        // Calculate total supply in yoctoBIT (satoshi * 10^16)
        let mut total_supply_yocto: u128 = 0;
        for (_addr, satoshis) in utxo_map {
            let yocto = *satoshis as u128 * 10u128.pow(16);
            total_supply_yocto = total_supply_yocto.checked_add(yocto)
                .ok_or("Total supply overflow")?;
        }

        // Create genesis config
        let genesis_config = GenesisConfig {
            chain_id: self.chain_id.clone(),
            protocol_version: 1,
            genesis_height: 0,
            genesis_time: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            total_supply: total_supply_yocto.to_string(),
            validators: vec![], // Will be set up later
            sharding_config: ShardingConfig { num_shards: 1 },
        };

        // Write genesis config
        let config_path = self.output_dir.join("genesis_config.json");
        let config_json = serde_json::to_string_pretty(&genesis_config)?;
        fs::write(&config_path, config_json)?;
        println!("✓ Wrote genesis config to {}", config_path.display());

        // Write account records
        self.write_account_records(utxo_map)?;

        Ok(())
    }

    fn write_account_records(
        &self,
        utxo_map: &BTreeMap<String, u64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let records_path = self.output_dir.join("records.json");
        let file = fs::File::create(&records_path)?;
        let mut writer = std::io::BufWriter::new(file);

        // Write records array start
        use std::io::Write;
        writeln!(writer, "[")?;

        let mut first = true;
        for (addr, satoshis) in utxo_map {
            if !first {
                writeln!(writer, ",")?;
            }
            first = false;

            // Convert satoshis to yoctoBIT
            let yocto = *satoshis as u128 * 10u128.pow(16);

            let record = StateRecord::Account {
                account_id: addr.clone(),
                balance: yocto.to_string(),
                nonce: 0,
            };

            let json = serde_json::to_string(&record)?;
            write!(writer, "  {}", json)?;
        }

        writeln!(writer)?;
        writeln!(writer, "]")?;

        println!("✓ Wrote {} account records to {}", utxo_map.len(), records_path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_builder() -> Result<(), Box<dyn std::error::Error>> {
        // Create temporary directory for test
        let temp_dir = std::path::PathBuf::from("/tmp/test_genesis");
        let _ = fs::remove_dir_all(&temp_dir);

        let builder = GenesisBuilder::new("test-chain".to_string(), temp_dir.clone());

        // Create test UTXO map
        let mut utxos = BTreeMap::new();
        utxos.insert("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string(), 5_000_000_000); // 50 BTC
        utxos.insert("1address2".to_string(), 1_000_000_000); // 10 BTC

        builder.build(&utxos)?;

        // Verify files were created
        assert!(temp_dir.join("genesis_config.json").exists());
        assert!(temp_dir.join("records.json").exists());

        // Verify content
        let config_content = fs::read_to_string(temp_dir.join("genesis_config.json"))?;
        assert!(config_content.contains("test-chain"));
        assert!(config_content.contains("total_supply"));

        // Cleanup
        fs::remove_dir_all(&temp_dir)?;
        Ok(())
    }
}
