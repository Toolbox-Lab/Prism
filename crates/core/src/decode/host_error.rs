//! Host error classifier.
//!
//! Parses error category + code from TransactionResult XDR and classifies
//! into known error families using the taxonomy database.

use crate::taxonomy::schema::ErrorCategory;
use crate::types::error::{PrismError, PrismResult};

/// Classified error information extracted from a transaction result.
#[derive(Debug, Clone)]
pub struct ClassifiedError {
    /// Error category.
    pub category: ErrorCategory,
    /// Numeric error code.
    pub error_code: u32,
    /// Whether this is a contract-defined error (vs host error).
    pub is_contract_error: bool,
    /// Contract ID if this is a contract error.
    pub contract_id: Option<String>,
    /// Raw error data for further processing.
    pub raw_data: serde_json::Value,
}

/// Classify the error from a transaction result JSON.
///
/// Extracts the error category, code, and determines whether it's a host error
/// or a contract-defined error.
pub fn classify_error(tx_data: &serde_json::Value) -> PrismResult<ClassifiedError> {
    // Extract the result status
    let status = tx_data
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("UNKNOWN");

    if status == "SUCCESS" {
        return Err(PrismError::Internal(
            "Transaction succeeded — no error to classify".to_string(),
        ));
    }

    // TODO: Parse the TransactionResult XDR to extract the specific error
    // For now, return a placeholder classified error

    Ok(ClassifiedError {
        category: ErrorCategory::Contract,
        error_code: 0,
        is_contract_error: false,
        contract_id: None,
        raw_data: tx_data.clone(),
    })
}

/// Map an error category string to an `ErrorCategory` enum value.
pub fn parse_error_category(category_str: &str) -> Option<ErrorCategory> {
    match category_str.to_lowercase().as_str() {
        "budget" => Some(ErrorCategory::Budget),
        "storage" => Some(ErrorCategory::Storage),
        "auth" => Some(ErrorCategory::Auth),
        "context" => Some(ErrorCategory::Context),
        "value" => Some(ErrorCategory::Value),
        "object" => Some(ErrorCategory::Object),
        "crypto" => Some(ErrorCategory::Crypto),
        "contract" => Some(ErrorCategory::Contract),
        "wasm" => Some(ErrorCategory::Wasm),
        "events" => Some(ErrorCategory::Events),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error_category() {
        assert_eq!(parse_error_category("budget"), Some(ErrorCategory::Budget));
        assert_eq!(
            parse_error_category("STORAGE"),
            Some(ErrorCategory::Storage)
        );
        assert_eq!(parse_error_category("unknown"), None);
    }
}
