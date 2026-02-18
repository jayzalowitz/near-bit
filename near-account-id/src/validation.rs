use crate::{ParseAccountError, ParseErrorKind};

/// Shortest valid length for a NEAR Account ID.
pub const MIN_LEN: usize = 2;
/// Longest valid length for a NEAR Account ID.
pub const MAX_LEN: usize = 64;

pub const fn validate_const(account_id: &str) {
    const fn validate_format_const(id: &[u8], idx: usize, current_char_is_separator: bool) {
        if idx >= id.len() {
            if current_char_is_separator {
                panic!("NEAR Account ID cannot end with char separator (-, _, .)");
            }
            return;
        }

        match id[idx] {
            b'a'..=b'z' | b'0'..=b'9' => validate_format_const(id, idx + 1, false),
            b'-' | b'_' | b'.' => {
                if current_char_is_separator {
                    panic!("NEAR Account ID cannot contain redundant separator (-, _, .)")
                } else if idx == 0 {
                    panic!("NEAR Account ID cannot start with char separator (-, _, .)")
                } else {
                    validate_format_const(id, idx + 1, true)
                }
            }
            _ => panic!(
                "NEAR Account ID cannot contain invalid chars (only a-z, 0-9, -, _, and . are allowed)"
            ),
        }
    }

    if account_id.len() < MIN_LEN {
        panic!("NEAR Account ID is too short")
    } else if account_id.len() > MAX_LEN {
        panic!("NEAR Account ID is too long")
    }

    validate_format_const(account_id.as_bytes(), 0, false);
}

pub fn validate(account_id: &str) -> Result<(), ParseAccountError> {
    if account_id.len() < MIN_LEN {
        Err(ParseAccountError {
            kind: ParseErrorKind::TooShort,
            char: None,
        })
    } else if account_id.len() > MAX_LEN {
        Err(ParseAccountError {
            kind: ParseErrorKind::TooLong,
            char: None,
        })
    } else {
        // NOTE: We don't want to use Regex here, because it requires extra time to compile it.
        // The valid account ID regex is /^(([a-z\d]+[-_])*[a-z\d]+\.)*([a-z\d]+[-_])*[a-z\d]+$/
        // Instead the implementation is based on the previous character checks.

        // We can safely assume that last char was a separator.
        let mut last_char_is_separator = true;

        let mut this = None;
        for (i, c) in account_id.chars().enumerate() {
            this.replace((i, c));
            let current_char_is_separator = match c {
                'a'..='z' | '0'..='9' => false,
                '-' | '_' | '.' => true,
                _ => {
                    return Err(ParseAccountError {
                        kind: ParseErrorKind::InvalidChar,
                        char: this,
                    });
                }
            };
            if current_char_is_separator && last_char_is_separator {
                return Err(ParseAccountError {
                    kind: ParseErrorKind::RedundantSeparator,
                    char: this,
                });
            }
            last_char_is_separator = current_char_is_separator;
        }

        if last_char_is_separator {
            return Err(ParseAccountError {
                kind: ParseErrorKind::RedundantSeparator,
                char: this,
            });
        }
        Ok(())
    }
}

pub fn is_eth_implicit(account_id: &str) -> bool {
    account_id.len() == 42
        && account_id.starts_with("0x")
        && account_id[2..]
            .as_bytes()
            .iter()
            .all(|b| matches!(b, b'a'..=b'f' | b'0'..=b'9'))
}

pub fn is_near_deterministic(account_id: &str) -> bool {
    account_id.len() == 42
        && account_id.starts_with("0s")
        && account_id[2..]
            .as_bytes()
            .iter()
            .all(|b| matches!(b, b'a'..=b'f' | b'0'..=b'9'))
}

pub fn is_near_implicit(account_id: &str) -> bool {
    account_id.len() == 64
        && account_id
            .as_bytes()
            .iter()
            .all(|b| matches!(b, b'a'..=b'f' | b'0'..=b'9'))
}

/// Checks if an account ID looks like a lowercased Bitcoin address.
///
/// Bitcoin addresses stored in BitInfinity are lowercased since NEAR AccountId
/// only allows lowercase. We detect them by pattern:
/// - P2PKH: starts with '1', 25-34 chars, all lowercase alphanumeric
/// - P2SH: starts with '3', 33-34 chars, all lowercase alphanumeric
/// - Bech32 P2WPKH/P2WSH: starts with "bc1q", 42-62 chars
/// - Bech32m P2TR (Taproot): starts with "bc1p", 62 chars
pub fn is_bitcoin_implicit(account_id: &str) -> bool {
    let bytes = account_id.as_bytes();
    let len = account_id.len();

    // Bech32 SegWit (P2WPKH 42 chars, P2WSH 62 chars)
    if account_id.starts_with("bc1q") && len >= 42 && len <= 62 {
        return bytes
            .iter()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'));
    }

    // Bech32m Taproot (P2TR, 62 chars)
    if account_id.starts_with("bc1p") && len >= 62 && len <= 62 {
        return bytes
            .iter()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'));
    }

    // P2PKH (lowercased, starts with '1', 25-34 chars)
    if bytes.first() == Some(&b'1') && len >= 25 && len <= 34 {
        return bytes
            .iter()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'));
    }

    // P2SH (lowercased, starts with '3', 33-34 chars)
    if bytes.first() == Some(&b'3') && len >= 33 && len <= 34 {
        return bytes
            .iter()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'));
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_near_account_ids() {
        let ok = &[
            "aa", "a-a", "100", "near", "alice.near", "b-o_w_e-n",
            "0xb794f5ea0ba39494ce839613fffba74279579268",
            "0123456789012345678901234567890123456789012345678901234567890123",
        ];
        for id in ok {
            assert!(validate(id).is_ok(), "should be valid: {}", id);
        }
    }

    #[test]
    fn test_invalid_near_account_ids() {
        let bad = &[
            "a", "A", "-near", "near-", ".near", "near.", "a..b",
            "abcdefghijklmnopqrstuvwxyz.abcdefghijklmnopqrstuvwxyz.abcdefghijklmnopqrstuvwxyz",
        ];
        for id in bad {
            assert!(validate(id).is_err(), "should be invalid: {}", id);
        }
    }

    #[test]
    fn test_bitcoin_implicit_detection() {
        // Lowercased P2PKH (Satoshi's address lowered)
        assert!(is_bitcoin_implicit("1a1zp1ep5qgefi2dmptftl5slmv7divfna"));

        // Lowercased P2SH
        assert!(is_bitcoin_implicit("3j98t1wpez73cnmqviecrnyiwrnqrhwnly"));

        // Bech32 P2WPKH (already lowercase)
        assert!(is_bitcoin_implicit("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"));

        // Not a Bitcoin address
        assert!(!is_bitcoin_implicit("alice.near"));
        assert!(!is_bitcoin_implicit("near"));

        // Too short for P2PKH
        assert!(!is_bitcoin_implicit("1abc"));
    }

    #[test]
    fn test_eth_implicit() {
        assert!(is_eth_implicit("0xb794f5ea0ba39494ce839613fffba74279579268"));
        assert!(!is_eth_implicit("alice.near"));
    }

    #[test]
    fn test_near_implicit() {
        assert!(is_near_implicit("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"));
        assert!(!is_near_implicit("alice.near"));
    }
}
