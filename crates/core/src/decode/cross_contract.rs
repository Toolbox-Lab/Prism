//! Cross-contract call failure attribution.
//!
//! Walks the diagnostic event stream to find the deepest contract in the call
//! chain that emitted a failure event, attributing the error to that contract
//! rather than the top-level invoker.

use stellar_xdr::curr::{ContractEventBody, ContractEventType, DiagnosticEvent, Hash, ScVal};

use crate::error::PrismResult;
use crate::types::report::{DiagnosticReport, FailureAttribution};
use crate::xdr::codec::XdrCodec;

/// A lightweight record of one call-frame seen in the event stream.
#[derive(Debug, Clone)]
struct CallFrame {
    contract_address: String,
    function_name: Option<String>,
    depth: usize,
}

/// Analyse the `diagnosticEventsXdr` array in `tx_data` and, if a
/// cross-contract failure is found, populate `report.cross_contract_attribution`.
pub fn attribute_failure(
    report: &mut DiagnosticReport,
    tx_data: &serde_json::Value,
) -> PrismResult<()> {
    let events_b64 = match tx_data
        .get("diagnosticEventsXdr")
        .and_then(|v| v.as_array())
    {
        Some(arr) if !arr.is_empty() => arr,
        _ => return Ok(()),
    };

    let mut call_stack: Vec<CallFrame> = Vec::new();
    let mut failure: Option<CallFrame> = None;

    for event_b64 in events_b64 {
        let b64_str = match event_b64.as_str() {
            Some(s) => s,
            None => continue,
        };
        let event = match DiagnosticEvent::from_xdr_base64(b64_str) {
            Ok(e) => e,
            Err(_) => continue,
        };

        process_event(&event, &mut call_stack, &mut failure);
    }

    // Only surface an attribution when the failure is deeper than depth 0,
    // i.e., a sub-contract caused it, not the top-level invoker.
    if let Some(frame) = failure {
        if frame.depth > 0 {
            report.cross_contract_attribution = Some(FailureAttribution {
                origin_description: build_description(&frame),
                contract_address: frame.contract_address,
                function_name: frame.function_name,
                call_depth: frame.depth,
            });
        }
    }

    Ok(())
}

fn process_event(
    event: &DiagnosticEvent,
    call_stack: &mut Vec<CallFrame>,
    failure: &mut Option<CallFrame>,
) {
    // Only care about system-emitted diagnostic events (in_successful_contract_call == false
    // means the surrounding call failed, but we want the frame itself).
    let ContractEventBody::V0(v0) = &event.event.body;

    let contract_address = match &event.event.contract_id {
        Some(hash) => hash_to_string(hash),
        None => return,
    };

    let topics: Vec<String> = v0.topics.iter().filter_map(scval_to_string).collect();
    let first_topic = topics.first().map_or("", std::string::String::as_str);

    match first_topic {
        // fn_call / fn_return are emitted by the host for every cross-contract
        // invocation boundary.
        "fn_call" => {
            let function_name = topics.get(1).cloned();
            call_stack.push(CallFrame {
                contract_address,
                function_name,
                depth: call_stack.len(),
            });
        }
        "fn_return" => {
            call_stack.pop();
        }
        // Any explicit "error" or "panic" topic while inside a call frame
        // marks that frame as the origin.
        "error" | "panic" => {
            // Prefer the deepest frame on the stack; fall back to the event's
            // own contract address.
            let frame = call_stack.last().cloned().unwrap_or(CallFrame {
                contract_address,
                function_name: topics.get(1).cloned(),
                depth: call_stack.len(),
            });
            // Keep only the first (deepest) failure seen.
            if failure.is_none() {
                *failure = Some(frame);
            }
        }
        _ => {
            // For non-system event types that arrive while in_successful_contract_call
            // is false we treat the emitting contract as the failure origin.
            if event.event.type_ == ContractEventType::System && !event.in_successful_contract_call
            {
                let frame = call_stack.last().cloned().unwrap_or(CallFrame {
                    contract_address,
                    function_name: topics.first().cloned(),
                    depth: call_stack.len(),
                });
                if failure.is_none() {
                    *failure = Some(frame);
                }
            }
        }
    }
}

fn build_description(frame: &CallFrame) -> String {
    match &frame.function_name {
        Some(fn_name) => format!(
            "Failure originated in contract {} at function `{}` (call depth {})",
            frame.contract_address, fn_name, frame.depth
        ),
        None => format!(
            "Failure originated in contract {} (call depth {})",
            frame.contract_address, frame.depth
        ),
    }
}

fn hash_to_string(hash: &Hash) -> String {
    bytes_to_hex(&hash.0)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn scval_to_string(val: &ScVal) -> Option<String> {
    match val {
        ScVal::Symbol(sym) => Some(sym.to_string()),
        ScVal::String(s) => Some(s.to_string()),
        ScVal::U32(u) => Some(u.to_string()),
        ScVal::I32(i) => Some(i.to_string()),
        _ => None,
    }
}
