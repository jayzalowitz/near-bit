//! NEAR Account ID with Bitcoin address support (Bitcoin Infinity fork).
//!
//! This is a fork of `near-account-id` v2.0.0 that adds Bitcoin address detection.
//! Lowercased Bitcoin addresses (P2PKH, P2SH, Bech32) pass standard NEAR validation
//! since they consist of lowercase alphanumeric characters. This fork adds
//! `AccountType::BtcImplicitAccount` to identify them.
//!
//! ## Account ID Rules (same as upstream)
//!
//! - Minimum length is `2`
//! - Maximum length is `64`
//! - Lowercase alphanumeric with `-`, `_`, `.` separators
//! - No leading/trailing/consecutive separators
//!
//! ## Bitcoin Address Support
//!
//! Bitcoin addresses are stored lowercased and detected by pattern:
//! - P2PKH: starts with '1', 25-34 chars
//! - P2SH: starts with '3', 33-34 chars
//! - Bech32 P2WPKH/P2WSH: starts with "bc1q", 42-62 chars
//! - Bech32m P2TR: starts with "bc1p", 62 chars

mod errors;

mod account_id;
mod account_id_ref;
#[cfg(feature = "borsh")]
mod borsh;
#[cfg(feature = "serde")]
mod serde;
mod validation;

pub use account_id::AccountId;
pub use account_id_ref::{AccountIdRef, AccountType};
pub use errors::{ParseAccountError, ParseErrorKind};
