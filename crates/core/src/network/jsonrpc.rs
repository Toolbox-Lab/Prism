//! Generic JSON-RPC 2.0 client primitives.
//!
//! Provides strongly-typed request/response envelopes and a reusable HTTP
//! transport so every RPC call is validated at compile time via Serde.
//!
//! # Timeouts
//!
//! [`JsonRpcTransport`] enforces a per-attempt wall-clock timeout via
//! [`tokio::time::timeout`]. The timeout covers the full round-trip: TCP
//! connect, request send, and response body read. If any attempt exceeds the
//! deadline, the future is cancelled and
//! [`PrismError::NetworkTimeout`] is returned immediately — no thread is
//! blocked and no resource is leaked.
//!
//! The default is [`crate::types::config::DEFAULT_REQUEST_TIMEOUT_SECS`]
//! (30 s). Pass a custom [`Duration`] to [`JsonRpcTransport::new`] to
//! override it.

use crate::types::error::{PrismError, PrismResult};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

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
/// Handles serialization, deserialization, per-attempt timeout, retry, and
/// rate-limit backoff. Higher-level clients (e.g. [`super::rpc::RpcClient`])
/// build on top of this.
///
/// # Timeout behaviour
///
/// Each attempt (send + body read) is wrapped in [`tokio::time::timeout`].
/// Exceeding the deadline cancels the in-flight future and returns
/// [`PrismError::NetworkTimeout`] — the retry loop then treats it like any
/// other transient failure and backs off before the next attempt.
pub struct JsonRpcTransport {
    /// Underlying HTTP client. No reqwest-level timeout is set; the async
    /// timeout in [`Self::call`] is the authoritative deadline.
    client: reqwest::Client,
    endpoint: String,
    max_retries: u32,
    /// Per-attempt timeout applied to the full send + body-read round-trip.
    timeout: Duration,
}

impl JsonRpcTransport {
    /// Create a transport pointed at `endpoint`.
    ///
    /// - `max_retries`: number of additional attempts after the first failure.
    /// - `timeout`: per-attempt deadline; use
    ///   [`Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS)`] for the
    ///   standard 30 s default.
    pub fn new(endpoint: impl Into<String>, max_retries: u32, timeout: Duration) -> Self {
        Self {
            // No reqwest-level timeout — tokio::time::timeout owns the deadline.
            client: reqwest::Client::builder()
                .build()
                .expect("failed to build HTTP client"),
            endpoint: endpoint.into(),
            max_retries,
            timeout,
        }
    }

    /// Execute a typed JSON-RPC call and return the typed result.
    ///
    /// Each attempt is bounded by `self.timeout`. On expiry the attempt is
    /// cancelled and [`PrismError::NetworkTimeout`] is stored as the last
    /// error. Retries use exponential backoff starting at 100 ms. HTTP 429
    /// responses are also retried.
    pub async fn call<P, R>(&self, request: &JsonRpcRequest<P>) -> PrismResult<R>
    where
        P: Serialize + std::fmt::Debug,
        R: for<'de> Deserialize<'de>,
    {
        let method = request.method;
        let timeout_secs = self.timeout.as_secs();
        let mut last_error: Option<PrismError> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let backoff = Duration::from_millis(100 * 2u64.pow(attempt));
                tokio::time::sleep(backoff).await;
                tracing::debug!(attempt, method, "retrying RPC request");
            }

            let started_at = Instant::now();
            tracing::debug!(
                method,
                endpoint = %self.endpoint,
                attempt,
                timeout_secs,
                "sending RPC request"
            );

            // Wrap the full send + body-read in a single timeout future so a
            // stalled connection cannot block the task indefinitely.
            let attempt_result = tokio::time::timeout(
                self.timeout,
                self.send_and_read(request),
            )
            .await;

