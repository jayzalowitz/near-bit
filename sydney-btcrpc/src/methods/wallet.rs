//! Wallet-related RPC methods: getbalance, listunspent, sendrawtransaction, etc.

pub async fn get_balance(_address: &str) -> serde_json::Value {
    todo!("Implement getbalance - query Sydney account balance")
}

pub async fn list_unspent() -> serde_json::Value {
    todo!("Implement listunspent - synthesize UTXOs from account balances")
}

pub async fn send_raw_transaction(_raw_tx: &str) -> serde_json::Value {
    todo!("Implement sendrawtransaction - parse Bitcoin tx and convert to NEAR transfer")
}

pub async fn get_new_address() -> serde_json::Value {
    todo!("Implement getnewaddress - generate secp256k1 keypair")
}
