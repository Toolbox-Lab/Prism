use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
    Fatal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootCause {
    pub description: String,

    pub likelihood: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedFix {
    pub description: String,

    pub difficulty: String,

    pub requires_upgrade: bool,

    pub example: Option<String>,

    pub id: String,

    pub remedy_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractErrorInfo {
    pub contract_id: String,

    pub error_code: u32,

    pub error_name: Option<String>,

    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionContext {
    pub tx_hash: String,

    pub ledger_sequence: u32,

    pub function_name: Option<String>,

    pub arguments: Vec<String>,

    pub return_value: Option<String>,

    pub fee: FeeBreakdown,

    pub resources: ResourceSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeBreakdown {
    pub total_charged_fee: i64,
    pub inclusion_fee: i64,
    pub resource_fee: i64,
    pub refundable_fee: i64,
    pub non_refundable_fee: i64,
    pub bid_fee: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSummary {
    pub cpu_instructions_used: u64,
    pub cpu_instructions_limit: u64,
    pub memory_bytes_used: u64,
    pub memory_bytes_limit: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
}

/// Pinpoints the exact contract and function where a cross-contract call chain failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureAttribution {
    /// The contract address that directly caused the failure.
    pub contract_address: String,
    /// The function name at the point of failure, if determinable.
    pub function_name: Option<String>,
    /// The call depth at which the failure occurred (0 = top-level invoker).
    pub call_depth: usize,
    /// Human-readable description of where in the call chain the failure originated.
    pub origin_description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub error_category: String,

    pub error_code: u32,

    pub error_name: String,

    pub summary: String,

    pub detailed_explanation: String,

    pub severity: Severity,

    pub root_causes: Vec<RootCause>,

    pub suggested_fixes: Vec<SuggestedFix>,

    pub contract_error: Option<ContractErrorInfo>,

    pub transaction_context: Option<TransactionContext>,

    pub related_errors: Vec<String>,

    /// Present when a cross-contract call chain was detected and the failure
    /// was attributed to a specific sub-contract, not the top-level invoker.
    pub cross_contract_attribution: Option<FailureAttribution>,

    /// Decoded hex strings for ed25519 signatures found in auth entries.
    /// Malformed or empty byte sequences produce a human-readable error label.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub auth_signatures: Vec<String>,
}

impl DiagnosticReport {
    pub fn new(category: &str, code: u32, name: &str, summary: &str) -> Self {
        Self {
            error_category: category.to_string(),
            error_code: code,
            error_name: name.to_string(),
            summary: summary.to_string(),
            detailed_explanation: String::new(),
            severity: Severity::Error,
            root_causes: Vec::new(),
            suggested_fixes: Vec::new(),
            contract_error: None,
            transaction_context: None,
            related_errors: Vec::new(),
            cross_contract_attribution: None,
            auth_signatures: Vec::new(),
        }
    }
}
