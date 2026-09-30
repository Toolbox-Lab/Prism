use serde::Serialize;
use stellar_xdr::curr::{
    InvokeHostFunctionResult, OperationResult, OperationResultTr, TransactionResult,
    TransactionResultResult,
};

use crate::error::{PrismError, PrismResult};
use crate::taxonomy::schema::ErrorCategory;
use crate::xdr::codec::XdrCodec;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum HostError {
    Budget {
        code: u32,
    },
    Storage {
        code: u32,
    },
    Auth {
        code: u32,
    },
    Context {
        code: u32,
    },
    Value {
        code: u32,
    },
    Object {
        code: u32,
    },
    Crypto {
        code: u32,
    },
    Contract {
        code: u32,
    },
    Wasm {
        code: u32,
    },
    Events {
        code: u32,
    },

    ContractSpecific {
        contract_id: Option<String>,
        code: u32,
    },

    Unknown {
        type_code: u32,
        sub_code: u32,
    },
}

impl HostError {
    pub fn category_name(&self) -> &str {
        match self {
            Self::Budget { .. } => "Budget",
            Self::Storage { .. } => "Storage",
            Self::Auth { .. } => "Auth",
            Self::Context { .. } => "Context",
            Self::Value { .. } => "Value",
            Self::Object { .. } => "Object",
            Self::Crypto { .. } => "Crypto",
            Self::Contract { .. } => "Contract",
            Self::Wasm { .. } => "Wasm",
            Self::Events { .. } => "Events",
            Self::ContractSpecific { .. } => "ContractSpecific",
            Self::Unknown { .. } => "Unknown",
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::Budget { code } => {
                if let Some(detail) = crate::decode::mappings::budget::lookup(*code) {
                    detail.summary.to_string()
                } else {
                    format!("Budget error (code {code}): the transaction exceeded an allocated resource budget.")
                }
            },
            Self::Storage { code } => {
                if let Some(detail) = crate::decode::mappings::storage::lookup(*code) {
                    detail.summary.to_string()
                } else {
                    format!("Storage error (code {code}): an unexpected error occurred while accessing contract data.")
                }
            },
            Self::Auth { code } => {
                if let Some(detail) = crate::decode::mappings::auth::lookup(*code) {
                    detail.summary.to_string()
                } else {
                    format!("Auth error (code {code}): an authorization requirement was not satisfied.")
                }
            },
            Self::Context { code } => {
                if let Some(detail) = crate::decode::mappings::context::lookup(*code) {
                    detail.summary.to_string()
                } else {
                    format!("Context error (code {code}): the contract was invoked in an invalid execution context.")
                }
            },
            Self::Value { code } => {
                if let Some(detail) = crate::decode::mappings::value::lookup(*code) {
                    detail.summary.to_string()
                } else {
                    format!("Value error (code {code}): a host value could not be converted or validated.")
                }
            },
            Self::Object { code } => {
                if let Some(detail) = crate::decode::mappings::object::lookup(*code) {
                    detail.summary.to_string()
                } else {
                    format!("Object error (code {code}): an operation on a host object (vector, map, bytes) failed.")
                }
            },
            Self::Crypto { code } => match code {
                0 => "Invalid cryptographic input: a public key, signature, or hash input has the wrong length or format.".to_string(),
                _ => format!("Crypto error (code {code}): a cryptographic operation failed due to invalid input."),
            },
            Self::Contract { code } => match code {
               0 => "Contract error: the contract's own logic rejected this call — run with --resolve to map the code to its name.".to_string(),
                _ => format!("Contract error (code {code}): the contract returned a non-zero error code — run with --resolve to identify it."),
            },
            Self::Wasm { code } => match code {
               0 => "Invalid WASM module: the contract bytecode failed validation — recompile with a compatible Soroban SDK version.".to_string(),
                _ => format!("WASM error (code {code}): the contract's WASM module could not be loaded or executed."),
            },
            Self::Events { code } => match code {
                0 => "Event size limit exceeded: the transaction emitted more event data than the protocol allows in a single execution.".to_string(),
                _ => format!("Events error (code {code}): an error occurred during event emission."),
            },
            Self::ContractSpecific { contract_id, code } => {
                let contract = contract_id
                    .as_deref()
                    .unwrap_or("unknown contract");
                format!(
                    "Contract-specific error {code} from {contract}: run with --resolve to look up the error name from the contract's WASM metadata."
                )
            }
            Self::Unknown { type_code, sub_code } => {
                format!(
                    "Unknown error (type {type_code}, sub-code {sub_code}): this error code is not recognised — the network may be running a newer protocol version."
                )
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClassifiedError {
    pub category: ErrorCategory,
    pub error_code: u32,
    pub is_contract_error: bool,
    pub contract_id: Option<String>,
    pub raw_data: serde_json::Value,
}

pub fn from_transaction_result(tx_result: TransactionResult) -> PrismResult<ClassifiedError> {
    let op_results = match tx_result.result {
        TransactionResultResult::TxSuccess(_) => return Err(PrismError::TransactionSucceeded),
        TransactionResultResult::TxFailed(ops) => ops,
        TransactionResultResult::TxFeeBumpInnerSuccess(_) => {
            return Err(PrismError::TransactionSucceeded)
        }

        _ => return Err(PrismError::NotSorobanTransaction),
    };

    let ihf_result = op_results
        .iter()
        .find_map(|op| {
            if let OperationResult::OpInner(OperationResultTr::InvokeHostFunction(r)) = op {
                Some(r.clone())
            } else {
                None
            }
        })
        .ok_or(PrismError::NotSorobanTransaction)?;

    let (category, error_code, is_contract_error) = match ihf_result {
        InvokeHostFunctionResult::Success(_) => return Err(PrismError::TransactionSucceeded),
        InvokeHostFunctionResult::Trapped => (ErrorCategory::Contract, 0u32, false),
        InvokeHostFunctionResult::ResourceLimitExceeded => (ErrorCategory::Budget, 0, false),
        InvokeHostFunctionResult::EntryArchived => (ErrorCategory::Storage, 0, false),
        InvokeHostFunctionResult::Malformed
        | InvokeHostFunctionResult::InsufficientRefundableFee => (ErrorCategory::Context, 0, false),
    };

    Ok(ClassifiedError {
        category,
        error_code,
        is_contract_error,
        contract_id: None,
        raw_data: serde_json::Value::Null,
    })
}

pub fn classify_error(tx_data: &serde_json::Value) -> PrismResult<ClassifiedError> {
    let status = tx_data
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("UNKNOWN");

    if status == "SUCCESS" {
        return Err(PrismError::TransactionSucceeded);
    }

    if let Some(result_xdr_b64) = tx_data.get("resultXdr").and_then(|r| r.as_str()) {
        if let Ok(tx_result) = TransactionResult::from_xdr_base64(result_xdr_b64) {
            let mut classified = from_transaction_result(tx_result)?;
            classified.raw_data = tx_data.clone();
            return Ok(classified);
        }
    }

    Ok(ClassifiedError {
        category: ErrorCategory::Contract,
        error_code: 0,
        is_contract_error: false,
        contract_id: None,
        raw_data: tx_data.clone(),
    })
}

pub fn parse_error_category(category_str: &str) -> Option<ErrorCategory> {
    match category_str.to_lowercase().as_str() {
        "budget" => Some(ErrorCategory::Budget),
        "storage" => Some(ErrorCategory::Storage),
        "auth" => Some(ErrorCategory::Auth),
        "context" => Some(ErrorCategory::Context),
        "value" => Some(ErrorCategory::Value),
        "object" => Some(ErrorCategory::Object),
        "crypto" => Some(ErrorCategory::Crypto),
        "contract" => Some(ErrorCategory::Contract),
        "wasm" => Some(ErrorCategory::Wasm),
        "events" => Some(ErrorCategory::Events),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        Hash, InvokeHostFunctionResult, OperationResult, OperationResultTr, TransactionResult,
        TransactionResultResult, VecM,
    };

    fn make_tx_result(op_result: InvokeHostFunctionResult) -> TransactionResult {
        TransactionResult {
            fee_charged: 100,
            result: TransactionResultResult::TxFailed(
                vec![OperationResult::OpInner(
                    OperationResultTr::InvokeHostFunction(op_result),
                )]
                .try_into()
                .unwrap(),
            ),
            ext: stellar_xdr::curr::TransactionResultExt::V0,
        }
    }

    #[test]
    fn test_category_name() {
        assert_eq!(HostError::Budget { code: 1 }.category_name(), "Budget");
        assert_eq!(HostError::Storage { code: 2 }.category_name(), "Storage");
        assert_eq!(HostError::Auth { code: 3 }.category_name(), "Auth");
        assert_eq!(HostError::Context { code: 0 }.category_name(), "Context");
        assert_eq!(HostError::Value { code: 0 }.category_name(), "Value");
        assert_eq!(HostError::Object { code: 0 }.category_name(), "Object");
        assert_eq!(HostError::Crypto { code: 0 }.category_name(), "Crypto");
        assert_eq!(HostError::Contract { code: 0 }.category_name(), "Contract");
        assert_eq!(HostError::Wasm { code: 0 }.category_name(), "Wasm");
        assert_eq!(HostError::Events { code: 0 }.category_name(), "Events");
        assert_eq!(
            HostError::ContractSpecific {
                contract_id: None,
                code: 42
            }
            .category_name(),
            "ContractSpecific"
        );
        assert_eq!(
            HostError::Unknown {
                type_code: 99,
                sub_code: 1
            }
            .category_name(),
            "Unknown"
        );
    }

    #[test]
    fn test_serialize_to_json() {
        let err = HostError::ContractSpecific {
            contract_id: Some("CABC123".to_string()),
            code: 3,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"category\":\"contract_specific\""));
        assert!(json.contains("\"code\":3"));
        assert!(json.contains("CABC123"));
    }

    #[test]
    fn test_unknown_variant() {
        let err = HostError::Unknown {
            type_code: 7,
            sub_code: 255,
        };
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["category"], "unknown");
        assert_eq!(json["type_code"], 7);
        assert_eq!(json["sub_code"], 255);
    }

    #[test]
    fn test_parse_error_category() {
        assert_eq!(parse_error_category("budget"), Some(ErrorCategory::Budget));
        assert_eq!(
            parse_error_category("STORAGE"),
            Some(ErrorCategory::Storage)
        );
        assert_eq!(parse_error_category("unknown_xyz"), None);
    }

    #[test]
    fn test_summary_known_codes() {
        assert_eq!(
            HostError::Budget { code: 0 }.summary(),
            "CPU budget exceeded: the transaction ran out of CPU instructions before completing execution."
        );
        assert_eq!(
            HostError::Storage { code: 0 }.summary(),
            "The contract attempted to access a ledger entry not included in the transaction's footprint."
        );
        assert_eq!(
            HostError::Auth { code: 0 }.summary(),
            "The authorization context is malformed or does not match the current invocation."
        );
        assert_eq!(
            HostError::Context { code: 0 }.summary(),
            "Host internal error: an unexpected Soroban runtime error occurred — this may be a platform bug, not a contract bug."
        );
        assert_eq!(
            HostError::Value { code: 0 }.summary(),
            "Invalid value: a host function received an argument of the wrong type or format."
        );
        assert_eq!(
            HostError::Object { code: 0 }.summary(),
            "An unknown or unclassified host object error occurred."
        );
        assert_eq!(
            HostError::Object { code: 5 }.summary(),
            "An index out of bounds was used when accessing a host vector or byte array."
        );
        assert_eq!(
            HostError::Crypto { code: 0 }.summary(),
            "Invalid cryptographic input: a public key, signature, or hash input has the wrong length or format."
        );
        assert_eq!(
            HostError::Contract { code: 0 }.summary(),
            "Contract error: the contract's own logic rejected this call — run with --resolve to map the code to its name."
        );
        assert_eq!(
            HostError::Wasm { code: 0 }.summary(),
            "Invalid WASM module: the contract bytecode failed validation — recompile with a compatible Soroban SDK version."
        );
        assert_eq!(
            HostError::Events { code: 0 }.summary(),
            "Event size limit exceeded: the transaction emitted more event data than the protocol allows in a single execution."
        );
    }

    #[test]
    fn test_summary_contract_specific_with_id() {
        let s = HostError::ContractSpecific {
            contract_id: Some("CABC123".to_string()),
            code: 3,
        }
        .summary();
        assert!(s.contains("CABC123"));
        assert!(s.contains("3"));
        assert!(s.contains("--resolve"));
    }

    #[test]
    fn test_summary_contract_specific_no_id() {
        let s = HostError::ContractSpecific {
            contract_id: None,
            code: 7,
        }
        .summary();
        assert!(s.contains("unknown contract"));
        assert!(s.contains("--resolve"));
    }

    #[test]
    fn test_summary_unknown_variant() {
        let s = HostError::Unknown {
            type_code: 9,
            sub_code: 42,
        }
        .summary();
        assert!(s.contains("9"));
        assert!(s.contains("42"));
        assert!(s.contains("not recognised"));
    }

    #[test]
    fn test_summary_unknown_codes_fallback() {
        let s = HostError::Budget { code: 99 }.summary();
        assert!(s.contains("99"));
        assert!(s.contains("Budget") || s.contains("budget"));
    }

    #[test]
    fn test_summary_under_120_chars() {
        let errors = vec![
            HostError::Budget { code: 0 },
            HostError::Storage { code: 0 },
            HostError::Auth { code: 0 },
            HostError::Context { code: 0 },
            HostError::Value { code: 0 },
            HostError::Object { code: 0 },
            HostError::Crypto { code: 0 },
            HostError::Contract { code: 0 },
            HostError::Wasm { code: 0 },
            HostError::Events { code: 0 },
        ];
        for err in errors {
            let summary = err.summary();
            assert!(
                summary.len() <= 120,
                "Summary too long ({} chars) for {:?}: {}",
                summary.len(),
                err,
                summary
            );
        }
    }

    #[test]
    fn test_from_transaction_result_trapped() {
        let result = make_tx_result(InvokeHostFunctionResult::Trapped);
        let classified = from_transaction_result(result).unwrap();
        assert_eq!(classified.category, ErrorCategory::Contract);
        assert!(!classified.is_contract_error);
    }

    #[test]
    fn test_from_transaction_result_resource_limit() {
        let result = make_tx_result(InvokeHostFunctionResult::ResourceLimitExceeded);
        let classified = from_transaction_result(result).unwrap();
        assert_eq!(classified.category, ErrorCategory::Budget);
    }

    #[test]
    fn test_from_transaction_result_entry_archived() {
        let result = make_tx_result(InvokeHostFunctionResult::EntryArchived);
        let classified = from_transaction_result(result).unwrap();
        assert_eq!(classified.category, ErrorCategory::Storage);
    }

    #[test]
    fn test_from_transaction_result_success_returns_error() {
        let tx_result = TransactionResult {
            fee_charged: 100,
            result: TransactionResultResult::TxSuccess(vec![].try_into().unwrap()),
            ext: stellar_xdr::curr::TransactionResultExt::V0,
        };
        assert!(matches!(
            from_transaction_result(tx_result),
            Err(PrismError::TransactionSucceeded)
        ));
    }

    #[test]
    fn test_from_transaction_result_no_ihf_returns_error() {
        let tx_result = TransactionResult {
            fee_charged: 100,
            result: TransactionResultResult::TxFailed(vec![].try_into().unwrap()),
            ext: stellar_xdr::curr::TransactionResultExt::V0,
        };
        assert!(matches!(
            from_transaction_result(tx_result),
            Err(PrismError::NotSorobanTransaction)
        ));
    }

    #[test]
    fn test_from_transaction_result_ihf_success_returns_error() {
        let result = make_tx_result(InvokeHostFunctionResult::Success(Hash([0; 32])));
        assert!(matches!(
            from_transaction_result(result),
            Err(PrismError::TransactionSucceeded)
        ));
    }
}
