use crate::decode::function_call_decoder::FunctionCallDecoder;
use crate::error::{GratError, GratResult};
use crate::network::config::NetworkConfig;
use crate::rpc::SorobanRpcClient;
use crate::types::report::{DiagnosticReport, Severity};
use crate::xdr::codec::XdrCodec;
use serde::Serialize;
use serde_json::Value;
use stellar_xdr::curr::{
    ContractEvent, ContractEventBody, DiagnosticEvent, FeeBumpTransactionInnerTx, HostFunction,
    InnerTransactionResultResult, Operation, OperationBody, OperationResult, OperationResultTr,
    ScAddress, ScVal, SorobanTransactionMeta, TransactionEnvelope, TransactionMeta,
    TransactionResult, TransactionResultResult,
};

pub struct MultiOpDecoder {
    function_call_decoder: FunctionCallDecoder,
}

struct DecodedOperation {
    // One report-facing representation per envelope operation.
    operation_name: String,
    display_name: String,
    arguments: Vec<String>,
    return_value: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct DecodedOperationResult {
    success: bool,
    attempted: bool,
    category: String,
    code: String,
}

fn decode_contract_call(
    decoder: &FunctionCallDecoder,
    args: &stellar_xdr::curr::InvokeContractArgs,
    value: Option<&ScVal>,
) -> DecodedOperation {
    let call = decoder.decode(
        &args.function_name.to_string(),
        args.args.as_ref(),
        None,
        value,
        None,
    );
    DecodedOperation {
        operation_name: "InvokeHostFunction".into(),
        display_name: call.function_name,
        arguments: format_arguments(call.arguments),
        return_value: call.formatted_return_value,
    }
}

struct TransactionResultSet<'a> {
    // Results stay aligned with the envelope by index.
    operation_results: &'a [OperationResult],
    transaction_failure: Option<&'static str>,
    committed: bool,
}

fn format_arguments(values: Vec<crate::decode::DecodedArgument>) -> Vec<String> {
    values.into_iter().map(format_argument).collect()
}

fn format_argument(value: crate::decode::DecodedArgument) -> String {
    format!("{}: {}", value.name, value.formatted)
}

fn decode_operation_payload(
    decoder: &FunctionCallDecoder,
    operation: &Operation,
    return_value: Option<&ScVal>,
) -> DecodedOperation {
    let OperationBody::InvokeHostFunction(invoke) = &operation.body else {
        return DecodedOperation {
            operation_name: operation.body.name().to_string(),
            display_name: operation.body.name().to_string(),
            arguments: extract_xdr_arguments(&operation.body),
            return_value: return_value.map(format_scval),
        };
    };
    match &invoke.host_function {
        HostFunction::InvokeContract(args) => decode_contract_call(decoder, args, return_value),
        function => DecodedOperation {
            operation_name: operation.body.name().to_string(),
            display_name: function.name().to_string(),
            arguments: extract_xdr_arguments(&operation.body),
            return_value: return_value.map(format_scval),
        },
    }
}

