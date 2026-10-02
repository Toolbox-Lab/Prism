use crate::error::{GratError, GratResult, JsonRpcError};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const BASE_DELAY_MS: u64 = 100;

const MAX_DELAY_MS: u64 = 10_000;

fn backoff_duration(attempt: u32) -> Duration {
    let ms = BASE_DELAY_MS.saturating_mul(2u64.saturating_pow(attempt));
    Duration::from_millis(ms.min(MAX_DELAY_MS))
}

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest<T: Serialize> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'static str,
    pub params: T,
}

impl<T: Serialize> JsonRpcRequest<T> {
    pub fn new(id: u64, method: &'static str, params: T) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method,
            params,
        }
    }
}

/// JSON-RPC 2.0 response envelope.
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

/// Low-level JSON-RPC HTTP transport.
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
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent(concat!("grat-cli/", env!("CARGO_PKG_VERSION")))
                .default_headers(headers)
                .build()
                .expect("failed to build HTTP client"),
            endpoint: endpoint.into(),
            max_retries,
        }
    }

    /// Execute a typed JSON-RPC call and return the typed result.
    /// Retries are triggered by:
    /// - Transport-level failures (connection refused, timeout, etc.)
    /// - HTTP 429 Too Many Requests
    /// - HTTP 5xx Server Errors (500–599)
    ///   Backoff follows `BASE_DELAY_MS × 2^attempt`, capped at `MAX_DELAY_MS`.
    pub async fn call<P, R>(&self, request: &JsonRpcRequest<P>) -> GratResult<R>
    where
        P: Serialize + std::fmt::Debug,
        R: for<'de> Deserialize<'de>,
    {
        let method = request.method;
        let mut last_error: Option<GratError> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let delay = backoff_duration(attempt);
                tracing::debug!(
                    attempt,
                    method,
                    delay_ms = delay.as_millis(),
                    "backing off before retry"
                );
                tokio::time::sleep(delay).await;
                tracing::debug!(attempt, method, "retrying RPC request");
            }

            // Start the Prometheus latency timer before the network request.
            let started_at = Instant::now();
            tracing::debug!(method, endpoint = %self.endpoint, attempt, "sending RPC request");

            match self.client.post(&self.endpoint).json(request).send().await {
                Ok(response) => {
                    let status = response.status();
                    let elapsed_ms = started_at.elapsed().as_millis();
                    // Exact latency delta recorded into the HistogramVec.
                    let duration_secs = started_at.elapsed().as_secs_f64();
                    crate::rpc::record_rpc_duration(&self.endpoint, method, duration_secs);

                    tracing::debug!(
                        method,
                        endpoint = %self.endpoint,
                        attempt,
                        status = %status,
                        elapsed_ms,
                        "RPC response received"
                    );

                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        crate::rpc::record_rpc_http_error(&self.endpoint, method, 429);
                        tracing::warn!(method, attempt, "rate limited by RPC endpoint, will retry");
                        last_error = Some(GratError::RpcError(format!(
                            "rate limited (attempt {attempt})"
                        )));
                        continue;
                    }

                    if status.is_server_error() {
                        crate::rpc::record_rpc_http_error(&self.endpoint, method, status.as_u16());
                        tracing::warn!(
                            method,
                            attempt,
                            status = %status,
                            elapsed_ms,
                            "RPC endpoint returned server error (5xx), will retry"
                        );
                        last_error = Some(GratError::RpcError(format!(
                            "server error {status} on attempt {attempt}"
                        )));
                        continue;
                    }

                    let body = response
                        .text()
                        .await
                        .map_err(|e| GratError::RpcError(format!("response read error: {e}")))?;

                    tracing::trace!(method, elapsed_ms, response = %body, "RPC response payload");

                    let envelope: JsonRpcResponse<R> = serde_json::from_str(&body)
                        .map_err(|e| GratError::RpcError(format!("response parse error: {e}")))?;

                    if let Some(err) = envelope.error {
                        tracing::debug!(
                            method,
                            endpoint = %self.endpoint,
                            error = %err.message,
                            code = err.code,
                            "RPC returned error response"
                        );
                        return Err(GratError::JsonRpc(err));
                    }

                    return envelope
                        .result
                        .ok_or_else(|| GratError::RpcError("empty result".to_string()));
                }
                Err(e) => {
                    let duration_secs = started_at.elapsed().as_secs_f64();
                    crate::rpc::record_rpc_duration(&self.endpoint, method, duration_secs);
                    tracing::debug!(
                        method,
                        endpoint = %self.endpoint,
                        attempt,
                        elapsed_ms = started_at.elapsed().as_millis(),
                        error = %e,
                        "RPC request failed"
                    );
                    last_error = Some(GratError::RpcError(format!("request failed: {e}")));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| GratError::RpcError("unknown error".to_string())))
    }
}
