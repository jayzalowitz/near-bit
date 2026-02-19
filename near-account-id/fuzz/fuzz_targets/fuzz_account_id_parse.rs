#![no_main]

use libfuzzer_sys::fuzz_target;
use std::str::FromStr;

fuzz_target!(|data: &[u8]| {
    // Try to parse arbitrary bytes as a UTF-8 string, then as an AccountId.
    // The parser must never panic — it should return Ok or Err, never crash.
    if let Ok(s) = std::str::from_utf8(data) {
        // Ignore the result; we're testing that parsing never panics.
        let _ = near_account_id::AccountId::from_str(s);
    }
});
