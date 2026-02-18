use std::borrow::Cow;

use crate::{AccountId, ParseAccountError};

/// Account identifier reference type. This is to [`AccountId`] what [`str`] is to [`String`].
#[derive(Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct AccountIdRef(pub(crate) str);

/// Enum representing possible types of accounts.
#[derive(PartialEq)]
pub enum AccountType {
    NamedAccount,
    NearImplicitAccount,
    EthImplicitAccount,
    NearDeterministicAccount,
    BtcImplicitAccount,
}

impl AccountType {
    pub fn is_implicit(&self) -> bool {
        match &self {
            Self::NearImplicitAccount => true,
            Self::EthImplicitAccount => true,
            Self::NearDeterministicAccount => true,
            Self::BtcImplicitAccount => true,
            Self::NamedAccount => false,
        }
    }
}

impl AccountIdRef {
    pub const MIN_LEN: usize = crate::validation::MIN_LEN;
    pub const MAX_LEN: usize = crate::validation::MAX_LEN;

    pub fn new<S: AsRef<str> + ?Sized>(id: &S) -> Result<&Self, ParseAccountError> {
        let id = id.as_ref();
        crate::validation::validate(id)?;
        Ok(unsafe { &*(id as *const str as *const Self) })
    }

    pub const fn new_or_panic(id: &str) -> &Self {
        crate::validation::validate_const(id);
        unsafe { &*(id as *const str as *const Self) }
    }

    pub(crate) fn new_unvalidated<S: AsRef<str> + ?Sized>(id: &S) -> &Self {
        let id = id.as_ref();
        #[cfg(not(feature = "internal_unstable"))]
        debug_assert!(crate::validation::validate(id).is_ok());
        unsafe { &*(id as *const str as *const Self) }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_top_level(&self) -> bool {
        !self.is_system() && !self.0.contains('.')
    }

    pub fn is_sub_account_of(&self, parent: &AccountIdRef) -> bool {
        self.0
            .strip_suffix(parent.as_str())
            .and_then(|s| s.strip_suffix('.'))
            .map_or(false, |s| !s.contains('.'))
    }

    pub fn get_account_type(&self) -> AccountType {
        if crate::validation::is_bitcoin_implicit(self.as_str()) {
            return AccountType::BtcImplicitAccount;
        }
        if crate::validation::is_eth_implicit(self.as_str()) {
            return AccountType::EthImplicitAccount;
        }
        if crate::validation::is_near_implicit(self.as_str()) {
            return AccountType::NearImplicitAccount;
        }
        if crate::validation::is_near_deterministic(self.as_str()) {
            return AccountType::NearDeterministicAccount;
        }
        AccountType::NamedAccount
    }

    pub fn is_system(&self) -> bool {
        self == "system"
    }

    pub const fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get_parent_account_id(&self) -> Option<&AccountIdRef> {
        let parent_str = self.as_str().split_once('.')?.1;
        Some(AccountIdRef::new_unvalidated(parent_str))
    }
}

impl std::fmt::Display for AccountIdRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl ToOwned for AccountIdRef {
    type Owned = AccountId;

    fn to_owned(&self) -> Self::Owned {
        AccountId(self.0.into())
    }
}

impl<'a> From<&'a AccountIdRef> for AccountId {
    fn from(id: &'a AccountIdRef) -> Self {
        id.to_owned()
    }
}

impl<'s> TryFrom<&'s str> for &'s AccountIdRef {
    type Error = ParseAccountError;

    fn try_from(value: &'s str) -> Result<Self, Self::Error> {
        AccountIdRef::new(value)
    }
}

