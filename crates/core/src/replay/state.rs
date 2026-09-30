use crate::error::{PrismError, PrismResult};
use crate::types::config::NetworkConfig;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LedgerState {
    pub ledger_sequence: u32,

    pub entries: HashMap<String, Vec<u8>>,

    pub reconstruction_path: ReconstructionPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconstructionPath {
    HotPath,

    ColdPath,
}

const HOT_PATH_THRESHOLD: u32 = 50_000;

pub async fn reconstruct_state(tx_hash: &str, network: &NetworkConfig) -> PrismResult<LedgerState> {
    let rpc = crate::rpc::SorobanRpcClient::new(network);

    let tx_data = rpc.get_transaction(tx_hash).await?;
    let tx_ledger = tx_data.ledger.ok_or_else(|| {
        PrismError::ReplayError("Cannot determine transaction ledger".to_string())
    })?;

    let latest: serde_json::Value = rpc.get_latest_ledger().await?;
    let latest_ledger = latest
        .get("sequence")
        .and_then(|s: &serde_json::Value| s.as_u64())
        .unwrap_or(0) as u32;

    let age = latest_ledger.saturating_sub(tx_ledger);

    if age <= HOT_PATH_THRESHOLD {
        tracing::info!("Using hot path (RPC) for ledger {tx_ledger} (age: {age} ledgers)");
        reconstruct_hot_path(tx_ledger, &rpc).await
    } else {
        tracing::info!("Using cold path (archive) for ledger {tx_ledger} (age: {age} ledgers)");
        reconstruct_cold_path(tx_ledger, network).await
    }
}

async fn reconstruct_hot_path(
    ledger_sequence: u32,
    _rpc: &crate::rpc::SorobanRpcClient,
) -> PrismResult<LedgerState> {
    Ok(LedgerState {
        ledger_sequence,
        entries: HashMap::new(),
        reconstruction_path: ReconstructionPath::HotPath,
    })
}

async fn reconstruct_cold_path(
    ledger_sequence: u32,
    _network: &NetworkConfig,
) -> PrismResult<LedgerState> {
    tracing::warn!("Cold path reconstruction is computationally heavy — this may take a while");

    Ok(LedgerState {
        ledger_sequence,
        entries: HashMap::new(),
        reconstruction_path: ReconstructionPath::ColdPath,
    })
}
