//! Translate Bitcoin raw transactions to NEAR transfer actions

pub struct TransactionTranslator {
    // TODO: implement
}

impl TransactionTranslator {
    pub fn parse_raw_tx(_raw_tx_hex: &str) -> Result<ParsedTransaction, Box<dyn std::error::Error>> {
        todo!("Parse Bitcoin raw transaction hex, extract sender/receiver/amount")
    }
}

pub struct ParsedTransaction {
    pub sender: String,
    pub receiver: String,
    pub amount_satoshis: u64,
}
