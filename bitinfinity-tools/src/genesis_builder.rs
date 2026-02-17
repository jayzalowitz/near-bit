//! Convert aggregated UTXO balances into NEAR genesis format

pub struct GenesisBuilder {
    // TODO: implement
}

impl GenesisBuilder {
    pub fn new() -> Self {
        todo!("Initialize genesis builder")
    }

    pub fn build(
        &self,
        _utxo_map: &std::collections::BTreeMap<String, u64>,
        _chain_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!("Stream UTXO balances to genesis records, create genesis config")
    }
}
