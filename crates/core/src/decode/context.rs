use crate::decode::auth::{AuthChain, AuthCredential};
use crate::decode::auth_signature::decode_auth_entry_signatures;
use crate::decode::fee_analyzer::analyze_fee_breakdown;
use crate::error::GratResult;
use crate::types::report::{
    AuthEntryInfo, DiagnosticReport, FeeBreakdown, ResourceSummary, TransactionContext,
};

pub fn enrich_report(report: &mut DiagnosticReport, tx_data: &serde_json::Value) -> GratResult<()> {
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

    report.auth_signatures = extract_auth_signatures(tx_data);

    report.auth_entries = extract_auth_entries(tx_data);

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
    analyze_fee_breakdown(tx_data)
}

fn extract_resource_summary(tx_data: &serde_json::Value) -> ResourceSummary {
    let meta = crate::decode::resource_analyzer::TransactionResultMeta::from_tx_data(tx_data);
    ResourceSummary {
        cpu_instructions_used: meta.resources_consumed.cpu_instructions,
        cpu_instructions_limit: meta.resources_allocated.cpu_instructions,
        memory_bytes_used: meta.resources_consumed.memory_bytes,
        memory_bytes_limit: meta.resources_allocated.memory_bytes,
        read_bytes: meta.resources_consumed.read_bytes,
        read_bytes_limit: meta.resources_allocated.read_bytes,
        write_bytes: meta.resources_consumed.write_bytes,
    }
}

fn extract_auth_signatures(tx_data: &serde_json::Value) -> Vec<String> {
    let mut signatures = Vec::new();

    if let Some(auth_array) = tx_data.get("auth").and_then(|a| a.as_array()) {
        for entry in auth_array {
            if let Some(xdr_b64) = entry.as_str() {
                signatures.extend(decode_auth_entry_signatures(xdr_b64));
            }
        }
    }

    signatures
}

fn extract_auth_entries(tx_data: &serde_json::Value) -> Vec<AuthEntryInfo> {
    let mut entries = Vec::new();

    if let Some(auth_array) = tx_data.get("auth").and_then(|a| a.as_array()) {
        for entry in auth_array {
            if let Some(xdr_b64) = entry.as_str() {
                if let Ok(chain) = AuthChain::from_xdr_base64(xdr_b64) {
                    if let Some(info) = auth_entry_info_from_chain(&chain) {
                        entries.push(info);
                    }
                }
            }
        }
    }

    entries
}

