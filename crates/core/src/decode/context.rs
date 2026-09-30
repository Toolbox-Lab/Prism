use crate::decode::auth_signature::decode_auth_entry_signatures;
use crate::error::PrismResult;
use crate::types::report::{DiagnosticReport, FeeBreakdown, ResourceSummary, TransactionContext};

pub fn enrich_report(
    report: &mut DiagnosticReport,
    tx_data: &serde_json::Value,
) -> PrismResult<()> {
    let tx_hash = tx_data
        .get("hash")
        .and_then(|h| h.as_str())
        .unwrap_or("unknown")
        .to_string();

    let ledger_sequence = tx_data
        .get("ledger")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;

    let context = TransactionContext {
        tx_hash,
        ledger_sequence,
        function_name: extract_function_name(tx_data),
        arguments: extract_arguments(tx_data),
        return_value: extract_return_value(tx_data),
        fee: extract_fee_breakdown(tx_data),
        resources: extract_resource_summary(tx_data),
    };

    report.transaction_context = Some(context);

    // Decode ed25519 signatures from auth entries embedded in the transaction envelope.
    report.auth_signatures = extract_auth_signatures(tx_data);

    Ok(())
}

fn extract_function_name(tx_data: &serde_json::Value) -> Option<String> {
    tx_data
        .get("functionName")
        .and_then(|f| f.as_str())
        .map(std::string::ToString::to_string)
}

fn extract_arguments(tx_data: &serde_json::Value) -> Vec<String> {
    tx_data
        .get("arguments")
        .and_then(|a| a.as_array())
        .map(|args| args.iter().map(std::string::ToString::to_string).collect())
        .unwrap_or_default()
}

fn extract_return_value(tx_data: &serde_json::Value) -> Option<String> {
    tx_data
        .get("returnValue")
        .and_then(|r| r.as_str())
        .map(std::string::ToString::to_string)
}

fn extract_fee_breakdown(tx_data: &serde_json::Value) -> FeeBreakdown {
    use crate::xdr::codec::XdrCodec;
    use stellar_xdr::curr::{TransactionEnvelope, TransactionMeta, TransactionResult};

    // 1. Get total fee from resultXdr
    let mut total_fee = 0;
    if let Some(result_xdr_b64) = tx_data.get("resultXdr").and_then(|v| v.as_str()) {
        if let Ok(tx_result) = TransactionResult::from_xdr_base64(result_xdr_b64) {
            total_fee = tx_result.fee_charged;
        }
    }

    // 2. Get bid fee from envelopeXdr
    let mut bid_fee = None;
    if let Some(envelope_xdr_b64) = tx_data.get("envelopeXdr").and_then(|v| v.as_str()) {
        if let Ok(tx_envelope) = TransactionEnvelope::from_xdr_base64(envelope_xdr_b64) {
            match tx_envelope {
                TransactionEnvelope::Tx(v1) => {
                    bid_fee = Some(i64::from(v1.tx.fee));
                }
                TransactionEnvelope::TxFeeBump(fee_bump) => {
                    bid_fee = Some(fee_bump.tx.fee);
                }
                TransactionEnvelope::TxV0(v0) => {
                    bid_fee = Some(i64::from(v0.tx.fee));
                }
            }
        }
    }

    // 3. Get resource fee components from resultMetaXdr
    let mut non_refundable_fee = 0;
    let mut refundable_fee = 0;
    let mut rent_fee = 0;
    let mut has_soroban_meta = false;

    if let Some(meta_xdr_b64) = tx_data.get("resultMetaXdr").and_then(|v| v.as_str()) {
        if let Ok(tx_meta) = TransactionMeta::from_xdr_base64(meta_xdr_b64) {
            if let TransactionMeta::V3(v3) = tx_meta {
                if let Some(soroban_meta) = v3.soroban_meta {
                    match soroban_meta.ext {
                        stellar_xdr::curr::SorobanTransactionMetaExt::V0 => {}
                        stellar_xdr::curr::SorobanTransactionMetaExt::V1(v1) => {
                            non_refundable_fee = v1.total_non_refundable_resource_fee_charged;
                            refundable_fee = v1.total_refundable_resource_fee_charged;
                            rent_fee = v1.rent_fee_charged;
                            has_soroban_meta = true;
                        }
                    }
                }
            }
        }
    }

    let resource_fee = if has_soroban_meta {
        non_refundable_fee + refundable_fee + rent_fee
    } else {
        0
    };

    let inclusion_fee = total_fee - resource_fee;

    FeeBreakdown {
        total_charged_fee: total_fee,
        inclusion_fee,
        resource_fee,
        refundable_fee: refundable_fee + rent_fee,
        non_refundable_fee,
        bid_fee,
    }
}

fn extract_resource_summary(_tx_data: &serde_json::Value) -> ResourceSummary {
    ResourceSummary {
        cpu_instructions_used: 0,
        cpu_instructions_limit: 0,
        memory_bytes_used: 0,
        memory_bytes_limit: 0,
        read_bytes: 0,
        write_bytes: 0,
    }
}

