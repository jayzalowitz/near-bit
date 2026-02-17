//! Blockchain-related RPC methods: getblockchaininfo, getblock, getblockcount, etc.

pub async fn get_blockchain_info() -> serde_json::Value {
    todo!("Implement getblockchaininfo - return Sydney chain info")
}

pub async fn get_blockcount() -> serde_json::Value {
    todo!("Implement getblockcount")
}

pub async fn get_best_blockhash() -> serde_json::Value {
    todo!("Implement getbestblockhash")
}
