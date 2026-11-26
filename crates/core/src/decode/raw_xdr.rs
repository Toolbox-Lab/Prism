//! Offline decoding of raw base64 XDR, used by `grat decode --raw`.
//!
//! The network path fetches a transaction's `envelopeXdr`, `resultXdr` and
//! `resultMetaXdr` over RPC and passes them to [`MultiOpDecoder`]. With
//! `--raw` the user pastes those blobs instead, so this module works out what
//! each blob is and runs the same diagnosis without any network access:
//!
//! - envelope and result (meta optional): the full per-operation pipeline,
//!   exactly as for a transaction fetched by hash
//! - result alone: the failure explained from its result code, with likely
//!   causes, suggested fixes and the fee charged
//! - envelope alone or meta alone: a summary of what the blob contains and
//!   what else is needed to explain a failure
//!
//! Blobs can be given in any order, separated by whitespace or commas.

use serde_json::{json, Value};
use stellar_xdr::curr::{
    HostFunction, InnerTransactionResultResult, Operation, OperationBody, OperationResult,
    TransactionEnvelope, TransactionMeta, TransactionResult, TransactionResultResult,
};

use crate::decode::multi_op_decoder::{envelope_operations, inner_result_code, MultiOpDecoder};
use crate::decode::result_guidance::add_guidance;
use crate::error::{GratError, GratResult};
use crate::types::report::{
    DiagnosticReport, ResourceSummary, Severity, SuggestedFix, TransactionContext,
};
use crate::xdr::codec::{decode_xdr_base64, XdrCodec};

/// Shown where a transaction hash would go. Raw XDR does not carry its hash,
/// and computing it needs the network passphrase.
pub const RAW_XDR_LABEL: &str = "raw-xdr";

/// What a raw XDR blob decoded as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawXdrKind {
    Envelope,
    Result,
    Meta,
}

impl RawXdrKind {
    #[must_use]
    pub fn type_name(self) -> &'static str {
        match self {
            Self::Envelope => "TransactionEnvelope",
            Self::Result => "TransactionResult",
            Self::Meta => "TransactionMeta",
        }
    }
}

/// The blobs from the command line, sorted by what they decoded as.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RawXdrInput {
    pub envelope: Option<String>,
    pub result: Option<String>,
    pub meta: Option<String>,
}

impl RawXdrInput {
    /// The same field names RPC `getTransaction` uses, so the existing
    /// analyzers can read raw input unchanged.
    fn to_tx_data(&self) -> Value {
        let mut tx_data = json!({ "hash": RAW_XDR_LABEL, "ledger": 0 });
        if let Some(envelope) = &self.envelope {
            tx_data["envelopeXdr"] = json!(envelope);
        }
        if let Some(result) = &self.result {
            tx_data["resultXdr"] = json!(result);
        }
        if let Some(meta) = &self.meta {
            tx_data["resultMetaXdr"] = json!(meta);
        }
        tx_data
    }
}

/// Work out whether a base64 blob is a transaction envelope, result or meta.
pub fn classify_raw_xdr(blob: &str) -> GratResult<RawXdrKind> {
    // Report bad base64 on its own, before trying any XDR type.
    decode_xdr_base64(blob)?;
    if TransactionEnvelope::from_xdr_base64(blob).is_ok() {
        return Ok(RawXdrKind::Envelope);
    }
    if TransactionResult::from_xdr_base64(blob).is_ok() {
        return Ok(RawXdrKind::Result);
    }
    if TransactionMeta::from_xdr_base64(blob).is_ok() {
        return Ok(RawXdrKind::Meta);
    }
    Err(GratError::XdrError(
        "Input is valid base64 but is not a TransactionEnvelope, TransactionResult or \
         TransactionMeta XDR"
            .to_string(),
    ))
}

/// Split the input into blobs and sort them by kind.
pub fn parse_raw_input(input: &str) -> GratResult<RawXdrInput> {
    let mut parsed = RawXdrInput::default();
    let mut blob_count = 0usize;

    for blob in input
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|blob| !blob.is_empty())
    {
        blob_count += 1;
        let kind = classify_raw_xdr(blob)?;
        let slot = match kind {
            RawXdrKind::Envelope => &mut parsed.envelope,
            RawXdrKind::Result => &mut parsed.result,
            RawXdrKind::Meta => &mut parsed.meta,
        };
        if slot.is_some() {
            return Err(GratError::XdrError(format!(
                "More than one {} XDR was provided; pass at most one envelope, one result and one meta",
                kind.type_name()
            )));
        }
        *slot = Some(blob.to_string());
    }

    if blob_count == 0 {
        return Err(GratError::XdrError(
            "No XDR was provided; pass base64 XDR for the transaction envelope, result and/or meta"
                .to_string(),
        ));
    }
    Ok(parsed)
}

