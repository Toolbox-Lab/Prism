pub mod auth;
pub mod auth_signature;
pub mod context;
pub mod contract_error;
pub mod cross_contract;
pub mod decode_context;
pub mod diagnostic;
pub mod host_error;
pub mod mappings;
pub mod report;
pub mod walker;

pub use auth::{AddressCredential, AuthChain, AuthCredential, AuthFunctionKind, AuthInvocation};
pub use walker::{
    walk_diagnostic_events, DiagnosticEventKind, DiagnosticEventWalker, StructuredDiagnosticEvent,
};

use crate::error::{PrismError, PrismResult};
use crate::types::report::DiagnosticReport;
use crate::xdr::codec::XdrCodec;
use stellar_xdr::curr::{
    ScVal, SorobanTransactionMetaExt,
    TransactionMeta, TransactionResult,
};

/// Decode `resultMetaXdr` as `TransactionMeta` and, if it is V3, inject the
/// Soroban contract events, diagnostic events, and return value into the JSON
/// payload so downstream enrichment code sees the same shape it does for V1/V2.
///
/// Also extracts `fee_charged` from `resultXdr` so fee details are not lost.
fn parse_v3_metadata(tx_data: &mut serde_json::Value) -> PrismResult<()> {
    // Inject fee_charged from TransactionResult regardless of V3.
    if let Some(result_b64) = tx_data.get("resultXdr").and_then(|r| r.as_str()) {
        if let Ok(tx_result) = TransactionResult::from_xdr_base64(result_b64) {
            tx_data["inclusionFee"] = serde_json::json!(tx_result.fee_charged);
        }
    }

    let meta_b64 = match tx_data.get("resultMetaXdr").and_then(|r| r.as_str()) {
        Some(s) => s.to_string(),
        None => return Ok(()),
    };

    let meta =
        TransactionMeta::from_xdr_base64(&meta_b64).map_err(|e| PrismError::XdrDecodingFailed {
            type_name: "TransactionMeta",
            reason: e.to_string(),
        })?;

    if let TransactionMeta::V3(v3) = meta {
        let soroban_meta = match v3.soroban_meta {
            Some(s) => s,
            None => return Ok(()),
        };

        // Inject contract events as base64 XDR strings.
        if !soroban_meta.events.is_empty() {
            let contract_events: Vec<String> = soroban_meta
                .events
                .iter()
                .filter_map(|e| XdrCodec::to_xdr_base64(e).ok())
                .collect();
            tx_data["events"] = serde_json::json!({
                "contractEventsXdr": contract_events
            });
        }

        // Inject diagnostic events as base64 XDR strings.
        if !soroban_meta.diagnostic_events.is_empty() {
            let diagnostic_events: Vec<String> = soroban_meta
                .diagnostic_events
                .iter()
                .filter_map(|e| XdrCodec::to_xdr_base64(e).ok())
                .collect();
            tx_data["diagnosticEventsXdr"] = serde_json::json!(diagnostic_events);
        }

        // Encode the return value as a base64 XDR string.
        if soroban_meta.return_value != ScVal::Void {
            if let Ok(b64) = XdrCodec::to_xdr_base64(&soroban_meta.return_value) {
                tx_data["returnValue"] = serde_json::json!(b64);
            }
        }

        // Extract resource fee and refundable fee from SorobanTransactionMetaExtV1.
        if let SorobanTransactionMetaExt::V1(v1) = &soroban_meta.ext {
            tx_data["resourceFee"] = serde_json::json!({
                "totalNonRefundableResourceFeeCharged": v1.total_non_refundable_resource_fee_charged,
                "totalRefundableResourceFeeCharged": v1.total_refundable_resource_fee_charged,
                "rentFeeCharged": v1.rent_fee_charged,
            });
        }
    }

    Ok(())
}

