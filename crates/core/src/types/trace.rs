//! Execution trace types — the primary output of the replay engine (Tier 2).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single host function call captured during replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostFunctionCall {
    /// Name of the host function (e.g., "storage::get", "auth::require").
    pub function_name: String,
    /// Decoded arguments to the host function.
    pub arguments: Vec<String>,
    /// Return value, if any.
    pub return_value: Option<String>,
    /// CPU instructions consumed by this call.
    pub cpu_instructions: u64,
    /// Memory bytes allocated by this call.
    pub memory_bytes: u64,
    /// Whether this call caused an error.
    pub is_error: bool,
    /// Error details if `is_error` is true.
    pub error: Option<String>,
}

/// A contract invocation in the execution tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractInvocation {
    /// The contract address being invoked.
    pub contract_id: String,
    /// The function being called.
    pub function_name: String,
    /// Decoded arguments.
    pub arguments: Vec<String>,
    /// Return value, if the invocation succeeded.
    pub return_value: Option<String>,
    /// Host function calls made during this invocation.
    pub host_calls: Vec<HostFunctionCall>,
    /// Nested cross-contract invocations.
    pub sub_invocations: Vec<ContractInvocation>,
    /// Total CPU instructions consumed.
    pub total_cpu_instructions: u64,
    /// Total memory bytes allocated.
    pub total_memory_bytes: u64,
    /// Whether this invocation failed.
    pub is_error: bool,
}

/// A change to a single ledger entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntryDiff {
    /// The ledger key (decoded).
    pub key: String,
    /// Value before the transaction.
    pub before: Option<String>,
    /// Value after the transaction.
    pub after: Option<String>,
    /// Type of change.
    pub change_type: DiffChangeType,
}

/// Type of ledger entry change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiffChangeType {
    Created,
    Updated,
    Deleted,
    Unchanged,
}

/// Complete state diff for a transaction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateDiff {
    /// Individual entry changes.
    pub entries: Vec<LedgerEntryDiff>,
}

/// A resource consumption hotspot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceHotspot {
    /// Description of the hotspot (function name, contract, etc.).
    pub location: String,
    /// CPU instructions consumed.
    pub cpu_instructions: u64,
    /// Percentage of total CPU budget.
    pub cpu_percentage: f64,
    /// Memory bytes consumed.
    pub memory_bytes: u64,
    /// Percentage of total memory budget.
    pub memory_percentage: f64,
}

/// Resource consumption profile for a transaction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceProfile {
    /// Total CPU instructions used.
    pub total_cpu: u64,
    /// CPU instruction limit.
    pub cpu_limit: u64,
    /// Total memory bytes used.
    pub total_memory: u64,
    /// Memory limit.
    pub memory_limit: u64,
    /// Read bytes total.
    pub total_read_bytes: u64,
    /// Write bytes total.
    pub total_write_bytes: u64,
    /// Top resource hotspots.
    pub hotspots: Vec<ResourceHotspot>,
    /// Warnings about nearing limits.
    pub warnings: Vec<String>,
}

/// A diagnostic event captured during replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    /// Event type/category.
    pub event_type: String,
    /// Event topics.
    pub topics: Vec<String>,
    /// Event data.
    pub data: HashMap<String, String>,
    /// Position in the execution timeline.
    pub timeline_position: usize,
}

/// The complete execution trace — the primary output of Tier 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    /// The transaction hash.
    pub tx_hash: String,
    /// The ledger sequence number.
    pub ledger_sequence: u32,
    /// The network this trace was captured on.
    pub network: String,
    /// Root-level contract invocations.
    pub invocations: Vec<ContractInvocation>,
    /// State diff.
    pub state_diff: StateDiff,
    /// Resource profile.
    pub resource_profile: ResourceProfile,
    /// Diagnostic events.
    pub diagnostic_events: Vec<DiagnosticEvent>,
}
