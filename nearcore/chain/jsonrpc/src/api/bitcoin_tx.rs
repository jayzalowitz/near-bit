use serde_json::Value;

use near_jsonrpc_primitives::errors::RpcParseError;

use super::{Params, RpcRequest};

/// Request for the `broadcast_bitcoin_tx` RPC method.
/// Accepts a raw Bitcoin transaction hex string.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RpcBroadcastBitcoinTxRequest {
    pub tx_hex: String,
}

impl RpcRequest for RpcBroadcastBitcoinTxRequest {
    fn parse(value: Value) -> Result<Self, RpcParseError> {
        Params::new(value)
            .try_singleton(|tx_hex: String| Ok(RpcBroadcastBitcoinTxRequest { tx_hex }))
            .unwrap_or_parse()
    }
}
