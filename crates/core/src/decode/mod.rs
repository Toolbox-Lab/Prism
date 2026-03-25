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

/// Decode a transaction error from its hash, returning a full diagnostic report.
///
/// This is the main entry point for Tier 1 functionality.
pub async fn decode_transaction(
    tx_hash: &str,
    network: &crate::types::config::NetworkConfig,
) -> PrismResult<DiagnosticReport> {
    // 1. Fetch the transaction result
    let rpc = crate::network::rpc::RpcClient::new(network.clone());
    let tx_data = rpc.get_transaction(tx_hash).await?;

    // 2. Classify the error
    let error_info = host_error::classify_error(&tx_data)?;

    // 3. Build the diagnostic report
    let mut report = report::build_report(&error_info)?;

    // 4. If it's a contract error, resolve via contract spec
    if error_info.is_contract_error {
        if let Ok(contract_info) = contract_error::resolve(
            &error_info.contract_id.unwrap_or_default(),
            error_info.error_code,
            network,
        )
        .await
        {
            report.contract_error = Some(contract_info);
        }
    }

    // 5. Analyze diagnostic events for additional context
    diagnostic::enrich_report(&mut report, &tx_data)?;

    // 6. Enrich with transaction context
    context::enrich_report(&mut report, &tx_data)?;

    Ok(report)
}
