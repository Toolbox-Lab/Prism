//! Resource profiler — generates consumption profiles and identifies hotspots.

use crate::replay::sandbox::SandboxResult;
use crate::types::error::PrismResult;
use crate::types::trace::ResourceProfile;

/// Generate a resource consumption profile from sandbox execution results.
pub fn generate_profile(result: &SandboxResult) -> PrismResult<ResourceProfile> {
    let mut profile = ResourceProfile {
        total_cpu: result.total_cpu,
        cpu_limit: 0, // TODO: Extract from transaction resource limits
        total_memory: result.total_memory,
        memory_limit: 0,
        total_read_bytes: 0,
        total_write_bytes: 0,
        hotspots: Vec::new(),
        warnings: Vec::new(),
    };

    // Identify hotspots from trace events
    // TODO: Aggregate CPU/memory by function call to find the most expensive operations

    // Generate warnings for near-limit usage
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

    Ok(profile)
}
