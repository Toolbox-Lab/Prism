use crate::error::GratResult;
use crate::xdr::codec::XdrCodec;
use serde::{Deserialize, Serialize};
use stellar_xdr::curr::{
    AccountId, Hash, PublicKey, ScAddress, ScVal, SorobanAddressCredentials,
    SorobanAuthorizationEntry, SorobanAuthorizedFunction, SorobanAuthorizedInvocation,
    SorobanCredentials, Uint256,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationType {
    Ed25519,
    SmartWallet,
}

impl std::fmt::Display for AuthorizationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthorizationType::Ed25519 => write!(f, "Ed25519"),
            AuthorizationType::SmartWallet => write!(f, "Smart Wallet"),
        }
    }
}

/// The credential that authorizes an [`AuthChain`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AuthCredential {
    /// Authorized implicitly by the transaction's source account — no nonce or
    SourceAccount,
    Address(AddressCredential),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressCredential {
    pub address: String,
    pub auth_type: AuthorizationType,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub contract_id: Option<String>,
    pub nonce: i64,
    pub signature_expiration_ledger: u32,
    pub signed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthFunctionKind {
    ContractFn,
    CreateContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthInvocation {
    pub depth: usize,
    pub kind: AuthFunctionKind,
    pub contract: Option<String>,
    pub function: Option<String>,
    pub arg_count: usize,
    pub args: Vec<String>,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthChain {
    pub credential: AuthCredential,
    pub invocations: Vec<AuthInvocation>,
}

impl AuthChain {
    pub fn from_xdr_base64(b64: &str) -> GratResult<Self> {
        let entry = SorobanAuthorizationEntry::from_xdr_base64(b64)?;
        Ok(Self::from_entry(&entry))
    }

    pub fn from_entry(entry: &SorobanAuthorizationEntry) -> Self {
        let credential = parse_credential(&entry.credentials);
        let mut invocations = Vec::new();
        walk_invocation(&entry.root_invocation, 0, &mut invocations);
        Self {
            credential,
            invocations,
        }
    }
}

fn parse_credential(credentials: &SorobanCredentials) -> AuthCredential {
    match credentials {
        SorobanCredentials::SourceAccount => AuthCredential::SourceAccount,
        SorobanCredentials::Address(addr) => {
            AuthCredential::Address(parse_address_credential(addr))
        }
    }
}

fn parse_address_credential(creds: &SorobanAddressCredentials) -> AddressCredential {
    let address = scaddress_to_strkey(&creds.address);
    let (auth_type, contract_id) = match &creds.address {
        ScAddress::Account(_) => (AuthorizationType::Ed25519, None),
        ScAddress::Contract(_) => (AuthorizationType::SmartWallet, Some(address.clone())),
    };
    AddressCredential {
        address,
        auth_type,
        contract_id,
        nonce: creds.nonce,
        signature_expiration_ledger: creds.signature_expiration_ledger,
        signed: creds.signature != ScVal::Void,
    }
}

fn walk_invocation(
    invocation: &SorobanAuthorizedInvocation,
    depth: usize,
    out: &mut Vec<AuthInvocation>,
) {
    out.push(parse_function(&invocation.function, depth));
    for sub in invocation.sub_invocations.iter() {
        walk_invocation(sub, depth + 1, out);
    }
}

fn parse_function(function: &SorobanAuthorizedFunction, depth: usize) -> AuthInvocation {
    match function {
        SorobanAuthorizedFunction::ContractFn(args) => {
            let contract = scaddress_to_strkey(&args.contract_address);
            let function = args.function_name.to_string();
            let readable_args: Vec<String> =
                args.args.iter().map(scval_to_readable_string).collect();
            let target = format!("{}.{}({})", contract, function, readable_args.join(", "));

            AuthInvocation {
                depth,
                kind: AuthFunctionKind::ContractFn,
                contract: Some(contract),
                function: Some(function),
                arg_count: readable_args.len(),
                args: readable_args,
                target,
            }
        }

        SorobanAuthorizedFunction::CreateContractHostFn(_) => AuthInvocation {
            depth,
            kind: AuthFunctionKind::CreateContract,
            contract: None,
            function: None,
            arg_count: 0,
            args: Vec::new(),
            target: "create_contract".to_string(),
        },
    }
}

pub fn scval_to_readable_string(val: &ScVal) -> String {
    match val {
        ScVal::Void => "void".to_string(),
        ScVal::Bool(b) => b.to_string(),
        ScVal::Symbol(sym) => sym.to_string(),
        ScVal::String(s) => format!("\"{}\"", String::from_utf8_lossy(s.as_ref())),
        ScVal::U32(u) => u.to_string(),
        ScVal::I32(i) => i.to_string(),
        ScVal::U64(u) => u.to_string(),
        ScVal::I64(i) => i.to_string(),
        ScVal::Timepoint(t) => t.0.to_string(),
        ScVal::Duration(d) => d.0.to_string(),
        ScVal::U128(u) => ((u128::from(u.hi) << 64) | u128::from(u.lo)).to_string(),
        ScVal::I128(i) => {
            #[allow(clippy::cast_possible_wrap)]
            let lo = u128::from(i.lo) as i128;
            ((i128::from(i.hi) << 64) | lo).to_string()
        }
        ScVal::Address(address) => scaddress_to_strkey(address),
        ScVal::Bytes(bytes) => format!(
            "0x{}",
            bytes.iter().fold(String::new(), |mut output, b| {
                use std::fmt::Write;
                let _ = write!(output, "{b:02x}");
                output
            })
        ),
        ScVal::Vec(Some(vec)) => {
            let items: Vec<String> = vec.iter().map(scval_to_readable_string).collect();
            format!("[{}]", items.join(", "))
        }
        ScVal::Vec(None) => "[]".to_string(),
        ScVal::Map(Some(map)) => {
            let items: Vec<String> = map
                .iter()
                .map(|entry| {
                    format!(
                        "{}: {}",
                        scval_to_readable_string(&entry.key),
                        scval_to_readable_string(&entry.val)
                    )
                })
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        ScVal::Map(None) => "{}".to_string(),
        ScVal::Error(err) => format!("error:{err:?}"),
        other => format!("{other:?}"),
    }
}

pub(crate) fn scaddress_to_strkey(address: &ScAddress) -> String {
    match address {
        ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(bytes)))) => {
            stellar_strkey::ed25519::PublicKey(*bytes).to_string()
        }
        ScAddress::Contract(Hash(bytes)) => stellar_strkey::Contract(*bytes).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{InvokeContractArgs, ScSymbol};

    fn account_address(seed: u8) -> ScAddress {
        ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(
            [seed; 32],
        ))))
    }

    fn contract_address(seed: u8) -> ScAddress {
        ScAddress::Contract(Hash([seed; 32]))
    }

    fn contract_fn(addr: ScAddress, name: &str, args: Vec<ScVal>) -> SorobanAuthorizedFunction {
        SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
            contract_address: addr,
            function_name: ScSymbol(name.try_into().unwrap()),
            args: args.try_into().unwrap(),
        })
    }

    fn invocation(
        function: SorobanAuthorizedFunction,
        subs: Vec<SorobanAuthorizedInvocation>,
    ) -> SorobanAuthorizedInvocation {
        SorobanAuthorizedInvocation {
            function,
            sub_invocations: subs.try_into().unwrap(),
        }
    }

    fn empty_subs() -> Vec<SorobanAuthorizedInvocation> {
        Vec::new()
    }

    #[test]
    fn source_account_credential_is_recognized() {
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: invocation(
                contract_fn(contract_address(7), "transfer", vec![ScVal::U32(1)]),
                empty_subs(),
            ),
        };

        let chain = AuthChain::from_entry(&entry);
        assert_eq!(chain.credential, AuthCredential::SourceAccount);
        assert_eq!(chain.invocations.len(), 1);
        let root = &chain.invocations[0];
        assert_eq!(root.depth, 0);
        assert_eq!(root.kind, AuthFunctionKind::ContractFn);
        assert_eq!(root.function.as_deref(), Some("transfer"));
        assert_eq!(root.arg_count, 1);
        assert_eq!(root.args, vec!["1"]);
        assert!(root.target.contains(".transfer(1)"));
        assert!(root.contract.as_deref().unwrap().starts_with('C'));
    }

    #[test]
    fn address_credential_extracts_nonce_and_signed_state() {
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: account_address(3),
                nonce: 42,
                signature_expiration_ledger: 1000,
                signature: ScVal::Bool(true),
            }),
            root_invocation: invocation(
                contract_fn(contract_address(9), "approve", vec![]),
                empty_subs(),
            ),
        };

        let chain = AuthChain::from_entry(&entry);
        match chain.credential {
            AuthCredential::Address(creds) => {
                assert!(creds.address.starts_with('G'));
                assert_eq!(creds.nonce, 42);
                assert_eq!(creds.signature_expiration_ledger, 1000);
                assert!(creds.signed);
            }
            other @ AuthCredential::SourceAccount => {
                panic!("expected address credential, got {other:?}")
            }
        }
    }

    #[test]
    fn ed25519_account_credential_has_correct_auth_type() {
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: account_address(3),
                nonce: 1,
                signature_expiration_ledger: 100,
                signature: ScVal::Void,
            }),
            root_invocation: invocation(
                contract_fn(contract_address(9), "transfer", vec![]),
                empty_subs(),
            ),
        };

        let chain = AuthChain::from_entry(&entry);
        match chain.credential {
            AuthCredential::Address(creds) => {
                assert_eq!(creds.auth_type, AuthorizationType::Ed25519);
                assert!(creds.address.starts_with('G'));
                assert!(
                    creds.contract_id.is_none(),
                    "Ed25519 should have no contract_id"
                );
                assert_eq!(creds.auth_type.to_string(), "Ed25519");
            }
            other @ AuthCredential::SourceAccount => {
                panic!("expected address credential, got {other:?}")
            }
        }
    }

    #[test]
    fn smart_wallet_contract_credential_has_correct_auth_type() {
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: contract_address(5),
                nonce: 99,
                signature_expiration_ledger: 200,
                signature: ScVal::Void,
            }),
            root_invocation: invocation(
                contract_fn(contract_address(8), "invoke", vec![]),
                empty_subs(),
            ),
        };

        let chain = AuthChain::from_entry(&entry);
        match chain.credential {
            AuthCredential::Address(creds) => {
                assert_eq!(creds.auth_type, AuthorizationType::SmartWallet);
                assert!(creds.address.starts_with('C'));

                let contract_id = creds
                    .contract_id
                    .as_deref()
                    .expect("smart wallet must have contract_id");
                assert_eq!(contract_id, creds.address);
                assert!(contract_id.starts_with('C'));
                assert_eq!(creds.auth_type.to_string(), "Smart Wallet");
            }
            other @ AuthCredential::SourceAccount => {
                panic!("expected address credential, got {other:?}")
            }
        }
    }

    #[test]
    fn smart_wallet_contract_id_matches_address() {
        let seed = 42u8;
        let addr = contract_address(seed);
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: addr.clone(),
                nonce: 0,
                signature_expiration_ledger: 0,
                signature: ScVal::Void,
            }),
            root_invocation: invocation(
                contract_fn(contract_address(1), "fn", vec![]),
                empty_subs(),
            ),
        };

        let chain = AuthChain::from_entry(&entry);
        if let AuthCredential::Address(creds) = chain.credential {
            assert_eq!(creds.contract_id.as_deref(), Some(creds.address.as_str()));
        } else {
            panic!("expected address credential");
        }
    }

    #[test]
    fn authorization_type_display_labels() {
        assert_eq!(AuthorizationType::Ed25519.to_string(), "Ed25519");
        assert_eq!(AuthorizationType::SmartWallet.to_string(), "Smart Wallet");
    }

    #[test]
    fn void_signature_is_reported_as_unsigned() {
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: account_address(1),
                nonce: 0,
                signature_expiration_ledger: 0,
                signature: ScVal::Void,
            }),
            root_invocation: invocation(
                contract_fn(contract_address(2), "f", vec![]),
                empty_subs(),
            ),
        };

        let chain = AuthChain::from_entry(&entry);
        match chain.credential {
            AuthCredential::Address(creds) => assert!(!creds.signed),
            other @ AuthCredential::SourceAccount => {
                panic!("expected address credential, got {other:?}")
            }
        }
    }

    #[test]
    fn nested_invocations_are_flattened_depth_first() {
        let grandchild = invocation(
            contract_fn(contract_address(30), "gc", vec![]),
            empty_subs(),
        );
        let child_a = invocation(
            contract_fn(
                contract_address(20),
                "a",
                vec![ScVal::U32(1), ScVal::U32(2)],
            ),
            vec![grandchild],
        );
        let child_b = invocation(contract_fn(contract_address(21), "b", vec![]), empty_subs());
        let root = invocation(
            contract_fn(contract_address(10), "root", vec![]),
            vec![child_a, child_b],
        );

        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: root,
        };

        let chain = AuthChain::from_entry(&entry);
        let steps: Vec<(usize, &str)> = chain
            .invocations
            .iter()
            .map(|i| (i.depth, i.function.as_deref().unwrap()))
            .collect();

        assert_eq!(steps, vec![(0, "root"), (1, "a"), (2, "gc"), (1, "b")]);

        assert_eq!(chain.invocations[1].arg_count, 2);
        assert_eq!(chain.invocations[1].args, vec!["1", "2"]);
        assert!(chain.invocations[1].target.contains(".a(1, 2)"));
    }

    #[test]
    fn contract_invocation_decodes_readable_argument_values() {
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: invocation(
                contract_fn(
                    contract_address(8),
                    "transfer",
                    vec![
                        ScVal::Address(account_address(4)),
                        ScVal::Bool(true),
                        ScVal::Vec(Some(
                            vec![ScVal::U32(10), ScVal::I32(-2)].try_into().unwrap(),
                        )),
                    ],
                ),
                empty_subs(),
            ),
        };

        let chain = AuthChain::from_entry(&entry);
        let root = &chain.invocations[0];

        assert_eq!(root.function.as_deref(), Some("transfer"));
        assert_eq!(root.arg_count, 3);
        assert!(root.args[0].starts_with('G'));
        assert_eq!(root.args[1], "true");
        assert_eq!(root.args[2], "[10, -2]");
        assert!(root.target.contains(".transfer("));
        assert!(root.target.contains("true"));
        assert!(root.target.contains("[10, -2]"));
    }

    #[test]
    fn round_trips_through_base64() {
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: invocation(
                contract_fn(contract_address(5), "mint", vec![ScVal::U32(7)]),
                empty_subs(),
            ),
        };
        let b64 = XdrCodec::to_xdr_base64(&entry).expect("encode");

        let chain = AuthChain::from_xdr_base64(&b64).expect("parse");
        assert_eq!(chain.credential, AuthCredential::SourceAccount);
        assert_eq!(chain.invocations.len(), 1);
        assert_eq!(chain.invocations[0].function.as_deref(), Some("mint"));
    }

    #[test]
    fn invalid_base64_is_an_error() {
        assert!(AuthChain::from_xdr_base64("!!!not-valid!!!").is_err());
    }
}
