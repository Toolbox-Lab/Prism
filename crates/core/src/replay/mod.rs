pub mod differ;
pub mod export;
pub mod profiler;
pub mod sandbox;
pub mod state;
pub mod trace;

use crate::decode::auth::scval_to_readable_string;
use crate::decode::auth::AuthChain;
use crate::decode::walk_diagnostic_events;
use crate::error::GratResult;
use crate::types::config::NetworkConfig;
use crate::types::trace::{DiagnosticEvent, ExecutionTrace, TraceId};
use stellar_xdr::curr::{Limits, ReadXdr, SorobanAuthorizationEntry};

pub async fn replay_transaction(
    tx_hash: &str,
    network: &NetworkConfig,
) -> GratResult<ExecutionTrace> {
    let ledger_state = state::reconstruct_state(tx_hash, network).await?;
    replay_with_state(tx_hash, network, &ledger_state, sandbox::MEM_BYTES_CEILING).await
}

/// Replays `tx_hash` against an already reconstructed (and possibly patched)
/// ledger state, using `memory_limit` bytes as the sandbox memory ceiling.
///
/// This is the shared execution path behind [`replay_transaction`] and the
/// what-if simulator, which clones and patches the state before calling it.
pub async fn replay_with_state(
    tx_hash: &str,
    network: &NetworkConfig,
    ledger_state: &state::LedgerState,
    memory_limit: u64,
) -> GratResult<ExecutionTrace> {
    let trace_id = TraceId::new();

    let raw_trace =
        sandbox::execute_with_tracing_limited(ledger_state, tx_hash, trace_id, memory_limit)
            .await?;

    build_execution_trace(tx_hash, network, ledger_state, &raw_trace)
}

/// Converts raw XDR diagnostic events into the display-oriented
/// [`DiagnosticEvent`] form, preserving their order as the timeline position.
pub fn structure_diagnostic_events(
    events: &[stellar_xdr::curr::DiagnosticEvent],
) -> Vec<DiagnosticEvent> {
    walk_diagnostic_events(events)
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
        .collect()
}

fn build_execution_trace(
    tx_hash: &str,
    network: &NetworkConfig,
    ledger_state: &state::LedgerState,
    raw_trace: &sandbox::SandboxResult,
) -> GratResult<ExecutionTrace> {
    let trace_tree = trace::build_trace_tree(raw_trace)?;

    let state_diff = differ::compute_diff(ledger_state, raw_trace)?;

    let profile = profiler::generate_profile(raw_trace, &state_diff)?;

    let diagnostic_events = structure_diagnostic_events(&raw_trace.diagnostic_events);

    let auth_chains = ledger_state
        .invocation
        .auth_entries_xdr
        .iter()
        .filter_map(|bytes| {
            SorobanAuthorizationEntry::from_xdr(bytes, Limits::none())
                .ok()
                .map(|entry| AuthChain::from_entry(&entry))
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
        auth_chains,
    })
}
