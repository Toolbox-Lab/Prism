use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    Cpu,
    Memory,
    Reads,
    Writes,
}

impl MetricKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
            Self::Reads => "Reads",
            Self::Writes => "Writes",
        }
    }

    pub fn unit(&self) -> &'static str {
        match self {
            Self::Cpu => "instructions",
            Self::Memory | Self::Reads | Self::Writes => "bytes",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricDiagnostic {
    pub kind: MetricKind,
    pub name: String,
    pub unit: String,
    pub allocated: u64,
    pub consumed: u64,
    pub delta: i128,
    pub percentage_utilization: f64,
    pub breached_limit: bool,
}

impl MetricDiagnostic {
    pub fn format_summary(&self) -> String {
        let (label, diff_val) = if self.delta > 0 {
            ("Overage", self.delta as u64)
        } else {
            ("Remaining", (-self.delta) as u64)
        };

        format!(
            "{} Utilization: {}% (Allocated: {} {}, Consumed: {} {}. {}: {} {}).",
            self.name,
            format_percentage(self.percentage_utilization),
            format_number(self.allocated),
            self.unit,
            format_number(self.consumed),
            self.unit,
            label,
            format_number(diff_val),
            self.unit
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceDiagnostics {
    pub cpu: MetricDiagnostic,
    pub memory: MetricDiagnostic,
    pub reads: MetricDiagnostic,
    pub writes: MetricDiagnostic,
    pub breached_metrics: Vec<MetricKind>,
}

impl ResourceDiagnostics {
    pub fn metrics(&self) -> Vec<&MetricDiagnostic> {
        vec![&self.cpu, &self.memory, &self.reads, &self.writes]
    }

    pub fn get_metric(&self, kind: MetricKind) -> &MetricDiagnostic {
        match kind {
            MetricKind::Cpu => &self.cpu,
            MetricKind::Memory => &self.memory,
            MetricKind::Reads => &self.reads,
            MetricKind::Writes => &self.writes,
        }
    }

    pub fn breached_diagnostics(&self) -> Vec<&MetricDiagnostic> {
        self.metrics()
            .into_iter()
            .filter(|m| m.breached_limit)
            .collect()
    }

    pub fn has_breach(&self) -> bool {
        !self.breached_metrics.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ResourceMetrics {
    pub cpu_instructions: u64,
    pub memory_bytes: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
}

impl ResourceMetrics {
    pub fn new(
        cpu_instructions: u64,
        memory_bytes: u64,
        read_bytes: u64,
        write_bytes: u64,
    ) -> Self {
        Self {
            cpu_instructions,
            memory_bytes,
            read_bytes,
            write_bytes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TransactionResultMeta {
    pub resources_allocated: ResourceMetrics,
    pub resources_consumed: ResourceMetrics,
    pub is_budget_error: bool,
}

impl TransactionResultMeta {
    pub fn new(
        resources_allocated: ResourceMetrics,
        resources_consumed: ResourceMetrics,
        is_budget_error: bool,
    ) -> Self {
        Self {
            resources_allocated,
            resources_consumed,
            is_budget_error,
        }
    }

    pub fn from_tx_data(tx_data: &serde_json::Value) -> Self {
        let mut allocated = ResourceMetrics::default();
        let mut consumed = ResourceMetrics::default();
        let mut is_budget_error = false;

        if let Some(events) = tx_data.get("diagnosticEvents").and_then(|e| e.as_array()) {
            for event in events {
                if event.get("type").and_then(|t| t.as_str()) == Some("budget") {
                    if let Some(data) = event.get("data") {
                        let category = data.get("category").and_then(|c| c.as_str()).unwrap_or("");
                        let used = data
                            .get("used")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0);
                        let limit = data
                            .get("limit")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0);

                        match category {
                            "cpu" => {
                                consumed.cpu_instructions = used;
                                allocated.cpu_instructions = limit;
                            }
                            "memory" => {
                                consumed.memory_bytes = used;
                                allocated.memory_bytes = limit;
                            }
                            "read" => {
                                consumed.read_bytes = used;
                                allocated.read_bytes = limit;
                            }
                            "write" => {
                                consumed.write_bytes = used;
                                allocated.write_bytes = limit;
                            }
                            _ => {}
                        }

                        if used > limit {
                            is_budget_error = true;
                        }
                    }
                }
            }
        }

        if let Some(alloc_obj) = tx_data.get("resourcesAllocated") {
            allocated.cpu_instructions = alloc_obj
                .get("cpuInstructions")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(allocated.cpu_instructions);
            allocated.memory_bytes = alloc_obj
                .get("memoryBytes")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(allocated.memory_bytes);
            allocated.read_bytes = alloc_obj
                .get("readBytes")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(allocated.read_bytes);
            allocated.write_bytes = alloc_obj
                .get("writeBytes")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(allocated.write_bytes);
        }
        if let Some(cons_obj) = tx_data.get("resourcesConsumed") {
            consumed.cpu_instructions = cons_obj
                .get("cpuInstructions")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(consumed.cpu_instructions);
            consumed.memory_bytes = cons_obj
                .get("memoryBytes")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(consumed.memory_bytes);
            consumed.read_bytes = cons_obj
                .get("readBytes")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(consumed.read_bytes);
            consumed.write_bytes = cons_obj
                .get("writeBytes")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(consumed.write_bytes);
        }

        if tx_data.get("status").and_then(|s| s.as_str()) == Some("FAILED") {
            if let Some(err) = tx_data.get("error").and_then(|e| e.as_str()) {
                if err.to_lowercase().contains("budget")
                    || err.to_lowercase().contains("resourcelimitexceeded")
                {
                    is_budget_error = true;
                }
            }
        }

        Self {
            resources_allocated: allocated,
            resources_consumed: consumed,
            is_budget_error,
        }
    }
}

pub struct ResourceUsageAnalyzer;

impl ResourceUsageAnalyzer {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(&self, meta: &TransactionResultMeta) -> ResourceDiagnostics {
        Self::analyze_meta(meta)
    }

    pub fn analyze_meta(meta: &TransactionResultMeta) -> ResourceDiagnostics {
        let cpu = analyze_metric(
            MetricKind::Cpu,
            meta.resources_allocated.cpu_instructions,
            meta.resources_consumed.cpu_instructions,
            meta.is_budget_error,
        );
        let memory = analyze_metric(
            MetricKind::Memory,
            meta.resources_allocated.memory_bytes,
            meta.resources_consumed.memory_bytes,
            meta.is_budget_error,
        );
        let reads = analyze_metric(
            MetricKind::Reads,
            meta.resources_allocated.read_bytes,
            meta.resources_consumed.read_bytes,
            meta.is_budget_error,
        );
        let writes = analyze_metric(
            MetricKind::Writes,
            meta.resources_allocated.write_bytes,
            meta.resources_consumed.write_bytes,
            meta.is_budget_error,
        );

        let mut breached_metrics = Vec::new();
        if cpu.breached_limit {
            breached_metrics.push(MetricKind::Cpu);
        }
        if memory.breached_limit {
            breached_metrics.push(MetricKind::Memory);
        }
        if reads.breached_limit {
            breached_metrics.push(MetricKind::Reads);
        }
        if writes.breached_limit {
            breached_metrics.push(MetricKind::Writes);
        }

        ResourceDiagnostics {
            cpu,
            memory,
            reads,
            writes,
            breached_metrics,
        }
    }
}

impl Default for ResourceUsageAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

fn analyze_metric(
    kind: MetricKind,
    allocated: u64,
    consumed: u64,
    is_budget_error: bool,
) -> MetricDiagnostic {
    let delta = i128::from(consumed) - i128::from(allocated);
    let percentage_utilization = if allocated > 0 {
        (consumed as f64 / allocated as f64) * 100.0
    } else if consumed > 0 {
        100.0
    } else {
        0.0
    };

    let breached_limit =
        consumed > allocated || (is_budget_error && allocated > 0 && consumed >= allocated);

    MetricDiagnostic {
        kind,
        name: kind.name().to_string(),
        unit: kind.unit().to_string(),
        allocated,
        consumed,
        delta,
        percentage_utilization,
        breached_limit,
    }
}

fn format_number(mut val: u64) -> String {
    if val == 0 {
        return "0".to_string();
    }
    let mut parts = Vec::new();
    while val > 0 {
        let rem = val % 1000;
        val /= 1000;
        if val > 0 {
            parts.push(format!("{rem:03}"));
        } else {
            parts.push(rem.to_string());
        }
    }
    parts.reverse();
    parts.join(",")
}

fn format_percentage(val: f64) -> String {
    if val.fract() == 0.0 {
        format!("{val:.0}")
    } else {
        let s = format!("{val:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn enrich_report(
    report: &mut crate::types::report::DiagnosticReport,
    tx_data: &serde_json::Value,
) {
    let meta = TransactionResultMeta::from_tx_data(tx_data);
    let diag = ResourceUsageAnalyzer::analyze_meta(&meta);
    if diag.has_breach()
        || meta.resources_consumed.cpu_instructions > 0
        || meta.resources_consumed.memory_bytes > 0
        || meta.resources_consumed.read_bytes > 0
        || meta.resources_consumed.write_bytes > 0
    {
        report.resource_diagnostics = Some(diag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_limit_breach_format() {
        let meta = TransactionResultMeta::new(
            ResourceMetrics::new(50_000, 10_000, 1_000, 500),
            ResourceMetrics::new(45_000, 10_500, 800, 400),
            true,
        );

        let diagnostics = ResourceUsageAnalyzer::analyze_meta(&meta);
        let memory_summary = diagnostics.memory.format_summary();

        assert_eq!(
            memory_summary,
            "Memory Utilization: 105% (Allocated: 10,000 bytes, Consumed: 10,500 bytes. Overage: 500 bytes)."
        );
        assert!(diagnostics.memory.breached_limit);
        assert_eq!(diagnostics.breached_metrics, vec![MetricKind::Memory]);
    }

    #[test]
    fn test_all_four_metrics_delta_and_utilization() {
        let meta = TransactionResultMeta::new(
            ResourceMetrics::new(100_000, 20_000, 50_000, 10_000),
            ResourceMetrics::new(110_000, 15_000, 50_000, 5_000),
            false,
        );

        let diag = ResourceUsageAnalyzer::analyze_meta(&meta);

        assert_eq!(diag.cpu.delta, 10_000);
        assert!((diag.cpu.percentage_utilization - 110.0).abs() < 1e-6);
        assert!(diag.cpu.breached_limit);

        assert_eq!(diag.memory.delta, -5_000);
        assert_eq!(diag.memory.percentage_utilization, 75.0);
        assert!(!diag.memory.breached_limit);
        assert_eq!(
            diag.memory.format_summary(),
            "Memory Utilization: 75% (Allocated: 20,000 bytes, Consumed: 15,000 bytes. Remaining: 5,000 bytes)."
        );

        assert_eq!(diag.reads.delta, 0);
        assert_eq!(diag.reads.percentage_utilization, 100.0);
        assert!(!diag.reads.breached_limit);

        assert_eq!(diag.writes.delta, -5_000);
        assert_eq!(diag.writes.percentage_utilization, 50.0);
    }

    #[test]
    fn test_from_tx_data_extraction() {
        let tx_data = serde_json::json!({
            "status": "FAILED",
            "error": "Budget limit exceeded",
            "diagnosticEvents": [
                {
                    "type": "budget",
                    "data": { "category": "cpu", "used": 12000, "limit": 10000 }
                },
                {
                    "type": "budget",
                    "data": { "category": "memory", "used": 8000, "limit": 10000 }
                }
            ]
        });

        let meta = TransactionResultMeta::from_tx_data(&tx_data);
        assert!(meta.is_budget_error);
        assert_eq!(meta.resources_allocated.cpu_instructions, 10000);
        assert_eq!(meta.resources_consumed.cpu_instructions, 12000);
        assert_eq!(meta.resources_allocated.memory_bytes, 10000);
        assert_eq!(meta.resources_consumed.memory_bytes, 8000);

        let diag = ResourceUsageAnalyzer::analyze_meta(&meta);
        assert_eq!(diag.breached_metrics, vec![MetricKind::Cpu]);
    }

    #[test]
    fn test_resource_diagnostics_helpers() {
        let meta = TransactionResultMeta::new(
            ResourceMetrics::new(100, 100, 100, 100),
            ResourceMetrics::new(200, 50, 150, 80),
            true,
        );

        let diag = ResourceUsageAnalyzer::analyze_meta(&meta);
        assert!(diag.has_breach());
        assert_eq!(diag.breached_diagnostics().len(), 2);
        assert_eq!(diag.get_metric(MetricKind::Cpu).consumed, 200);
        assert_eq!(diag.metrics().len(), 4);
    }
}
