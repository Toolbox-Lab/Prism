//! Error types for the Prism crate.

use std::fmt;

/// Top-level error type for all Prism operations.
#[derive(Debug)]
pub enum PrismError {
    /// A network request exceeded the configured timeout duration.
    NetworkTimeout { method: String, timeout_secs: u64 },
    /// Error communicating with the Soroban RPC endpoint.
    RpcError(String),
    /// Error fetching or parsing history archive data.
    ArchiveError(String),
    /// Error decoding XDR data.
    XdrError(String),
    /// XDR base64 decoding failed for a specific type.
    ///
    /// Returned by [`crate::xdr::codec::XdrCodec::from_xdr_base64`] when the
    /// input is malformed or does not match the expected XDR type.
    XdrDecodingFailed { type_name: &'static str, reason: String },
    /// Error parsing WASM or contract spec data.
    SpecError(String),
    /// Error in the local cache layer.
    CacheError(String),
    /// Error loading or querying the taxonomy database.
    TaxonomyError(String),
    /// Error during transaction replay.
    ReplayError(String),
    /// The requested transaction was not found.
    TransactionNotFound(String),
    /// The requested contract was not found on the ledger.
    ContractNotFound(String),
    /// An invalid network or configuration was provided.
    ConfigError(String),
    /// An invalid Stellar address was provided.
    InvalidAddress(String),
    /// Generic internal error.
    Internal(String),
}

impl fmt::Display for PrismError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NetworkTimeout { method, timeout_secs } => {
                write!(f, "RPC request timed out after {timeout_secs}s (method: {method})")
            }
            Self::RpcError(msg) => write!(f, "RPC error: {msg}"),
            Self::ArchiveError(msg) => write!(f, "Archive error: {msg}"),
            Self::XdrError(msg) => write!(f, "XDR error: {msg}"),
            Self::XdrDecodingFailed { type_name, reason } => {
                write!(f, "XDR decoding failed for {type_name}: {reason}")
            }
            Self::SpecError(msg) => write!(f, "Spec error: {msg}"),
            Self::CacheError(msg) => write!(f, "Cache error: {msg}"),
            Self::TaxonomyError(msg) => write!(f, "Taxonomy error: {msg}"),
            Self::ReplayError(msg) => write!(f, "Replay error: {msg}"),
            Self::TransactionNotFound(hash) => write!(f, "Transaction not found: {hash}"),
            Self::ContractNotFound(id) => write!(f, "Contract not found: {id}"),
            Self::ConfigError(msg) => write!(f, "Config error: {msg}"),
            Self::InvalidAddress(msg) => write!(f, "Invalid address: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl std::error::Error for PrismError {}

/// Convenience Result type for Prism operations.
pub type PrismResult<T> = Result<T, PrismError>;
