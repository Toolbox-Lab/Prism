use crate::error::GratResult;
use crate::replay::sandbox::{SandboxResult, TraceEvent, TraceEventType};
use crate::types::trace::{
    DiffChangeType, ResourceHotspot, ResourceLimits, ResourceProfile, StateDiff,
};
use std::collections::HashMap;

/// Number of host functions reported in the profile's ranked list.
const TOP_HOST_CALLS: usize = 5;

/// Share of the network limit at which a warning is emitted.
const WARNING_THRESHOLD_PERCENT: f64 = 90.0;

pub fn generate_profile(
    result: &SandboxResult,
    state_diff: &StateDiff,
    limits: ResourceLimits,
) -> GratResult<ResourceProfile> {
    let (total_read_bytes, total_write_bytes) = storage_bytes(state_diff);

    let mut profile = ResourceProfile {
        total_cpu: result.total_cpu,
        cpu_limit: limits.cpu_instructions,
        total_memory: result.total_memory,
        memory_limit: limits.memory_bytes,
        total_read_bytes,
        total_write_bytes,
        read_limit: limits.read_bytes,
        write_limit: limits.write_bytes,
        hotspots: rank_host_calls(result),
        warnings: Vec::new(),
    };

    warn_when_near_limit(
        &mut profile.warnings,
        "CPU",
        profile.total_cpu,
        profile.cpu_limit,
    );
    warn_when_near_limit(
        &mut profile.warnings,
        "Memory",
        profile.total_memory,
        profile.memory_limit,
    );
    warn_when_near_limit(
        &mut profile.warnings,
        "Read bytes",
        profile.total_read_bytes,
        profile.read_limit,
    );
    warn_when_near_limit(
        &mut profile.warnings,
        "Write bytes",
        profile.total_write_bytes,
        profile.write_limit,
    );

    Ok(profile)
}

/// Bytes of ledger state touched by the transaction, estimated from the XDR
/// size of the touched entries (hex encoded in the diff, so two characters are
/// one byte). This is a proxy for the network's read/write byte accounting,
/// which also charges footprint bookkeeping; it favours "a little low" over
/// inventing bytes that were never recorded.
fn storage_bytes(state_diff: &StateDiff) -> (u64, u64) {
    let mut read_bytes: u64 = 0;
    let mut write_bytes: u64 = 0;

    for entry in &state_diff.entries {
        match entry.change_type {
            DiffChangeType::Created => {
                if let Some(after) = &entry.after {
                    write_bytes += after.len() as u64 / 2;
                }
            }
            DiffChangeType::Updated => {
                if let Some(after) = &entry.after {
                    write_bytes += after.len() as u64 / 2;
                }
                if let Some(before) = &entry.before {
                    read_bytes += before.len() as u64 / 2;
                }
            }
            DiffChangeType::Deleted | DiffChangeType::Unchanged => {
                if let Some(before) = &entry.before {
                    read_bytes += before.len() as u64 / 2;
                }
            }
        }
    }

    let mut profile = ResourceProfile {
        total_cpu: result.total_cpu,
        cpu_limit: result.cpu_limit,
        total_memory: result.total_memory,
        memory_limit: result.memory_limit,
        total_read_bytes,
        total_write_bytes,
        read_limit: 0,
        write_limit: 0,
        hotspots: Vec::new(),
        warnings: Vec::new(),
    };

fn event_counter(event: &TraceEvent, key: &str) -> u64 {
    event
        .data
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

fn percentage(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 / whole as f64) * 100.0
    }
}

