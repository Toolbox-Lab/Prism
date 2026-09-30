pub mod differ;
pub mod profiler;
pub mod sandbox;
pub mod state;
pub mod trace;

use crate::error::PrismResult;
use crate::types::config::NetworkConfig;
use crate::types::trace::ExecutionTrace;

pub async fn replay_transaction(
    tx_hash: &str,
    network: &NetworkConfig,
) -> PrismResult<ExecutionTrace> {
    let ledger_state = state::reconstruct_state(tx_hash, network).await?;

    let raw_trace = sandbox::execute_with_tracing(&ledger_state, tx_hash).await?;

    let trace_tree = trace::build_trace_tree(&raw_trace)?;

    let state_diff = differ::compute_diff(&ledger_state, &raw_trace)?;

    let profile = profiler::generate_profile(&raw_trace)?;

    Ok(ExecutionTrace {
        tx_hash: tx_hash.to_string(),
        ledger_sequence: ledger_state.ledger_sequence,
        network: network.network.to_string(),
        invocations: trace_tree,
        state_diff,
        resource_profile: profile,
        diagnostic_events: Vec::new(),
    })
}
