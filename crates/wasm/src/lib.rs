use grat_core::decode::host_error::ClassifiedError;
use grat_core::decode::report::build_report;
use grat_core::spec::decoder::{decode_contract_spec, resolve_error_code, ContractErrorEntry};
use grat_core::taxonomy::schema::ErrorCategory;
use grat_core::types::report::DiagnosticReport;
use grat_core::xdr::codec::XdrCodec;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn init() {}

#[wasm_bindgen]
pub fn decode_error(tx_result_json: &str) -> Result<JsValue, JsValue> {
    let report = decode_report_inner(tx_result_json).map_err(|e| JsValue::from_str(&e))?;
    serde_wasm_bindgen::to_value(&report).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[allow(clippy::too_many_lines)]
fn decode_report_inner(tx_result_json: &str) -> Result<DiagnosticReport, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(tx_result_json).map_err(|e| format!("invalid JSON input: {e}"))?;

    let error_info = classify(&parsed)?;

    let mut report = build_report(&error_info).map_err(|e| format!("{e}"))?;

    if let Some(events_b64) = parsed.get("diagnosticEventsXdr").and_then(|e| e.as_array()) {
        let mut events = Vec::new();
        for ev_b64 in events_b64 {
            if let Some(s) = ev_b64.as_str() {
                if let Ok(ev) = stellar_xdr::curr::DiagnosticEvent::from_xdr_base64(s) {
                    events.push(ev);
                }
            }
        }
        report.failing_contract_id =
            grat_core::decode::walker::DiagnosticEventWalker::find_failing_contract(&events);
    }

    if let Some(root_causes) = parsed.get("rootCauses").and_then(|v| v.as_array()) {
        for cause in root_causes {
            let desc = cause
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let likelihood = cause
                .get("likelihood")
                .and_then(|l| l.as_str())
                .unwrap_or("medium")
                .to_string();
            if !desc.is_empty() {
                report
                    .root_causes
                    .push(grat_core::types::report::RootCause {
                        description: desc,
                        likelihood,
                    });
            }
        }
    }

    if let Some(fixes) = parsed.get("suggestedFixes").and_then(|v| v.as_array()) {
        for fix in fixes {
            let desc = fix
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let difficulty = fix
                .get("difficulty")
                .and_then(|d| d.as_str())
                .unwrap_or("medium")
                .to_string();
            let requires_upgrade = fix
                .get("requiresUpgrade")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let example = fix
                .get("example")
                .and_then(|e| e.as_str())
                .map(String::from);
            let id = fix
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("unknown")
                .to_string();
            let remedy_code = fix
                .get("remedyCode")
                .and_then(|r| r.as_str())
                .map(String::from);
            if !desc.is_empty() {
                report
                    .suggested_fixes
                    .push(grat_core::types::report::SuggestedFix {
                        description: desc,
                        difficulty,
                        requires_upgrade,
                        example,
                        id,
                        remedy_code,
                    });
            }
        }
    }

    if let Some(tx_hash) = parsed
        .get("txHash")
        .and_then(|h| h.as_str())
        .or_else(|| parsed.get("hash").and_then(|h| h.as_str()))
    {
        if let Some(ref mut ctx) = report.transaction_context {
            ctx.tx_hash = tx_hash.to_string();
        } else {
            report.transaction_context = Some(grat_core::types::report::TransactionContext {
                tx_hash: tx_hash.to_string(),
                ledger_sequence: parsed
                    .get("ledgerSequence")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0) as u32,
                operation_count: None,
                operation_index: None,
                function_name: parsed
                    .get("functionName")
                    .and_then(|f| f.as_str())
                    .map(String::from),
                arguments: Vec::new(),
                return_value: parsed
                    .get("returnValue")
                    .and_then(|r| r.as_str())
                    .map(String::from),
                fee: grat_core::types::report::FeeBreakdown {
                    total_charged_fee: 0,
                    inclusion_fee: 0,
                    resource_fee: 0,
                    refundable_resource_fee: 0,
                    refundable_fee: 0,
                    non_refundable_fee: 0,
                    bid_fee: None,
                },
                resources: grat_core::types::report::ResourceSummary {
                    cpu_instructions_used: 0,
                    cpu_instructions_limit: 0,
                    memory_bytes_used: 0,
                    memory_bytes_limit: 0,
                    read_bytes: 0,
                    read_bytes_limit: 0,
                    write_bytes: 0,
                },
            });
        }
    }

    Ok(report)
}

