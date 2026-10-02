use grat_core::decode::host_error::ClassifiedError;
use grat_core::decode::report::build_report;
use grat_core::taxonomy::schema::ErrorCategory;
use grat_core::types::report::DiagnosticReport;
use grat_core::xdr::codec::XdrCodec;
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

#[wasm_bindgen]
pub fn resolve_contract_error(wasm_bytes: &[u8], error_code: u32) -> Result<JsValue, JsValue> {
    let _ = (wasm_bytes, error_code);
    let result = serde_json::json!({ "status": "not_yet_implemented" });
    serde_wasm_bindgen::to_value(&result).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
