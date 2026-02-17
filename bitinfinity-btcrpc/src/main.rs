use axum::{
    extract::{rejection::JsonRejection, Json},
    http::StatusCode,
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

mod methods;
mod utxo_synth;
mod tx_translator;

/// Bitcoin-compatible JSON-RPC request
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Bitcoin-compatible JSON-RPC response
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

/// Bitcoin Infinity RPC State
#[derive(Clone)]
pub struct RpcState {
    chain_id: Arc<String>,
    version: Arc<String>,
}

impl RpcState {
    pub fn new(chain_id: String, version: String) -> Self {
        RpcState {
            chain_id: Arc::new(chain_id),
            version: Arc::new(version),
        }
    }
}

/// Main RPC handler - routes Bitcoin JSON-RPC methods to Bitcoin Infinity
async fn rpc_handler(
    state: Arc<RwLock<RpcState>>,
    body: Result<Json<JsonRpcRequest>, JsonRejection>,
) -> (StatusCode, Json<JsonRpcResponse>) {
    let request = match body {
        Ok(Json(req)) => req,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: json!(null),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: "Parse error".to_string(),
                        data: None,
                    }),
                }),
            );
        }
    };

    let state = state.read().await;
    let response = match request.method.as_str() {
        // Blockchain info
        "getblockchaininfo" => handle_getblockchaininfo(&state, &request),
        "getblockcount" => handle_getblockcount(&state, &request),
        "getbestblockhash" => handle_getbestblockhash(&state, &request),
        "getblock" => handle_getblock(&state, &request),
        "getblockhash" => handle_getblockhash(&state, &request),

        // Account info
        "getbalance" => handle_getbalance(&state, &request),
        "getaccount" => handle_getaccount(&state, &request),

        // Address utilities
        "validateaddress" => handle_validateaddress(&state, &request),
        "getnewaddress" => handle_getnewaddress(&state, &request),

        // Network
        "getnetworkinfo" => handle_getnetworkinfo(&state, &request),
        "getconnectioncount" => handle_getconnectioncount(&state, &request),

        // Utilities
        "getinfo" => handle_getinfo(&state, &request),

        // Unimplemented methods return "method not found"
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", request.method),
                data: None,
            }),
        },
    };

    (StatusCode::OK, Json(response))
}

// RPC Method Handlers

fn handle_getblockchaininfo(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!({
            "chain": state.chain_id.as_ref(),
            "blocks": 100,
            "headers": 100,
            "bestblockhash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "difficulty": 1.0,
            "time": chrono::Utc::now().timestamp(),
            "mediantime": chrono::Utc::now().timestamp(),
            "verificationprogress": 1.0,
            "initialblockdownload": false,
            "chainwork": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "size_on_disk": 0,
            "pruned": false,
            "warnings": ""
        })),
        error: None,
    }
}

fn handle_getblockcount(
    _state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!(100)),
        error: None,
    }
}

fn handle_getbestblockhash(
    _state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!("0x0000000000000000000000000000000000000000000000000000000000000000")),
        error: None,
    }
}

fn handle_getblock(
    _state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!({
            "hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "height": 0,
            "time": chrono::Utc::now().timestamp(),
            "tx": [],
            "nTx": 0
        })),
        error: None,
    }
}

fn handle_getblockhash(
    _state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!("0x0000000000000000000000000000000000000000000000000000000000000000")),
        error: None,
    }
}

fn handle_getbalance(
    _state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!(0.0)),
        error: None,
    }
}

fn handle_getaccount(
    _state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!({
            "address": "unknown",
            "balance": 0.0,
            "nonce": 0
        })),
        error: None,
    }
}

fn handle_validateaddress(
    _state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let is_valid = if let Some(params) = request.params.as_array() {
        if let Some(addr) = params.first().and_then(|v| v.as_str()) {
            addr.starts_with('1') || addr.starts_with('3') || addr.starts_with("bc1")
        } else {
            false
        }
    } else {
        false
    };

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!({
            "isvalid": is_valid,
            "address": "",
            "ismine": false,
            "iswatchonly": false,
            "isscript": false
        })),
        error: None,
    }
}

fn handle_getnewaddress(
    _state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")),
        error: None,
    }
}

fn handle_getnetworkinfo(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!({
            "version": state.version.as_ref(),
            "subversion": "/BitcoinInfinity:0.1.0/",
            "protocolversion": 70015,
            "timeoffset": 0,
            "connections": 0,
            "networks": [{
                "name": "ipv4",
                "limited": false,
                "reachable": true,
                "proxy": "",
                "proxy_randomize_credentials": false
            }],
            "reachable_through_ipv6": false,
            "local_addresses": [],
            "warnings": "This is Bitcoin Infinity, a NEAR-based L1 with Bitcoin addresses"
        })),
        error: None,
    }
}

fn handle_getconnectioncount(
    _state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!(0)),
        error: None,
    }
}

fn handle_getinfo(
    state: &RpcState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id.clone(),
        result: Some(json!({
            "version": state.version.as_ref(),
            "protocolversion": 70015,
            "walletversion": 160300,
            "balance": 0.0,
            "blocks": 100,
            "timeoffset": 0,
            "connections": 0,
            "difficulty": 1.0,
            "testnet": true,
            "keypoololdest": 0,
            "keypoolsize": 0,
            "unlocked_until": 0,
            "paytxfee": 0.00001,
            "relayfee": 0.00001,
            "warnings": "Bitcoin Infinity testnet"
        })),
        error: None,
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let state = Arc::new(RwLock::new(RpcState::new(
        "bitinfinity-testnet".to_string(),
        "0.1.0".to_string(),
    )));

    let app = Router::new()
        .route("/", post({
            let state = state.clone();
            |body| rpc_handler(state, body)
        }))
        .fallback(|| async { (StatusCode::NOT_FOUND, "Bitcoin Infinity JSON-RPC Server") });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8332")
        .await
        .expect("Failed to bind to 127.0.0.1:8332");

    println!("Bitcoin Infinity RPC Server");
    println!("===========================");
    println!();
    println!("Listening on: http://127.0.0.1:8332");
    println!("Chain: bitinfinity-testnet");
    println!("Version: 0.1.0");
    println!();
    println!("This server provides Bitcoin-compatible JSON-RPC endpoints.");
    println!("Existing Bitcoin wallets can connect by changing their RPC endpoint.");
    println!();

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
