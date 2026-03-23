use crate::AccountIdRef;

use super::AccountId;

use std::io::{Read, Write};

use borsh::{BorshDeserialize, BorshSerialize};

impl BorshSerialize for AccountId {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.0.serialize(writer)
    }
}

impl BorshSerialize for AccountIdRef {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.0.serialize(writer)
    }
}

impl BorshDeserialize for AccountId {
    fn deserialize_reader<R: Read>(rd: &mut R) -> std::io::Result<Self> {
        let account_id = Box::<str>::deserialize_reader(rd)?;
        crate::validation::validate(&account_id).map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid value: \"{}\", {}", account_id, err),
            )
        })?;
        Ok(Self(account_id))
    }
}
