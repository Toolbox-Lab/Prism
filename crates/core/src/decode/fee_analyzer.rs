use crate::error::GratResult;
use crate::types::report::FeeBreakdown;
use crate::xdr::codec::XdrCodec;
use stellar_xdr::curr::{TransactionEnvelope, TransactionMeta, TransactionResult};

pub fn analyze_fee_breakdown(tx_data: &serde_json::Value) -> FeeBreakdown {
    let total_fee = tx_data
        .get("resultXdr")
        .and_then(|v| v.as_str())
        .and_then(parse_total_fee)
        .unwrap_or(0);

    let bid_fee = tx_data
        .get("envelopeXdr")
        .and_then(|v| v.as_str())
        .and_then(parse_bid_fee)
        .or_else(|| tx_data.get("feeBid")?.as_i64());

    let (non_refundable_fee, refundable_resource_fee, rent_fee, has_soroban_resource_fee) = tx_data
        .get("resourceFee")
        .and_then(serde_json::Value::as_object)
        .map_or_else(
            || parse_resource_fee_from_meta(tx_data),
            parse_resource_fee_object,
        );

    let resource_fee = if has_soroban_resource_fee {
        non_refundable_fee + refundable_resource_fee + rent_fee
    } else {
        0
    };

    let inclusion_fee = tx_data
        .get("inclusionFee")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_else(|| {
            if resource_fee > 0 {
                total_fee.saturating_sub(resource_fee)
            } else {
                total_fee
            }
        });

    FeeBreakdown {
        total_charged_fee: total_fee,
        inclusion_fee,
        resource_fee,
        refundable_resource_fee,
        refundable_fee: refundable_resource_fee + rent_fee,
        non_refundable_fee,
        bid_fee,
    }
}

pub fn inject_fee_metadata(tx_data: &mut serde_json::Value) -> GratResult<()> {
    let breakdown = analyze_fee_breakdown(tx_data);

    tx_data["inclusionFee"] = serde_json::json!(breakdown.inclusion_fee);
    if breakdown.resource_fee > 0 {
        tx_data["resourceFee"] = serde_json::json!({
            "totalNonRefundableResourceFeeCharged": breakdown.non_refundable_fee,
            "totalRefundableResourceFeeCharged": breakdown.refundable_resource_fee,
            "rentFeeCharged": breakdown.refundable_fee.saturating_sub(breakdown.refundable_resource_fee),
        });
    }

    Ok(())
}

fn parse_total_fee(result_xdr_b64: &str) -> Option<i64> {
    TransactionResult::from_xdr_base64(result_xdr_b64)
        .ok()
        .map(|result| result.fee_charged)
}

fn parse_bid_fee(envelope_xdr_b64: &str) -> Option<i64> {
    let tx_envelope = TransactionEnvelope::from_xdr_base64(envelope_xdr_b64).ok()?;
    Some(match tx_envelope {
        TransactionEnvelope::Tx(v1) => i64::from(v1.tx.fee),
        TransactionEnvelope::TxFeeBump(fee_bump) => fee_bump.tx.fee,
        TransactionEnvelope::TxV0(v0) => i64::from(v0.tx.fee),
    })
}

fn parse_resource_fee_object(
    resource_fee_obj: &serde_json::Map<String, serde_json::Value>,
) -> (i64, i64, i64, bool) {
    (
        resource_fee_obj
            .get("totalNonRefundableResourceFeeCharged")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        resource_fee_obj
            .get("totalRefundableResourceFeeCharged")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        resource_fee_obj
            .get("rentFeeCharged")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        true,
    )
}

fn parse_resource_fee_from_meta(tx_data: &serde_json::Value) -> (i64, i64, i64, bool) {
    let Some(meta_xdr_b64) = tx_data.get("resultMetaXdr").and_then(|v| v.as_str()) else {
        return (0, 0, 0, false);
    };

    let Ok(TransactionMeta::V3(v3)) = TransactionMeta::from_xdr_base64(meta_xdr_b64) else {
        return (0, 0, 0, false);
    };

    let Some(soroban_meta) = v3.soroban_meta else {
        return (0, 0, 0, false);
    };

    match soroban_meta.ext {
        stellar_xdr::curr::SorobanTransactionMetaExt::V0 => (0, 0, 0, false),
        stellar_xdr::curr::SorobanTransactionMetaExt::V1(v1) => (
            v1.total_non_refundable_resource_fee_charged,
            v1.total_refundable_resource_fee_charged,
            v1.rent_fee_charged,
            true,
        ),
    }
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
    fn analyzes_non_soroban_fee_breakdown() {
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

        let breakdown = analyze_fee_breakdown(&tx_data);
        assert_eq!(breakdown.total_charged_fee, 120);
        assert_eq!(breakdown.bid_fee, Some(150));
        assert_eq!(breakdown.inclusion_fee, 120);
        assert_eq!(breakdown.resource_fee, 0);
        assert_eq!(breakdown.refundable_resource_fee, 0);
        assert_eq!(breakdown.refundable_fee, 0);
        assert_eq!(breakdown.non_refundable_fee, 0);
    }

    #[test]
    fn analyzes_soroban_fee_breakdown() {
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

        let breakdown = analyze_fee_breakdown(&tx_data);
        assert_eq!(breakdown.total_charged_fee, 450);
        assert_eq!(breakdown.bid_fee, Some(500));
        assert_eq!(breakdown.resource_fee, 350);
        assert_eq!(breakdown.inclusion_fee, 100);
        assert_eq!(breakdown.refundable_resource_fee, 200);
        assert_eq!(breakdown.refundable_fee, 250);
        assert_eq!(breakdown.non_refundable_fee, 100);
    }

    #[test]
    fn handles_insufficient_fee_surges() {
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([0; 32])),
            fee: 400,
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
            fee_charged: 600,
            result: TransactionResultResult::TxSuccess(vec![].try_into().unwrap()),
            ext: stellar_xdr::curr::TransactionResultExt::V0,
        };
        let result_xdr = result.to_xdr_base64().unwrap();

        let tx_data = serde_json::json!({
            "envelopeXdr": envelope_xdr,
            "resultXdr": result_xdr,
        });

        let breakdown = analyze_fee_breakdown(&tx_data);
        assert_eq!(breakdown.total_charged_fee, 600);
        assert_eq!(breakdown.bid_fee, Some(400));
        assert_eq!(breakdown.inclusion_fee, 600);
        assert_eq!(breakdown.resource_fee, 0);
        assert_eq!(breakdown.refundable_fee, 0);
    }
}