impl Default for MultiOpDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiOpDecoder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            function_call_decoder: FunctionCallDecoder::new(),
        }
    }

    /// Decode every operation in the envelope, preserving its original index.
    pub fn decode_transaction(&self, tx_data: &Value) -> GratResult<Vec<DiagnosticReport>> {
        let envelope = decode_required_xdr::<TransactionEnvelope>(tx_data, "envelopeXdr")?;
        let tx_result = decode_required_xdr::<TransactionResult>(tx_data, "resultXdr")?;
        let operations = envelope_operations(&envelope);
        if operations.is_empty() {
            return Ok(Vec::new());
        }
        let results = transaction_result_set(&tx_result.result);
        let meta = decode_soroban_meta(tx_data)?;
        let diagnostics = partition_diagnostic_events(
            meta.as_ref()
                .map(|m| m.diagnostic_events.as_ref())
                .unwrap_or_default(),
            operations,
        );
        let events = partition_contract_events(
            meta.as_ref().map(|m| m.events.as_ref()).unwrap_or_default(),
            operations,
        );
        let single_return = single_invoke_contract_index(operations)
            .and_then(|index| meta.as_ref().map(|m| (index, &m.return_value)));
        let resources =
            crate::decode::resource_analyzer::TransactionResultMeta::from_tx_data(tx_data);
        let summary = resource_summary(&resources);
        let fee = crate::decode::fee_analyzer::analyze_fee_breakdown(tx_data);

        operations
            .iter()
            .enumerate()
            .map(|(index, operation)| {
                let diagnostics = diagnostics
                    .get(index)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let return_value = return_value_from_events(diagnostics).or_else(|| {
                    single_return
                        .and_then(|(return_index, value)| (return_index == index).then_some(value))
                });
                Ok(build_operation_report(
                    tx_data,
                    operations.len(),
                    index,
                    operation,
                    self.decode_operation(operation, return_value),
                    decode_operation_result(
                        operation,
                        results.operation_results.get(index),
                        results.transaction_failure,
                    ),
                    results.committed,
                    diagnostics,
                    events.get(index).map(Vec::as_slice).unwrap_or_default(),
                    &resources,
                    fee.clone(),
                    summary.clone(),
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{InflationResult, VecM};

    fn operation(body: OperationBody) -> Operation {
        Operation {
            source_account: None,
            body,
        }
    }

    #[test]
    fn classic_success_is_not_reported_as_non_invoke_failure() {
        let operation = operation(OperationBody::Inflation);
        let result = OperationResult::OpInner(OperationResultTr::Inflation(
            InflationResult::Success(VecM::default()),
        ));
        let decoded = decode_operation_result(&operation, Some(&result), None);
        assert!(decoded.success);
        assert!(decoded.attempted);
        assert_eq!(decoded.category, "ClassicStellar");
        assert_eq!(decoded.code, "Success");
    }

    #[test]
    fn missing_result_is_not_fabricated_as_success() {
        let operation = operation(OperationBody::Inflation);
        let decoded = decode_operation_result(&operation, None, Some("TxBadSeq"));
        assert!(!decoded.success);
        assert!(!decoded.attempted);
        assert_eq!(decoded.code, "TxBadSeq");
    }

    #[test]
    fn result_type_must_match_envelope_operation() {
        let operation = operation(OperationBody::Inflation);
        let result = OperationResult::OpInner(OperationResultTr::InvokeHostFunction(
            stellar_xdr::curr::InvokeHostFunctionResult::Trapped,
        ));
        let decoded = decode_operation_result(&operation, Some(&result), Some("TxFailed"));
        assert!(!decoded.success);
        assert_eq!(decoded.category, "Xdr");
        assert_eq!(decoded.code, "OperationResultTypeMismatch");
    }
}

fn decode_required_xdr<T: XdrCodec>(tx_data: &Value, field: &'static str) -> GratResult<T> {
    let value = tx_data
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| GratError::Internal(format!("Missing {field} in transaction data")))?;
    T::from_xdr_base64(value)
        .map_err(|error| GratError::Internal(format!("Failed to decode {field}: {error}")))
}

fn envelope_operations(envelope: &TransactionEnvelope) -> &[Operation] {
    match envelope {
        TransactionEnvelope::TxV0(envelope) => envelope.tx.operations.as_ref(),
        TransactionEnvelope::Tx(envelope) => envelope.tx.operations.as_ref(),
        TransactionEnvelope::TxFeeBump(envelope) => match &envelope.tx.inner_tx {
            FeeBumpTransactionInnerTx::Tx(envelope) => envelope.tx.operations.as_ref(),
        },
    }
}

fn transaction_result_set(result: &TransactionResultResult) -> TransactionResultSet<'_> {
    match result {
        TransactionResultResult::TxSuccess(results) => TransactionResultSet {
            operation_results: results.as_ref(),
            transaction_failure: None,
            committed: true,
        },
        TransactionResultResult::TxFailed(results) => TransactionResultSet {
            operation_results: results.as_ref(),
            transaction_failure: Some("TxFailed"),
            committed: false,
        },
        TransactionResultResult::TxFeeBumpInnerSuccess(pair)
        | TransactionResultResult::TxFeeBumpInnerFailed(pair) => {
            inner_transaction_result_set(&pair.result.result)
        }
        result => TransactionResultSet {
            operation_results: &[],
            transaction_failure: Some(result.name()),
            committed: false,
        },
    }
}

fn inner_transaction_result_set(result: &InnerTransactionResultResult) -> TransactionResultSet<'_> {
    match result {
        InnerTransactionResultResult::TxSuccess(results) => TransactionResultSet {
            operation_results: results.as_ref(),
            transaction_failure: None,
            committed: true,
        },
        InnerTransactionResultResult::TxFailed(results) => TransactionResultSet {
            operation_results: results.as_ref(),
            transaction_failure: Some("TxFailed"),
            committed: false,
        },
        result => TransactionResultSet {
            operation_results: &[],
            transaction_failure: Some(result.name()),
            committed: false,
        },
    }
}

fn resource_summary(
    resources: &crate::decode::resource_analyzer::TransactionResultMeta,
) -> crate::types::report::ResourceSummary {
    crate::types::report::ResourceSummary {
        cpu_instructions_used: resources.resources_consumed.cpu_instructions,
        cpu_instructions_limit: resources.resources_allocated.cpu_instructions,
        memory_bytes_used: resources.resources_consumed.memory_bytes,
        memory_bytes_limit: resources.resources_allocated.memory_bytes,
        read_bytes: resources.resources_consumed.read_bytes,
        read_bytes_limit: resources.resources_allocated.read_bytes,
        write_bytes: resources.resources_consumed.write_bytes,
    }
}

fn decode_operation_result(
    operation: &Operation,
    result: Option<&OperationResult>,
    transaction_failure: Option<&str>,
) -> DecodedOperationResult {
    let default_category = if is_soroban_operation(operation) {
        "Soroban"
    } else {
        "ClassicStellar"
    };
    let Some(result) = result else {
        return DecodedOperationResult {
            success: false,
            attempted: false,
            category: default_category.to_string(),
            code: transaction_failure
                .unwrap_or("MissingOperationResult")
                .to_string(),
        };
    };
    match result {
        OperationResult::OpInner(inner) if inner.name() != operation.body.name() => {
            DecodedOperationResult {
                success: false,
                attempted: true,
                category: "Xdr".into(),
                code: "OperationResultTypeMismatch".into(),
            }
        }
        OperationResult::OpInner(inner) => {
            let code = inner_result_code(inner).unwrap_or_else(|| "UnknownResult".into());
            DecodedOperationResult {
                success: code == "Success",
                attempted: true,
                category: result_category(operation, &code).into(),
                code,
            }
        }
        error => DecodedOperationResult {
            success: false,
            attempted: true,
            category: default_category.into(),
            code: error.name().into(),
        },
    }
}

fn result_category(operation: &Operation, code: &str) -> &'static str {
    if !is_soroban_operation(operation) {
        return "ClassicStellar";
    }
    match code {
        "ResourceLimitExceeded" => "Budget",
        "EntryArchived" => "Storage",
        "Malformed" | "InsufficientRefundableFee" => "Context",
        _ => "Soroban",
    }
}