fn warn_when_near_limit(warnings: &mut Vec<String>, label: &str, used: u64, limit: u64) {
    if limit == 0 {
        return;
    }

    let usage = percentage(used, limit);
    if usage >= WARNING_THRESHOLD_PERCENT {
        warnings.push(format!(
            "{label} usage is at {usage:.0}% of the network limit — consider optimizing to stay \
             below the {WARNING_THRESHOLD_PERCENT:.0}% warning threshold"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::trace::{LedgerEntryDiff, TraceId};

    fn event(
        event_type: TraceEventType,
        function: &str,
        cpu_insns: u64,
        mem_bytes: u64,
    ) -> TraceEvent {
        TraceEvent {
            event_type,
            timestamp_us: 0,
            data: serde_json::json!({
                "function": function,
                "cpu_insns": cpu_insns,
                "mem_bytes": mem_bytes,
            }),
        }
    }

    fn call(function: &str, cpu_insns: u64, mem_bytes: u64) -> TraceEvent {
        event(
            TraceEventType::HostFunctionCall,
            function,
            cpu_insns,
            mem_bytes,
        )
    }

    fn ret(function: &str, cpu_insns: u64, mem_bytes: u64) -> TraceEvent {
        event(
            TraceEventType::HostFunctionReturn,
            function,
            cpu_insns,
            mem_bytes,
        )
    }

    fn sandbox_result(events: Vec<TraceEvent>, total_cpu: u64, total_memory: u64) -> SandboxResult {
        SandboxResult {
            trace_id: TraceId::new(),
            success: true,
            events,
            final_state: HashMap::new(),
            total_cpu,
            total_memory,
            diagnostic_events: Vec::new(),
        }
    }

    fn limits(cpu: u64, memory: u64, read: u64, write: u64) -> ResourceLimits {
        ResourceLimits {
            cpu_instructions: cpu,
            memory_bytes: memory,
            read_bytes: read,
            write_bytes: write,
        }
    }

    fn diff(
        before: Option<&str>,
        after: Option<&str>,
        change_type: DiffChangeType,
    ) -> LedgerEntryDiff {
        LedgerEntryDiff {
            key: "contract_data".to_string(),
            before: before.map(str::to_string),
            after: after.map(str::to_string),
            change_type,
        }
    }

    #[test]
    fn host_call_costs_exclude_nested_calls() {
        let result = sandbox_result(
            vec![
                call("call", 0, 0),
                call("storage_get", 0, 0),
                ret("storage_get", 50, 5),
                ret("call", 200, 20),
            ],
            200,
            20,
        );

        let profile = generate_profile(&result, &StateDiff::default(), limits(0, 0, 0, 0))
            .expect("profile generation should not fail");

        assert_eq!(profile.hotspots.len(), 2);
        let top = &profile.hotspots[0];
        assert_eq!(top.location, "call");
        assert_eq!(top.call_count, 1);
        assert_eq!(top.cpu_instructions, 150);
        assert_eq!(top.memory_bytes, 15);
        assert!((top.cpu_percentage - 75.0).abs() < f64::EPSILON);

        let nested = &profile.hotspots[1];
        assert_eq!(nested.location, "storage_get");
        assert_eq!(nested.cpu_instructions, 50);
        assert_eq!(nested.memory_bytes, 5);
        assert!((nested.cpu_percentage - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn repeated_calls_are_aggregated_and_ranked() {
        let result = sandbox_result(
            vec![
                call("storage_get", 0, 0),
                ret("storage_get", 10, 1),
                call("xfer", 10, 1),
                ret("xfer", 90, 11),
                call("storage_get", 90, 11),
                ret("storage_get", 120, 13),
            ],
            120,
            13,
        );

        let profile = generate_profile(&result, &StateDiff::default(), limits(0, 0, 0, 0))
            .expect("profile generation should not fail");

        assert_eq!(profile.hotspots[0].location, "xfer");
        assert_eq!(profile.hotspots[0].cpu_instructions, 80);
        assert_eq!(profile.hotspots[1].location, "storage_get");
        assert_eq!(profile.hotspots[1].call_count, 2);
        assert_eq!(profile.hotspots[1].cpu_instructions, 40);
    }

    #[test]
    fn unmatched_events_are_ignored() {
        let result = sandbox_result(
            vec![
                ret("ghost", 100, 10),
                call("dangling", 0, 0),
                call("real", 0, 0),
                ret("real", 25, 3),
            ],
            25,
            3,
        );

        let profile = generate_profile(&result, &StateDiff::default(), limits(0, 0, 0, 0))
            .expect("profile generation should not fail");

        assert_eq!(profile.hotspots.len(), 1);
        assert_eq!(profile.hotspots[0].location, "real");
        assert_eq!(profile.hotspots[0].cpu_instructions, 25);
    }

    #[test]
    fn hotspot_list_is_capped_at_the_top_five() {
        let mut events = Vec::new();
        for (index, name) in ["a", "b", "c", "d", "e", "f", "g"].iter().enumerate() {
            events.push(call(name, 0, 0));
            events.push(ret(name, (index as u64 + 1) * 10, 0));
        }
        let result = sandbox_result(events, 280, 0);

        let profile = generate_profile(&result, &StateDiff::default(), limits(0, 0, 0, 0))
            .expect("profile generation should not fail");

        let names: Vec<&str> = profile
            .hotspots
            .iter()
            .map(|hotspot| hotspot.location.as_str())
            .collect();
        assert_eq!(names, vec!["g", "f", "e", "d", "c"]);
    }

    #[test]
    fn limits_are_reported_and_warnings_fire_at_ninety_percent() {
        let result = sandbox_result(vec![], 950, 10);
        let entries = vec![
            diff(None, Some("aabb"), DiffChangeType::Created),
            diff(Some("aabbcc"), Some("ddee"), DiffChangeType::Updated),
            diff(Some("ff"), None, DiffChangeType::Deleted),
        ];

        let profile =
            generate_profile(&result, &StateDiff { entries }, limits(1000, 100, 100, 100))
                .expect("profile generation should not fail");

        assert_eq!(profile.cpu_limit, 1000);
        assert_eq!(profile.memory_limit, 100);
        assert_eq!(profile.read_limit, 100);
        assert_eq!(profile.write_limit, 100);
        assert_eq!(profile.total_read_bytes, 4);
        assert_eq!(profile.total_write_bytes, 4);
        assert_eq!(profile.warnings.len(), 1);
        assert!(profile.warnings[0].starts_with("CPU usage is at 95%"));
    }

    #[test]
    fn unknown_limits_produce_no_warnings_or_percentages() {
        let result = sandbox_result(
            vec![call("storage_get", 0, 0), ret("storage_get", 10, 0)],
            10,
            0,
        );

        let profile = generate_profile(&result, &StateDiff::default(), ResourceLimits::default())
            .expect("profile generation should not fail");

        assert_eq!(profile.warnings.len(), 0);
        assert_eq!(profile.cpu_limit, 0);
        assert_eq!(profile.hotspots[0].cpu_percentage, 100.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::trace::TraceId;

    fn sandbox_result(
        total_cpu: u64,
        cpu_limit: u64,
        total_memory: u64,
        memory_limit: u64,
    ) -> SandboxResult {
        SandboxResult {
            trace_id: TraceId::new(),
            success: true,
            events: Vec::new(),
            final_state: std::collections::HashMap::new(),
            total_cpu,
            total_memory,
            cpu_limit,
            memory_limit,
            diagnostic_events: Vec::new(),
        }
    }

    #[test]
    fn profile_records_effective_sandbox_limits() {
        let result = sandbox_result(1_000, 10_000_000, 2_000, 1_048_576);
        let profile = generate_profile(
            &result,
            &StateDiff {
                entries: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(profile.cpu_limit, 10_000_000);
        assert_eq!(profile.memory_limit, 1_048_576);
        assert!(profile.warnings.is_empty());
    }

    #[test]
    fn profile_warns_when_usage_is_near_the_applied_limits() {
        let result = sandbox_result(95, 100, 960, 1_000);
        let profile = generate_profile(
            &result,
            &StateDiff {
                entries: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(profile.cpu_limit, 100);
        assert_eq!(profile.memory_limit, 1_000);
        assert!(profile
            .warnings
            .iter()
            .any(|w| w.starts_with("CPU usage is at 95%")));
        assert!(profile
            .warnings
            .iter()
            .any(|w| w.starts_with("Memory usage is at 96%")));
    }
}
