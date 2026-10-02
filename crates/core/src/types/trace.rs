use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceId(pub String);

impl TraceId {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostFunctionCall {
    pub function_name: String,

    pub arguments: Vec<String>,

    pub return_value: Option<String>,

    pub cpu_instructions: u64,

    pub memory_bytes: u64,

    pub is_error: bool,

    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractInvocation {
    pub trace_id: TraceId,

    pub contract_id: String,

    pub function_name: String,

    pub arguments: Vec<String>,

    pub return_value: Option<String>,

    pub host_calls: Vec<HostFunctionCall>,

    pub sub_invocations: Vec<ContractInvocation>,

    pub total_cpu_instructions: u64,

    pub total_memory_bytes: u64,

    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntryDiff {
    pub key: String,

    pub before: Option<String>,

    pub after: Option<String>,

    pub change_type: DiffChangeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiffChangeType {
    Created,
    Updated,
    Deleted,
    Unchanged,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateDiff {
    pub entries: Vec<LedgerEntryDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceHotspot {
    pub location: String,

    pub cpu_instructions: u64,

    pub cpu_percentage: f64,

    pub memory_bytes: u64,

    pub memory_percentage: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceProfile {
    pub total_cpu: u64,

    pub cpu_limit: u64,

    pub total_memory: u64,

    pub memory_limit: u64,

    pub total_read_bytes: u64,

    pub total_write_bytes: u64,

    pub read_limit: u64,

    pub write_limit: u64,

    pub hotspots: Vec<ResourceHotspot>,

    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub event_type: String,

    pub topics: Vec<String>,

    pub data: HashMap<String, String>,

    pub timeline_position: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub tx_hash: String,

    pub ledger_sequence: u32,

    pub network: String,

    pub invocations: Vec<ContractInvocation>,

    pub state_diff: StateDiff,

    pub resource_profile: ResourceProfile,

    pub diagnostic_events: Vec<DiagnosticEvent>,
}
