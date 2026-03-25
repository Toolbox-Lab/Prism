//! WASM ContractSpec decoder.
//!
//! Extracts `contractspecv0` and `SCMetaEntry` metadata from WASM custom sections.
//! Used to resolve contract-specific error enums, function signatures, and type definitions.

use crate::types::error::{PrismError, PrismResult};
use serde::{Deserialize, Serialize};

/// A decoded contract error enum variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractErrorEntry {
    /// Numeric error code.
    pub code: u32,
    /// Name of the error variant (e.g., "InsufficientBalance").
    pub name: String,
    /// Doc comment, if present in the contract spec.
    pub doc: Option<String>,
}

/// A decoded contract function signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractFunction {
    /// Function name.
    pub name: String,
    /// Parameter names and types.
    pub params: Vec<(String, String)>,
    /// Return type description.
    pub return_type: String,
    /// Doc comment, if present.
    pub doc: Option<String>,
}

/// Fully decoded contract specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSpec {
    /// Error enum variants defined in the contract.
    pub errors: Vec<ContractErrorEntry>,
    /// Function signatures.
    pub functions: Vec<ContractFunction>,
    /// Contract name from meta, if available.
    pub name: Option<String>,
    /// Contract version from meta, if available.
    pub version: Option<String>,
}

/// Parse WASM bytecode and extract the contract specification.
///
/// # Arguments
/// * `wasm_bytes` - Raw WASM binary data
///
/// # Returns
/// A `ContractSpec` with all decoded metadata.
pub fn decode_contract_spec(wasm_bytes: &[u8]) -> PrismResult<ContractSpec> {
    // Parse WASM to find custom sections named "contractspecv0" and "contractmetav0"
    let parser = wasmparser::Parser::new(0);
    let mut spec = ContractSpec {
        errors: Vec::new(),
        functions: Vec::new(),
        name: None,
        version: None,
    };

    for payload in parser.parse_all(wasm_bytes) {
        let payload =
            payload.map_err(|e| PrismError::SpecError(format!("WASM parse error: {e}")))?;

        if let wasmparser::Payload::CustomSection(section) = payload {
            match section.name() {
                "contractspecv0" => {
                    // TODO: Parse SCSpecEntry items from section data
                    // Each entry can be a function, error enum, struct, or union definition
                    tracing::debug!(
                        "Found contractspecv0 section ({} bytes)",
                        section.data().len()
                    );
                }
                "contractmetav0" => {
                    // TODO: Parse SCMetaEntry items from section data
                    tracing::debug!(
                        "Found contractmetav0 section ({} bytes)",
                        section.data().len()
                    );
                }
                _ => {}
            }
        }
    }

    Ok(spec)
}

/// Resolve a numeric error code to its named variant using a contract spec.
///
/// # Arguments
/// * `spec` - The decoded contract specification
/// * `error_code` - The numeric error code to resolve
///
/// # Returns
/// The matching error entry, if found.
pub fn resolve_error_code(spec: &ContractSpec, error_code: u32) -> Option<&ContractErrorEntry> {
    spec.errors.iter().find(|e| e.code == error_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_error_code_not_found() {
        let spec = ContractSpec {
            errors: vec![ContractErrorEntry {
                code: 1,
                name: "NotFound".to_string(),
                doc: None,
            }],
            functions: Vec::new(),
            name: None,
            version: None,
        };
        assert!(resolve_error_code(&spec, 99).is_none());
        assert!(resolve_error_code(&spec, 1).is_some());
    }
}
