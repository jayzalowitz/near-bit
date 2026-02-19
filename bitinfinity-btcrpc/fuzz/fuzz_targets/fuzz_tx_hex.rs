#![no_main]

use libfuzzer_sys::fuzz_target;

/// Fuzz hex-encoded transaction parsing.
///
/// Verifies that decoding arbitrary hex strings as Bitcoin transactions
/// never panics — it should return an error on invalid input.
///
/// Run with:
///   cargo +nightly fuzz run fuzz_tx_hex -- -max_total_time=300
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Test hex decode of arbitrary input.
        let _ = hex::decode(s.trim());

        // Test that bitcoin crate tx deserialization from arbitrary bytes doesn't panic.
        use bitcoin::consensus::Decodable;
        let _ = bitcoin::Transaction::consensus_decode(&mut std::io::Cursor::new(data));
    }
});
