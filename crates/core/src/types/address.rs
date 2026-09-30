use serde::{Deserialize, Serialize};
use std::fmt;
use stellar_strkey::{ed25519::PublicKey, Contract, Strkey};

use crate::error::{PrismError, PrismResult};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address {
    pub bytes: Vec<u8>,

    pub address_type: AddressType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AddressType {
    Account,

    Contract,
}

impl Address {
    pub fn new(bytes: Vec<u8>, address_type: AddressType) -> Self {
        Self {
            bytes,
            address_type,
        }
    }

    pub fn from_strkey(strkey: &str) -> Result<Self, String> {
        if let Ok(contract) = Contract::from_string(strkey) {
            Ok(Self {
                bytes: contract.0.to_vec(),
                address_type: AddressType::Contract,
            })
        } else if let Ok(account) = PublicKey::from_string(strkey) {
            Ok(Self {
                bytes: account.0.to_vec(),
                address_type: AddressType::Account,
            })
        } else {
            Err(format!("Invalid strkey: {strkey}"))
        }
    }

    pub fn from_string(s: &str) -> PrismResult<Self> {
        let strkey = Strkey::from_string(s)
            .map_err(|e| PrismError::InvalidAddress(format!("Failed to parse strkey: {e}")))?;

        match strkey {
            Strkey::PublicKeyEd25519(pk) => Ok(Self {
                bytes: pk.0.to_vec(),
                address_type: AddressType::Account,
            }),
            Strkey::Contract(c) => Ok(Self {
                bytes: c.0.to_vec(),
                address_type: AddressType::Contract,
            }),
            _ => Err(PrismError::InvalidAddress(format!(
                "Unsupported address type: {s}"
            ))),
        }
    }

    pub fn validate_contract_id(contract_id: &str) -> PrismResult<()> {
        if !contract_id.starts_with('C') {
            return Err(PrismError::InvalidAddress(
                "Contract ID must start with 'C'".to_string(),
            ));
        }

        Contract::from_string(contract_id).map_err(|e| {
            PrismError::InvalidAddress(format!("Invalid contract ID '{contract_id}': {e}"))
        })?;

        Ok(())
    }

    pub fn from_contract_id(contract_id: &str) -> PrismResult<Self> {
        Self::validate_contract_id(contract_id)?;
        let contract = Contract::from_string(contract_id).map_err(|e| {
            PrismError::InvalidAddress(format!("Invalid contract ID '{contract_id}': {e}"))
        })?;

        Ok(Self {
            bytes: contract.0.to_vec(),
            address_type: AddressType::Contract,
        })
    }

    pub fn to_strkey(&self) -> String {
        match self.address_type {
            AddressType::Account => {
                let pk = PublicKey(self.bytes.clone().try_into().unwrap());
                pk.to_string()
            }
            AddressType::Contract => {
                let contract = Contract(self.bytes.clone().try_into().unwrap());
                contract.to_string()
            }
        }
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_strkey())
    }
}

impl From<Address> for String {
    fn from(addr: Address) -> String {
        addr.to_strkey()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_strkey::ed25519::PrivateKey;

    fn valid_account_strkey() -> String {
        PublicKey([1; 32]).to_string()
    }

    fn valid_contract_strkey() -> String {
        Contract([2; 32]).to_string()
    }

    fn valid_private_key_strkey() -> String {
        PrivateKey([3; 32]).to_string()
    }

    #[test]
    fn test_address_from_string_valid_account() {
        let s = valid_account_strkey();
        let res = Address::from_string(&s);
        assert!(res.is_ok());
        let addr = res.unwrap();
        assert_eq!(addr.address_type, AddressType::Account);
        assert_eq!(addr.to_strkey(), s);
    }

    #[test]
    fn test_address_from_string_valid_contract() {
        let s = valid_contract_strkey();
        let res = Address::from_string(&s);
        assert!(res.is_ok());
        let addr = res.unwrap();
        assert_eq!(addr.address_type, AddressType::Contract);
        assert_eq!(addr.to_strkey(), s);
    }

    #[test]
    fn test_address_from_string_invalid() {
        let s = "invalid";
        let res = Address::from_string(s);
        assert!(res.is_err());
        if let Err(PrismError::InvalidAddress(msg)) = res {
            assert!(msg.contains("Failed to parse strkey"));
        } else {
            panic!("Expected InvalidAddress error");
        }
    }

    #[test]
    fn test_address_from_string_unsupported() {
        let s = valid_private_key_strkey();
        let res = Address::from_string(&s);
        assert!(res.is_err());
        match res {
            Err(PrismError::InvalidAddress(msg)) => {
                assert!(msg.contains("Unsupported address type"));
            }
            _ => panic!("Expected InvalidAddress error for unsupported type"),
        }
    }

    #[test]
    fn test_address_from_string_corrupted_checksum() {
        let mut s = valid_account_strkey();
        let last = s.pop().unwrap();
        s.push(if last == 'A' { 'B' } else { 'A' });

        let res = Address::from_string(&s);
        assert!(res.is_err());
        match res {
            Err(PrismError::InvalidAddress(msg)) => {
                assert!(msg.contains("Failed to parse strkey"));
            }
            _ => panic!("Expected InvalidAddress error for corrupted checksum"),
        }
    }

    #[test]
    fn test_validate_contract_id_valid() {
        let s = valid_contract_strkey();
        let res = Address::validate_contract_id(&s);
        assert!(res.is_ok());
    }

    #[test]
    fn test_validate_contract_id_wrong_prefix() {
        let s = valid_account_strkey();
        let res = Address::validate_contract_id(&s);
        assert!(res.is_err());
        match res {
            Err(PrismError::InvalidAddress(msg)) => {
                assert!(msg.contains("must start with 'C'"));
            }
            _ => panic!("Expected InvalidAddress error for wrong prefix"),
        }
    }

    #[test]
    fn test_validate_contract_id_malformed() {
        let mut s = valid_contract_strkey();
        let last = s.pop().unwrap();
        s.push(if last == 'A' { 'B' } else { 'A' });

        let res = Address::validate_contract_id(&s);
        assert!(res.is_err());
        match res {
            Err(PrismError::InvalidAddress(msg)) => {
                assert!(msg.contains("Invalid contract ID"));
            }
            _ => panic!("Expected InvalidAddress error for malformed contract id"),
        }
    }

    #[test]
    fn test_from_contract_id_valid() {
        let s = valid_contract_strkey();
        let res = Address::from_contract_id(&s);
        assert!(res.is_ok());
        let addr = res.unwrap();
        assert_eq!(addr.address_type, AddressType::Contract);
        assert_eq!(addr.to_strkey(), s);
    }
}
