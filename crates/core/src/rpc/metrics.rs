use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

#[derive(Debug, Serialize, Deserialize)]
struct Histogram {
    bucket_counts: Vec<u64>,

    count: u64,

    sum: f64,
}

impl Histogram {
    fn new() -> Self {
        Self {
            bucket_counts: vec![0u64; BUCKETS.len()],
            count: 0,
            sum: 0.0,
        }
    }

    fn observe(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;

        for (i, &bound) in BUCKETS.iter().enumerate() {
            if value <= bound {
                self.bucket_counts[i] += 1;
            }
        }
    }

    fn render(&self, method: &str, outcome: &str, out: &mut String) {
        let mut cumulative = 0u64;

        for (i, &bound) in BUCKETS.iter().enumerate() {
            cumulative += self.bucket_counts[i];
            out.push_str(&format!(
                "rpc_request_duration_seconds_bucket\
                 {{method=\"{method}\",outcome=\"{outcome}\",le=\"{bound}\"}} {cumulative}\n"
            ));
        }

        out.push_str(&format!(
            "rpc_request_duration_seconds_bucket\
             {{method=\"{method}\",outcome=\"{outcome}\",le=\"+Inf\"}} {}\n",
            self.count
        ));
        out.push_str(&format!(
            "rpc_request_duration_seconds_count\
             {{method=\"{method}\",outcome=\"{outcome}\"}} {}\n",
            self.count
        ));
        out.push_str(&format!(
            "rpc_request_duration_seconds_sum\
             {{method=\"{method}\",outcome=\"{outcome}\"}} {:.9}\n",
            self.sum
        ));
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RpcMetricsRegistry {
    histograms: HashMap<String, Histogram>,
}

impl RpcMetricsRegistry {
    pub fn record(&mut self, method: &str, duration_secs: f64, success: bool) {
        let outcome = if success { "success" } else { "error" };
        let key = format!("{method}:{outcome}");
        self.histograms
            .entry(key)
            .or_insert_with(Histogram::new)
            .observe(duration_secs);
    }

    pub fn gather(&self) -> String {
        let mut out = String::with_capacity(512);
        out.push_str(
            "# HELP rpc_request_duration_seconds \
             Duration of Soroban RPC requests in seconds.\n",
        );
        out.push_str("# TYPE rpc_request_duration_seconds histogram\n");

        let mut keys: Vec<&String> = self.histograms.keys().collect();
        keys.sort();

        for key in keys {
            let hist = &self.histograms[key];
            let (method, outcome) = key.split_once(':').unwrap_or((key.as_str(), "unknown"));
            hist.render(method, outcome, &mut out);
        }

        out
    }
}

static REGISTRY: OnceLock<Mutex<RpcMetricsRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<RpcMetricsRegistry> {
    REGISTRY.get_or_init(|| Mutex::new(RpcMetricsRegistry::default()))
}

/// Record the duration of a single RPC round-trip in the global registry.
///
/// This function is called automatically by [`super::client::SorobanRpcClient`]
/// after every HTTP attempt (including retried ones). Callers outside the RPC
/// module generally do not need to call this directly.
///
/// # Arguments
/// * `method`        — JSON-RPC method name, e.g. `"getTransaction"`.
/// * `duration_secs` — Wall-clock time of the round-trip in seconds.
/// * `success`       — `true` when the call returned a valid result; `false`
///                     for any HTTP, network, parse, or RPC-level error.
///
/// # Panics
/// Does not panic. If the global mutex is poisoned the observation is silently
/// dropped to avoid crashing the caller.
pub fn record_rpc_duration(method: &str, duration_secs: f64, success: bool) {
    if let Ok(mut reg) = registry().lock() {
        reg.record(method, duration_secs, success);
    }
}

/// Render a snapshot of all RPC metrics in Prometheus text exposition format.
///
/// The returned string contains `HELP`, `TYPE`, and per-bucket `histogram`
/// lines and is suitable for serving directly from a `/metrics` HTTP endpoint.
///
/// # Example
/// ```
/// use prism_core::rpc::metrics;
///
/// // After some RPC activity …
/// let payload = metrics::gather();
/// assert!(payload.contains("rpc_request_duration_seconds"));
/// ```
pub fn gather() -> String {
    registry()
        .lock()
        .map(|reg| reg.gather())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_starts_empty() {
        let h = Histogram::new();
        assert_eq!(h.count, 0);
        assert_eq!(h.sum, 0.0);
        assert!(h.bucket_counts.iter().all(|&c| c == 0));
    }

    #[test]
    fn histogram_observe_increments_count_and_sum() {
        let mut h = Histogram::new();
        h.observe(0.05);
        h.observe(0.5);
        h.observe(5.0);

        assert_eq!(h.count, 3);
        assert!((h.sum - 5.55).abs() < 1e-9, "sum mismatch: {}", h.sum);
    }

    #[test]
    fn histogram_buckets_are_cumulative() {
        let mut h = Histogram::new();
        h.observe(0.08);

        let idx_005 = 0;
        let idx_01 = 1;
        let idx_025 = 2;
        let idx_05 = 3;
        let idx_01_s = 4; // 0.1

        assert_eq!(h.bucket_counts[idx_005], 0);
        assert_eq!(h.bucket_counts[idx_01], 0);
        assert_eq!(h.bucket_counts[idx_025], 0);
        assert_eq!(h.bucket_counts[idx_05], 0);
        assert_eq!(h.bucket_counts[idx_01_s], 1);
    }

    #[test]
    fn histogram_value_below_all_buckets_increments_first() {
        let mut h = Histogram::new();
        h.observe(0.001); // less than 0.005 (first bucket)
        assert_eq!(h.bucket_counts[0], 1);
    }

    #[test]
    fn histogram_value_above_all_buckets_not_in_finite_buckets() {
        let mut h = Histogram::new();
        h.observe(100.0); // above 10.0 (last bucket)
        assert!(
            h.bucket_counts.iter().all(|&c| c == 0),
            "value above all bounds should not increment any finite bucket"
        );
        assert_eq!(h.count, 1, "+Inf bucket == count");
    }

    #[test]
    fn registry_separates_success_and_error() {
        let mut reg = RpcMetricsRegistry::default();
        reg.record("getTransaction", 0.1, true);
        reg.record("getTransaction", 0.2, false);

        assert_eq!(reg.histograms.len(), 2);
        assert!(reg.histograms.contains_key("getTransaction:success"));
        assert!(reg.histograms.contains_key("getTransaction:error"));
    }

    #[test]
    fn registry_separates_methods() {
        let mut reg = RpcMetricsRegistry::default();
        reg.record("getTransaction", 0.1, true);
        reg.record("simulateTransaction", 0.5, true);

        assert_eq!(reg.histograms.len(), 2);
    }

    #[test]
    fn gather_contains_help_and_type_lines() {
        let reg = RpcMetricsRegistry::default();
        let output = reg.gather();
        assert!(output.contains("# HELP rpc_request_duration_seconds"));
        assert!(output.contains("# TYPE rpc_request_duration_seconds histogram"));
    }

    #[test]
    fn gather_produces_correct_labels() {
        let mut reg = RpcMetricsRegistry::default();
        reg.record("getLatestLedger", 0.042, true);

        let output = reg.gather();
        assert!(output.contains("method=\"getLatestLedger\""));
        assert!(output.contains("outcome=\"success\""));
        assert!(output.contains("le=\"+Inf\""));
    }

    #[test]
    fn gather_inf_bucket_equals_count() {
        let mut reg = RpcMetricsRegistry::default();
        reg.record("getLedgerEntries", 0.3, true);
        reg.record("getLedgerEntries", 0.7, true);
        reg.record("getLedgerEntries", 1.2, true);

        let output = reg.gather();
        assert!(
            output.contains(
                "rpc_request_duration_seconds_bucket\
                 {method=\"getLedgerEntries\",outcome=\"success\",le=\"+Inf\"} 3"
            ),
            "unexpected output:\n{output}"
        );
        assert!(
            output.contains(
                "rpc_request_duration_seconds_count\
                 {method=\"getLedgerEntries\",outcome=\"success\"} 3"
            ),
            "unexpected output:\n{output}"
        );
    }

    #[test]
    fn gather_output_is_sorted_by_key() {
        let mut reg = RpcMetricsRegistry::default();
        reg.record("simulateTransaction", 0.1, true);
        reg.record("getTransaction", 0.2, true);

        let output = reg.gather();
        let pos_get = output.find("method=\"getTransaction\"").unwrap();
        let pos_sim = output.find("method=\"simulateTransaction\"").unwrap();
        assert!(
            pos_get < pos_sim,
            "getTransaction should appear before simulateTransaction"
        );
    }

    #[test]
    fn record_rpc_duration_does_not_panic() {
        record_rpc_duration("getLatestLedger", 0.042, true);
        record_rpc_duration("getLatestLedger", 1.500, false);
    }

    #[test]
    fn global_gather_returns_prometheus_header() {
        let output = gather();
        assert!(output.contains("# HELP rpc_request_duration_seconds"));
    }
}
