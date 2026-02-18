use crate::AccountIdRef;

use super::AccountId;

use serde::{de, ser};

impl ser::Serialize for AccountId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl ser::Serialize for AccountIdRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for AccountId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let account_id = Box::<str>::deserialize(deserializer)?;
        crate::validation::validate(&account_id).map_err(|err| {
            de::Error::custom(format!("invalid value: \"{}\", {}", account_id, err))
        })?;
        Ok(AccountId(account_id))
    }
}

impl<'de> de::Deserialize<'de> for &'de AccountIdRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        <&str as de::Deserialize>::deserialize(deserializer)
            .and_then(|s| Self::try_from(s).map_err(de::Error::custom))
    }
}
