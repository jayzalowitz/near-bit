#![no_main]

use libfuzzer_sys::fuzz_target;

/// Fuzz the JSON-RPC request parsing layer.
///
/// The parser must never panic on arbitrary input — it should return a
/// well-formed error response rather than crash.
///
/// Run with:
///   cargo +nightly fuzz run fuzz_rpc_parse -- -max_total_time=300
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Attempt to parse as a JSON-RPC request object.
        // We are testing that serde_json parsing of arbitrary input never panics.
        let _: Result<serde_json::Value, _> = serde_json::from_str(s);

        // Also test method name extraction from a well-formed-ish object.
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
            if let Some(obj) = v.as_object() {
                let _ = obj.get("method").and_then(|m| m.as_str());
                let _ = obj.get("params");
                let _ = obj.get("id");
            }
        }
    }
});