/// Decode raw base64 XDR into diagnostic reports without any network access.
pub fn decode_raw_xdr(input: &str) -> GratResult<Vec<DiagnosticReport>> {
    let parsed = parse_raw_input(input)?;
    let tx_data = parsed.to_tx_data();

    match (&parsed.envelope, &parsed.result, &parsed.meta) {
        (Some(_), Some(_), _) => {
            let reports = MultiOpDecoder::new().decode_transaction(&tx_data)?;
            if reports.is_empty() {
                return Err(GratError::XdrError(
                    "The transaction envelope contains no operations to diagnose".to_string(),
                ));
            }
            Ok(reports)
        }
        (None, Some(result), _) => Ok(vec![result_report(result, &tx_data)?]),
        (Some(envelope), None, meta) => {
            Ok(vec![envelope_report(envelope, &tx_data, meta.is_some())?])
        }
        (None, None, Some(meta)) => Ok(vec![meta_report(meta)?]),
        (None, None, None) => Err(GratError::XdrError("No XDR was provided".to_string())),
    }
}

/// The transaction-level code and the operation results it carries, looking
/// through a fee bump to the inner transaction.
struct ResultOutline<'a> {
    code: &'static str,
    operations: &'a [OperationResult],
    fee_bump: bool,
}

fn outline(result: &TransactionResultResult) -> ResultOutline<'_> {
    match result {
        TransactionResultResult::TxFeeBumpInnerSuccess(pair)
        | TransactionResultResult::TxFeeBumpInnerFailed(pair) => {
            let operations: &[OperationResult] = match &pair.result.result {
                InnerTransactionResultResult::TxSuccess(results)
                | InnerTransactionResultResult::TxFailed(results) => results.as_ref(),
                _ => &[],
            };
            ResultOutline {
                code: pair.result.result.name(),
                operations,
                fee_bump: true,
            }
        }
        TransactionResultResult::TxSuccess(results)
        | TransactionResultResult::TxFailed(results) => ResultOutline {
            code: result.name(),
            operations: results.as_ref(),
            fee_bump: false,
        },
        other => ResultOutline {
            code: other.name(),
            operations: &[],
            fee_bump: false,
        },
    }
}

/// `(operation type, result code)` for one operation result, for example
/// `("InvokeHostFunction", "Trapped")` or `("Operation", "OpBadAuth")`.
fn describe_operation_result(result: &OperationResult) -> (String, String) {
    match result {
        OperationResult::OpInner(inner) => (
            inner.name().to_string(),
            inner_result_code(inner).unwrap_or_else(|| "UnknownResult".to_string()),
        ),
        other => ("Operation".to_string(), other.name().to_string()),
    }
}

/// Category for a failing code, matching the categories the network path uses.
fn result_category(operation_type: Option<&str>, code: &str) -> &'static str {
    match operation_type {
        None => "Transaction",
        Some("InvokeHostFunction" | "ExtendFootprintTtl" | "RestoreFootprint") => match code {
            "ResourceLimitExceeded" => "Budget",
            "EntryArchived" => "Storage",
            "Malformed" | "InsufficientRefundableFee" => "Context",
            _ => "Soroban",
        },
        Some(_) => "ClassicStellar",
    }
}

fn empty_resources() -> ResourceSummary {
    ResourceSummary {
        cpu_instructions_used: 0,
        cpu_instructions_limit: 0,
        memory_bytes_used: 0,
        memory_bytes_limit: 0,
        read_bytes: 0,
        read_bytes_limit: 0,
        write_bytes: 0,
    }
}

