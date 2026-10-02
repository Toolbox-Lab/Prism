use crate::error::GratError;
use std::convert::TryFrom;
use std::fmt;

const CONTRACT_ID_LEN: usize = 56;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContractId(String);

impl ContractId {
    pub fn new(raw: impl Into<String>) -> Result<Self, GratError> {
        let raw = raw.into();
        validate_contract_id(&raw)?;
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContractId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for ContractId {
    type Error = GratError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        ContractId::new(value)
    }
}

impl TryFrom<&str> for ContractId {
    type Error = GratError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        ContractId::new(value)
    }
}

fn validate_contract_id(raw: &str) -> Result<(), GratError> {
    if raw.len() != CONTRACT_ID_LEN {
        return Err(GratError::InvalidContractId(format!(
            "expected {CONTRACT_ID_LEN} characters, got {} (received '{raw}')",
            raw.len()
        )));
    }

    match raw.as_bytes()[0] {
        b'C' => {}
        b'G' => {
            return Err(GratError::InvalidContractId(format!(
                "Must start with 'C', but received 'G' — this looks like an \
                 Account ID, not a Contract ID: '{raw}'"
            )))
        }
        other => {
            return Err(GratError::InvalidContractId(format!(
                "Must start with 'C', but received '{}': '{raw}'",
                other as char
            )))
        }
    }

    stellar_strkey::Contract::from_string(raw).map_err(|e| {
        GratError::InvalidContractId(format!("'{raw}' is not valid StrKey-encoded data: {e}"))
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_contract_id() -> String {
        let bytes = [7u8; 32];
        stellar_strkey::Contract(bytes).to_string()
    }

    #[test]
    fn accepts_valid_contract_id() {
        let raw = valid_contract_id();
        assert!(ContractId::new(raw).is_ok());
    }

    #[test]
    fn rejects_wrong_length() {
        let err = ContractId::new("CABCDEFG").unwrap_err();
        assert!(matches!(err, GratError::InvalidContractId(msg) if msg.contains("56 characters")));
    }

    #[test]
    fn rejects_account_id_prefix() {
        let mut raw = valid_contract_id();
        raw.replace_range(0..1, "G");
        let err = ContractId::new(raw).unwrap_err();
        assert!(matches!(err, GratError::InvalidContractId(msg) if msg.contains("Account ID")));
    }

    #[test]
    fn rejects_bad_checksum() {
        let mut raw = valid_contract_id();
        let last = raw.len() - 1;
        let corrupted_char = if raw.as_bytes()[last] == b'A' {
            'B'
        } else {
            'A'
        };
        raw.replace_range(last..=last, &corrupted_char.to_string());
        assert!(ContractId::new(raw).is_err());
    }

    #[test]
    fn rejects_invalid_base32_char() {
        let mut raw = valid_contract_id();
        raw.replace_range(10..11, "0");
        assert!(ContractId::new(raw).is_err());
    }

    #[test]
    fn try_from_str_works() {
        let raw = valid_contract_id();
        let cid = ContractId::try_from(raw.as_str()).unwrap();
        assert_eq!(cid.as_str(), raw);
    }
}
