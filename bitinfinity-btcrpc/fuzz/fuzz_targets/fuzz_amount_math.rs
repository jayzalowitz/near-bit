#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../src/amounts.rs"]
#[allow(dead_code)]
mod amounts;

fuzz_target!(|data: &[u8]| {
    for chunk in data.chunks_exact(8).take(4096) {
        let mut bits = [0u8; 8];
        bits.copy_from_slice(chunk);
        let amount_btc = f64::from_bits(u64::from_le_bytes(bits));

        if let Some(satoshis) = amounts::btc_to_satoshis_checked(amount_btc) {
            let _ = satoshis.checked_sub(1000);
            let _ = satoshis.checked_mul(10);
        }
    }
});
