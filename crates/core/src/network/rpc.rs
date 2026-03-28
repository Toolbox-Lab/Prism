//! Soroban RPC client.
//!
//! Communicates with Soroban RPC endpoints: `getTransaction`, `simulateTransaction`,
//! `getLedgerEntries`, `getEvents`, `getLatestLedger`. Handles pagination, retries,
//! and rate-limit backoff via [`super::jsonrpc::JsonRpcTransport`].

use crate::network::jsonrpc::{
    EmptyParams, GetEventsParams, GetLedgerEntriesParams, GetTransactionParams,
    JsonRpcRequest, JsonRpcTransport, SimulateTransactionParams,
};
use crate::types::config::NetworkConfig;
use crate::types::error::PrismResult;
use std::time::Duration;

/// Soroban RPC client with retry and rate-limit handling.
pub struct RpcClient {
    transport: JsonRpcTransport,
}

impl RpcClient {
    /// Create a new RPC client for the given network.
    ///
    /// The per-request timeout is taken from
    /// [`NetworkConfig::request_timeout_secs`] (default 30 s). Any request
    /// that does not complete within that window is cancelled and returns
    /// [`crate::types::error::PrismError::NetworkTimeout`].
    pub fn new(config: NetworkConfig) -> Self {
        let timeout = Duration::from_secs(config.request_timeout_secs);
        Self {
            transport: JsonRpcTransport::new(config.rpc_url, 3, timeout),
        }
    }

    /// Fetch a transaction by hash (`getTransaction`).
    pub async fn get_transaction(&self, tx_hash: &str) -> PrismResult<serde_json::Value> {
        let req = JsonRpcRequest::new(
            1,
            "getTransaction",
            GetTransactionParams { hash: tx_hash.to_owned() },
        );
        self.transport.call(&req).await
    }

    /// Simulate a transaction (`simulateTransaction`).
    pub async fn simulate_transaction(&self, tx_xdr: &str) -> PrismResult<serde_json::Value> {
        let req = JsonRpcRequest::new(
            1,
            "simulateTransaction",
            SimulateTransactionParams { transaction: tx_xdr.to_owned() },
        );
        self.transport.call(&req).await
    }

    /// Get ledger entries by keys (`getLedgerEntries`).
    pub async fn get_ledger_entries(&self, keys: &[String]) -> PrismResult<serde_json::Value> {
        let req = JsonRpcRequest::new(
            1,
            "getLedgerEntries",
            GetLedgerEntriesParams { keys: keys.to_vec() },
        );
        self.transport.call(&req).await
    }

    /// Get events matching a filter (`getEvents`).
    pub async fn get_events(
        &self,
        start_ledger: u32,
        filters: serde_json::Value,
    ) -> PrismResult<serde_json::Value> {
        let req = JsonRpcRequest::new(
            1,
            "getEvents",
            GetEventsParams { start_ledger, filters },
        );
        self.transport.call(&req).await
    }

    /// Get the latest ledger info (`getLatestLedger`).
    pub async fn get_latest_ledger(&self) -> PrismResult<serde_json::Value> {
        let req = JsonRpcRequest::new(1, "getLatestLedger", EmptyParams {});
        self.transport.call(&req).await
    }
}
