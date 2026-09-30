use crate::error::{PrismError, PrismResult};
use crate::replay::state::LedgerState;

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
    pub success: bool,

    pub events: Vec<TraceEvent>,

    pub final_state: std::collections::HashMap<String, Vec<u8>>,

    pub total_cpu: u64,

    pub total_memory: u64,
}

pub async fn execute_with_tracing(
    _state: &LedgerState,
    _tx_hash: &str,
) -> PrismResult<SandboxResult> {
    tracing::info!("Sandbox execution with tracing — not yet implemented");

    Err(PrismError::ReplayError(
        "Sandbox execution not yet implemented. Requires soroban-env-host instrumentation."
            .to_string(),
    ))
}