/// Extract and decode ed25519 signatures from auth entries in the transaction envelope.
/// Auth entries are base64 XDR strings found under `tx.operations[*].body.invoke_host_function_op.auth`
/// or directly in the `auth` field of an RPC simulate response stored in tx_data.
fn extract_auth_signatures(tx_data: &serde_json::Value) -> Vec<String> {
    let mut signatures = Vec::new();

    // Auth entries may appear directly as a top-level "auth" array (simulate response shape)
    // or nested inside envelopeXdr operations.
    if let Some(auth_array) = tx_data.get("auth").and_then(|a| a.as_array()) {
        for entry in auth_array {
            if let Some(xdr_b64) = entry.as_str() {
                signatures.extend(decode_auth_entry_signatures(xdr_b64));
            }
        }
    }

    signatures
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xdr::codec::XdrCodec;
    use stellar_xdr::curr::{
        ExtensionPoint, Memo, MuxedAccount, Preconditions, SequenceNumber, SorobanTransactionMeta,
        SorobanTransactionMetaExt, SorobanTransactionMetaExtV1, Transaction, TransactionEnvelope,
        TransactionExt, TransactionMeta, TransactionMetaV3, TransactionResult,
        TransactionResultResult, TransactionV1Envelope, Uint256,
    };

    #[test]
    fn test_extract_fee_breakdown_non_soroban() {
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([0; 32])),
            fee: 150,
            seq_num: SequenceNumber(1),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![].try_into().unwrap(),
            ext: TransactionExt::V0,
        };
        let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: vec![].try_into().unwrap(),
        });
        let envelope_xdr = envelope.to_xdr_base64().unwrap();

        let result = TransactionResult {
            fee_charged: 120,
            result: TransactionResultResult::TxSuccess(vec![].try_into().unwrap()),
            ext: stellar_xdr::curr::TransactionResultExt::V0,
        };
        let result_xdr = result.to_xdr_base64().unwrap();

        let tx_data = serde_json::json!({
            "envelopeXdr": envelope_xdr,
            "resultXdr": result_xdr,
        });

        let breakdown = extract_fee_breakdown(&tx_data);
        assert_eq!(breakdown.total_charged_fee, 120);
        assert_eq!(breakdown.bid_fee, Some(150));
        assert_eq!(breakdown.inclusion_fee, 120);
        assert_eq!(breakdown.resource_fee, 0);
        assert_eq!(breakdown.refundable_fee, 0);
        assert_eq!(breakdown.non_refundable_fee, 0);
    }

    #[test]
    fn test_extract_fee_breakdown_soroban() {
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([0; 32])),
            fee: 500,
            seq_num: SequenceNumber(1),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![].try_into().unwrap(),
            ext: TransactionExt::V0,
        };
        let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: vec![].try_into().unwrap(),
        });
        let envelope_xdr = envelope.to_xdr_base64().unwrap();

        let result = TransactionResult {
            fee_charged: 450,
            result: TransactionResultResult::TxSuccess(vec![].try_into().unwrap()),
            ext: stellar_xdr::curr::TransactionResultExt::V0,
        };
        let result_xdr = result.to_xdr_base64().unwrap();

        let meta = TransactionMeta::V3(TransactionMetaV3 {
            ext: ExtensionPoint::V0,
            tx_changes_before: vec![].try_into().unwrap(),
            operations: vec![].try_into().unwrap(),
            tx_changes_after: vec![].try_into().unwrap(),
            soroban_meta: Some(SorobanTransactionMeta {
                ext: SorobanTransactionMetaExt::V1(SorobanTransactionMetaExtV1 {
                    ext: ExtensionPoint::V0,
                    total_non_refundable_resource_fee_charged: 100,
                    total_refundable_resource_fee_charged: 200,
                    rent_fee_charged: 50,
                }),
                events: vec![].try_into().unwrap(),
                return_value: stellar_xdr::curr::ScVal::Void,
                diagnostic_events: vec![].try_into().unwrap(),
            }),
        });
        let meta_xdr = meta.to_xdr_base64().unwrap();

        let tx_data = serde_json::json!({
            "envelopeXdr": envelope_xdr,
            "resultXdr": result_xdr,
            "resultMetaXdr": meta_xdr,
        });

        let breakdown = extract_fee_breakdown(&tx_data);
        assert_eq!(breakdown.total_charged_fee, 450);
        assert_eq!(breakdown.bid_fee, Some(500));
        assert_eq!(breakdown.resource_fee, 350); // 100 + 200 + 50
        assert_eq!(breakdown.inclusion_fee, 100); // 450 - 350
        assert_eq!(breakdown.refundable_fee, 250); // 200 + 50
        assert_eq!(breakdown.non_refundable_fee, 100);
    }
}
