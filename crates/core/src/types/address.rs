//! Address types for Stellar accounts and contracts.

use std::fmt;
use stellar_strkey::{ed25519::PublicKey, Contract, Strkey};

use crate::types::error::{PrismError, PrismResult};

/// Represents a Stellar address (account or contract).
///
/// Internally stores the raw bytes, but displays as the standard strkey format.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    /// The raw bytes of the address.
    pub bytes: Vec<u8>,
    /// The type of address (for strkey encoding).
    pub address_type: AddressType,
}

/// The type of Stellar address.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AddressType {
    /// Account public key (starts with 'G').
    Account,
    /// Contract ID (starts with 'C').
    Contract,
}

impl Address {
    /// Create a new Address from raw bytes.
    pub fn new(bytes: Vec<u8>, address_type: AddressType) -> Self {
        Self {
            bytes,
            address_type,
        }
    }

    /// Create an Address from a strkey string.
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
            Err(format!("Invalid strkey: {}", strkey))
        }
    }

    /// Instantiate an Address from raw user input with immediate validation.
    ///
    /// This method uses `stellar_strkey::Strkey` to ensure the input is a valid
    /// account public key or contract ID.
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

    /// Convert to strkey string.
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

    #[test]
    fn test_address_from_string_valid_account() {
        let s = "GA5W327P3O6PDH4YCWYAW5M36DREB6ZRYNRFCTO7C6XU66OJSXR6X6R6";
        let res = Address::from_string(s);
        assert!(res.is_ok());
        let addr = res.unwrap();
        assert_eq!(addr.address_type, AddressType::Account);
        assert_eq!(addr.to_strkey(), s);
    }

    #[test]
    fn test_address_from_string_valid_contract() {
        let s = "CA3D5YIF3S62FBE6SYO6ALSTCJR3Y3Y0V7SYCLTULV2S0S0S0S0S0S0S";
        let res = Address::from_string(s);
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
        // Seed (starts with S)
        let s = "SA5W327P3O6PDH4YCWYAW5M36DREB6ZRYNRFCTO7C6XU66OJSUSE4NNI";
        let res = Address::from_string(s);
        assert!(res.is_err());
        match res {
            Err(PrismError::InvalidAddress(msg)) => {
                assert!(msg.contains("Unsupported address type"));
            }
            _ => panic!("Expected InvalidAddress error for unsupported type"),
        }
    }
}
