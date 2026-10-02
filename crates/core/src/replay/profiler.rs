use crate::error::GratResult;
use crate::replay::sandbox::SandboxResult;
use crate::types::trace::{DiffChangeType, ResourceProfile, StateDiff};

pub fn generate_profile(
    result: &SandboxResult,
    state_diff: &StateDiff,
) -> GratResult<ResourceProfile> {
    let mut total_write_bytes: u64 = 0;
    let mut total_read_bytes: u64 = 0;

    for entry in &state_diff.entries {
        match entry.change_type {
            DiffChangeType::Created => {
                if let Some(after) = &entry.after {
                    total_write_bytes += after.len() as u64 / 2;
                }
            }
            DiffChangeType::Updated => {
                if let Some(after) = &entry.after {
                    total_write_bytes += after.len() as u64 / 2;
                }
                if let Some(before) = &entry.before {
                    total_read_bytes += before.len() as u64 / 2;
                }
            }
            DiffChangeType::Deleted | DiffChangeType::Unchanged => {
                if let Some(before) = &entry.before {
                    total_read_bytes += before.len() as u64 / 2;
                }
            }
        }
    }

    let mut profile = ResourceProfile {
        total_cpu: result.total_cpu,
        cpu_limit: 0,
        total_memory: result.total_memory,
        memory_limit: 0,
        total_read_bytes,
        total_write_bytes,
        read_limit: 0,
        write_limit: 0,
        hotspots: Vec::new(),
        warnings: Vec::new(),
    };

    if profile.cpu_limit > 0 {
        let cpu_usage = (profile.total_cpu as f64 / profile.cpu_limit as f64) * 100.0;
        if cpu_usage > 90.0 {
            profile.warnings.push(format!(
                "CPU usage is at {cpu_usage:.0}% of budget — consider increasing or optimizing"
            ));
        }
    }

    if profile.memory_limit > 0 {
        let mem_usage = (profile.total_memory as f64 / profile.memory_limit as f64) * 100.0;
        if mem_usage > 90.0 {
            profile.warnings.push(format!(
                "Memory usage is at {mem_usage:.0}% of budget — consider increasing or optimizing"
            ));
        }
    }

    if profile.write_limit > 0 {
        let write_usage = (profile.total_write_bytes as f64 / profile.write_limit as f64) * 100.0;
        if write_usage > 90.0 {
            profile.warnings.push(format!(
                "Write bytes usage is at {write_usage:.0}% of limit — consider reducing storage writes"
            ));
        }
    }

    Ok(profile)
}
