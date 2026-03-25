//! Tier 2: Execution Trace & Replay Engine.

pub mod differ;
pub mod profiler;
pub mod sandbox;
pub mod state;
pub mod trace;

use crate::types::config::NetworkConfig;
use crate::types::error::PrismResult;
use crate::types::trace::ExecutionTrace;

/// Replay a transaction and produce a full execution trace.
///
/// This is the main entry point for Tier 2 functionality.
pub async fn replay_transaction(
    tx_hash: &str,
    network: &NetworkConfig,
) -> PrismResult<ExecutionTrace> {
    // 1. Determine replay path (hot or cold)
    let ledger_state = state::reconstruct_state(tx_hash, network).await?;

    // 2. Execute in sandbox with tracing
    let raw_trace = sandbox::execute_with_tracing(&ledger_state, tx_hash).await?;

    // 3. Build hierarchical trace
    let trace_tree = trace::build_trace_tree(&raw_trace)?;

    // 4. Compute state diff
    let state_diff = differ::compute_diff(&ledger_state, &raw_trace)?;

    // 5. Generate resource profile
    let profile = profiler::generate_profile(&raw_trace)?;

    Ok(ExecutionTrace {
        tx_hash: tx_hash.to_string(),
        ledger_sequence: ledger_state.ledger_sequence,
        network: format!("{:?}", network.network),
        invocations: trace_tree,
        state_diff,
        resource_profile: profile,
        diagnostic_events: Vec::new(),
    })
}
