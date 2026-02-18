//! Bitcoin Infinity token (BIT) denomination
//! 1 BIT = 10^24 yoctobit (same internal precision as NEAR's 10^24 yoctoNEAR)
//! 1 BTC satoshi = 10^16 yoctobit

pub const ONE_BIT: u128 = 10_u128.pow(24);
pub const ONE_SATOSHI_IN_YOCTOBIT: u128 = 10_u128.pow(16);

pub fn satoshis_to_yoctosyd(satoshis: u64) -> u128 {
    satoshis as u128 * ONE_SATOSHI_IN_YOCTOBIT
}

pub fn yoctosyd_to_satoshis(yoctosyd: u128) -> Option<u64> {
    let satoshis = yoctosyd / ONE_SATOSHI_IN_YOCTOBIT;
    if satoshis > u64::MAX as u128 {
        None
    } else {
        Some(satoshis as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversion() {
        // 1 BTC = 100M satoshis = 10^17 yoctosyd
        let satoshis = 100_000_000_u64;
        let yoctosyd = satoshis_to_yoctosyd(satoshis);
        assert_eq!(yoctosyd, 100_000_000 * ONE_SATOSHI_IN_YOCTOBIT);
    }
}
