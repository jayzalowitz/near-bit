#![no_main]

use libfuzzer_sys::fuzz_target;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: serde_json::Value,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

fn get_str_param<'a>(params: &'a serde_json::Value, index: usize) -> Option<&'a str> {
    params
        .as_array()
        .and_then(|arr| arr.get(index))
        .and_then(|v| v.as_str())
}

fn get_u64_param(params: &serde_json::Value, index: usize) -> Option<u64> {
    params
        .as_array()
        .and_then(|arr| arr.get(index))
        .and_then(|v| v.as_u64())
}

fn exercise_request(req: &JsonRpcRequest) {
    let _ = req.method.as_str();
    let _ = req.id.is_null();
    let _ = serde_json::to_string(req);

    for i in 0..8 {
        let _ = get_str_param(&req.params, i);
        let _ = get_u64_param(&req.params, i);
    }

    if let Some(params_obj) = req.params.as_object() {
        let _ = params_obj.get("address").and_then(|v| v.as_str());
        let _ = params_obj.get("txid").and_then(|v| v.as_str());
        let _ = params_obj.get("verbose").and_then(|v| v.as_bool());
        let _ = params_obj.get("amount").and_then(|v| v.as_f64());
    }
}

// Fuzz the JSON-RPC request parsing layer.
//
// The parser must never panic on arbitrary input.
//
// Run with:
//   cargo +nightly fuzz run fuzz_rpc_parse -- -max_total_time=300
fuzz_target!(|data: &[u8]| {
    // Keep parse attempts bounded for pathological giant inputs.
    if data.len() > 64 * 1024 {
        return;
    }

    let _: Result<serde_json::Value, _> = serde_json::from_slice(data);
    let _: Result<JsonRpcRequest, _> = serde_json::from_slice::<JsonRpcRequest>(data).map(|req| {
        exercise_request(&req);
        req
    });
    let _: Result<Vec<JsonRpcRequest>, _> = serde_json::from_slice::<Vec<JsonRpcRequest>>(data)
        .map(|batch| {
            for req in batch.iter().take(32) {
                exercise_request(req);
            }
            batch
        });
});
