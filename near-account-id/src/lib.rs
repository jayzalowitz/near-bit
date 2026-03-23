#![allow(
    clippy::doc_lazy_continuation,
    clippy::len_without_is_empty,
    clippy::needless_lifetimes,
    clippy::sliced_string_as_bytes,
    clippy::unnecessary_map_or
)]

//! NEAR Account ID with Bitcoin address support (Bitcoin Infinity fork).
//!
//! This is a fork of `near-account-id` v2.0.0 that adds Bitcoin address detection.
//! Canonical Bitcoin addresses (Base58Check + Bech32/Bech32m) are accepted as
//! account IDs, and this fork adds `AccountType::BtcImplicitAccount` to identify them.
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
//! Bitcoin addresses are detected via strict parser validation:
//! - Base58Check P2PKH/P2SH (canonical casing preserved)
//! - Bech32/Bech32m SegWit/Taproot
//! A legacy lowercased Base58 compatibility path is retained for older snapshots.

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
