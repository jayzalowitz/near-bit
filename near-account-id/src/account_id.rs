use std::{borrow::Cow, fmt, ops::Deref, str::FromStr};

use crate::{AccountIdRef, ParseAccountError};

/// NEAR Account Identifier.
///
/// This is a unique, syntactically valid, human-readable account identifier on the NEAR network.
#[derive(Eq, Ord, Hash, Clone, Debug, PartialEq, PartialOrd)]
pub struct AccountId(pub(crate) Box<str>);

impl AccountId {
    pub const MIN_LEN: usize = crate::validation::MIN_LEN;
    pub const MAX_LEN: usize = crate::validation::MAX_LEN;

    #[doc(hidden)]
    #[cfg(feature = "internal_unstable")]
    #[deprecated = "AccountId construction without validation is illegal since nearcore#4440"]
    pub fn new_unvalidated(account_id: String) -> Self {
        Self(account_id.into_boxed_str())
    }

    pub fn validate(account_id: &str) -> Result<(), ParseAccountError> {
        crate::validation::validate(account_id)
    }
}

impl AsRef<str> for AccountId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<AccountIdRef> for AccountId {
    fn as_ref(&self) -> &AccountIdRef {
        self
    }
}

impl Deref for AccountId {
    type Target = AccountIdRef;

    fn deref(&self) -> &Self::Target {
        AccountIdRef::new_unvalidated(&self.0)
    }
}

impl std::borrow::Borrow<AccountIdRef> for AccountId {
    fn borrow(&self) -> &AccountIdRef {
        AccountIdRef::new_unvalidated(self)
    }
}

impl FromStr for AccountId {
    type Err = ParseAccountError;

    fn from_str(account_id: &str) -> Result<Self, Self::Err> {
        crate::validation::validate(account_id)?;
        Ok(Self(account_id.into()))
    }
}

impl TryFrom<Box<str>> for AccountId {
    type Error = ParseAccountError;

    fn try_from(account_id: Box<str>) -> Result<Self, Self::Error> {
        crate::validation::validate(&account_id)?;
        Ok(Self(account_id))
    }
}

impl TryFrom<String> for AccountId {
    type Error = ParseAccountError;

    fn try_from(account_id: String) -> Result<Self, Self::Error> {
        crate::validation::validate(&account_id)?;
        Ok(Self(account_id.into_boxed_str()))
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<AccountId> for String {
    fn from(account_id: AccountId) -> Self {
        account_id.0.into_string()
    }
}

impl From<AccountId> for Box<str> {
    fn from(value: AccountId) -> Box<str> {
        value.0
    }
}

impl PartialEq<AccountId> for AccountIdRef {
    fn eq(&self, other: &AccountId) -> bool {
        &self.0 == other.as_str()
    }
}

impl PartialEq<AccountIdRef> for AccountId {
    fn eq(&self, other: &AccountIdRef) -> bool {
        self.as_str() == &other.0
    }
}

impl<'a> PartialEq<AccountId> for &'a AccountIdRef {
    fn eq(&self, other: &AccountId) -> bool {
        &self.0 == other.as_str()
    }
}

impl<'a> PartialEq<&'a AccountIdRef> for AccountId {
    fn eq(&self, other: &&'a AccountIdRef) -> bool {
        self.as_str() == &other.0
    }
}

impl PartialEq<AccountId> for String {
    fn eq(&self, other: &AccountId) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<String> for AccountId {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<AccountId> for str {
    fn eq(&self, other: &AccountId) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<str> for AccountId {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<'a> PartialEq<AccountId> for &'a str {
    fn eq(&self, other: &AccountId) -> bool {
        *self == other.as_str()
    }
}

impl<'a> PartialEq<&'a str> for AccountId {
    fn eq(&self, other: &&'a str) -> bool {
        self.as_str() == *other
    }
}

impl PartialOrd<AccountId> for AccountIdRef {
    fn partial_cmp(&self, other: &AccountId) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other.as_str())
    }
}

impl PartialOrd<AccountIdRef> for AccountId {
    fn partial_cmp(&self, other: &AccountIdRef) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(&other.0)
    }
}

impl<'a> PartialOrd<AccountId> for &'a AccountIdRef {
    fn partial_cmp(&self, other: &AccountId) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other.as_str())
    }
}

impl<'a> PartialOrd<&'a AccountIdRef> for AccountId {
    fn partial_cmp(&self, other: &&'a AccountIdRef) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(&other.0)
    }
}

impl PartialOrd<AccountId> for String {
    fn partial_cmp(&self, other: &AccountId) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(other.as_str())
    }
}

impl PartialOrd<String> for AccountId {
    fn partial_cmp(&self, other: &String) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(other.as_str())
    }
}

impl PartialOrd<AccountId> for str {
    fn partial_cmp(&self, other: &AccountId) -> Option<std::cmp::Ordering> {
        self.partial_cmp(other.as_str())
    }
}

impl PartialOrd<str> for AccountId {
    fn partial_cmp(&self, other: &str) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(other)
    }
}

impl<'a> PartialOrd<AccountId> for &'a str {
    fn partial_cmp(&self, other: &AccountId) -> Option<std::cmp::Ordering> {
        self.partial_cmp(&other.as_str())
    }
}

impl<'a> PartialOrd<&'a str> for AccountId {
    fn partial_cmp(&self, other: &&'a str) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(*other)
    }
}

impl<'a> From<AccountId> for Cow<'a, AccountIdRef> {
    fn from(value: AccountId) -> Self {
        Cow::Owned(value)
    }
}

impl<'a> From<&'a AccountId> for Cow<'a, AccountIdRef> {
    fn from(value: &'a AccountId) -> Self {
        Cow::Borrowed(value)
    }
}

impl<'a> From<Cow<'a, AccountIdRef>> for AccountId {
    fn from(value: Cow<'a, AccountIdRef>) -> Self {
        value.into_owned()
    }
}