fn classify(parsed: &serde_json::Value) -> Result<ClassifiedError, String> {
    let error_type = parsed
        .get("errorType")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let error_code = parsed
        .get("errorCode")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0) as u32;
    let raw_data = parsed
        .get("rawData")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let is_contract_error = error_type.eq_ignore_ascii_case("ContractError")
        || parsed
            .get("isContractError")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

    let contract_id = parsed
        .get("contractId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    Ok(ClassifiedError {
        category: match error_type.to_lowercase().as_str() {
            "budget" => ErrorCategory::Budget,
            "storage" => ErrorCategory::Storage,
            "auth" | "authorization" => ErrorCategory::Auth,
            "context" => ErrorCategory::Context,
            "value" => ErrorCategory::Value,
            "object" => ErrorCategory::Object,
            "crypto" => ErrorCategory::Crypto,
            "wasm" => ErrorCategory::Wasm,
            "events" => ErrorCategory::Events,
            _ => ErrorCategory::Contract,
        },
        error_code,
        is_contract_error,
        contract_id,
        raw_data,
    })
}

/// Structured representation of a resolved contract error returned to WebAssembly / JavaScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedContractError {
    /// Qualified error name, e.g. "MyError::OperationFailed".
    pub name: String,
    /// Numerical error code.
    pub code: u32,
    /// Docstring documentation if defined in the contract spec.
    pub doc: Option<String>,
    /// Unqualified variant name, e.g. "OperationFailed".
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub variant: Option<String>,
    /// Alias for doc comment.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub docstring: Option<String>,
}

impl From<&ContractErrorEntry> for ResolvedContractError {
    fn from(entry: &ContractErrorEntry) -> Self {
        let variant = entry
            .name
            .split("::")
            .last()
            .unwrap_or(&entry.name)
            .to_string();
        Self {
            name: entry.name.clone(),
            code: entry.code,
            doc: entry.doc.clone(),
            variant: Some(variant),
            docstring: entry.doc.clone(),
        }
    }
}

/// Core logic to parse contract WASM spec and resolve error code into readable details.
pub fn resolve_contract_error_inner(
    wasm_bytes: &[u8],
    error_code: u32,
) -> Result<Option<ResolvedContractError>, String> {
    if wasm_bytes.len() < 8 || &wasm_bytes[0..4] != b"\0asm" {
        return Err("invalid WASM binary: invalid magic header".to_string());
    }

    let spec = decode_contract_spec(wasm_bytes).map_err(|e| e.to_string())?;

    Ok(resolve_error_code(&spec, error_code).map(ResolvedContractError::from))
}

