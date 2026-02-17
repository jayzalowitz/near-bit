//! Parse Bitcoin UTXO snapshot from dumptxoutset binary format

pub struct UtxoParser {
    // TODO: implement
}

impl UtxoParser {
    pub fn new(_path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        todo!("Implement UTXO parser using txoutset crate")
    }

    pub fn parse_and_aggregate(
        &mut self,
    ) -> Result<std::collections::BTreeMap<String, u64>, Box<dyn std::error::Error>> {
        todo!("Stream parse UTXOs and aggregate by address")
    }
}
