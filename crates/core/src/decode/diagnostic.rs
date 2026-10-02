use crate::decode::walker::DiagnosticEventWalker;
use crate::error::GratResult;
use crate::types::report::{DiagnosticReport, RootCause, SuggestedFix};
use crate::xdr::codec::XdrCodec;
use stellar_xdr::curr::{ContractEventBody, DiagnosticEvent, ScVal};

pub fn enrich_report(report: &mut DiagnosticReport, tx_data: &serde_json::Value) -> GratResult<()> {
    if let Some(events_b64) = tx_data
        .get("diagnosticEventsXdr")
        .and_then(|e| e.as_array())
    {
        let mut diagnostic_events = Vec::new();

        for event_b64 in events_b64 {
            if let Some(b64_str) = event_b64.as_str() {
                if let Ok(event) = DiagnosticEvent::from_xdr_base64(b64_str) {
                    analyze_diagnostic_event(report, &event);
                    diagnostic_events.push(event);
                }
            }
        }

        if let Some(root_cause_event) = deepest_error_event(&diagnostic_events) {
            add_deepest_error_root_cause(report, &root_cause_event);
        }

        report.failing_contract_id =
            DiagnosticEventWalker::find_failing_contract(&diagnostic_events);
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiagnosticErrorEvent {
    depth: usize,
    topics: Vec<String>,
    payload: String,
}

fn scval_to_string(val: &ScVal) -> Option<String> {
    match val {
        ScVal::Symbol(sym) => Some(sym.to_string()),
        ScVal::String(s) => Some(s.to_string()),
        ScVal::U32(u) => Some(u.to_string()),
        ScVal::I32(i) => Some(i.to_string()),
        ScVal::U64(u) => Some(u.to_string()),
        ScVal::I64(i) => Some(i.to_string()),
        ScVal::Void => Some("Void".to_string()),
        ScVal::Bool(b) => Some(b.to_string()),
        ScVal::U128(u) => {
            let num = (u128::from(u.hi) << 64) | u128::from(u.lo);
            Some(num.to_string())
        }
        ScVal::I128(i) => {
            #[allow(clippy::cast_possible_wrap)]
            let lo = u128::from(i.lo) as i128;
            let num = (i128::from(i.hi) << 64) | lo;
            Some(num.to_string())
        }
        ScVal::Vec(Some(v)) => {
            let items: Vec<String> = v
                .iter()
                .map(|item| scval_to_string(item).unwrap_or_else(|| "?".to_string()))
                .collect();
            Some(format!("[{}]", items.join(", ")))
        }
        ScVal::Map(Some(m)) => {
            let items: Vec<String> = m
                .iter()
                .map(|entry| {
                    let k = scval_to_string(&entry.key).unwrap_or_else(|| "?".to_string());
                    let v = scval_to_string(&entry.val).unwrap_or_else(|| "?".to_string());
                    format!("{k}: {v}")
                })
                .collect();
            Some(format!("{{{}}}", items.join(", ")))
        }
        _ => None,
    }
}

fn scval_to_payload_string(val: &ScVal) -> Option<String> {
    match val {
        ScVal::Void => None,
        _ => scval_to_string(val).or_else(|| Some(format!("{val:?}"))),
    }
}

fn normalized_topic(topic: &str) -> String {
    topic.trim().to_ascii_lowercase()
}

fn is_call_topic(topic: &str) -> bool {
    matches!(
        normalized_topic(topic).as_str(),
        "fn_call" | "function_call" | "call"
    )
}

fn is_return_topic(topic: &str) -> bool {
    matches!(
        normalized_topic(topic).as_str(),
        "fn_return" | "function_return" | "return"
    )
}

fn contains_error_signal(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value == "err"
        || value.contains("error")
        || value.contains("failed")
        || value.contains("failure")
        || value.contains("panic")
        || value.contains("trap")
}

fn error_payload(topics: &[String], data: &ScVal) -> Option<String> {
    let data_payload = scval_to_payload_string(data);

    if let Some(payload) = &data_payload {
        if contains_error_signal(payload) {
            return Some(payload.clone());
        }
    }

    if topics.iter().any(|topic| contains_error_signal(topic)) {
        return data_payload.or_else(|| Some(topics.join(" > ")));
    }

    None
}

#[allow(irrefutable_let_patterns)]
fn deepest_error_event(events: &[DiagnosticEvent]) -> Option<DiagnosticErrorEvent> {
    let mut depth = 0usize;
    let mut deepest = None;

    for event in events {
        if let ContractEventBody::V0(v0) = &event.event.body {
            let topics: Vec<String> = v0.topics.iter().filter_map(scval_to_string).collect();

            if topics.iter().any(|topic| is_call_topic(topic)) {
                depth += 1;
            }

            if let Some(payload) = error_payload(&topics, &v0.data) {
                let candidate = DiagnosticErrorEvent {
                    depth,
                    topics,
                    payload,
                };
                let is_deeper = deepest
                    .as_ref()
                    .is_none_or(|current: &DiagnosticErrorEvent| candidate.depth >= current.depth);

                if is_deeper {
                    deepest = Some(candidate);
                }
            }

            if v0
                .topics
                .iter()
                .filter_map(scval_to_string)
                .any(|topic| is_return_topic(&topic))
            {
                depth = depth.saturating_sub(1);
            }
        }
    }

    deepest
}

fn add_deepest_error_root_cause(report: &mut DiagnosticReport, error_event: &DiagnosticErrorEvent) {
    let topics = if error_event.topics.is_empty() {
        "untagged diagnostic event".to_string()
    } else {
        error_event.topics.join(" > ")
    };
    let description = format!(
        "Deepest diagnostic error occurred at call depth {} in [{}] with payload: {}.",
        error_event.depth, topics, error_event.payload
    );

    if let Some(existing_position) = report
        .root_causes
        .iter()
        .position(|cause| cause.description.contains("Deepest diagnostic error"))
    {
        report.root_causes.remove(existing_position);
    }
    report.root_causes.insert(
        0,
        RootCause {
            description: description.clone(),
            likelihood: "high".to_string(),
        },
    );

    let detail = format!("- Deepest error event: {description}");
    if !report.detailed_explanation.contains(&detail) {
        if report.detailed_explanation.is_empty() {
            report.detailed_explanation = format!("Diagnostic events trace:\n{detail}");
        } else {
            report.detailed_explanation.push('\n');
            report.detailed_explanation.push_str(&detail);
        }
    }
}

#[allow(irrefutable_let_patterns)]
#[allow(clippy::too_many_lines)]
fn analyze_diagnostic_event(report: &mut DiagnosticReport, event: &DiagnosticEvent) {
    if let ContractEventBody::V0(v0) = &event.event.body {
        let topics: Vec<String> = v0.topics.iter().filter_map(scval_to_string).collect();
        if topics.is_empty() {
            return;
        }

        if topics
            .iter()
            .any(|t| t.to_lowercase().contains("budget") || t.to_lowercase().contains("limit"))
        {
            if !report
                .root_causes
                .iter()
                .any(|c| c.description.contains("Resource limit"))
            {
                report.root_causes.push(RootCause {
                    description: "Resource limit was exceeded during contract execution."
                        .to_string(),
                    likelihood: "high".to_string(),
                });
            }
            if !report
                .suggested_fixes
                .iter()
                .any(|f| f.id == "increase_limits")
            {
                report.suggested_fixes.push(SuggestedFix {
                    description:
                        "Increase the resource limits when simulating/submitting the transaction."
                            .to_string(),
                    difficulty: "easy".to_string(),
                    requires_upgrade: false,
                    example: None,
                    id: "increase_limits".to_string(),
                    remedy_code: None,
                });
            }
        }

        if topics
            .iter()
            .any(|t| t.to_lowercase().contains("storage") || t.to_lowercase().contains("footprint"))
        {
            if !report
                .root_causes
                .iter()
                .any(|c| c.description.contains("footprint"))
            {
                report.root_causes.push(RootCause {
                    description: "The contract accessed or requested a storage key that was not declared in the footprint.".to_string(),
                    likelihood: "high".to_string(),
                });
            }
            if !report
                .suggested_fixes
                .iter()
                .any(|f| f.id == "resimulate_footprint")
            {
                report.suggested_fixes.push(SuggestedFix {
                    description: "Re-simulate the transaction to capture the correct footprint keys and footprint declaration.".to_string(),
                    difficulty: "easy".to_string(),
                    requires_upgrade: false,
                    example: None,
                    id: "resimulate_footprint".to_string(),
                    remedy_code: None,
                });
            }
        }

        if topics
            .iter()
            .any(|t| t.to_lowercase().contains("auth") || t.to_lowercase().contains("signature"))
        {
            if !report
                .root_causes
                .iter()
                .any(|c| c.description.contains("authorization"))
            {
                report.root_causes.push(RootCause {
                    description: "Transaction verification or authorization check failed in __check_auth or signature check.".to_string(),
                    likelihood: "high".to_string(),
                });
            }
            if !report
                .suggested_fixes
                .iter()
                .any(|f| f.id == "check_auth_signatures")
            {
                report.suggested_fixes.push(SuggestedFix {
                    description: "Check that the transaction signatures match the required signers and the nonce is correct.".to_string(),
                    difficulty: "medium".to_string(),
                    requires_upgrade: false,
                    example: None,
                    id: "check_auth_signatures".to_string(),
                    remedy_code: None,
                });
            }
        }

        let topics_str = topics.join(" > ");
        if !report.detailed_explanation.contains(&topics_str) {
            if report.detailed_explanation.is_empty() {
                report.detailed_explanation = format!("Diagnostic events trace:\n- [{topics_str}]");
            } else {
                report
                    .detailed_explanation
                    .push_str(&format!("\n- [{topics_str}]"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        ContractEvent, ContractEventBody, ContractEventType, ContractEventV0, ExtensionPoint,
        Int128Parts, ScMap, ScMapEntry, ScString, ScSymbol, ScVal, ScVec, StringM, UInt128Parts,
    };

    fn diagnostic_event(topic: &str, data: ScVal) -> DiagnosticEvent {
        DiagnosticEvent {
            in_successful_contract_call: false,
            event: ContractEvent {
                ext: ExtensionPoint::V0,
                contract_id: None,
                type_: ContractEventType::Diagnostic,
                body: ContractEventBody::V0(ContractEventV0 {
                    topics: vec![ScVal::Symbol(ScSymbol(
                        StringM::try_from(topic.as_bytes().to_vec()).unwrap(),
                    ))]
                    .try_into()
                    .unwrap(),
                    data,
                }),
            },
        }
    }

    fn event_b64(topic: &str, data: ScVal) -> String {
        XdrCodec::to_xdr_base64(&diagnostic_event(topic, data)).expect("encode diagnostic event")
    }

    fn symbol(value: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(
            StringM::try_from(value.as_bytes().to_vec()).unwrap(),
        ))
    }

    #[test]
    fn deepest_error_event_prefers_innermost_error_payload() {
        let events = vec![
            diagnostic_event("fn_call", ScVal::Void),
            diagnostic_event("error", symbol("outer failed")),
            diagnostic_event("fn_call", ScVal::Void),
            diagnostic_event("error", symbol("inner failed")),
            diagnostic_event("fn_return", ScVal::Void),
            diagnostic_event("fn_return", ScVal::Void),
        ];

        let deepest = deepest_error_event(&events).expect("deepest error event");

        assert_eq!(deepest.depth, 2);
        assert_eq!(deepest.payload, "inner failed");
        assert_eq!(deepest.topics, vec!["error".to_string()]);
    }

    #[test]
    fn enrich_report_adds_deepest_error_root_cause() {
        let tx_data = serde_json::json!({
            "diagnosticEventsXdr": [
                event_b64("fn_call", ScVal::Void),
                event_b64("error", symbol("outer failed")),
                event_b64("fn_call", ScVal::Void),
                event_b64("error", symbol("inner failed")),
                event_b64("fn_return", ScVal::Void),
                event_b64("fn_return", ScVal::Void),
            ]
        });
        let mut report = DiagnosticReport::new("contract", 1, "HostError", "failure");

        enrich_report(&mut report, &tx_data).expect("enrich report");

        assert!(report.root_causes[0]
            .description
            .contains("Deepest diagnostic error"));
        assert!(report.root_causes.iter().any(|cause| {
            cause.description.contains("call depth 2") && cause.description.contains("inner failed")
        }));
        assert!(report.detailed_explanation.contains("Deepest error event"));
        assert!(report.detailed_explanation.contains("inner failed"));
    }

    #[test]
    fn test_scval_to_string_supported_variants() {
        assert_eq!(
            scval_to_string(&ScVal::String(ScString(
                StringM::try_from(b"hello".to_vec()).unwrap()
            ))),
            Some("hello".to_string())
        );
        assert_eq!(
            scval_to_string(&ScVal::Bool(true)),
            Some("true".to_string())
        );
        assert_eq!(
            scval_to_string(&ScVal::Bool(false)),
            Some("false".to_string())
        );
        assert_eq!(
            scval_to_string(&ScVal::I32(-2147483648)),
            Some("-2147483648".to_string())
        );
        assert_eq!(
            scval_to_string(&ScVal::I32(2147483647)),
            Some("2147483647".to_string())
        );
        assert_eq!(
            scval_to_string(&ScVal::U64(18446744073709551615)),
            Some("18446744073709551615".to_string())
        );
        assert_eq!(
            scval_to_string(&ScVal::I64(-9223372036854775808)),
            Some("-9223372036854775808".to_string())
        );
        assert_eq!(
            scval_to_string(&ScVal::I64(9223372036854775807)),
            Some("9223372036854775807".to_string())
        );
    }

    #[test]
    fn test_scval_to_string_empty_args() {
        let empty_vec = ScVal::Vec(Some(ScVec(vec![].try_into().unwrap())));
        assert_eq!(scval_to_string(&empty_vec), Some("[]".to_string()));
        let void_val = ScVal::Void;
        assert_eq!(scval_to_string(&void_val), Some("Void".to_string()));
    }

    #[test]
    fn test_scval_to_string_large_integers() {
        let u128_val = ScVal::U128(UInt128Parts { hi: 1, lo: 0 });
        assert_eq!(
            scval_to_string(&u128_val),
            Some("18446744073709551616".to_string())
        );

        let i128_val = ScVal::I128(Int128Parts { hi: -1i64, lo: 0 });
        assert_eq!(
            scval_to_string(&i128_val),
            Some("-18446744073709551616".to_string())
        );

        let u128_max = ScVal::U128(UInt128Parts {
            hi: u64::MAX,
            lo: u64::MAX,
        });
        assert_eq!(
            scval_to_string(&u128_max),
            Some("340282366920938463463374607431768211455".to_string())
        );

        let u128_min = ScVal::U128(UInt128Parts { hi: 0, lo: 0 });
        assert_eq!(scval_to_string(&u128_min), Some("0".to_string()));

        let i128_max = ScVal::I128(Int128Parts {
            hi: i64::MAX,
            lo: u64::MAX,
        });
        assert_eq!(
            scval_to_string(&i128_max),
            Some("170141183460469231731687303715884105727".to_string())
        );

        let i128_min = ScVal::I128(Int128Parts {
            hi: i64::MIN,
            lo: 0,
        });
        assert_eq!(
            scval_to_string(&i128_min),
            Some("-170141183460469231731687303715884105728".to_string())
        );
    }

    #[test]
    fn test_scval_to_string_nested_map() {
        let map_entry = ScMapEntry {
            key: ScVal::Symbol(ScSymbol(StringM::try_from(b"key".to_vec()).unwrap())),
            val: ScVal::U32(42),
        };
        let scmap = ScMap(vec![map_entry].try_into().unwrap());
        let nested_map = ScVal::Map(Some(scmap));
        let vec_val = ScVal::Vec(Some(ScVec(vec![nested_map].try_into().unwrap())));
        assert_eq!(scval_to_string(&vec_val), Some("[{key: 42}]".to_string()));
    }
}
