#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../src/tx_translator.rs"]
#[allow(dead_code)]
mod tx_translator;

// Fuzz Bitcoin raw-tx translation into Bitcoin Infinity transfer metadata.
//
// This exercises address extraction and output parsing paths used by
// `sendrawtransaction` translation flow.
//
// Run with:
//   cargo +nightly fuzz run fuzz_tx_translator -- -max_total_time=300
fuzz_target!(|data: &[u8]| {
    if data.len() > 128 * 1024 {
        return;
    }

    // 1) Directly fuzz with bytes rendered as hex tx.
    let bytes_hex = hex::encode(data);
    if let Ok(parsed) = tx_translator::ParsedBitcoinTx::from_hex(&bytes_hex) {
        let _ = parsed.payment_output();
        let total = parsed.total_payment_satoshis();
        let _ = tx_translator::ParsedBitcoinTx::satoshis_to_yocto(total);
        let _ = parsed.op_return_data();
        let _ = parsed.decode_near_function_call();
    }

    // 2) Fuzz "string-like" inputs filtered to hex chars for malformed/partial hex paths.
    let filtered: String = String::from_utf8_lossy(data)
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(16 * 1024)
        .collect();

    let mut even_hex = filtered;
    if even_hex.len() % 2 == 1 {
        even_hex.push('0');
    }

    for hrp in ["bc", "tb", "bcrt"] {
        if let Ok(parsed) = tx_translator::ParsedBitcoinTx::from_hex_with_hrp(&even_hex, hrp) {
            let _ = parsed.payment_output();
            let total = parsed.total_payment_satoshis();
            let _ = tx_translator::ParsedBitcoinTx::satoshis_to_yocto(total);
            let _ = parsed.op_return_data();
            let _ = parsed.decode_near_function_call();
        }
    }
});