            match attempt_result {
                // Timeout fired — cancel this attempt and record the error.
                Err(_elapsed) => {
                    let elapsed_ms = started_at.elapsed().as_millis();
                    tracing::warn!(
                        method,
                        endpoint = %self.endpoint,
                        attempt,
                        elapsed_ms,
                        timeout_secs,
                        "RPC request timed out"
                    );
                    last_error = Some(PrismError::NetworkTimeout {
                        method: method.to_string(),
                        timeout_secs,
                    });
                }

                // Request completed within the deadline.
                Ok(Ok((status, body))) => {
                    let elapsed_ms = started_at.elapsed().as_millis();
                    tracing::debug!(
                        method,
                        endpoint = %self.endpoint,
                        attempt,
                        status = %status,
                        elapsed_ms,
                        "RPC response received"
                    );
                    tracing::trace!(
                        method,
                        elapsed_ms,
                        response = %body,
                        "RPC response payload"
                    );

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

                // Network / transport error.
                Ok(Err(e)) => {
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

    /// Send the request and read the full response body.
    ///
    /// Returns `(status_code, body_text)`. Extracted so the timeout future
    /// has a clean boundary to cancel.
    async fn send_and_read<P: Serialize>(
        &self,
        request: &JsonRpcRequest<P>,
    ) -> Result<(u16, String), String> {
        let response = self
            .client
            .post(&self.endpoint)
            .json(request)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let body = response.text().await.map_err(|e| e.to_string())?;
        Ok((status, body))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::config::{NetworkConfig, DEFAULT_REQUEST_TIMEOUT_SECS};

    // ── Config / default timeout ──────────────────────────────────────────────

    #[test]
    fn default_timeout_is_30s() {
        let cfg = NetworkConfig::testnet();
        assert_eq!(cfg.request_timeout_secs, DEFAULT_REQUEST_TIMEOUT_SECS);
        assert_eq!(cfg.request_timeout_secs, 30);
    }

    #[test]
    fn custom_timeout_round_trips_through_serde() {
        let mut cfg = NetworkConfig::testnet();
        cfg.request_timeout_secs = 5;

        let json = serde_json::to_string(&cfg).expect("serialize");
        let decoded: NetworkConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.request_timeout_secs, 5);
    }

    #[test]
    fn missing_timeout_field_deserializes_to_default() {
        // Simulate a config file written before the timeout field existed.
        let json = r#"{
            "network": "testnet",
            "rpc_url": "https://soroban-testnet.stellar.org",
            "network_passphrase": "Test SDF Network ; September 2015",
            "archive_urls": []
        }"#;
        let cfg: NetworkConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(cfg.request_timeout_secs, DEFAULT_REQUEST_TIMEOUT_SECS);
    }

    // ── Timeout fires on a stalled connection ─────────────────────────────────

    /// Verifies that a transport with a 1 s timeout returns `NetworkTimeout`
    /// when the server never responds. Uses `tokio::time::pause` so the test
    /// completes instantly without real wall-clock delay.
    #[tokio::test]
    async fn timeout_fires_on_stalled_server() {
        // Bind a TCP listener that accepts connections but never writes back.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");

        // Accept the connection silently so the client doesn't get a
        // connection-refused error — it just hangs waiting for a response.
        tokio::spawn(async move {
            if let Ok((_stream, _)) = listener.accept().await {
                // Hold the stream open indefinitely to simulate a stall.
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        });

        tokio::time::pause();

        let transport = JsonRpcTransport::new(
            format!("http://{addr}"),
            0, // no retries — we want exactly one attempt
            Duration::from_secs(1),
        );
        let req = JsonRpcRequest::new(1, "getLatestLedger", EmptyParams {});

        // Advance mock time past the 1 s deadline.
        let call_fut = transport.call::<_, serde_json::Value>(&req);
        tokio::pin!(call_fut);

        // Poll once to start the future, then advance time.
        let result = tokio::select! {
            res = &mut call_fut => res,
            _ = async {
                tokio::time::advance(Duration::from_secs(2)).await;
            } => {
                call_fut.await
            }
        };

        tokio::time::resume();

        match result {
            Err(PrismError::NetworkTimeout { method, timeout_secs }) => {
                assert_eq!(method, "getLatestLedger");
                assert_eq!(timeout_secs, 1);
            }
            other => panic!("expected NetworkTimeout, got {other:?}"),
        }
    }

    /// Verifies that a successful (fast) response is not affected by the
    /// timeout machinery.
    #[tokio::test]
    async fn successful_response_not_affected_by_timeout() {
        use std::io::Write;

        // Minimal HTTP/1.1 response with a valid JSON-RPC body.
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{"sequence":100}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local_addr");

        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // Drain the request.
                let mut buf = [0u8; 4096];
                let _ = std::io::Read::read(&mut stream, &mut buf);
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let transport = JsonRpcTransport::new(
            format!("http://{addr}"),
            0,
            Duration::from_secs(5),
        );
        let req = JsonRpcRequest::new(1, "getLatestLedger", EmptyParams {});
        let result = transport.call::<_, serde_json::Value>(&req).await;

        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }
}