impl AsRef<str> for AccountIdRef {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<AccountIdRef> for String {
    fn eq(&self, other: &AccountIdRef) -> bool {
        self == &other.0
    }
}

impl PartialEq<String> for AccountIdRef {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

impl PartialEq<AccountIdRef> for str {
    fn eq(&self, other: &AccountIdRef) -> bool {
        self == &other.0
    }
}

impl PartialEq<str> for AccountIdRef {
    fn eq(&self, other: &str) -> bool {
        &self.0 == other
    }
}

impl<'a> PartialEq<AccountIdRef> for &'a str {
    fn eq(&self, other: &AccountIdRef) -> bool {
        *self == &other.0
    }
}

impl<'a> PartialEq<&'a str> for AccountIdRef {
    fn eq(&self, other: &&'a str) -> bool {
        &self.0 == *other
    }
}

impl<'a> PartialEq<&'a AccountIdRef> for str {
    fn eq(&self, other: &&'a AccountIdRef) -> bool {
        self == &other.0
    }
}

impl<'a> PartialEq<str> for &'a AccountIdRef {
    fn eq(&self, other: &str) -> bool {
        &self.0 == other
    }
}

impl<'a> PartialEq<&'a AccountIdRef> for String {
    fn eq(&self, other: &&'a AccountIdRef) -> bool {
        self == &other.0
    }
}

impl<'a> PartialEq<String> for &'a AccountIdRef {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

impl PartialOrd<AccountIdRef> for String {
    fn partial_cmp(&self, other: &AccountIdRef) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(&other.0)
    }
}

impl PartialOrd<String> for AccountIdRef {
    fn partial_cmp(&self, other: &String) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other.as_str())
    }
}

impl PartialOrd<AccountIdRef> for str {
    fn partial_cmp(&self, other: &AccountIdRef) -> Option<std::cmp::Ordering> {
        self.partial_cmp(other.as_str())
    }
}

impl PartialOrd<str> for AccountIdRef {
    fn partial_cmp(&self, other: &str) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(other)
    }
}

impl<'a> PartialOrd<AccountIdRef> for &'a str {
    fn partial_cmp(&self, other: &AccountIdRef) -> Option<std::cmp::Ordering> {
        self.partial_cmp(&other.as_str())
    }
}

impl<'a> PartialOrd<&'a str> for AccountIdRef {
    fn partial_cmp(&self, other: &&'a str) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(*other)
    }
}

impl<'a> PartialOrd<&'a AccountIdRef> for String {
    fn partial_cmp(&self, other: &&'a AccountIdRef) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(&other.0)
    }
}

impl<'a> PartialOrd<String> for &'a AccountIdRef {
    fn partial_cmp(&self, other: &String) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other.as_str())
    }
}

impl<'a> PartialOrd<&'a AccountIdRef> for str {
    fn partial_cmp(&self, other: &&'a AccountIdRef) -> Option<std::cmp::Ordering> {
        self.partial_cmp(other.as_str())
    }
}

impl<'a> PartialOrd<str> for &'a AccountIdRef {
    fn partial_cmp(&self, other: &str) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(other)
    }
}

impl<'a> From<&'a AccountIdRef> for Cow<'a, AccountIdRef> {
    fn from(value: &'a AccountIdRef) -> Self {
        Cow::Borrowed(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_btc_implicit_account_type() {
        // Lowercased P2PKH
        let id: AccountId = "1a1zp1ep5qgefi2dmptftl5slmv7divfna".parse().unwrap();
        assert!(id.get_account_type() == AccountType::BtcImplicitAccount);
        assert!(id.get_account_type().is_implicit());

        // Bech32 P2WPKH
        let id: AccountId = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4".parse().unwrap();
        assert!(id.get_account_type() == AccountType::BtcImplicitAccount);
    }

    #[test]
    fn test_named_account_type() {
        let id: AccountId = "alice.near".parse().unwrap();
        assert!(id.get_account_type() == AccountType::NamedAccount);
        assert!(!id.get_account_type().is_implicit());
    }

    #[test]
    fn test_near_implicit_account_type() {
        let id: AccountId = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".parse().unwrap();
        assert!(id.get_account_type() == AccountType::NearImplicitAccount);
    }

    #[test]
    fn test_eth_implicit_account_type() {
        let id: AccountId = "0xb794f5ea0ba39494ce839613fffba74279579268".parse().unwrap();
        assert!(id.get_account_type() == AccountType::EthImplicitAccount);
    }
}
