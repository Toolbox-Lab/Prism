//! Historical state reconstruction.
//!
//! Two-path strategy:
//! - **Hot path:** Use Soroban RPC `getLedgerEntries` for recent transactions (last ~50k ledgers)
//! - **Cold path:** Fall back to Stellar History Archives for older transactions

use crate::types::config::NetworkConfig;
use crate::types::error::{PrismError, PrismResult};
use std::collections::HashMap;

/// Reconstructed ledger state at a specific sequence number.
#[derive(Debug, Clone)]
pub struct LedgerState {
    /// Ledger sequence number.
    pub ledger_sequence: u32,
    /// Reconstructed ledger entries keyed by their ledger key.
    pub entries: HashMap<String, Vec<u8>>,
    /// Whether this state was reconstructed from the hot or cold path.
    pub reconstruction_path: ReconstructionPath,
}

/// How the state was reconstructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconstructionPath {
    /// Via Soroban RPC (recent transaction).
    HotPath,
    /// Via History Archives + Captive Core (older transaction).
    ColdPath,
}

/// The hot path threshold — transactions within this many ledgers use the RPC directly.
const HOT_PATH_THRESHOLD: u32 = 50_000;

/// Reconstruct ledger state at the time of a transaction.
pub async fn reconstruct_state(tx_hash: &str, network: &NetworkConfig) -> PrismResult<LedgerState> {
    let rpc = crate::network::rpc::RpcClient::new(network.clone());

    // 1. Fetch the transaction to determine its ledger sequence
    let tx_data = rpc.get_transaction(tx_hash).await?;
    let tx_ledger = tx_data
        .get("ledger")
        .and_then(|l| l.as_u64())
        .ok_or_else(|| PrismError::ReplayError("Cannot determine transaction ledger".to_string()))?
        as u32;

    // 2. Get the current latest ledger
    let latest = rpc.get_latest_ledger().await?;
    let latest_ledger = latest.get("sequence").and_then(|s| s.as_u64()).unwrap_or(0) as u32;

    // 3. Choose reconstruction path
    let age = latest_ledger.saturating_sub(tx_ledger);

    if age <= HOT_PATH_THRESHOLD {
        tracing::info!("Using hot path (RPC) for ledger {tx_ledger} (age: {age} ledgers)");
        reconstruct_hot_path(tx_ledger, &rpc).await
    } else {
        tracing::info!("Using cold path (archive) for ledger {tx_ledger} (age: {age} ledgers)");
        reconstruct_cold_path(tx_ledger, network).await
    }
}

/// Hot path: reconstruct state from Soroban RPC.
async fn reconstruct_hot_path(
    ledger_sequence: u32,
    _rpc: &crate::network::rpc::RpcClient,
) -> PrismResult<LedgerState> {
    // TODO: Use getLedgerEntries to fetch all entries in the transaction's footprint
    Ok(LedgerState {
        ledger_sequence,
        entries: HashMap::new(),
        reconstruction_path: ReconstructionPath::HotPath,
    })
}

/// Cold path: reconstruct state from Stellar History Archives.
async fn reconstruct_cold_path(
    ledger_sequence: u32,
    _network: &NetworkConfig,
) -> PrismResult<LedgerState> {
    // TODO: Use archive client + Captive Core to reconstruct historical state
    tracing::warn!("Cold path reconstruction is computationally heavy — this may take a while");

    Ok(LedgerState {
        ledger_sequence,
        entries: HashMap::new(),
        reconstruction_path: ReconstructionPath::ColdPath,
    })
}
