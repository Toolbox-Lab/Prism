use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Error)]
#[error("JSON-RPC error (code: {code}): {message}")]
pub struct JsonRpcError {
    pub code: i64,

    pub message: String,
}

#[derive(Debug, Error)]
pub enum ArchiveErrorKind {
    #[error("checksum mismatch for '{file}': expected {expected}, got {actual}")]
    ChecksumMismatch {
        file: String,
        expected: String,
        actual: String,
    },

    #[error("malformed XDR in '{file}': {reason}")]
    MalformedXdr { file: String, reason: String },

    #[error("failed to fetch '{file}' from all archive backends: {reason}")]
    FetchFailed { file: String, reason: String },

    #[error("decompression failed for '{file}': {reason}")]
    DecompressionFailed { file: String, reason: String },
}
#[derive(Debug, Error)]
pub enum GratError {
    #[error("RPC request timed out after {timeout_secs}s (method: {method})")]
    NetworkTimeout { method: String, timeout_secs: u64 },

    #[error("RPC error: {0}")]
    RpcError(String),

    #[error("JSON-RPC error: {0}")]
    JsonRpc(JsonRpcError),

    #[error("Archive error: {0}")]
    ArchiveError(#[from] ArchiveErrorKind),

    #[error("XDR error: {0}")]
    XdrError(String),

    #[error("XDR decoding failed for {type_name}: {reason}")]
    XdrDecodingFailed {
        type_name: &'static str,
        reason: String,
    },

    #[error("Spec error: {0}")]
    SpecError(String),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Cache miss for key '{0}'")]
    CacheMiss(String),

    #[error(
        "Cache entry for key '{key}' ({entry_size} bytes) exceeds capacity limit of {limit} bytes"
    )]
    CacheCapacityExceeded {
        key: String,
        entry_size: u64,
        limit: u64,
    },

    #[error("Failed to serialize cache value for key '{key}': {reason}")]
    CacheSerializationError { key: String, reason: String },

    #[error("Failed to deserialize cache value for key '{key}': {reason}")]
    CacheDeserializationError { key: String, reason: String },

    #[error("Taxonomy error: {0}")]
    TaxonomyError(String),

    #[error("Replay error: {0}")]
    ReplayError(String),

    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),

    #[error("Contract not found: {0}")]
    ContractNotFound(String),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not a Soroban transaction: no InvokeHostFunction operation found")]
    NotSorobanTransaction,

    #[error("Transaction succeeded — no error to decode")]
    TransactionSucceeded,

    #[error("Invalid Contract ID: {0}")]
    InvalidContractId(String),

    #[error(
        "Function '{function_name}' expects {expected} argument(s), but {actual} were provided"
    )]
    ArityMismatch {
        function_name: String,
        expected: usize,
        actual: usize,
    },

    #[error("Unsupported host function: {0}")]
    UnsupportedHostFunction(String),
}

pub type GratResult<T> = Result<T, GratError>;
