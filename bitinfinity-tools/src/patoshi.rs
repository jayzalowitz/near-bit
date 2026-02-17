//! Identify and reassign Satoshi Nakamoto's Patoshi pattern coins

pub struct PatoshiIdentifier {
    // TODO: implement
}

impl PatoshiIdentifier {
    pub fn new(_csv_path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        todo!("Load Patoshi addresses from CSV")
    }

    pub fn identify_and_reassign(
        &self,
        utxo_map: &mut std::collections::BTreeMap<String, u64>,
    ) -> Result<PatoshiReassignment, Box<dyn std::error::Error>> {
        todo!("Identify Patoshi addresses, sum balances, reassign to new address")
    }
}

pub struct PatoshiReassignment {
    pub total_satoshis: u64,
    pub target_address: String,
    pub private_key_wif: String,
}
