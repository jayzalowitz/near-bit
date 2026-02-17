//! NEAR-compatible account ID type with Bitcoin address support
//! 
//! Account types:
//! - Named: standard NEAR-style accounts (e.g., "account.near")
//! - Bitcoin P2PKH: Bitcoin legacy addresses (e.g., "1A1z...")
//! - Bitcoin P2SH: Bitcoin multisig addresses (e.g., "3...")
//! - Bitcoin Bech32: Bitcoin SegWit addresses (e.g., "bc1q...")

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccountId(String);

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for AccountId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // TODO: Validate Bitcoin addresses and NEAR account IDs
        if s.is_empty() {
            return Err("Account ID cannot be empty".to_string());
        }
        Ok(AccountId(s.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountType {
    NamedAccount,
    NearImplicitAccount,
    BtcImplicitAccount,
}

pub fn get_account_type(account_id: &str) -> AccountType {
    // TODO: Implement Bitcoin address detection
    // - P2PKH: starts with '1', 25-34 chars, valid Base58Check
    // - P2SH: starts with '3', 34 chars, valid Base58Check
    // - Bech32: starts with 'bc1q' or 'bc1p', 42-62 chars
    AccountType::NamedAccount
}

pub fn validate_bitcoin_address(address: &str) -> bool {
    // TODO: Implement full Bitcoin address validation
    address.len() > 0
}
