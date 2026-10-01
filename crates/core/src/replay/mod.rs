pub mod differ;
pub mod profiler;
pub mod sandbox;
pub mod state;
pub mod trace;

use crate::decode::auth::scval_to_readable_string;
use crate::decode::walk_diagnostic_events;
use crate::error::GratResult;
use crate::types::config::NetworkConfig;
use crate::types::trace::{DiagnosticEvent, ExecutionTrace, TraceId};

pub async fn replay_transaction(
    tx_hash: &str,
    network: &NetworkConfig,
) -> GratResult<ExecutionTrace> {
    let ledger_state = state::reconstruct_state(tx_hash, network).await?;
    let trace_id = TraceId::new();

    let raw_trace = sandbox::execute_with_tracing(&ledger_state, tx_hash, trace_id).await?;

    let trace_tree = trace::build_trace_tree(&raw_trace)?;

    let state_diff = differ::compute_diff(&ledger_state, &raw_trace)?;

    let profile = profiler::generate_profile(&raw_trace, &state_diff)?;

    let diagnostic_events = walk_diagnostic_events(&raw_trace.diagnostic_events)
        .into_iter()
        .enumerate()
        .map(|(timeline_position, event)| DiagnosticEvent {
            event_type: event.kind.to_string(),
            topics: event.topics.iter().map(scval_to_readable_string).collect(),
            data: std::collections::HashMap::from([(
                "value".to_string(),
                scval_to_readable_string(&event.data),
            )]),
            timeline_position,
        })
        .collect();

    Ok(ExecutionTrace {
        tx_hash: tx_hash.to_string(),
        ledger_sequence: ledger_state.ledger_sequence,
        network: network.network.to_string(),
        invocations: trace_tree,
        state_diff,
        resource_profile: profile,
        diagnostic_events,
    })
}
