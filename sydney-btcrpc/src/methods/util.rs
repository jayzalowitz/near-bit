//! Utility RPC methods: validateaddress, estimatesmartfee, etc.

pub async fn validate_address(_address: &str) -> serde_json::Value {
    todo!("Implement validateaddress - validate Bitcoin address format")
}

pub async fn estimate_smart_fee() -> serde_json::Value {
    todo!("Implement estimatesmartfee - convert Sydney gas price to sat/vbyte")
}