fn inner_result_code(result: &OperationResultTr) -> Option<String> {
    let serialized = serde_json::to_value(result).ok()?;
    let payload = match serialized {
        Value::Object(map) => map.into_iter().next()?.1,
        _ => return None,
    };
    let name = match payload {
        Value::String(name) => name,
        Value::Object(map) => map.into_iter().next()?.0,
        _ => return None,
    };
    Some(to_pascal_case(&name))
}

fn to_pascal_case(value: &str) -> String {
    let mut output = String::new();
    for part in value.split('_') {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            output.extend(first.to_uppercase());
            output.extend(chars);
        }
    }
    output
}

fn extract_xdr_arguments<T: Serialize>(value: &T) -> Vec<String> {
    let Ok(serialized) = serde_json::to_value(value) else {
        return Vec::new();
    };
    let payload = match serialized {
        Value::String(_) => return Vec::new(),
        Value::Object(map) if map.len() == 1 => map.into_iter().next().map(|entry| entry.1),
        value => Some(value),
    };
    match payload {
        Some(Value::Object(fields)) => fields
            .into_iter()
            .map(|(name, value)| format!("{name}: {}", compact_json(&value)))
            .collect(),
        Some(Value::Null) | None => Vec::new(),
        Some(value) => vec![compact_json(&value)],
    }
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn format_scval(value: &ScVal) -> String {
    compact_json(&crate::decode::scval_to_json(value))
}

fn is_soroban_operation(operation: &Operation) -> bool {
    matches!(
        operation.body,
        OperationBody::InvokeHostFunction(_)
            | OperationBody::ExtendFootprintTtl(_)
            | OperationBody::RestoreFootprint(_)
    )
}

fn is_invoke_contract(operation: &Operation) -> bool {
    matches!(operation.body, OperationBody::InvokeHostFunction(ref invoke)
        if matches!(invoke.host_function, HostFunction::InvokeContract(_)))
}

fn single_invoke_contract_index(operations: &[Operation]) -> Option<usize> {
    let mut indexes = operations
        .iter()
        .enumerate()
        .filter_map(|(index, operation)| is_invoke_contract(operation).then_some(index));
    let only = indexes.next()?;
    indexes.next().is_none().then_some(only)
}

fn decode_soroban_meta(tx_data: &Value) -> GratResult<Option<SorobanTransactionMeta>> {
    let Some(xdr) = tx_data.get("resultMetaXdr").and_then(Value::as_str) else {
        return Ok(None);
    };
    let meta = TransactionMeta::from_xdr_base64(xdr)
        .map_err(|error| GratError::Internal(format!("Failed to decode resultMetaXdr: {error}")))?;
    Ok(match meta {
        TransactionMeta::V3(meta) => meta.soroban_meta,
        TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_) => None,
    })
}