fn auth_entry_info_from_chain(chain: &AuthChain) -> Option<AuthEntryInfo> {
    match &chain.credential {
        AuthCredential::SourceAccount => None,
        AuthCredential::Address(cred) => Some(AuthEntryInfo {
            auth_type: cred.auth_type.to_string(),
            address: cred.address.clone(),
            contract_id: cred.contract_id.clone(),
        }),
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
    fn test_extract_resource_summary_read() {
        let tx_data = serde_json::json!({
            "diagnosticEvents": [
                {
                    "type": "budget",
                    "data": {
                        "category": "read",
                        "used": 12345,
                        "limit": 100000
                    }
                }
            ]
        });
        let result = extract_resource_summary(&tx_data);
        assert_eq!(result.read_bytes, 12345);
        assert_eq!(result.read_bytes_limit, 100000);
        assert_eq!(result.cpu_instructions_used, 0);
        assert_eq!(result.memory_bytes_used, 0);
    }

    #[test]
    fn test_extract_resource_summary_empty() {
        let tx_data = serde_json::json!({});
        let result = extract_resource_summary(&tx_data);
        assert_eq!(result.read_bytes, 0);
        assert_eq!(result.read_bytes_limit, 0);
        assert_eq!(result.cpu_instructions_used, 0);
        assert_eq!(result.memory_bytes_used, 0);
    }

    #[test]
    fn test_extract_resource_summary_cpu_regression() {
        let tx_data = serde_json::json!({
            "diagnosticEvents": [
                {
                    "type": "budget",
                    "data": {
                        "category": "cpu",
                        "used": 5000,
                        "limit": 10000
                    }
                }
            ]
        });
        let result = extract_resource_summary(&tx_data);
        assert_eq!(result.cpu_instructions_used, 5000);
        assert_eq!(result.cpu_instructions_limit, 10000);
        assert_eq!(result.read_bytes, 0);
        assert_eq!(result.read_bytes_limit, 0);
    }

    #[test]
    fn test_extract_resource_summary_unknown_category() {
        let tx_data = serde_json::json!({
            "diagnosticEvents": [
                {
                    "type": "budget",
                    "data": {
                        "category": "unknown_category",
                        "used": 999,
                        "limit": 9999
                    }
                }
            ]
        });
        let result = extract_resource_summary(&tx_data);
        assert_eq!(result.read_bytes, 0);
        assert_eq!(result.read_bytes_limit, 0);
        assert_eq!(result.cpu_instructions_used, 0);
        assert_eq!(result.memory_bytes_used, 0);
    }

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
        assert_eq!(breakdown.refundable_resource_fee, 0);
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
        assert_eq!(breakdown.resource_fee, 350);
        assert_eq!(breakdown.inclusion_fee, 100);
        assert_eq!(breakdown.refundable_resource_fee, 200);
        assert_eq!(breakdown.refundable_fee, 250);
        assert_eq!(breakdown.non_refundable_fee, 100);
    }

    fn ed25519_auth_entry_b64(nonce: i64) -> String {
        use stellar_xdr::curr::{
            AccountId, Hash, InvokeContractArgs, PublicKey, ScAddress, ScSymbol, ScVal,
            SorobanAddressCredentials, SorobanAuthorizationEntry, SorobanAuthorizedFunction,
            SorobanAuthorizedInvocation, SorobanCredentials, Uint256,
        };
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(
                    [3u8; 32],
                )))),
                nonce,
                signature_expiration_ledger: 100,
                signature: ScVal::Void,
            }),
            root_invocation: SorobanAuthorizedInvocation {
                function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                    contract_address: ScAddress::Contract(Hash([9u8; 32])),
                    function_name: ScSymbol("transfer".try_into().unwrap()),
                    args: vec![].try_into().unwrap(),
                }),
                sub_invocations: vec![].try_into().unwrap(),
            },
        };
        XdrCodec::to_xdr_base64(&entry).expect("encode")
    }

    fn smart_wallet_auth_entry_b64(nonce: i64) -> String {
        use stellar_xdr::curr::{
            Hash, InvokeContractArgs, ScAddress, ScSymbol, ScVal, SorobanAddressCredentials,
            SorobanAuthorizationEntry, SorobanAuthorizedFunction, SorobanAuthorizedInvocation,
            SorobanCredentials,
        };
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: ScAddress::Contract(Hash([5u8; 32])),
                nonce,
                signature_expiration_ledger: 200,
                signature: ScVal::Void,
            }),
            root_invocation: SorobanAuthorizedInvocation {
                function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                    contract_address: ScAddress::Contract(Hash([8u8; 32])),
                    function_name: ScSymbol("invoke".try_into().unwrap()),
                    args: vec![].try_into().unwrap(),
                }),
                sub_invocations: vec![].try_into().unwrap(),
            },
        };
        XdrCodec::to_xdr_base64(&entry).expect("encode")
    }

    #[test]
    fn extract_auth_entries_detects_ed25519() {
        let b64 = ed25519_auth_entry_b64(42);
        let tx_data = serde_json::json!({ "auth": [b64] });
        let entries = extract_auth_entries(&tx_data);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].auth_type, "Ed25519");
        assert!(entries[0].address.starts_with('G'));
        assert!(entries[0].contract_id.is_none());
    }

    #[test]
    fn extract_auth_entries_detects_smart_wallet() {
        let b64 = smart_wallet_auth_entry_b64(99);
        let tx_data = serde_json::json!({ "auth": [b64] });
        let entries = extract_auth_entries(&tx_data);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].auth_type, "Smart Wallet");
        assert!(entries[0].address.starts_with('C'));
        let contract_id = entries[0]
            .contract_id
            .as_deref()
            .expect("smart wallet must have contract_id");
        assert_eq!(contract_id, entries[0].address);
    }

    #[test]
    fn extract_auth_entries_handles_multiple_entries() {
        let b64_ed = ed25519_auth_entry_b64(1);
        let b64_sw = smart_wallet_auth_entry_b64(2);
        let tx_data = serde_json::json!({ "auth": [b64_ed, b64_sw] });
        let entries = extract_auth_entries(&tx_data);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].auth_type, "Ed25519");
        assert_eq!(entries[1].auth_type, "Smart Wallet");
    }

    #[test]
    fn extract_auth_entries_skips_invalid_payloads() {
        let tx_data = serde_json::json!({ "auth": ["!!!not-valid-xdr!!!"] });
        let entries = extract_auth_entries(&tx_data);

        assert!(entries.is_empty());
    }

    #[test]
    fn extract_auth_entries_empty_when_no_auth_field() {
        let tx_data = serde_json::json!({ "hash": "abc123" });
        let entries = extract_auth_entries(&tx_data);
        assert!(entries.is_empty());
    }

    #[test]
    fn existing_ed25519_decoding_unchanged() {
        let b64 = ed25519_auth_entry_b64(7);
        let tx_data = serde_json::json!({ "auth": [b64] });

        let sigs = extract_auth_signatures(&tx_data);
        assert!(sigs.is_empty(), "no signature bytes in void-signed entry");
    }
}
