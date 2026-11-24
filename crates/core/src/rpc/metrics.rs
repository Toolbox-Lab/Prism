//! Prometheus instrumentation for Soroban RPC client traffic.

use prometheus::{
    opts, register_counter_vec, register_histogram_vec, CounterVec, Encoder, HistogramOpts,
    HistogramVec, TextEncoder,
};
use std::sync::OnceLock;

const DURATION_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

struct RpcMetrics {
    requests: CounterVec,
    request_duration: HistogramVec,
    retries: CounterVec,
    errors: CounterVec,
    http_errors: CounterVec,
}

static METRICS: OnceLock<RpcMetrics> = OnceLock::new();

fn metrics() -> &'static RpcMetrics {
    METRICS.get_or_init(|| {
        let requests = register_counter_vec!(
            opts!(
                "soroban_rpc_requests_total",
                "Total number of Soroban RPC requests"
            ),
            &["method", "status"]
        )
        .expect("failed to register soroban_rpc_requests_total counter");

        let request_duration = register_histogram_vec!(
            HistogramOpts::new(
                "soroban_rpc_request_duration_seconds",
                "Duration of Soroban RPC requests in seconds"
            )
            .buckets(DURATION_BUCKETS.to_vec()),
            &["method"]
        )
        .expect("failed to register soroban_rpc_request_duration_seconds histogram");

        let retries = register_counter_vec!(
            opts!(
                "soroban_rpc_retries_total",
                "Total number of Soroban RPC retry attempts"
            ),
            &["method"]
        )
        .expect("failed to register soroban_rpc_retries_total counter");

        let errors = register_counter_vec!(
            opts!(
                "soroban_rpc_errors_total",
                "Total number of Soroban RPC errors"
            ),
            &["error_code"]
        )
        .expect("failed to register soroban_rpc_errors_total counter");

        let http_errors = register_counter_vec!(
            opts!(
                "rpc_http_errors_total",
                "Total number of HTTP error responses from Soroban RPC nodes (e.g. 500, 429)"
            ),
            &["endpoint", "method", "status"]
        )
        .expect("failed to register rpc_http_errors_total counter");

        RpcMetrics {
            requests,
            request_duration,
            retries,
            errors,
            http_errors,
        }
    })
}

pub fn record_rpc_request(method: &str, status: &str) {
    metrics()
        .requests
        .with_label_values(&[method, status])
        .inc();
}

pub fn record_rpc_duration(endpoint: &str, method: &str, duration_secs: f64) {
    let _ = endpoint;
    metrics()
        .request_duration
        .with_label_values(&[method])
        .observe(duration_secs);
}

pub fn record_rpc_duration_simple(method: &str, duration_secs: f64, _success: bool) {
    record_rpc_duration("unknown", method, duration_secs);
}

pub fn record_rpc_retry(method: &str) {
    metrics().retries.with_label_values(&[method]).inc();
}

pub fn record_rpc_error(error_code: &str) {
    metrics().errors.with_label_values(&[error_code]).inc();
}

pub fn record_rpc_http_error(endpoint: &str, method: &str, status: u16) {
    let status = status.to_string();
    metrics()
        .http_errors
        .with_label_values(&[endpoint, method, &status])
        .inc();
    record_rpc_error(&status);
}

pub fn gather_metrics() -> String {
    let _ = metrics();
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    if encoder.encode(&prometheus::gather(), &mut buffer).is_err() {
        return String::new();
    }
    String::from_utf8(buffer).unwrap_or_default()
}

pub fn gather() -> String {
    gather_metrics()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gather_metrics_contains_required_metric_names() {
        record_rpc_request("getTransaction", "200");
        record_rpc_duration("https://example.com", "getTransaction", 0.042);
        record_rpc_retry("getTransaction");
        record_rpc_error("-32600");

        let output = gather_metrics();
        assert!(output.contains("soroban_rpc_requests_total"));
        assert!(output.contains("soroban_rpc_request_duration_seconds"));
        assert!(output.contains("soroban_rpc_retries_total"));
        assert!(output.contains("soroban_rpc_errors_total"));
    }
}