fn partition_diagnostic_events(
    events: &[DiagnosticEvent],
    operations: &[Operation],
) -> Vec<Vec<DiagnosticEvent>> {
    let mut partitions = vec![Vec::new(); operations.len()];
    if events.is_empty() {
        return partitions;
    }
    let invokes: Vec<usize> = operations
        .iter()
        .enumerate()
        .filter_map(|(i, operation)| is_invoke_contract(operation).then_some(i))
        .collect();
    let soroban: Vec<usize> = operations
        .iter()
        .enumerate()
        .filter_map(|(i, operation)| is_soroban_operation(operation).then_some(i))
        .collect();
    if soroban.len() == 1 {
        partitions[soroban[0]] = events.to_vec();
        return partitions;
    }

    let mut depth = 0usize;
    let mut cursor = 0usize;
    let mut active = None;
    for event in events {
        if diagnostic_event_has_topic(event, is_call_topic) {
            if depth == 0 {
                active = invokes.get(cursor).copied();
                if active.is_some() {
                    cursor += 1;
                }
            }
            depth += 1;
        }
        if let Some(index) = active {
            partitions[index].push(event.clone());
        }
        if diagnostic_event_has_topic(event, is_return_topic) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                active = None;
            }
        }
    }
    partitions
}

fn diagnostic_event_has_topic(event: &DiagnosticEvent, predicate: fn(&str) -> bool) -> bool {
    #[allow(irrefutable_let_patterns)]
    if let ContractEventBody::V0(body) = &event.event.body {
        return body.topics.iter().any(|topic| match topic {
            ScVal::Symbol(value) => predicate(&value.to_string()),
            ScVal::String(value) => predicate(&value.to_string()),
            _ => false,
        });
    }
    false
}

fn is_call_topic(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "fn_call" | "function_call" | "call"
    )
}

fn is_return_topic(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "fn_return" | "function_return" | "return"
    )
}

fn return_value_from_events(events: &[DiagnosticEvent]) -> Option<&ScVal> {
    events.iter().rev().find_map(|event| {
        if !diagnostic_event_has_topic(event, is_return_topic) {
            return None;
        }
        #[allow(irrefutable_let_patterns)]
        let ContractEventBody::V0(body) = &event.event.body;
        Some(&body.data)
    })
}

fn partition_contract_events(
    events: &[ContractEvent],
    operations: &[Operation],
) -> Vec<Vec<ContractEvent>> {
    let mut partitions = vec![Vec::new(); operations.len()];
    if events.is_empty() {
        return partitions;
    }
    let soroban: Vec<usize> = operations
        .iter()
        .enumerate()
        .filter_map(|(i, operation)| is_soroban_operation(operation).then_some(i))
        .collect();
    if soroban.len() == 1 {
        partitions[soroban[0]] = events.to_vec();
        return partitions;
    }
    for event in events {
        let Some(contract_id) = event.contract_id.as_ref() else {
            continue;
        };
        for (index, operation) in operations.iter().enumerate() {
            if invoked_contract_id(operation).is_some_and(|target| target == contract_id) {
                partitions[index].push(event.clone());
                break;
            }
        }
    }
    partitions
}

