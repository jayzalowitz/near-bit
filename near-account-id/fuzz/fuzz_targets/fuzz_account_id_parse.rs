#![no_main]

use libfuzzer_sys::fuzz_target;
use std::str::FromStr;

fn exercise_candidate(candidate: &str) {
    let _ = near_account_id::AccountId::validate(candidate);
    let parsed = near_account_id::AccountId::from_str(candidate);
    let _ = near_account_id::AccountIdRef::new(&candidate);

    if let Ok(account_id) = parsed {
        let account_type = account_id.get_account_type();
        let _ = account_type.is_implicit();
        let _ = account_id.as_str();
    }
}

fuzz_target!(|data: &[u8]| {
    // Keep candidate size bounded so mutational growth does not explode memory.
    let mut candidate = String::from_utf8_lossy(data).into_owned();
    if candidate.len() > 4096 {
        candidate.truncate(4096);
    }

    exercise_candidate(&candidate);
    exercise_candidate(candidate.trim());

    // Exercise casing/compatibility paths.
    let lower = candidate.to_ascii_lowercase();
    let upper = candidate.to_ascii_uppercase();
    exercise_candidate(&lower);
    exercise_candidate(&upper);

    // Exercise null-byte and separator edge cases explicitly.
    let mut with_nul = candidate.clone();
    with_nul.push('\0');
    exercise_candidate(&with_nul);

    let mut with_redundant_separators = candidate.clone();
    with_redundant_separators.push_str("..__--");
    exercise_candidate(&with_redundant_separators);

    // Exercise very long inputs (length cap in validator is 64) without allocating unboundedly.
    if !candidate.is_empty() {
        let repeat_count = ((data.first().copied().unwrap_or(0) as usize) % 8) + 1;
        let mut long = String::with_capacity(candidate.len().saturating_mul(repeat_count));
        for _ in 0..repeat_count {
            long.push_str(&candidate);
            if long.len() >= 4096 {
                break;
            }
        }
        if long.len() > 4096 {
            long.truncate(4096);
        }
        exercise_candidate(&long);
    }
});
