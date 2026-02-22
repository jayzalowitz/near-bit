#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz hex-encoded transaction parsing.
//
// Verifies that decoding arbitrary hex strings as Bitcoin transactions
// never panics.
//
// Run with:
//   cargo +nightly fuzz run fuzz_tx_hex -- -max_total_time=300
fuzz_target!(|data: &[u8]| {
    // Test that bitcoin crate tx deserialization from arbitrary bytes doesn't panic.
    use bitcoin::consensus::Decodable;
    let _ = bitcoin::Transaction::consensus_decode(&mut std::io::Cursor::new(data));

    if let Ok(s) = std::str::from_utf8(data) {
        let trimmed = s.trim();
        let trimmed_bytes = trimmed.as_bytes();

        // Test hex decode of arbitrary input.
        let _ = hex::decode(trimmed_bytes);

        // Exercise odd-length and truncated variants.
        let odd = if trimmed_bytes.len().is_multiple_of(2) && !trimmed_bytes.is_empty() {
            &trimmed_bytes[..trimmed_bytes.len() - 1]
        } else {
            trimmed_bytes
        };
        let _ = hex::decode(odd);

        // If it decodes, try consensus-decode from hex payload as tx bytes.
        if let Ok(bytes) = hex::decode(trimmed_bytes) {
            let _ = bitcoin::Transaction::consensus_decode(&mut std::io::Cursor::new(&bytes));
            let half = bytes.len() / 2;
            let _ =
                bitcoin::Transaction::consensus_decode(&mut std::io::Cursor::new(&bytes[..half]));
        }
    }
});