fn result_report(result_xdr: &str, tx_data: &Value) -> GratResult<DiagnosticReport> {
    let result = TransactionResult::from_xdr_base64(result_xdr)?;
    let outline = outline(&result.result);
    let operations: Vec<(String, String)> = outline
        .operations
        .iter()
        .map(describe_operation_result)
        .collect();
    let succeeded = outline.code == "TxSuccess";
    let failing = if succeeded {
        None
    } else {
        operations.iter().position(|(_, code)| code != "Success")
    };

    let (operation_type, name) = match failing {
        Some(index) => (
            Some(operations[index].0.as_str()),
            operations[index].1.clone(),
        ),
        None => (None, outline.code.to_string()),
    };
    let category = if succeeded {
        "Transaction"
    } else {
        result_category(operation_type, &name)
    };

    let summary = if succeeded {
        format!(
            "Transaction succeeded ({} operation(s)); {} stroops fee charged",
            operations.len(),
            result.fee_charged
        )
    } else {
        let what = match (failing, operation_type) {
            (Some(index), Some(kind)) => format!(
                "Operation {}/{} ({kind}) failed with {name}",
                index + 1,
                operations.len()
            ),
            _ => format!("Transaction failed with {name}"),
        };
        match crate::decode::result_guidance::guidance_for(&name) {
            Some(guidance) => format!("{what}: {}", guidance.summary),
            None => what,
        }
    };

    let mut report = DiagnosticReport::new(category, 0, &name, &summary);
    report.severity = if succeeded {
        Severity::Info
    } else {
        Severity::Error
    };

    let mut explanation = format!(
        "Decoded offline from TransactionResult XDR.\n\n- Transaction result: {}{}\n- Fee charged: {} stroops",
        outline.code,
        if outline.fee_bump {
            " (inner transaction of a fee bump)"
        } else {
            ""
        },
        result.fee_charged
    );
    for (index, (kind, code)) in operations.iter().enumerate() {
        explanation.push_str(&format!("\n- Operation {}: {kind} -> {code}", index + 1));
    }
    if !succeeded {
        explanation.push_str(
            "\n\nThe result XDR alone has no diagnostic events or resource usage. Pass the \
             envelope and result meta as well, or decode by transaction hash, for the full \
             per-operation diagnosis.",
        );
    }
    report.detailed_explanation = explanation;

    if !operations.is_empty() {
        report.operation_count = Some(operations.len());
        report.operation_index = failing;
    }
    report.transaction_context = Some(TransactionContext {
        tx_hash: RAW_XDR_LABEL.to_string(),
        ledger_sequence: 0,
        function_name: operation_type.map(str::to_string),
        arguments: Vec::new(),
        return_value: None,
        fee: crate::decode::fee_analyzer::analyze_fee_breakdown(tx_data),
        resources: empty_resources(),
        operation_index: report.operation_index,
        operation_count: report.operation_count,
    });

    add_guidance(&mut report);
    Ok(report)
}

/// A readable label for an envelope operation, naming the contract function
/// for contract calls.
fn operation_label(operation: &Operation) -> String {
    match &operation.body {
        OperationBody::InvokeHostFunction(invoke) => match &invoke.host_function {
            HostFunction::InvokeContract(args) => {
                format!("InvokeHostFunction ({})", args.function_name.to_string())
            }
            function => format!("InvokeHostFunction ({})", function.name()),
        },
        body => body.name().to_string(),
    }
}

fn envelope_report(
    envelope_xdr: &str,
    tx_data: &Value,
    has_meta: bool,
) -> GratResult<DiagnosticReport> {
    let envelope = TransactionEnvelope::from_xdr_base64(envelope_xdr)?;
    let operations = envelope_operations(&envelope);
    let labels: Vec<String> = operations.iter().map(operation_label).collect();

    let mut report = DiagnosticReport::new(
        "Transaction",
        0,
        "EnvelopeOnly",
        &format!(
            "Decoded a transaction envelope with {} operation(s); no result XDR was supplied, so \
             the outcome is unknown",
            labels.len()
        ),
    );
    report.severity = Severity::Info;

    let mut explanation = String::from("Decoded offline from TransactionEnvelope XDR.\n");
    for (index, label) in labels.iter().enumerate() {
        explanation.push_str(&format!("\n- Operation {}: {label}", index + 1));
    }
    if has_meta {
        explanation.push_str(
            "\n\nA result meta was also supplied, but the operations can only be matched to \
             their outcomes with the result XDR.",
        );
    }
    report.detailed_explanation = explanation;
    report.operation_count = Some(labels.len());

    let first_call = operations
        .iter()
        .find_map(|operation| match &operation.body {
            OperationBody::InvokeHostFunction(invoke) => match &invoke.host_function {
                HostFunction::InvokeContract(args) => Some(args.function_name.to_string()),
                _ => None,
            },
            _ => None,
        });
    report.transaction_context = Some(TransactionContext {
        tx_hash: RAW_XDR_LABEL.to_string(),
        ledger_sequence: 0,
        function_name: first_call,
        arguments: Vec::new(),
        return_value: None,
        fee: crate::decode::fee_analyzer::analyze_fee_breakdown(tx_data),
        resources: empty_resources(),
        operation_index: None,
        operation_count: Some(labels.len()),
    });

    report.suggested_fixes.push(SuggestedFix {
        description: "Pass the transaction's result XDR together with the envelope, or decode it \
                      by hash, to see whether and why it failed"
            .to_string(),
        difficulty: "easy".to_string(),
        requires_upgrade: false,
        example: Some("grat decode --raw \"<envelope-xdr> <result-xdr> <meta-xdr>\"".to_string()),
        id: "supply_result_xdr".to_string(),
        remedy_code: None,
    });
    Ok(report)
}

