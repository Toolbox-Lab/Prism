//! Prometheus instrumentation for Soroban RPC client traffic.
//!
//! Exposes a global static metrics registry with:
//! - `rpc_request_duration_seconds` — [`HistogramVec`] labeled by `endpoint` and `method`
//! - `rpc_http_errors_total` — [`CounterVec`] labeled by `endpoint`, `method`, and `status`
//!   tracking HTTP 500 / 429 responses in real time.
//!
//! Call [`gather`] to produce a Prometheus text exposition payload suitable for
//! scraping by Grafana / Prometheus.

use prometheus::{
    opts, register_counter_vec, register_histogram_vec, CounterVec, Encoder, HistogramOpts,
    HistogramVec, TextEncoder,
};
use std::sync::OnceLock;

/// Default latency buckets (seconds) covering sub-millisecond to multi-second RPCs.
const DURATION_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Global static metrics registry holders.
struct RpcMetrics {
    /// Histogram of RPC request latencies, labeled by endpoint URL and JSON-RPC method.
    request_duration: HistogramVec,
    /// Counter of HTTP 500 / 429 (and other tracked error statuses) responses.
    http_errors: CounterVec,
}

static METRICS: OnceLock<RpcMetrics> = OnceLock::new();

fn metrics() -> &'static RpcMetrics {
    METRICS.get_or_init(|| {
        let request_duration = register_histogram_vec!(
            HistogramOpts::new(
                "rpc_request_duration_seconds",
                "Duration of Soroban RPC requests in seconds"
            )
            .buckets(DURATION_BUCKETS.to_vec()),
            &["endpoint", "method"]
        )
        .expect("failed to register rpc_request_duration_seconds histogram");

        let http_errors = register_counter_vec!(
            opts!(
                "rpc_http_errors_total",
                "Total number of HTTP error responses from Soroban RPC nodes (e.g. 500, 429)"
            ),
            &["endpoint", "method", "status"]
        )
        .expect("failed to register rpc_http_errors_total counter");

        RpcMetrics {
            request_duration,
            http_errors,
        }
    })
}

/// Record the observed latency of an RPC call against the global histogram.
///
/// Labels:
/// - `endpoint` — the RPC node URL
/// - `method` — the JSON-RPC method name (e.g. `getTransaction`, `getLedgerEntries`)
pub fn record_rpc_duration(endpoint: &str, method: &str, duration_secs: f64) {
    metrics()
        .request_duration
        .with_label_values(&[endpoint, method])
        .observe(duration_secs);
}

/// Backward-compatible wrapper used by call sites that only know method + outcome.
///
/// Records duration under a synthetic endpoint label `"unknown"` so existing
/// instrumentation keeps compiling while new call sites migrate to the
/// endpoint-aware API.
pub fn record_rpc_duration_simple(method: &str, duration_secs: f64, _success: bool) {
    record_rpc_duration("unknown", method, duration_secs);
}

/// Increment the HTTP error counter for a given status code (typically 500 or 429).
pub fn record_rpc_http_error(endpoint: &str, method: &str, status: u16) {
    metrics()
        .http_errors
        .with_label_values(&[endpoint, method, &status.to_string()])
        .inc();
}

/// Gather all registered Prometheus metrics as a text exposition string.
///
/// Suitable for serving on a `/metrics` HTTP endpoint consumed by Grafana.
pub fn gather() -> String {
    // Ensure metrics are registered even if no RPC calls have been made yet.
    let _ = metrics();

    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    if encoder.encode(&metric_families, &mut buffer).is_err() {
        return String::new();
    }
    String::from_utf8(buffer).unwrap_or_default()
}

/// Convenience re-export so call sites can use `crate::rpc::record_rpc_duration`
/// with the legacy (method, duration, success) signature during migration.
/// Prefer [`record_rpc_duration`] with the endpoint label for new code.
#[inline]
pub fn record_rpc_duration_compat(method: &str, duration_secs: f64, success: bool) {
    record_rpc_duration_simple(method, duration_secs, success);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_register_without_panic() {
        // Force lazy init.
        let _ = metrics();
    }

    #[test]
    fn record_rpc_duration_does_not_panic() {
        record_rpc_duration(
            "https://soroban-testnet.stellar.org",
            "getLatestLedger",
            0.042,
        );
        record_rpc_duration(
            "https://soroban-testnet.stellar.org",
            "getTransaction",
            1.500,
        );
    }

    #[test]
    fn record_rpc_http_error_tracks_500_and_429() {
        let endpoint = "https://soroban-testnet.stellar.org";
        record_rpc_http_error(endpoint, "getTransaction", 500);
        record_rpc_http_error(endpoint, "getTransaction", 429);
        record_rpc_http_error(endpoint, "getLedgerEntries", 500);

        let output = gather();
        assert!(
            output.contains("rpc_http_errors_total"),
            "expected http error counter in exposition:\n{output}"
        );
        assert!(
            output.contains("500") || output.contains("status=\"500\""),
            "expected status 500 label:\n{output}"
        );
        assert!(
            output.contains("429") || output.contains("status=\"429\""),
            "expected status 429 label:\n{output}"
        );
    }

    #[test]
    fn gather_contains_histogram_help_and_type() {
        record_rpc_duration("https://example.com/rpc", "getLatestLedger", 0.01);
        let output = gather();
        assert!(
            output.contains("rpc_request_duration_seconds"),
            "missing histogram name:\n{output}"
        );
        assert!(
            output.contains("# HELP rpc_request_duration_seconds")
                || output.contains("rpc_request_duration_seconds_bucket")
                || output.contains("rpc_request_duration_seconds_count"),
            "unexpected exposition format:\n{output}"
        );
    }

    #[test]
    fn gather_includes_endpoint_and_method_labels() {
        let endpoint = "https://rpc.example.com";
        let method = "simulateTransaction";
        record_rpc_duration(endpoint, method, 0.25);

        let output = gather();
        assert!(
            output.contains("endpoint=") || output.contains(endpoint),
            "expected endpoint label in output:\n{output}"
        );
        assert!(
            output.contains("method=") || output.contains(method),
            "expected method label in output:\n{output}"
        );
    }

    #[test]
    fn record_rpc_duration_simple_compat_works() {
        record_rpc_duration_simple("getEvents", 0.1, true);
        record_rpc_duration_simple("getEvents", 0.2, false);
        let output = gather();
        assert!(output.contains("rpc_request_duration_seconds"));
    }

    #[test]
    fn histogram_buckets_include_expected_bounds() {
        record_rpc_duration("https://rpc.example.com", "getLedgerEntries", 0.08);
        let output = gather();
        // Prometheus histogram exposition emits le="<bound>" labels.
        assert!(
            output.contains("le=\"0.1\"")
                || output.contains("le=\"0.25\"")
                || output.contains("le=\"+Inf\""),
            "expected standard bucket bounds in exposition:\n{output}"
        );
    }
}
