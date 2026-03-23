/// Convert a BTC-denominated decimal amount to satoshis with bounds/precision checks.
/// Returns `None` for non-finite, non-positive, overflow, or sub-satoshi-precision values.
pub(crate) fn btc_to_satoshis_checked(amount_btc: f64) -> Option<u64> {
    if !amount_btc.is_finite() || amount_btc <= 0.0 {
        return None;
    }

    let satoshis_float = amount_btc * 100_000_000.0;
    if !satoshis_float.is_finite() || satoshis_float > u64::MAX as f64 {
        return None;
    }

    let satoshis_rounded = satoshis_float.round();
    if satoshis_rounded <= 0.0 {
        return None;
    }

    // Enforce satoshi precision (<= 8 decimal places in BTC).
    if (satoshis_float - satoshis_rounded).abs() > 1e-6 {
        return None;
    }

    Some(satoshis_rounded as u64)
}

/// Convert a BTC-denominated decimal amount to satoshis with bounds/precision checks.
/// Accepts zero and returns `Some(0)` for exact zero.
/// Returns `None` for non-finite, negative, overflow, or sub-satoshi-precision values.
pub(crate) fn btc_to_satoshis_non_negative_checked(amount_btc: f64) -> Option<u64> {
    if !amount_btc.is_finite() || amount_btc < 0.0 {
        return None;
    }
    if amount_btc == 0.0 {
        return Some(0);
    }
    btc_to_satoshis_checked(amount_btc)
}

/// Sum satoshi-denominated values with overflow checks.
/// Returns `None` on `u64` overflow.
pub(crate) fn checked_sum_satoshis<I>(values: I) -> Option<u64>
where
    I: IntoIterator<Item = u64>,
{
    let mut total = 0u64;
    for value in values {
        total = total.checked_add(value)?;
    }
    Some(total)
}
