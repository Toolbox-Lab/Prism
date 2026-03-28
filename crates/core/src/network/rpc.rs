//! Soroban RPC client.
//!
//! Communicates with Soroban RPC endpoints: `getTransaction`, `simulateTransaction`,
//! `getLedgerEntries`, `getEvents`, `getLatestLedger`. Handles pagination, retries,
//! and rate limit backoff.

use crate::types::config::NetworkConfig;
use crate::types::error::{PrismError, PrismResult};
use serde::{Deserialize, Serialize};
use std::time::Instant;

// ── simulateTransaction response types ──────────────────────────────────────

/// Ledger footprint returned by `simulateTransaction`.
///
/// Contains the read-only and read-write ledger keys the transaction will
/// access, expressed as base64-encoded XDR `LedgerKey` values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateFootprint {
    /// Keys the transaction reads but does not modify.
    #[serde(rename = "readOnly", default)]
    pub read_only: Vec<String>,
    /// Keys the transaction reads and may modify.
    #[serde(rename = "readWrite", default)]
    pub read_write: Vec<String>,
}

/// Per-invocation authorization entry returned by `simulateTransaction`.
///
/// Each entry is a base64-encoded XDR `SorobanAuthorizationEntry` that the
/// caller must sign (or leave unsigned for `invoker_contract_auth`) before
/// submitting the transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateAuthEntry {
    /// Base64-encoded XDR `SorobanAuthorizationEntry`.
    pub xdr: String,
}

/// Resource cost estimates returned by `simulateTransaction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateCost {
    /// CPU instruction count consumed.
    #[serde(rename = "cpuInsns", default)]
    pub cpu_insns: String,
    /// Memory bytes consumed.
    #[serde(rename = "memBytes", default)]
    pub mem_bytes: String,
}

/// Soroban resource limits and fees returned by `simulateTransaction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateSorobanData {
    /// Base64-encoded XDR `SorobanTransactionData` (footprint + resource limits).
    pub data: String,
    /// Minimum resource fee in stroops.
    #[serde(rename = "minResourceFee")]
    pub min_resource_fee: String,
}

/// Typed response from the `simulateTransaction` RPC method.
///
/// Callers use `soroban_data` to stamp the transaction's `SorobanTransactionData`
/// extension and `auth` to populate the authorization entries before submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateTransactionResponse {
    /// Latest ledger sequence number at the time of simulation.
    #[serde(rename = "latestLedger")]
    pub latest_ledger: u32,
    /// Soroban resource data (footprint + fees) to attach to the transaction.
    #[serde(rename = "transactionData", default)]
    pub soroban_data: Option<String>,
    /// Minimum resource fee in stroops required for submission.
    #[serde(rename = "minResourceFee", default)]
    pub min_resource_fee: Option<String>,
    /// Authorization entries that must be signed before submission.
    #[serde(default)]
    pub auth: Vec<String>,
    /// Return value of the simulated invocation (base64 XDR `ScVal`), if any.
    #[serde(default)]
    pub results: Vec<SimulateResult>,
    /// Error message if the simulation failed.
    #[serde(default)]
    pub error: Option<String>,
    /// Diagnostic events emitted during simulation.
    #[serde(default)]
    pub events: Vec<String>,
    /// Cost estimates for the simulation.
    #[serde(default)]
    pub cost: Option<SimulateCost>,
}

/// A single invocation result within a `simulateTransaction` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateResult {
    /// Base64-encoded XDR `ScVal` return value.
    #[serde(default)]
    pub xdr: String,
    /// Authorization entries required for this invocation.
    #[serde(default)]
    pub auth: Vec<String>,
}

impl SimulateTransactionResponse {
    /// Returns `true` if the simulation completed without an error.
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    /// Convenience accessor for the first return value XDR, if present.
    pub fn return_value_xdr(&self) -> Option<&str> {
        self.results.first().map(|r| r.xdr.as_str())
    }
}

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

    /// Simulate a transaction against the current ledger state.
    ///
    /// Fires the `simulateTransaction` JSON-RPC method and returns a typed
    /// [`SimulateTransactionResponse`] containing:
    /// - `soroban_data` — the `SorobanTransactionData` XDR to stamp onto the
    ///   transaction before submission (footprint + resource limits).
    /// - `min_resource_fee` — the minimum fee in stroops required.
    /// - `auth` — authorization entries that must be signed by the relevant
    ///   parties before the transaction is submitted.
    /// - `results` — per-invocation return values.
    ///
    /// If the node returns an `error` field the method returns
    /// [`PrismError::RpcError`] so callers can surface the simulation failure
    /// without having to inspect the raw JSON.
    ///
    /// # Arguments
    /// * `tx_xdr` — base64-encoded XDR of the unsigned `TransactionEnvelope`.
    pub async fn simulate_transaction(
        &self,
        tx_xdr: &str,
    ) -> PrismResult<SimulateTransactionResponse> {
        let params = serde_json::json!({ "transaction": tx_xdr });
        let raw = self.call("simulateTransaction", params).await?;

        let response: SimulateTransactionResponse =
            serde_json::from_value(raw).map_err(|e| {
                PrismError::RpcError(format!("Failed to parse simulateTransaction response: {e}"))
            })?;

        // Surface simulation-level errors as a proper Rust error so callers
        // don't need to inspect the struct themselves.
        if let Some(ref err) = response.error {
            return Err(PrismError::RpcError(format!(
                "simulateTransaction failed: {err}"
            )));
        }

        Ok(response)
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
