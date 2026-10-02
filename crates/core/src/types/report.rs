use crate::decode::chain_analyzer::CallChain;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthEntryInfo {
    pub auth_type: String,
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub contract_id: Option<String>,
}

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

    pub learn_more: String,
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

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub operation_index: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub operation_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeBreakdown {
    pub total_charged_fee: i64,
    pub inclusion_fee: i64,
    pub resource_fee: i64,
    pub refundable_resource_fee: i64,
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
    pub read_bytes_limit: u64,
    pub write_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureAttribution {
    pub contract_address: String,
    pub function_name: Option<String>,
    pub call_depth: usize,
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

    pub cross_contract_attribution: Option<FailureAttribution>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub auth_signatures: Vec<String>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub auth_entries: Vec<AuthEntryInfo>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub failing_contract_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub call_chain: Option<CallChain>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub resource_diagnostics: Option<crate::decode::resource_analyzer::ResourceDiagnostics>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub operation_index: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub operation_count: Option<usize>,

    pub learn_more: String,
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
            auth_entries: Vec::new(),
            failing_contract_id: None,
            call_chain: None,
            resource_diagnostics: None,
            operation_index: None,
            operation_count: None,
            learn_more: "https://developers.stellar.org/docs/learn/smart-contracts/errors"
                .to_string(),
        }
    }
}
