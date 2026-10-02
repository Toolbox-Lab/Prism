use serde::{Deserialize, Serialize};
use std::fmt;
use stellar_xdr::curr::{SorobanAddressCredentials, SorobanAuthorizationEntry, SorobanCredentials};

use crate::decode::auth::scaddress_to_strkey;
use crate::error::GratResult;
use crate::xdr::codec::XdrCodec;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressWithNonce {
    pub address: String,
    pub nonce: i64,
}

impl AddressWithNonce {
    pub fn from_auth_entry_base64(b64: &str) -> GratResult<Option<Self>> {
        let entry = SorobanAuthorizationEntry::from_xdr_base64(b64)?;
        Ok(Self::from_entry(&entry))
    }

    pub fn from_entry(entry: &SorobanAuthorizationEntry) -> Option<Self> {
        match &entry.credentials {
            SorobanCredentials::SourceAccount => None,
            SorobanCredentials::Address(creds) => Some(Self::from_credentials(creds)),
        }
    }

    fn from_credentials(creds: &SorobanAddressCredentials) -> Self {
        Self {
            address: scaddress_to_strkey(&creds.address),
            nonce: creds.nonce,
        }
    }
}

impl fmt::Display for AddressWithNonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Address: {}", self.address)?;
        write!(f, "Nonce:   {}", self.nonce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        AccountId, Hash, InvokeContractArgs, PublicKey, ScAddress, ScSymbol, ScVal,
        SorobanAuthorizedFunction, SorobanAuthorizedInvocation, Uint256,
    };

    fn account_address(seed: u8) -> ScAddress {
        ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(
            [seed; 32],
        ))))
    }

    fn contract_address(seed: u8) -> ScAddress {
        ScAddress::Contract(Hash([seed; 32]))
    }

    fn root_invocation(addr: ScAddress) -> SorobanAuthorizedInvocation {
        SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                contract_address: addr,
                function_name: ScSymbol("f".try_into().unwrap()),
                args: Vec::new().try_into().unwrap(),
            }),
            sub_invocations: Vec::new().try_into().unwrap(),
        }
    }

    fn address_entry(addr: ScAddress, nonce: i64) -> SorobanAuthorizationEntry {
        SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: addr.clone(),
                nonce,
                signature_expiration_ledger: 0,
                signature: ScVal::Void,
            }),
            root_invocation: root_invocation(addr),
        }
    }

    #[test]
    fn extracts_account_address_and_nonce() {
        let entry = address_entry(account_address(3), 42);
        let parsed = AddressWithNonce::from_entry(&entry).expect("address credential");
        assert!(parsed.address.starts_with('G'));
        assert_eq!(parsed.nonce, 42);
    }

    #[test]
    fn extracts_contract_address_and_nonce() {
        let entry = address_entry(contract_address(9), -7);
        let parsed = AddressWithNonce::from_entry(&entry).expect("address credential");
        assert!(parsed.address.starts_with('C'));
        assert_eq!(parsed.nonce, -7);
    }

    #[test]
    fn source_account_has_no_address_with_nonce() {
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: root_invocation(contract_address(1)),
        };
        assert!(AddressWithNonce::from_entry(&entry).is_none());
    }

    #[test]
    fn display_separates_address_and_nonce() {
        let entry = address_entry(account_address(5), 1234);
        let parsed = AddressWithNonce::from_entry(&entry).expect("address credential");
        let rendered = parsed.to_string();
        assert!(rendered.contains(&format!("Address: {}", parsed.address)));
        assert!(rendered.contains("Nonce:   1234"));

        assert_eq!(rendered.lines().count(), 2);
    }

    #[test]
    fn round_trips_through_base64() {
        let entry = address_entry(account_address(7), 99);
        let b64 = XdrCodec::to_xdr_base64(&entry).expect("encode");
        let parsed = AddressWithNonce::from_auth_entry_base64(&b64)
            .expect("decode")
            .expect("address credential");
        assert!(parsed.address.starts_with('G'));
        assert_eq!(parsed.nonce, 99);
    }

    #[test]
    fn source_account_base64_yields_none() {
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: root_invocation(contract_address(2)),
        };
        let b64 = XdrCodec::to_xdr_base64(&entry).expect("encode");
        let parsed = AddressWithNonce::from_auth_entry_base64(&b64).expect("decode");
        assert!(parsed.is_none());
    }

    #[test]
    fn invalid_base64_is_an_error() {
        assert!(AddressWithNonce::from_auth_entry_base64("!!!not-valid!!!").is_err());
    }
}