fn invoked_contract_id(operation: &Operation) -> Option<&stellar_xdr::curr::Hash> {
    let OperationBody::InvokeHostFunction(invoke) = &operation.body else {
        return None;
    };
    let HostFunction::InvokeContract(args) = &invoke.host_function else {
        return None;
    };
    match &args.contract_address {
        ScAddress::Contract(contract_id) => Some(contract_id),
        ScAddress::Account(_) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_operation_report(
    tx_data: &Value,
    count: usize,
    index: usize,
    operation: &Operation,
    decoded: DecodedOperation,
    result: DecodedOperationResult,
    transaction_committed: bool,
    diagnostics: &[DiagnosticEvent],
    events: &[ContractEvent],
    resources: &crate::decode::resource_analyzer::TransactionResultMeta,
    fee: crate::types::report::FeeBreakdown,
    resource_summary: crate::types::report::ResourceSummary,
) -> DiagnosticReport {
    let committed = transaction_committed && result.success;
    let outcome = if committed {
        "succeeded"
    } else if result.success {
        "was rolled back"
    } else if result.attempted {
        "failed"
    } else {
        "was not executed"
    };
    let mut report = DiagnosticReport::new(
        &result.category,
        0,
        &result.code,
        &format!(
            "Operation {}/{} ({}) {}",
            index + 1,
            count,
            decoded.operation_name,
            outcome
        ),
    );
    report.severity = if committed {
        Severity::Info
    } else if result.success {
        Severity::Warning
    } else {
        Severity::Error
    };
    report.operation_index = Some(index);
    report.operation_count = Some(count);
    report.detailed_explanation = operation_explanation(
        index,
        count,
        &decoded.operation_name,
        &result,
        transaction_committed,
    );
    report.transaction_context = Some(crate::types::report::TransactionContext {
        tx_hash: tx_data
            .get("hash")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        ledger_sequence: tx_data.get("ledger").and_then(Value::as_u64).unwrap_or(0) as u32,
        function_name: Some(decoded.display_name),
        arguments: decoded.arguments,
        return_value: decoded.return_value,
        fee,
        resources: resource_summary,
        operation_index: Some(index),
        operation_count: Some(count),
    });
    enrich_operation_report(
        &mut report,
        tx_data,
        operation,
        &result,
        diagnostics,
        events,
        resources,
        index,
    );
    report
}

#[allow(clippy::too_many_arguments)]
fn enrich_operation_report(
    report: &mut DiagnosticReport,
    tx_data: &Value,
    operation: &Operation,
    result: &DecodedOperationResult,
    diagnostics: &[DiagnosticEvent],
    events: &[ContractEvent],
    _resources: &crate::decode::resource_analyzer::TransactionResultMeta,
    index: usize,
) {
    if !diagnostics.is_empty() {
        let encoded: Vec<String> = diagnostics
            .iter()
            .filter_map(|event| event.to_xdr_base64().ok())
            .collect();
        let data = serde_json::json!({
            "diagnosticEventsXdr": encoded,
            "hash": tx_data.get("hash"),
            "ledger": tx_data.get("ledger"),
        });
        let _ = crate::decode::diagnostic::enrich_report(report, &data);
    }
    if is_soroban_operation(operation) && !result.success {
        crate::decode::resource_analyzer::enrich_report(report, tx_data);
    }
    let function_name = report
        .transaction_context
        .as_ref()
        .and_then(|context| context.function_name.clone());
    report.cross_contract_attribution = failure_attribution(events, function_name, index);
}

fn failure_attribution(
    events: &[ContractEvent],
    function_name: Option<String>,
    index: usize,
) -> Option<crate::types::report::FailureAttribution> {
    let contract_id = events.iter().find_map(|event| event.contract_id.as_ref())?;
    Some(crate::types::report::FailureAttribution {
        contract_address: contract_id
            .0
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        function_name,
        call_depth: 0,
        origin_description: format!("Operation {}", index + 1),
    })
}

fn operation_explanation(
    index: usize,
    count: usize,
    name: &str,
    result: &DecodedOperationResult,
    transaction_committed: bool,
) -> String {
    let mut explanation = format!(
        "Operation {}/{} is a {} operation. Its XDR result is {}.",
        index + 1,
        count,
        name,
        result.code,
    );
    if result.success && !transaction_committed {
        explanation.push_str(
            " The operation-level result was successful, but its effects were rolled back because the atomic transaction failed.",
        );
    } else if !result.attempted {
        explanation.push_str(" The transaction failed before this operation was executed.");
    }
    explanation
}

pub async fn decode_transaction_with_op_filter(
    tx_hash: &str,
    network: &NetworkConfig,
    op_index: Option<usize>,
) -> GratResult<Vec<DiagnosticReport>> {
    let rpc = SorobanRpcClient::new(network);
    let tx_data = rpc.get_transaction(tx_hash).await?;
    let tx_json =
        serde_json::to_value(&tx_data).map_err(|error| GratError::Internal(error.to_string()))?;
    let reports = MultiOpDecoder::new().decode_transaction(&tx_json)?;
    match op_index {
        Some(index) => reports
            .get(index)
            .cloned()
            .map(|report| vec![report])
            .ok_or_else(|| {
                GratError::Internal(format!(
                    "Operation index {index} is out of range for a transaction with {} operations",
                    reports.len(),
                ))
            }),
        None => Ok(reports),
    }
}

impl MultiOpDecoder {
    fn decode_operation(
        &self,
        operation: &Operation,
        return_value: Option<&ScVal>,
    ) -> DecodedOperation {
        decode_operation_payload(&self.function_call_decoder, operation, return_value)
    }
}
