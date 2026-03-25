//! Soroban RPC client.
//!
//! Communicates with Soroban RPC endpoints: `getTransaction`, `simulateTransaction`,
//! `getLedgerEntries`, `getEvents`, `getLatestLedger`. Handles pagination, retries,
//! and rate limit backoff.

use crate::types::config::NetworkConfig;
use crate::types::error::{PrismError, PrismResult};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Soroban RPC client with retry and rate-limit handling.
pub struct RpcClient {
    /// HTTP client instance.
    client: reqwest::Client,
    /// Network configuration.
    config: NetworkConfig,
    /// Maximum number of retries for failed requests.
    max_retries: u32,
}

/// JSON-RPC request envelope.
#[derive(Debug, Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: serde_json::Value,
}

/// JSON-RPC response envelope.
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

/// JSON-RPC error.
#[derive(Debug, Deserialize)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
}

impl RpcClient {
    /// Create a new RPC client for the given network.
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            config,
            max_retries: 3,
        }
    }

    /// Fetch a transaction by hash.
    pub async fn get_transaction(&self, tx_hash: &str) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({
            "hash": tx_hash,
        });
        self.call("getTransaction", params).await
    }

    /// Simulate a transaction.
    pub async fn simulate_transaction(&self, tx_xdr: &str) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({
            "transaction": tx_xdr,
        });
        self.call("simulateTransaction", params).await
    }

    /// Get ledger entries by keys.
    pub async fn get_ledger_entries(&self, keys: &[String]) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({
            "keys": keys,
        });
        self.call("getLedgerEntries", params).await
    }

    /// Get events matching a filter.
    pub async fn get_events(
        &self,
        start_ledger: u32,
        filters: serde_json::Value,
    ) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({
            "startLedger": start_ledger,
            "filters": filters,
        });
        self.call("getEvents", params).await
    }

    /// Get the latest ledger info.
    pub async fn get_latest_ledger(&self) -> PrismResult<serde_json::Value> {
        self.call("getLatestLedger", serde_json::json!({})).await
    }

    /// Internal JSON-RPC call with retry logic.
    async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> PrismResult<serde_json::Value> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method,
            params,
        };

        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let backoff = std::time::Duration::from_millis(100 * 2u64.pow(attempt));
                tokio::time::sleep(backoff).await;
                tracing::debug!("Retry attempt {attempt} for RPC method {method}");
            }

            let started_at = Instant::now();
            let request_body = serde_json::to_string(&request)
                .unwrap_or_else(|_| "<failed to serialize request>".to_string());
            tracing::debug!(
                method,
                endpoint = %self.config.rpc_url,
                attempt,
                "Sending RPC request"
            );
            tracing::trace!(
                method,
                endpoint = %self.config.rpc_url,
                attempt,
                request = %request_body,
                "RPC request payload"
            );

            match self
                .client
                .post(&self.config.rpc_url)
                .json(&request)
                .send()
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    let response_body = response
                        .text()
                        .await
                        .map_err(|e| PrismError::RpcError(format!("Response read error: {e}")))?;
                    let elapsed_ms = started_at.elapsed().as_millis();

                    tracing::debug!(
                        method,
                        endpoint = %self.config.rpc_url,
                        attempt,
                        status = %status,
                        elapsed_ms,
                        "RPC response received"
                    );
                    tracing::trace!(
                        method,
                        endpoint = %self.config.rpc_url,
                        attempt,
                        elapsed_ms,
                        response = %response_body,
                        "RPC response payload"
                    );

                    if status == 429 {
                        tracing::warn!("Rate limited by RPC, backing off...");
                        last_error = Some(PrismError::RpcError("Rate limited".to_string()));
                        continue;
                    }

                    let rpc_response: JsonRpcResponse = serde_json::from_str(&response_body)
                        .map_err(|e| PrismError::RpcError(format!("Response parse error: {e}")))?;

                    if let Some(err) = rpc_response.error {
                        tracing::debug!(
                            method,
                            endpoint = %self.config.rpc_url,
                            attempt,
                            error = %err.message,
                            "RPC returned an error response"
                        );
                        return Err(PrismError::RpcError(err.message));
                    }

                    return rpc_response
                        .result
                        .ok_or_else(|| PrismError::RpcError("Empty response".to_string()));
                }
                Err(e) => {
                    tracing::debug!(
                        method,
                        endpoint = %self.config.rpc_url,
                        attempt,
                        elapsed_ms = started_at.elapsed().as_millis(),
                        error = %e,
                        "RPC request failed"
                    );
                    last_error = Some(PrismError::RpcError(format!("Request failed: {e}")));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| PrismError::RpcError("Unknown error".to_string())))
    }
}