fn filter_transaction_by_operation(
    tx_data: &mut serde_json::Value,
    op_index: usize,
) -> PrismResult<()> {
    if let Some(events) = tx_data.get_mut("events") {
        if let Some(contract_events) = events.get_mut("contractEventsXdr") {
            if let Some(events_array) = contract_events.as_array_mut() {
                if op_index < events_array.len() {
                    let target_events = events_array[op_index].clone();
                    *events_array = vec![target_events];
                } else {
                    *events_array = vec![];
                }
            }
        }
    }

    if let Some(diagnostic_events) = tx_data.get_mut("diagnosticEventsXdr") {
        if let Some(events_array) = diagnostic_events.as_array_mut() {
            if op_index == 0 && !events_array.is_empty() {
                let first_event = events_array[0].clone();
                *events_array = vec![first_event];
            } else {
                *events_array = vec![];
            }
        }
    }

    Ok(())
}

pub async fn decode_transaction(
    tx_hash: &str,
    network: &crate::types::config::NetworkConfig,
) -> PrismResult<Vec<DiagnosticReport>> {
    decode_transaction_with_op_filter(tx_hash, network, None).await
}

pub async fn decode_transaction_with_op_filter(
    tx_hash: &str,
    network: &crate::types::config::NetworkConfig,
    op_index: Option<usize>,
) -> PrismResult<Vec<DiagnosticReport>> {
    let rpc = crate::rpc::SorobanRpcClient::new(network);
    let tx_data = rpc.get_transaction(tx_hash).await?;
    let mut base_tx_data = serde_json::to_value(tx_data)
        .map_err(|e| crate::error::PrismError::Internal(e.to_string()))?;

    // Parse V3 metadata and inject events/returnValue/fees into the base transaction JSON.
    parse_v3_metadata(&mut base_tx_data)?;

    // Decode the envelope XDR to determine the number of operations in the transaction.
    let num_ops = if let Some(envelope_str) =
        base_tx_data.get("envelopeXdr").and_then(|v| v.as_str())
    {
        // Use the XDR codec to parse the envelope.
        let envelope = <stellar_xdr::curr::TransactionEnvelope as crate::xdr::codec::XdrCodec>::from_xdr_base64(envelope_str)
            .map_err(|e| crate::error::PrismError::Internal(format!("Failed to decode envelope XDR: {e}")))?;
        match envelope {
            stellar_xdr::curr::TransactionEnvelope::TxV0(v0) => v0.tx.operations.len(),
            stellar_xdr::curr::TransactionEnvelope::Tx(v1) => v1.tx.operations.len(),
            stellar_xdr::curr::TransactionEnvelope::TxFeeBump(fb) => {
                // Fee bump transaction contains an inner transaction with its own operations.
                match &fb.tx.inner_tx {
                    stellar_xdr::curr::FeeBumpTransactionInnerTx::Tx(v1) => v1.tx.operations.len(),
                }
            }
        }
    } else {
        // Fallback to a single operation if envelope missing
        1
    };

    let mut reports = Vec::new();
    let indices = match op_index {
        Some(i) => vec![i],
        None => (0..num_ops).collect(),
    };

    for i in indices {
        let mut tx_data = base_tx_data.clone();
        filter_transaction_by_operation(&mut tx_data, i)?;

        let error_info = host_error::classify_error(&tx_data)?;
        let mut report = report::build_report(&error_info)?;

        if error_info.is_contract_error {
            let ctx = crate::decode::decode_context::DecodeContextBuilder::from(network).build();
            if let Ok(contract_info) = contract_error::resolve(
                &error_info.contract_id.unwrap_or_default(),
                error_info.error_code,
                &ctx,
            )
            .await
            {
                report.contract_error = Some(contract_info);
            }
        }

        diagnostic::enrich_report(&mut report, &tx_data)?;
        context::enrich_report(&mut report, &tx_data)?;
        cross_contract::attribute_failure(&mut report, &tx_data)?;
        reports.push(report);
    }

    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        ContractEvent, ContractEventBody, ContractEventType, ContractEventV0, DiagnosticEvent,
        ExtensionPoint, OperationMeta, SorobanTransactionMeta, SorobanTransactionMetaExtV1,
        TransactionMetaV3, TransactionResultExt, TransactionResultResult,
    };

    fn make_v3_meta_with_v1_ext(
        non_refundable: i64,
        refundable: i64,
        rent: i64,
    ) -> TransactionMeta {
        TransactionMeta::V3(TransactionMetaV3 {
            ext: ExtensionPoint::V0,
            tx_changes_before: vec![].try_into().unwrap(),
            operations: vec![OperationMeta {
                changes: vec![].try_into().unwrap(),
            }]
            .try_into()
            .unwrap(),
            tx_changes_after: vec![].try_into().unwrap(),
            soroban_meta: Some(SorobanTransactionMeta {
                ext: SorobanTransactionMetaExt::V1(SorobanTransactionMetaExtV1 {
                    ext: ExtensionPoint::V0,
                    total_non_refundable_resource_fee_charged: non_refundable,
                    total_refundable_resource_fee_charged: refundable,
                    rent_fee_charged: rent,
                }),
                events: vec![].try_into().unwrap(),
                return_value: ScVal::Void,
                diagnostic_events: vec![].try_into().unwrap(),
            }),
        })
    }

    fn make_v3_meta_with_soroban(soroban: SorobanTransactionMeta) -> TransactionMeta {
        TransactionMeta::V3(TransactionMetaV3 {
            ext: ExtensionPoint::V0,
            tx_changes_before: vec![].try_into().unwrap(),
            operations: vec![OperationMeta {
                changes: vec![].try_into().unwrap(),
            }]
            .try_into()
            .unwrap(),
            tx_changes_after: vec![].try_into().unwrap(),
            soroban_meta: Some(soroban),
        })
    }

    fn make_tx_result(fee: i64) -> TransactionResult {
        TransactionResult {
            fee_charged: fee,
            result: TransactionResultResult::TxSuccess(vec![].try_into().unwrap()),
            ext: TransactionResultExt::V0,
        }
    }

    #[test]
    fn test_missing_meta_returns_ok() {
        let mut data = serde_json::json!({
            "resultXdr": "",
        });
        let result = parse_v3_metadata(&mut data);
        assert!(result.is_ok());
        // No inclusionFee because TransactionResult decode failed (empty string).
        assert!(data.get("inclusionFee").is_none());
    }

    #[test]
    fn test_invalid_meta_base64_returns_error() {
        let mut data = serde_json::json!({
            "resultMetaXdr": "!!!not-valid-base64!!!",
        });
        let result = parse_v3_metadata(&mut data);
        assert!(result.is_err());
        match result.unwrap_err() {
            PrismError::XdrDecodingFailed { type_name, .. } => {
                assert_eq!(type_name, "TransactionMeta");
            }
            e => panic!("expected XdrDecodingFailed, got {e}"),
        }
    }

    #[test]
    fn test_corrupt_meta_xdr_returns_error() {
        let mut data = serde_json::json!({
            "resultMetaXdr": "AAAA",  // valid base64 but not valid XDR
        });
        let result = parse_v3_metadata(&mut data);
        assert!(result.is_err());
        match result.unwrap_err() {
            PrismError::XdrDecodingFailed { type_name, .. } => {
                assert_eq!(type_name, "TransactionMeta");
            }
            e => panic!("expected XdrDecodingFailed, got {e}"),
        }
    }

    #[test]
    fn test_v3_without_soroban_meta_returns_ok() {
        let meta = TransactionMeta::V3(TransactionMetaV3 {
            ext: ExtensionPoint::V0,
            tx_changes_before: vec![].try_into().unwrap(),
            operations: vec![OperationMeta {
                changes: vec![].try_into().unwrap(),
            }]
            .try_into()
            .unwrap(),
            tx_changes_after: vec![].try_into().unwrap(),
            soroban_meta: None,
        });
        let b64 = XdrCodec::to_xdr_base64(&meta).unwrap();
        let mut data = serde_json::json!({
            "resultMetaXdr": b64,
        });
        let result = parse_v3_metadata(&mut data);
        assert!(result.is_ok());
        assert!(data.get("events").is_none());
        assert!(data.get("diagnosticEventsXdr").is_none());
        assert!(data.get("resourceFee").is_none());
    }

    #[test]
    fn test_v1_ext_resource_fee_extraction() {
        let meta = make_v3_meta_with_v1_ext(500, 300, 100);
        let result_b64 = XdrCodec::to_xdr_base64(&make_tx_result(999)).unwrap();
        let meta_b64 = XdrCodec::to_xdr_base64(&meta).unwrap();

        let mut data = serde_json::json!({
            "resultXdr": result_b64,
            "resultMetaXdr": meta_b64,
        });

        let result = parse_v3_metadata(&mut data);
        assert!(result.is_ok());

        assert_eq!(data["inclusionFee"], serde_json::json!(999));

        let fee = data["resourceFee"].as_object().expect("resourceFee");
        assert_eq!(fee["totalNonRefundableResourceFeeCharged"], 500);
        assert_eq!(fee["totalRefundableResourceFeeCharged"], 300);
        assert_eq!(fee["rentFeeCharged"], 100);
    }

    #[test]
    fn test_v0_ext_does_not_inject_resource_fee() {
        let soroban = SorobanTransactionMeta {
            ext: SorobanTransactionMetaExt::V0,
            events: vec![].try_into().unwrap(),
            return_value: ScVal::Void,
            diagnostic_events: vec![].try_into().unwrap(),
        };
        let meta = make_v3_meta_with_soroban(soroban);
        let meta_b64 = XdrCodec::to_xdr_base64(&meta).unwrap();

        let mut data = serde_json::json!({
            "resultMetaXdr": meta_b64,
        });

        let result = parse_v3_metadata(&mut data);
        assert!(result.is_ok());
        assert!(data.get("resourceFee").is_none());
    }

    #[test]
    fn test_events_extracted() {
        let event = ContractEvent {
            ext: ExtensionPoint::V0,
            contract_id: None,
            type_: ContractEventType::Contract,
            body: ContractEventBody::V0(ContractEventV0 {
                topics: vec![].try_into().unwrap(),
                data: ScVal::I32(42),
            }),
        };
        let diagnostic = DiagnosticEvent {
            in_successful_contract_call: true,
            event: event.clone(),
        };
        let soroban = SorobanTransactionMeta {
            ext: SorobanTransactionMetaExt::V0,
            events: vec![event.clone()].try_into().unwrap(),
            return_value: ScVal::I32(99),
            diagnostic_events: vec![diagnostic].try_into().unwrap(),
        };
        let meta = make_v3_meta_with_soroban(soroban);
        let meta_b64 = XdrCodec::to_xdr_base64(&meta).unwrap();

        let mut data = serde_json::json!({
            "resultMetaXdr": meta_b64,
        });

        let result = parse_v3_metadata(&mut data);
        assert!(result.is_ok());

        let events = data["events"]["contractEventsXdr"]
            .as_array()
            .expect("contractEventsXdr");
        assert_eq!(events.len(), 1);

        let diag = data["diagnosticEventsXdr"]
            .as_array()
            .expect("diagnosticEventsXdr");
        assert_eq!(diag.len(), 1);

        let rv = data["returnValue"].as_str().expect("returnValue");
        assert!(!rv.is_empty());
    }

    #[test]
    fn test_void_return_not_injected() {
        let soroban = SorobanTransactionMeta {
            ext: SorobanTransactionMetaExt::V0,
            events: vec![].try_into().unwrap(),
            return_value: ScVal::Void,
            diagnostic_events: vec![].try_into().unwrap(),
        };
        let meta = make_v3_meta_with_soroban(soroban);
        let meta_b64 = XdrCodec::to_xdr_base64(&meta).unwrap();

        let mut data = serde_json::json!({
            "resultMetaXdr": meta_b64,
        });

        let result = parse_v3_metadata(&mut data);
        assert!(result.is_ok());
        assert!(data.get("returnValue").is_none());
    }

    #[test]
    fn test_invalid_result_xdr_does_not_fail() {
        // Even with bad resultXdr, valid meta should succeed
        let meta = make_v3_meta_with_v1_ext(100, 50, 25);
        let meta_b64 = XdrCodec::to_xdr_base64(&meta).unwrap();

        let mut data = serde_json::json!({
            "resultXdr": "!!!bad!!!",
            "resultMetaXdr": meta_b64,
        });

        let result = parse_v3_metadata(&mut data);
        assert!(result.is_ok());

        // inclusionFee should not be present since resultXdr was bad
        assert!(data.get("inclusionFee").is_none());

        // But resource fee should still be extracted
        assert!(data.get("resourceFee").is_some());
    }
}
