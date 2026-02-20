use crate::{ParseAccountError, ParseErrorKind};
use sha2::{Digest, Sha256};

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
    } else if is_bitcoin_implicit(account_id) {
        // Bitcoin addresses are valid account IDs on Bitcoin Infinity.
        Ok(())
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

/// Legacy compatibility path for older lowercased Base58 account IDs.
///
/// Historically this fork lowercased P2PKH/P2SH addresses to fit strict
/// lowercase NEAR account rules. That representation is non-standard for
/// Bitcoin Base58Check, but we keep accepting it for backward compatibility
/// with existing local/test chain state.
fn is_legacy_lowercased_bitcoin(account_id: &str) -> bool {
    let bytes = account_id.as_bytes();
    let len = account_id.len();

    if account_id.starts_with("bc1q") && (42..=62).contains(&len) {
        return bytes.iter().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'));
    }
    if account_id.starts_with("bc1p") && len == 62 {
        return bytes.iter().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'));
    }
    if bytes.first() == Some(&b'1') && (25..=34).contains(&len) {
        return bytes.iter().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'));
    }
    if bytes.first() == Some(&b'3') && (33..=34).contains(&len) {
        return bytes.iter().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'));
    }

    false
}

fn has_valid_base58check_address(account_id: &str) -> bool {
    if !(26..=35).contains(&account_id.len()) {
        return false;
    }

    let decoded = match bs58::decode(account_id).into_vec() {
        Ok(v) => v,
        Err(_) => return false,
    };
    if decoded.len() != 25 {
        return false;
    }

    let payload = &decoded[..21];
    let checksum = &decoded[21..25];
    let hash1 = Sha256::digest(payload);
    let hash2 = Sha256::digest(hash1);
    if checksum != &hash2[..4] {
        return false;
    }

    matches!(payload[0], 0x00 | 0x05 | 0x6f | 0xc4)
}

fn has_valid_segwit_address(account_id: &str) -> bool {
    match bech32::segwit::decode(account_id) {
        Ok((hrp, _, _)) => {
            hrp == bech32::hrp::BC || hrp == bech32::hrp::TB || hrp == bech32::hrp::BCRT
        }
        Err(_) => false,
    }
}

/// Checks if an account ID is a Bitcoin address.
///
/// Preferred path uses strict Base58Check/SegWit checksum validation for
/// canonical addresses. A legacy lowercase fallback is kept for compatibility
/// with older snapshots.
pub fn is_bitcoin_implicit(account_id: &str) -> bool {
    has_valid_base58check_address(account_id)
        || has_valid_segwit_address(account_id)
        || is_legacy_lowercased_bitcoin(account_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_near_account_ids() {
        let ok = &[
            "aa",
            "a-a",
            "100",
            "near",
            "alice.near",
            "b-o_w_e-n",
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
            "a",
            "A",
            "-near",
            "near-",
            ".near",
            "near.",
            "a..b",
            "abcdefghijklmnopqrstuvwxyz.abcdefghijklmnopqrstuvwxyz.abcdefghijklmnopqrstuvwxyz",
        ];
        for id in bad {
            assert!(validate(id).is_err(), "should be invalid: {}", id);
        }
    }

    #[test]
    fn test_bitcoin_implicit_detection() {
        // Canonical P2PKH (mixed-case Base58Check)
        assert!(is_bitcoin_implicit("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"));

        // Canonical P2SH
        assert!(is_bitcoin_implicit("3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy"));

        // Legacy lowercased compatibility path
        assert!(is_bitcoin_implicit("1a1zp1ep5qgefi2dmptftl5slmv7divfna"));

        // Bech32 P2WPKH (already lowercase)
        assert!(is_bitcoin_implicit(
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
        ));

        // Invalid checksum should be rejected by strict parser
        assert!(!is_bitcoin_implicit("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNb"));

        // Not a Bitcoin address
        assert!(!is_bitcoin_implicit("alice.near"));
        assert!(!is_bitcoin_implicit("near"));

        // Too short for P2PKH
        assert!(!is_bitcoin_implicit("1abc"));
    }

    #[test]
    fn test_validate_accepts_bitcoin_addresses() {
        assert!(validate("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa").is_ok());
        assert!(validate("3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy").is_ok());
        assert!(validate("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4").is_ok());
    }

    #[test]
    fn test_eth_implicit() {
        assert!(is_eth_implicit(
            "0xb794f5ea0ba39494ce839613fffba74279579268"
        ));
        assert!(!is_eth_implicit("alice.near"));
    }

    #[test]
    fn test_near_implicit() {
        assert!(is_near_implicit(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(!is_near_implicit("alice.near"));
    }
}
