//! Tier 1: Error Decode Engine.
//!
//! Provides error classification, contract error resolution, diagnostic event
//! analysis, context enrichment, and report generation.

pub mod context;
pub mod contract_error;
pub mod diagnostic;
pub mod host_error;
pub mod report;

use crate::types::error::PrismResult;
use crate::types::report::DiagnosticReport;

/// Filter transaction data to focus on a specific operation index.
fn filter_transaction_by_operation(
    tx_data: &mut serde_json::Value,
    op_index: usize
) -> PrismResult<()> {
    // Filter contract events to only show events from the specified operation
    if let Some(events) = tx_data.get_mut("events") {
        if let Some(contract_events) = events.get_mut("contractEventsXdr") {
            if let Some(events_array) = contract_events.as_array_mut() {
                if op_index < events_array.len() {
                    // Keep only the events from the specified operation
                    let target_events = events_array[op_index].clone();
                    *events_array = vec![target_events];
                } else {
                    // Operation index out of bounds, return empty array
                    *events_array = vec![];
                }
            }
        }
    }

    // Filter diagnostic events if they exist
    if let Some(diagnostic_events) = tx_data.get_mut("diagnosticEventsXdr") {
        if let Some(events_array) = diagnostic_events.as_array_mut() {
            // For diagnostic events, we need to identify which ones belong to which operation
            // This is a simplified approach - in a full implementation, we'd need to parse
            // the XDR to properly associate events with operations
            if op_index == 0 && !events_array.is_empty() {
                // For now, if op_index is 0, keep the first event (typically the main invocation)
                let first_event = events_array[0].clone();
                *events_array = vec![first_event];
            } else {
                // For other indices, clear the array as we can't easily map them
                *events_array = vec![];
            }
        }
    }

    Ok(())
}

/// Decode a transaction error from its hash, returning a full diagnostic report.
///
/// This is the main entry point for Tier 1 functionality.
pub async fn decode_transaction(
    tx_hash: &str,
    network: &crate::types::config::NetworkConfig
) -> PrismResult<DiagnosticReport> {
    decode_transaction_with_op_filter(tx_hash, network, None).await
}

/// Decode a transaction error from its hash, returning a full diagnostic report.
/// Optionally filter to focus on a specific operation index.
pub async fn decode_transaction_with_op_filter(
    tx_hash: &str,
    network: &crate::types::config::NetworkConfig,
    op_index: Option<usize>
) -> PrismResult<DiagnosticReport> {
    // 1. Fetch the transaction result
    let rpc = crate::network::rpc::RpcClient::new(network.clone());
    let mut tx_data = rpc.get_transaction(tx_hash).await?;

    // 2. Filter by operation index if specified
    if let Some(index) = op_index {
        filter_transaction_by_operation(&mut tx_data, index)?;
    }

    // 3. Classify the error
    let error_info = host_error::classify_error(&tx_data)?;

    // 4. Build the diagnostic report
    let mut report = report::build_report(&error_info)?;

    // 5. If it's a contract error, resolve via contract spec
    if error_info.is_contract_error {
        if
            let Ok(contract_info) = contract_error::resolve(
                &error_info.contract_id.unwrap_or_default(),
                error_info.error_code,
                network
            ).await
        {
            report.contract_error = Some(contract_info);
        }
    }

    // 6. Analyze diagnostic events for additional context
    diagnostic::enrich_report(&mut report, &tx_data)?;

    // 7. Enrich with transaction context
    context::enrich_report(&mut report, &tx_data)?;

    Ok(report)
}
