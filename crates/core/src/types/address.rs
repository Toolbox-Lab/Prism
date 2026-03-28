//! Address types for Stellar accounts and contracts.

use std::fmt;
use stellar_strkey::{Contract, ed25519::PublicKey};

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
        Self { bytes, address_type }
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