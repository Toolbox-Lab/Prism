//! Transaction context enricher.
//!
//! Decodes the full transaction envelope: called function, decoded arguments,
//! auth requirements, resource footprint, and fee breakdown.

use crate::types::error::PrismResult;
use crate::types::report::{DiagnosticReport, FeeBreakdown, ResourceSummary, TransactionContext};

/// Enrich a diagnostic report with full transaction context.
pub fn enrich_report(
    report: &mut DiagnosticReport,
    tx_data: &serde_json::Value,
) -> PrismResult<()> {
    let tx_hash = tx_data
        .get("hash")
        .and_then(|h| h.as_str())
        .unwrap_or("unknown")
        .to_string();

    let ledger_sequence = tx_data.get("ledger").and_then(|l| l.as_u64()).unwrap_or(0) as u32;

    let context = TransactionContext {
        tx_hash,
        ledger_sequence,
        function_name: extract_function_name(tx_data),
        arguments: extract_arguments(tx_data),
        fee: extract_fee_breakdown(tx_data),
        resources: extract_resource_summary(tx_data),
    };

    report.transaction_context = Some(context);
    Ok(())
}

/// Extract the called function name from the transaction envelope.
fn extract_function_name(tx_data: &serde_json::Value) -> Option<String> {
    // TODO: Decode the InvokeHostFunction operation to extract the function name
    tx_data
        .get("functionName")
        .and_then(|f| f.as_str())
        .map(|s| s.to_string())
}

/// Extract and decode function arguments.
fn extract_arguments(tx_data: &serde_json::Value) -> Vec<String> {
    // TODO: Decode SCVal arguments using contractspec type information
    tx_data
        .get("arguments")
        .and_then(|a| a.as_array())
        .map(|args| args.iter().map(|a| a.to_string()).collect())
        .unwrap_or_default()
}

/// Extract fee breakdown from the transaction.
fn extract_fee_breakdown(tx_data: &serde_json::Value) -> FeeBreakdown {
    // TODO: Parse actual fee components from the transaction
    FeeBreakdown {
        inclusion_fee: tx_data
            .get("inclusionFee")
            .and_then(|f| f.as_i64())
            .unwrap_or(0),
        resource_fee: tx_data
            .get("resourceFee")
            .and_then(|f| f.as_i64())
            .unwrap_or(0),
        refundable_fee: tx_data
            .get("refundableFee")
            .and_then(|f| f.as_i64())
            .unwrap_or(0),
        non_refundable_fee: tx_data
            .get("nonRefundableFee")
            .and_then(|f| f.as_i64())
            .unwrap_or(0),
    }
}

/// Extract resource usage summary.
fn extract_resource_summary(tx_data: &serde_json::Value) -> ResourceSummary {
    // TODO: Parse actual resource usage from the transaction result
    ResourceSummary {
        cpu_instructions_used: 0,
        cpu_instructions_limit: 0,
        memory_bytes_used: 0,
        memory_bytes_limit: 0,
        read_bytes: 0,
        write_bytes: 0,
    }
}
