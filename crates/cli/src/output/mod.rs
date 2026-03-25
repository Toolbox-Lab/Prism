//! Output formatting for CLI reports.

use prism_core::types::{
    report::DiagnosticReport,
    trace::{DiffChangeType, ExecutionTrace, ResourceProfile, StateDiff},
};

pub mod compact;
pub mod human;
pub mod json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
    Short,
}

impl OutputMode {
    pub fn parse(value: &str) -> Self {
        match value {
            "json" => Self::Json,
            "compact" | "short" => Self::Short,
            _ => Self::Human,
        }
    }
}

pub fn print_diagnostic_report(
    report: &DiagnosticReport,
    output_format: &str,
) -> anyhow::Result<()> {
    match OutputMode::parse(output_format) {
        OutputMode::Json => json::print_report(report),
        OutputMode::Short => compact::print_report(report),
        OutputMode::Human => human::print_report(report),
    }
}

pub fn format_trace(trace: &ExecutionTrace, output_format: &str) -> anyhow::Result<String> {
    Ok(match OutputMode::parse(output_format) {
        OutputMode::Json => serde_json::to_string_pretty(trace)?,
        OutputMode::Short => format_trace_summary(trace),
        OutputMode::Human => format!("{trace:#?}"),
    })
}

pub fn print_resource_profile(
    profile: &ResourceProfile,
    output_format: &str,
) -> anyhow::Result<()> {
    match OutputMode::parse(output_format) {
        OutputMode::Json => println!("{}", serde_json::to_string_pretty(profile)?),
        OutputMode::Short => println!("{}", format_resource_profile_summary(profile)),
        OutputMode::Human => {
            println!("{}", colored::Colorize::bold("Resource Profile"));
            println!(
                "CPU: {}/{} instructions",
                profile.total_cpu, profile.cpu_limit
            );
            println!(
                "Memory: {}/{} bytes",
                profile.total_memory, profile.memory_limit
            );
            for warning in &profile.warnings {
                println!("{} {warning}", colored::Colorize::yellow("!"));
            }
        }
    }

    Ok(())
}

pub fn print_state_diff(diff: &StateDiff, output_format: &str) -> anyhow::Result<()> {
    match OutputMode::parse(output_format) {
        OutputMode::Json => println!("{}", serde_json::to_string_pretty(diff)?),
        OutputMode::Short => println!("{}", format_state_diff_summary(diff)),
        OutputMode::Human => {
            println!("{}", colored::Colorize::bold("State Diff"));
            for entry in &diff.entries {
                let symbol = match entry.change_type {
                    DiffChangeType::Created => colored::Colorize::green("+"),
                    DiffChangeType::Deleted => colored::Colorize::red("-"),
                    DiffChangeType::Updated => colored::Colorize::yellow("~"),
                    DiffChangeType::Unchanged => colored::Colorize::dimmed(" "),
                };
                println!("{symbol} {}", entry.key);
            }
        }
    }

    Ok(())
}

pub fn print_whatif_status(
    tx_hash: &str,
    patch_file: Option<&str>,
    patch_count: Option<usize>,
    output_format: &str,
) -> anyhow::Result<()> {
    match OutputMode::parse(output_format) {
        OutputMode::Short => match (patch_file, patch_count) {
            (Some(path), Some(count)) => {
                println!("Status: Ready | Tx: {tx_hash} | Patches: {count} | Source: {path}");
            }
            _ => println!("Status: MissingModifyFile | Tx: {tx_hash}"),
        },
        OutputMode::Json => {
            let payload = serde_json::json!({
                "tx_hash": tx_hash,
                "patch_file": patch_file,
                "patch_count": patch_count,
                "ready": patch_file.is_some(),
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputMode::Human => {
            match patch_file {
                Some(path) => println!("Patches loaded from {path}"),
                None => {
                    println!("No --modify file provided. Use a JSON patch file to specify modifications.")
                }
            }
        }
    }

    Ok(())
}

fn format_trace_summary(trace: &ExecutionTrace) -> String {
    format!(
        "Status: Complete | Tx: {} | Invocations: {} | Changes: {} | CPU: {}/{}",
        trace.tx_hash,
        trace.invocations.len(),
        trace.state_diff.entries.len(),
        trace.resource_profile.total_cpu,
        trace.resource_profile.cpu_limit
    )
}

fn format_resource_profile_summary(profile: &ResourceProfile) -> String {
    let warning_suffix = if profile.warnings.is_empty() {
        String::new()
    } else {
        format!(" | Warnings: {}", profile.warnings.len())
    };

    format!(
        "Status: Complete | CPU: {}/{} | Memory: {}/{}{}",
        profile.total_cpu,
        profile.cpu_limit,
        profile.total_memory,
        profile.memory_limit,
        warning_suffix
    )
}

fn format_state_diff_summary(diff: &StateDiff) -> String {
    let mut created = 0usize;
    let mut updated = 0usize;
    let mut deleted = 0usize;

    for entry in &diff.entries {
        match entry.change_type {
            DiffChangeType::Created => created += 1,
            DiffChangeType::Updated => updated += 1,
            DiffChangeType::Deleted => deleted += 1,
            DiffChangeType::Unchanged => {}
        }
    }

    format!(
        "Status: Complete | Changes: {} | Created: {} | Updated: {} | Deleted: {}",
        diff.entries.len(),
        created,
        updated,
        deleted
    )
}

#[cfg(test)]
mod tests {
    use super::OutputMode;

    #[test]
    fn parses_short_and_compact_as_short_mode() {
        assert_eq!(OutputMode::parse("short"), OutputMode::Short);
        assert_eq!(OutputMode::parse("compact"), OutputMode::Short);
    }
}