fn meta_report(meta_xdr: &str) -> GratResult<DiagnosticReport> {
    let meta = TransactionMeta::from_xdr_base64(meta_xdr)?;
    let soroban_meta = match meta {
        TransactionMeta::V3(v3) => v3.soroban_meta,
        _ => None,
    };

    let mut report = DiagnosticReport::new(
        "Transaction",
        0,
        "MetaOnly",
        "Decoded transaction result meta; no result XDR was supplied, so the outcome is unknown",
    );
    report.severity = Severity::Info;

    let mut explanation = String::from("Decoded offline from TransactionMeta XDR.\n");
    match soroban_meta {
        Some(soroban) => {
            explanation.push_str(&format!(
                "\n- Contract events: {}\n- Diagnostic events: {}",
                soroban.events.len(),
                soroban.diagnostic_events.len()
            ));
            let encoded: Vec<String> = soroban
                .diagnostic_events
                .iter()
                .filter_map(|event| event.to_xdr_base64().ok())
                .collect();
            if !encoded.is_empty() {
                crate::decode::diagnostic::enrich_report(
                    &mut report,
                    &json!({ "diagnosticEventsXdr": encoded }),
                )?;
            }
            if !report.root_causes.is_empty() {
                report.severity = Severity::Warning;
            }
        }
        None => explanation.push_str("\nThis meta has no Soroban section (a classic transaction)."),
    }
    if report.detailed_explanation.is_empty() {
        report.detailed_explanation = explanation;
    } else {
        report.detailed_explanation = format!("{explanation}\n\n{}", report.detailed_explanation);
    }

    report.suggested_fixes.push(SuggestedFix {
        description: "Pass the transaction's envelope and result XDR together with the meta, or \
                      decode it by hash, for a full diagnosis"
            .to_string(),
        difficulty: "easy".to_string(),
        requires_upgrade: false,
        example: Some("grat decode --raw \"<envelope-xdr> <result-xdr> <meta-xdr>\"".to_string()),
        id: "supply_result_xdr".to_string(),
        remedy_code: None,
    });
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        ExtensionPoint, Hash, InflationResult, InnerTransactionResult, InnerTransactionResultExt,
        InnerTransactionResultPair, InvokeContractArgs, InvokeHostFunctionOp,
        InvokeHostFunctionResult, Memo, MuxedAccount, OperationResultTr, Preconditions, ScAddress,
        ScSymbol, ScVal, SequenceNumber, SorobanTransactionMeta, SorobanTransactionMetaExt,
        Transaction, TransactionExt, TransactionMetaV3, TransactionResultExt,
        TransactionV1Envelope, Uint256,
    };

    fn invoke_envelope_xdr() -> String {
        let operation = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(Hash([2; 32])),
                    function_name: ScSymbol("transfer".try_into().unwrap()),
                    args: vec![ScVal::U32(7)].try_into().unwrap(),
                }),
                auth: vec![].try_into().unwrap(),
            }),
        };
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([1; 32])),
            fee: 500,
            seq_num: SequenceNumber(42),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![operation].try_into().unwrap(),
            ext: TransactionExt::V0,
        };
        TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: vec![].try_into().unwrap(),
        })
        .to_xdr_base64()
        .unwrap()
    }

    fn result_xdr(result: TransactionResultResult, fee_charged: i64) -> String {
        TransactionResult {
            fee_charged,
            result,
            ext: TransactionResultExt::V0,
        }
        .to_xdr_base64()
        .unwrap()
    }

    fn invoke_result(result: InvokeHostFunctionResult) -> OperationResult {
        OperationResult::OpInner(OperationResultTr::InvokeHostFunction(result))
    }

    fn trapped_result_xdr() -> String {
        result_xdr(
            TransactionResultResult::TxFailed(
                vec![invoke_result(InvokeHostFunctionResult::Trapped)]
                    .try_into()
                    .unwrap(),
            ),
            450,
        )
    }

    fn soroban_meta_xdr() -> String {
        TransactionMeta::V3(TransactionMetaV3 {
            ext: ExtensionPoint::V0,
            tx_changes_before: vec![].try_into().unwrap(),
            operations: vec![].try_into().unwrap(),
            tx_changes_after: vec![].try_into().unwrap(),
            soroban_meta: Some(SorobanTransactionMeta {
                ext: SorobanTransactionMetaExt::V0,
                events: vec![].try_into().unwrap(),
                return_value: ScVal::Void,
                diagnostic_events: vec![].try_into().unwrap(),
            }),
        })
        .to_xdr_base64()
        .unwrap()
    }

    #[test]
    fn classifies_each_kind_of_blob() {
        assert_eq!(
            classify_raw_xdr(&invoke_envelope_xdr()).unwrap(),
            RawXdrKind::Envelope
        );
        assert_eq!(
            classify_raw_xdr(&trapped_result_xdr()).unwrap(),
            RawXdrKind::Result
        );
        assert_eq!(
            classify_raw_xdr(&soroban_meta_xdr()).unwrap(),
            RawXdrKind::Meta
        );
    }

    #[test]
    fn rejects_invalid_base64_and_unknown_xdr() {
        let bad_base64 = classify_raw_xdr("not base64!").unwrap_err().to_string();
        assert!(bad_base64.contains("Base64"), "{bad_base64}");

        // Valid base64, but three bytes cannot be any of the supported types.
        let unknown = classify_raw_xdr("AAAA").unwrap_err().to_string();
        assert!(unknown.contains("not a TransactionEnvelope"), "{unknown}");
    }

    #[test]
    fn parses_blobs_in_any_order_with_spaces_or_commas() {
        let envelope = invoke_envelope_xdr();
        let result = trapped_result_xdr();
        let meta = soroban_meta_xdr();

        let parsed = parse_raw_input(&format!(" {meta},{envelope}\n{result} ")).unwrap();

        assert_eq!(parsed.envelope.as_deref(), Some(envelope.as_str()));
        assert_eq!(parsed.result.as_deref(), Some(result.as_str()));
        assert_eq!(parsed.meta.as_deref(), Some(meta.as_str()));
    }

    #[test]
    fn rejects_empty_input_and_duplicate_kinds() {
        assert!(parse_raw_input("  , ").is_err());

        let result = trapped_result_xdr();
        let duplicate = parse_raw_input(&format!("{result} {result}"))
            .unwrap_err()
            .to_string();
        assert!(
            duplicate.contains("More than one TransactionResult"),
            "{duplicate}"
        );
    }

    #[test]
    fn result_only_explains_a_trapped_contract_call() {
        let reports = decode_raw_xdr(&trapped_result_xdr()).unwrap();
        assert_eq!(reports.len(), 1);
        let report = &reports[0];

        assert_eq!(report.error_name, "Trapped");
        assert_eq!(report.error_category, "Soroban");
        assert_eq!(report.severity, Severity::Error);
        assert_eq!(report.operation_index, Some(0));
        assert!(report
            .summary
            .contains("Operation 1/1 (InvokeHostFunction) failed"));
        assert!(!report.root_causes.is_empty());
        assert!(report
            .suggested_fixes
            .iter()
            .any(|fix| fix.id == "inspect_diagnostic_events"));
        let context = report.transaction_context.as_ref().unwrap();
        assert_eq!(context.fee.total_charged_fee, 450);
        assert_eq!(context.tx_hash, RAW_XDR_LABEL);
    }

    #[test]
    fn result_only_maps_soroban_codes_to_their_categories() {
        for (code, expected_name, expected_category) in [
            (
                InvokeHostFunctionResult::ResourceLimitExceeded,
                "ResourceLimitExceeded",
                "Budget",
            ),
            (
                InvokeHostFunctionResult::EntryArchived,
                "EntryArchived",
                "Storage",
            ),
            (
                InvokeHostFunctionResult::InsufficientRefundableFee,
                "InsufficientRefundableFee",
                "Context",
            ),
        ] {
            let xdr = result_xdr(
                TransactionResultResult::TxFailed(vec![invoke_result(code)].try_into().unwrap()),
                100,
            );
            let report = &decode_raw_xdr(&xdr).unwrap()[0];
            assert_eq!(report.error_name, expected_name);
            assert_eq!(report.error_category, expected_category);
            assert!(!report.suggested_fixes.is_empty(), "{expected_name}");
        }
    }

    #[test]
    fn result_only_explains_transaction_level_failures() {
        let report =
            &decode_raw_xdr(&result_xdr(TransactionResultResult::TxBadSeq, 100)).unwrap()[0];

        assert_eq!(report.error_name, "TxBadSeq");
        assert_eq!(report.error_category, "Transaction");
        assert!(report.summary.contains("sequence number"));
        assert!(report
            .suggested_fixes
            .iter()
            .any(|fix| fix.id == "refresh_sequence_number"));
        assert_eq!(report.operation_count, None);
    }

    #[test]
    fn result_only_looks_through_fee_bumps() {
        let inner = InnerTransactionResultPair {
            transaction_hash: Hash([9; 32]),
            result: InnerTransactionResult {
                fee_charged: 300,
                result: InnerTransactionResultResult::TxFailed(
                    vec![invoke_result(InvokeHostFunctionResult::EntryArchived)]
                        .try_into()
                        .unwrap(),
                ),
                ext: InnerTransactionResultExt::V0,
            },
        };
        let xdr = result_xdr(TransactionResultResult::TxFeeBumpInnerFailed(inner), 400);

        let report = &decode_raw_xdr(&xdr).unwrap()[0];

        assert_eq!(report.error_name, "EntryArchived");
        assert!(report.detailed_explanation.contains("fee bump"));
        assert!(report
            .detailed_explanation
            .contains("Fee charged: 400 stroops"));
    }

    #[test]
    fn result_only_reports_success_as_info() {
        let xdr = result_xdr(
            TransactionResultResult::TxSuccess(
                vec![OperationResult::OpInner(OperationResultTr::Inflation(
                    InflationResult::Success(vec![].try_into().unwrap()),
                ))]
                .try_into()
                .unwrap(),
            ),
            100,
        );

        let report = &decode_raw_xdr(&xdr).unwrap()[0];

        assert_eq!(report.severity, Severity::Info);
        assert_eq!(report.error_name, "TxSuccess");
        assert!(report.root_causes.is_empty());
        assert!(report.suggested_fixes.is_empty());
    }

    #[test]
    fn envelope_and_result_run_the_full_pipeline() {
        let input = format!("{} {}", trapped_result_xdr(), invoke_envelope_xdr());

        let reports = decode_raw_xdr(&input).unwrap();

        assert_eq!(reports.len(), 1);
        let report = &reports[0];
        assert_eq!(report.error_name, "Trapped");
        assert_eq!(report.severity, Severity::Error);
        let context = report.transaction_context.as_ref().unwrap();
        assert_eq!(context.function_name.as_deref(), Some("transfer"));
        assert_eq!(context.fee.bid_fee, Some(500));
        assert_eq!(context.fee.total_charged_fee, 450);
        assert!(report
            .suggested_fixes
            .iter()
            .any(|fix| fix.id == "inspect_diagnostic_events"));
    }

    #[test]
    fn envelope_only_lists_operations_and_asks_for_the_result() {
        let report = &decode_raw_xdr(&invoke_envelope_xdr()).unwrap()[0];

        assert_eq!(report.error_name, "EnvelopeOnly");
        assert_eq!(report.severity, Severity::Info);
        assert!(report
            .detailed_explanation
            .contains("Operation 1: InvokeHostFunction (transfer)"));
        assert_eq!(report.suggested_fixes[0].id, "supply_result_xdr");
        let context = report.transaction_context.as_ref().unwrap();
        assert_eq!(context.fee.bid_fee, Some(500));
        assert_eq!(context.function_name.as_deref(), Some("transfer"));
    }

    #[test]
    fn meta_only_summarizes_events() {
        let report = &decode_raw_xdr(&soroban_meta_xdr()).unwrap()[0];

        assert_eq!(report.error_name, "MetaOnly");
        assert_eq!(report.severity, Severity::Info);
        assert!(report.detailed_explanation.contains("Diagnostic events: 0"));
        assert_eq!(report.suggested_fixes[0].id, "supply_result_xdr");
    }
}