/// Resolves a custom contract error code from a contract's WASM bytecode.
///
/// Parses the `contractspecv0` custom section in the provided contract WASM bytes,
/// searches for error enum definitions (`ScSpecUdtErrorEnumV0`), and maps the
/// `error_code` into its human-readable variant name and docstrings.
///
/// Returns a structured JS object `{ name, code, doc, variant, docstring }` if found,
/// or `null` if the error code is not defined in the specification.
/// Returns an error if the passed WASM bytes are invalid.
#[wasm_bindgen]
pub fn resolve_contract_error(wasm_bytes: &[u8], error_code: u32) -> Result<JsValue, JsValue> {
    let resolved =
        resolve_contract_error_inner(wasm_bytes, error_code).map_err(|e| JsValue::from_str(&e))?;

    match resolved {
        Some(entry) => {
            serde_wasm_bindgen::to_value(&entry).map_err(|e| JsValue::from_str(&e.to_string()))
        }
        None => Ok(JsValue::NULL),
    }
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        Limits, ScSpecEntry, ScSpecUdtErrorEnumCaseV0, ScSpecUdtErrorEnumV0, WriteXdr,
    };

    fn leb128_encode(mut value: u64) -> Vec<u8> {
        let mut result = Vec::new();
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            result.push(byte);
            if value == 0 {
                break;
            }
        }
        result
    }

    fn build_wasm_with_error_enums(enums: Vec<ScSpecUdtErrorEnumV0>) -> Vec<u8> {
        let mut section_data = Vec::new();
        for err_enum in enums {
            let entry = ScSpecEntry::UdtErrorEnumV0(err_enum);
            let bytes = entry.to_xdr(Limits::none()).unwrap();
            section_data.extend_from_slice(&bytes);
        }

        let section_name = "contractspecv0";
        let mut custom_payload = Vec::new();
        custom_payload.extend_from_slice(&leb128_encode(section_name.len() as u64));
        custom_payload.extend_from_slice(section_name.as_bytes());
        custom_payload.extend_from_slice(&section_data);

        let mut wasm = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        wasm.push(0);
        wasm.extend_from_slice(&leb128_encode(custom_payload.len() as u64));
        wasm.extend(custom_payload);
        wasm
    }

    fn sample_contract_wasm() -> Vec<u8> {
        let error_enum = ScSpecUdtErrorEnumV0 {
            doc: "General contract errors".try_into().unwrap(),
            lib: "".try_into().unwrap(),
            name: "ContractError".try_into().unwrap(),
            cases: vec![
                ScSpecUdtErrorEnumCaseV0 {
                    doc: "Caller is not authorized to perform action"
                        .try_into()
                        .unwrap(),
                    name: "Unauthorized".try_into().unwrap(),
                    value: 1,
                },
                ScSpecUdtErrorEnumCaseV0 {
                    doc: "Account balance is too low".try_into().unwrap(),
                    name: "InsufficientBalance".try_into().unwrap(),
                    value: 2,
                },
                ScSpecUdtErrorEnumCaseV0 {
                    doc: "".try_into().unwrap(),
                    name: "Expired".try_into().unwrap(),
                    value: 3,
                },
            ]
            .try_into()
            .unwrap(),
        };
        build_wasm_with_error_enums(vec![error_enum])
    }

    #[test]
    fn test_resolve_contract_error_success() {
        let wasm = sample_contract_wasm();

        let res = resolve_contract_error_inner(&wasm, 1)
            .unwrap()
            .expect("should find error");
        assert_eq!(res.name, "ContractError::Unauthorized");
        assert_eq!(res.code, 1);
        assert_eq!(
            res.doc,
            Some("Caller is not authorized to perform action".to_string())
        );
        assert_eq!(res.variant, Some("Unauthorized".to_string()));
        assert_eq!(
            res.docstring,
            Some("Caller is not authorized to perform action".to_string())
        );

        let res2 = resolve_contract_error_inner(&wasm, 2)
            .unwrap()
            .expect("should find error");
        assert_eq!(res2.name, "ContractError::InsufficientBalance");
        assert_eq!(res2.code, 2);
        assert_eq!(res2.doc, Some("Account balance is too low".to_string()));
        assert_eq!(res2.variant, Some("InsufficientBalance".to_string()));

        // Case with empty docstring falls back to enum docstring
        let res3 = resolve_contract_error_inner(&wasm, 3)
            .unwrap()
            .expect("should find error");
        assert_eq!(res3.name, "ContractError::Expired");
        assert_eq!(res3.code, 3);
        assert_eq!(res3.doc, Some("General contract errors".to_string()));
    }

    #[test]
    fn test_resolve_contract_error_not_found_returns_none() {
        let wasm = sample_contract_wasm();

        let res = resolve_contract_error_inner(&wasm, 999).unwrap();
        assert_eq!(res, None);
    }

    #[test]
    fn test_resolve_contract_error_missing_spec_section_returns_none() {
        // Valid WASM header without contractspecv0 custom section
        let wasm = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

        let res = resolve_contract_error_inner(&wasm, 1).unwrap();
        assert_eq!(res, None);
    }

    #[test]
    fn test_resolve_contract_error_multiple_enums() {
        let auth_error = ScSpecUdtErrorEnumV0 {
            doc: "Auth errors".try_into().unwrap(),
            lib: "".try_into().unwrap(),
            name: "AuthError".try_into().unwrap(),
            cases: vec![ScSpecUdtErrorEnumCaseV0 {
                doc: "Access forbidden".try_into().unwrap(),
                name: "Forbidden".try_into().unwrap(),
                value: 100,
            }]
            .try_into()
            .unwrap(),
        };

        let token_error = ScSpecUdtErrorEnumV0 {
            doc: "Token errors".try_into().unwrap(),
            lib: "".try_into().unwrap(),
            name: "TokenError".try_into().unwrap(),
            cases: vec![ScSpecUdtErrorEnumCaseV0 {
                doc: "Transfer rejected".try_into().unwrap(),
                name: "TransferFailed".try_into().unwrap(),
                value: 200,
            }]
            .try_into()
            .unwrap(),
        };

        let wasm = build_wasm_with_error_enums(vec![auth_error, token_error]);

        let res1 = resolve_contract_error_inner(&wasm, 100).unwrap().unwrap();
        assert_eq!(res1.name, "AuthError::Forbidden");
        assert_eq!(res1.code, 100);

        let res2 = resolve_contract_error_inner(&wasm, 200).unwrap().unwrap();
        assert_eq!(res2.name, "TokenError::TransferFailed");
        assert_eq!(res2.code, 200);
    }

    #[test]
    fn test_resolve_contract_error_invalid_wasm_bytes() {
        let invalid_bytes = [1, 2, 3, 4];
        let err = resolve_contract_error_inner(&invalid_bytes, 1).unwrap_err();
        assert!(err.contains("invalid WASM"));
    }

    #[cfg(target_arch = "wasm32")]
    #[test]
    fn test_resolve_contract_error_wasm32() {
        let wasm = sample_contract_wasm();
        let res_wasm = resolve_contract_error(&wasm, 2).expect("should resolve successfully");
        assert!(!res_wasm.is_null());
        let parsed: ResolvedContractError = serde_wasm_bindgen::from_value(res_wasm).unwrap();
        assert_eq!(parsed.name, "ContractError::InsufficientBalance");
        assert_eq!(parsed.code, 2);

        let res_null = resolve_contract_error(&wasm, 999).expect("should return null");
        assert!(res_null.is_null());
    }
}
