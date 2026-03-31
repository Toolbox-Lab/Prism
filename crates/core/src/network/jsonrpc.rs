//! Generic JSON-RPC 2.0 client primitives.
//!
//! Provides strongly-typed request/response envelopes and a reusable HTTP
//! transport so every RPC call is validated at compile time via Serde.

use crate::types::error::{PrismError, PrismResult};
use serde::{Deserialize, Serialize};
use std::time::Instant;

// ── Wire types ────────────────────────────────────────────────────────────────

/// JSON-RPC 2.0 request envelope.
///
/// `T` is the method-specific params struct; it must implement [`Serialize`].
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest<T: Serialize> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'static str,
    pub params: T,
}

impl<T: Serialize> JsonRpcRequest<T> {
    /// Construct a request with the standard `"2.0"` version string.
    pub fn new(id: u64, method: &'static str, params: T) -> Self {
        Self { jsonrpc: "2.0", id, method, params }
    }
}

/// JSON-RPC 2.0 response envelope.
///
/// `T` is the method-specific result struct; it must implement [`Deserialize`].
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    pub jsonrpc: String,
    #[allow(dead_code)]
    pub id: u64,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error object returned inside a response.
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

// ── Soroban RPC param/result types ───────────────────────────────────────────

/// Params for `getTransaction`.
#[derive(Debug, Serialize)]
pub struct GetTransactionParams {
    pub hash: String,
}

/// Params for `simulateTransaction`.
#[derive(Debug, Serialize)]
pub struct SimulateTransactionParams {
    pub transaction: String,
}

/// Params for `getLedgerEntries`.
#[derive(Debug, Serialize)]
pub struct GetLedgerEntriesParams {
    pub keys: Vec<String>,
}

/// Params for `getEvents`.
#[derive(Debug, Serialize)]
pub struct GetEventsParams {
    #[serde(rename = "startLedger")]
    pub start_ledger: u32,
    pub filters: serde_json::Value,
}

/// Params for `getLatestLedger` — the method takes no parameters.
#[derive(Debug, Serialize)]
pub struct EmptyParams {}

/// Params for `getHealth` — the method takes no parameters.
pub type GetHealthParams = EmptyParams;

// ── Transport ─────────────────────────────────────────────────────────────────

/// Low-level JSON-RPC HTTP transport.
///
/// Handles serialization, deserialization, retry, and rate-limit backoff.
/// Higher-level clients (e.g. [`super::rpc::RpcClient`]) build on top of this.
pub struct JsonRpcTransport {
    client: reqwest::Client,
    endpoint: String,
    max_retries: u32,
}

impl JsonRpcTransport {
    /// Create a transport pointed at `endpoint` with the given retry limit.
    pub fn new(endpoint: impl Into<String>, max_retries: u32) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
            endpoint: endpoint.into(),
            max_retries,
        }
    }

    /// Execute a typed JSON-RPC call and return the typed result.
    ///
    /// Retries on network errors and HTTP 429 with exponential backoff.
    pub async fn call<P, R>(&self, request: &JsonRpcRequest<P>) -> PrismResult<R>
    where
        P: Serialize + std::fmt::Debug,
        R: for<'de> Deserialize<'de>,
    {
        let method = request.method;
        let mut last_error: Option<PrismError> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let backoff = std::time::Duration::from_millis(100 * 2u64.pow(attempt));
                tokio::time::sleep(backoff).await;
                tracing::debug!(attempt, method, "retrying RPC request");
            }

            let started_at = Instant::now();
            tracing::debug!(method, endpoint = %self.endpoint, attempt, "sending RPC request");

            match self.client.post(&self.endpoint).json(request).send().await {
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.map_err(|e| {
                        PrismError::RpcError(format!("response read error: {e}"))
                    })?;
                    let elapsed_ms = started_at.elapsed().as_millis();

                    tracing::debug!(
                        method,
                        endpoint = %self.endpoint,
                        attempt,
                        status = %status,
                        elapsed_ms,
                        "RPC response received"
                    );
                    tracing::trace!(method, elapsed_ms, response = %body, "RPC response payload");

                    if status == 429 {
                        tracing::warn!("rate limited by RPC endpoint, backing off");
                        last_error = Some(PrismError::RpcError("rate limited".to_string()));
                        continue;
                    }

                    let envelope: JsonRpcResponse<R> =
                        serde_json::from_str(&body).map_err(|e| {
                            PrismError::RpcError(format!("response parse error: {e}"))
                        })?;

                    if let Some(err) = envelope.error {
                        tracing::debug!(
                            method,
                            endpoint = %self.endpoint,
                            error = %err.message,
                            "RPC returned error response"
                        );
                        return Err(PrismError::RpcError(err.message));
                    }

                    return envelope
                        .result
                        .ok_or_else(|| PrismError::RpcError("empty result".to_string()));
                }
                Err(e) => {
                    tracing::debug!(
                        method,
                        endpoint = %self.endpoint,
                        attempt,
                        elapsed_ms = started_at.elapsed().as_millis(),
                        error = %e,
                        "RPC request failed"
                    );
                    last_error = Some(PrismError::RpcError(format!("request failed: {e}")));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| PrismError::RpcError("unknown error".to_string())))
    }
}
