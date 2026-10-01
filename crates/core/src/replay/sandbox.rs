use crate::error::{GratError, GratResult};
use crate::replay::state::{encode_xdr, LedgerState};
use crate::types::trace::TraceId;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha2::{Digest, Sha256};
use soroban_env_host::budget::Budget;
use soroban_env_host::e2e_invoke::invoke_host_function_with_trace_hook;
use soroban_env_host::{LedgerInfo, TraceEvent as HostTraceEvent, TraceHook};
use std::cell::RefCell;
use std::rc::Rc;
use stellar_xdr::curr::{DiagnosticEvent, Hash as XdrHash, TtlEntry};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceEvent {
    pub event_type: TraceEventType,

    pub timestamp_us: u64,

    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum TraceEventType {
    InvocationStart,

    InvocationEnd,

    HostFunctionCall,

    HostFunctionReturn,

    StorageRead,

    StorageWrite,

    AuthCheck,

    EventEmit,

    BudgetCheckpoint,
}

#[derive(Debug)]
pub struct SandboxResult {
    pub trace_id: TraceId,

    pub success: bool,

    pub events: Vec<TraceEvent>,

    pub final_state: std::collections::HashMap<String, Vec<u8>>,

    pub total_cpu: u64,

    pub total_memory: u64,

    pub diagnostic_events: Vec<DiagnosticEvent>,
}

/// Generous fixed ceiling for the replay memory budget. Unlike the CPU
/// instruction limit (which is taken directly from the transaction's own
/// recorded `SorobanResources`, so it matches what actually ran at
/// consensus), the network's exact per-transaction memory ceiling isn't
/// fetched here — this only needs to sit comfortably above real usage so
/// tracing is never cut short by an artificial memory limit.
const MEM_BYTES_CEILING: u64 = 500 * 1024 * 1024;

#[allow(clippy::too_many_lines)]
pub async fn execute_with_tracing(state: &LedgerState, tx_hash: &str, trace_id: TraceId) -> GratResult<SandboxResult> {
    let inv = &state.invocation;

    let budget = Budget::try_from_configs(
        inv.cpu_instructions_limit.max(1),
        MEM_BYTES_CEILING,
        inv.cpu_cost_params.clone(),
        inv.mem_cost_params.clone(),
    )
    .map_err(|e| GratError::ReplayError(format!("failed to build execution budget: {e}")))?;

    let ledger_info = LedgerInfo {
        protocol_version: inv.protocol_version,
        sequence_number: state.ledger_sequence,
        timestamp: inv.timestamp,
        network_id: inv.network_id,
        base_reserve: inv.base_reserve,
        min_temp_entry_ttl: inv.min_temp_entry_ttl,
        min_persistent_entry_ttl: inv.min_persistent_entry_ttl,
        max_entry_ttl: inv.max_entry_ttl,
    };

    let mut encoded_ledger_entries: Vec<Vec<u8>> = Vec::new();
    let mut encoded_ttl_entries: Vec<Vec<u8>> = Vec::new();
    for key_b64 in inv.read_only_keys.iter().chain(inv.read_write_keys.iter()) {
        let Some(entry_bytes) = state.entries.get(key_b64) else {
            // Footprint key that doesn't exist yet (e.g. storage the
            // contract is about to create for the first time).
            continue;
        };
        encoded_ledger_entries.push(entry_bytes.clone());
        encoded_ttl_entries.push(match inv.ttl_by_key.get(key_b64) {
            Some(live_until) => encode_ttl_entry(key_b64, *live_until)?,
            None => Vec::new(),
        });
    }

    let seed: [u8; 32] = Sha256::digest(tx_hash.as_bytes()).into();

    let events: Rc<RefCell<Vec<TraceEvent>>> = Rc::new(RefCell::new(Vec::new()));
    let events_for_hook = Rc::clone(&events);
    let budget_for_hook = budget.clone();
    let start_time = std::time::Instant::now();

    let trace_hook: TraceHook = Rc::new(move |_host, event| {
        let cpu = budget_for_hook.get_cpu_insns_consumed().unwrap_or(0);
        let mem = budget_for_hook.get_mem_bytes_consumed().unwrap_or(0);
        let timestamp_us = start_time.elapsed().as_micros() as u64;

        let (event_type, mut data) = classify_trace_event(&event);
        if let serde_json::Value::Object(ref mut map) = data {
            map.insert("cpu_insns".to_string(), serde_json::json!(cpu));
            map.insert("mem_bytes".to_string(), serde_json::json!(mem));
        }

        events_for_hook.borrow_mut().push(TraceEvent {
            event_type,
            timestamp_us,
            data,
        });
        Ok(())
    });

    let mut diagnostic_events: Vec<DiagnosticEvent> = Vec::new();

    let invoke_result = invoke_host_function_with_trace_hook(
        &budget,
        true,
        inv.host_function_xdr.clone(),
        inv.resources_xdr.clone(),
        inv.source_account_xdr.clone(),
        inv.auth_entries_xdr.clone().into_iter(),
        ledger_info,
        encoded_ledger_entries.into_iter(),
        encoded_ttl_entries.into_iter(),
        seed.to_vec(),
        &mut diagnostic_events,
        Some(trace_hook),
    )
    .map_err(|e| {
        GratError::ReplayError(format!(
            "sandbox execution setup failed for transaction {tx_hash}: {e}"
        ))
    })?;

    if !diagnostic_events.is_empty() {
        tracing::debug!(
            tx_hash,
            count = diagnostic_events.len(),
            "captured diagnostic events during sandbox execution"
        );
    }

    let success = invoke_result.encoded_invoke_result.is_ok();
    if let Err(e) = &invoke_result.encoded_invoke_result {
        tracing::info!(tx_hash, error = %e, "sandboxed invocation did not succeed");
    }

    let mut final_state = state.entries.clone();
    for change in &invoke_result.ledger_changes {
        if change.read_only {
            continue;
        }
        let key_b64 = STANDARD.encode(&change.encoded_key);
        match &change.encoded_new_value {
            Some(new_value) => {
                final_state.insert(key_b64, new_value.clone());
            }
            None => {
                final_state.remove(&key_b64);
            }
        }
    }

    let total_cpu = budget.get_cpu_insns_consumed().unwrap_or(0);
    let total_memory = budget.get_mem_bytes_consumed().unwrap_or(0);

    let events = match Rc::try_unwrap(events) {
        Ok(cell) => cell.into_inner(),
        Err(shared) => shared.borrow().clone(),
    };

    Ok(SandboxResult {
        trace_id,
        success,
        events,
        final_state,
        total_cpu,
        total_memory,
        diagnostic_events,
    })
}

fn encode_ttl_entry(key_b64: &str, live_until_ledger_seq: u32) -> GratResult<Vec<u8>> {
    let key_bytes = STANDARD
        .decode(key_b64)
        .map_err(|e| GratError::XdrDecodingFailed {
            type_name: "LedgerKey",
            reason: format!("invalid base64: {e}"),
        })?;
    let key_hash: [u8; 32] = Sha256::digest(&key_bytes).into();
    let ttl_entry = TtlEntry {
        key_hash: XdrHash(key_hash),
        live_until_ledger_seq,
    };
    encode_xdr(&ttl_entry, "TtlEntry")
}

/// Classifies one host trace event and renders it into our own event schema.
///
/// The host's `Context`/`Frame` types backing `PushCtx`/`PopCtx` aren't public
/// outside `soroban-env-host`, so the contract id and function name for those
/// two variants are recovered by best-effort parsing of the host's own
/// `Display` rendering (of the form `push VM:<id>:<fn>(...)` /
/// `pop VM:<id>:<fn> -> Ok(...)`) rather than field access. `EnvCall`/`EnvRet`
/// carry their function name directly and don't need this.
fn classify_trace_event(event: &HostTraceEvent<'_>) -> (TraceEventType, serde_json::Value) {
    match event {
        HostTraceEvent::Begin => (
            TraceEventType::BudgetCheckpoint,
            serde_json::json!({ "phase": "begin" }),
        ),
        HostTraceEvent::End => (
            TraceEventType::BudgetCheckpoint,
            serde_json::json!({ "phase": "end" }),
        ),
        HostTraceEvent::PushCtx(..) => {
            let detail = format!("{event}");
            let (contract_id, function) = parse_frame_label(&detail, "push ", "(");
            (
                TraceEventType::InvocationStart,
                serde_json::json!({
                    "contract_id": contract_id,
                    "function": function,
                    "detail": detail,
                }),
            )
        }
        HostTraceEvent::PopCtx(.., result) => {
            let detail = format!("{event}");
            let (contract_id, function) = parse_frame_label(&detail, "pop ", " ->");
            (
                TraceEventType::InvocationEnd,
                serde_json::json!({
                    "contract_id": contract_id,
                    "function": function,
                    "is_error": result.is_err(),
                    "detail": detail,
                }),
            )
        }
        HostTraceEvent::EnvCall(name, _args) => (
            TraceEventType::HostFunctionCall,
            serde_json::json!({
                "function": name,
                "detail": format!("{event}"),
            }),
        ),
        HostTraceEvent::EnvRet(name, result) => (
            TraceEventType::HostFunctionReturn,
            serde_json::json!({
                "function": name,
                "is_error": result.is_err(),
                "detail": format!("{event}"),
            }),
        ),
    }
}

fn parse_frame_label(
    detail: &str,
    prefix: &str,
    end_marker: &str,
) -> (Option<String>, Option<String>) {
    let Some(rest) = detail.strip_prefix(prefix) else {
        return (None, None);
    };
    let Some(end) = rest.find(end_marker) else {
        return (None, None);
    };
    let label = &rest[..end];
    let mut parts = label.splitn(3, ':');
    let ty = parts.next().unwrap_or_default();
    let id = parts.next().unwrap_or_default();
    let function = parts.next().filter(|f| !f.is_empty()).map(str::to_string);

    if ty.is_empty() && id.is_empty() {
        return (None, function);
    }
    (Some(format!("{ty}:{id}")), function)
}
