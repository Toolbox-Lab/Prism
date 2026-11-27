#![allow(dead_code)]

use grat_core::debugger::whatif::WhatIfResult;
use grat_core::types::{
    report::DiagnosticReport,
    trace::{DiffChangeType, ExecutionTrace, ResourceProfile, StateDiff},
};

pub mod auth_tree;
pub mod compact;
pub mod human;
pub mod json;
pub mod renderers;
pub mod theme;
pub mod trace_tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
    Short,
}

impl OutputFormat {
    pub fn parse(value: &str) -> Self {
        match value {
            "json" => Self::Json,
            "compact" | "short" => Self::Short,
            _ => Self::Human,
        }
    }
}

pub type OutputMode = OutputFormat;

pub fn print_diagnostic_report(
    report: &DiagnosticReport,
    output_format: &str,
) -> anyhow::Result<()> {
    match OutputFormat::parse(output_format) {
        OutputFormat::Json => json::print_report(report),
        OutputFormat::Short => compact::print_report(report),
        OutputFormat::Human => human::print_report(report),
    }
}

pub fn format_trace(trace: &ExecutionTrace, output_format: &str) -> anyhow::Result<String> {
    Ok(match OutputFormat::parse(output_format) {
        OutputFormat::Json => serde_json::to_string_pretty(trace)?,
        OutputFormat::Short => format_trace_summary(trace),
        OutputFormat::Human => {
            use termcolor::Buffer;
            let mut buffer = Buffer::no_color();
            trace_tree::render_trace_tree(&mut buffer, trace)
                .map_err(|error| anyhow::anyhow!("failed to render trace tree: {error}"))?;
            String::from_utf8_lossy(&buffer.into_inner()).into_owned()
        }
    })
}

pub fn print_resource_profile(
    profile: &ResourceProfile,
    output_format: &str,
) -> anyhow::Result<()> {
    match OutputFormat::parse(output_format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(profile)?),
        OutputFormat::Short => println!("{}", format_resource_profile_summary(profile)),
        OutputFormat::Human => {
            println!("{}", renderers::render_section_header("Resource Profile"));
            println!(
                "{}",
                renderers::BudgetBar::new("CPU", profile.total_cpu, profile.cpu_limit).render()
            );
            println!(
                "{}",
                renderers::BudgetBar::new("Memory", profile.total_memory, profile.memory_limit)
                    .render()
            );
            println!(
                "{}",
                renderers::BudgetBar::new("Read", profile.total_read_bytes, profile.read_limit)
                    .render()
            );
            println!(
                "{}",
                renderers::BudgetBar::new("Write", profile.total_write_bytes, profile.write_limit)
                    .render()
            );
            let palette = theme::ColorPalette::default();
            for warning in &profile.warnings {
                println!("{} {warning}", palette.warning_text("⚠"));
            }
            if !profile.hotspots.is_empty() {
                println!();
                println!(
                    "{}",
                    renderers::render_section_header("Top Host Calls by CPU")
                );
                println!("{}", renderers::render_host_call_table(profile));
            }
            println!();
            print!("{}", renderers::render_heatmap(profile));
        }
    }

    Ok(())
}
pub fn print_state_diff(diff: &StateDiff, output_format: &str) -> anyhow::Result<()> {
    match OutputFormat::parse(output_format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(diff)?),
        OutputFormat::Short => println!("{}", format_state_diff_summary(diff)),
        OutputFormat::Human => {
            let _palette = theme::ColorPalette::default();
            println!("{}", renderers::render_section_header("State Differential"));
            println!("{}", renderers::render_state_diff_table(diff));
            println!();
            println!("Summary: {}", format_state_diff_summary(diff));
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
    match OutputFormat::parse(output_format) {
        OutputFormat::Short => match (patch_file, patch_count) {
            (Some(path), Some(count)) => {
                println!("Status: Ready | Tx: {tx_hash} | Patches: {count} | Source: {path}");
            }
            _ => println!("Status: MissingModifyFile | Tx: {tx_hash}"),
        },
        OutputFormat::Json => {
            let payload = serde_json::json!({
                "tx_hash": tx_hash,
                "patch_file": patch_file,
                "patch_count": patch_count,
                "ready": patch_file.is_some(),
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputFormat::Human => match patch_file {
            Some(path) => println!("Patches loaded from {path}"),
            None => {
                println!(
                    "No --modify file provided. Use a JSON patch file to specify modifications."
                );
            }
        },
    }

    Ok(())
}

pub fn print_whatif_result(
    result: &WhatIfResult,
    patch_count: usize,
    output_format: &str,
) -> anyhow::Result<()> {
    match OutputFormat::parse(output_format) {
        OutputFormat::Short => println!("{}", format_whatif_summary(result, patch_count)),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&whatif_json(result, patch_count))?
        ),
        OutputFormat::Human => {
            let (original, modified) = (&result.original, &result.modified);
            println!("{}", renderers::render_section_header("What-If Simulation"));
            println!("Transaction:      {}", original.tx_hash);
            println!("Patches applied:  {patch_count}");
            println!(
                "Divergence point: {}",
                result.divergence_point.map_or_else(
                    || "none (matches the original run)".to_string(),
                    |p| p.to_string()
                )
            );
            println!("Summary:          {}", result.summary);
            println!();
            println!("{:<16}{:>16}{:>16}", "", "Original", "Modified");
            for (label, before, after) in [
                (
                    "Invocations",
                    original.invocations.len() as u64,
                    modified.invocations.len() as u64,
                ),
                (
                    "State changes",
                    original.state_diff.entries.len() as u64,
                    modified.state_diff.entries.len() as u64,
                ),
                (
                    "CPU",
                    original.resource_profile.total_cpu,
                    modified.resource_profile.total_cpu,
                ),
                (
                    "Memory",
                    original.resource_profile.total_memory,
                    modified.resource_profile.total_memory,
                ),
                (
                    "Write bytes",
                    original.resource_profile.total_write_bytes,
                    modified.resource_profile.total_write_bytes,
                ),
            ] {
                println!("{label:<16}{before:>16}{after:>16}");
            }
        }
    }

    Ok(())
}

fn format_whatif_summary(result: &WhatIfResult, patch_count: usize) -> String {
    format!(
        "Status: Complete | Tx: {} | Patches: {} | Divergence: {} | Invocations: {} -> {} | Changes: {} -> {}",
        result.original.tx_hash,
        patch_count,
        result
            .divergence_point
            .map_or_else(|| "none".to_string(), |p| p.to_string()),
        result.original.invocations.len(),
        result.modified.invocations.len(),
        result.original.state_diff.entries.len(),
        result.modified.state_diff.entries.len()
    )
}

fn whatif_json(result: &WhatIfResult, patch_count: usize) -> serde_json::Value {
    serde_json::json!({
        "tx_hash": result.original.tx_hash,
        "patch_count": patch_count,
        "divergence_point": result.divergence_point,
        "summary": result.summary,
        "original": result.original,
        "modified": result.modified,
    })
}

fn format_trace_summary(trace: &ExecutionTrace) -> String {
    format!(
        "Status: Complete | Tx: {} | Invocations: {} | Changes: {} | CPU: {}/{} | Write: {}/{}",
        trace.tx_hash,
        trace.invocations.len(),
        trace.state_diff.entries.len(),
        trace.resource_profile.total_cpu,
        trace.resource_profile.cpu_limit,
        trace.resource_profile.total_write_bytes,
        trace.resource_profile.write_limit
    )
}

fn format_resource_profile_summary(profile: &ResourceProfile) -> String {
    let warning_suffix = if profile.warnings.is_empty() {
        String::new()
    } else {
        format!(" | Warnings: {}", profile.warnings.len())
    };

    format!(
        "Status: Complete | CPU: {}/{} | Memory: {}/{} | Read: {}/{} | Write: {}/{}{}",
        profile.total_cpu,
        profile.cpu_limit,
        profile.total_memory,
        profile.memory_limit,
        profile.total_read_bytes,
        profile.read_limit,
        profile.total_write_bytes,
        profile.write_limit,
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
    use super::{format_whatif_summary, whatif_json, OutputFormat};
    use grat_core::debugger::whatif::WhatIfResult;
    use grat_core::types::trace::ExecutionTrace;

    #[test]
    fn parses_short_and_compact_as_short_mode() {
        assert_eq!(OutputFormat::parse("short"), OutputFormat::Short);
        assert_eq!(OutputFormat::parse("compact"), OutputFormat::Short);
    }

    fn trace(tx_hash: &str) -> ExecutionTrace {
        ExecutionTrace {
            tx_hash: tx_hash.to_string(),
            ledger_sequence: 100,
            network: "testnet".to_string(),
            invocations: Vec::new(),
            state_diff: Default::default(),
            resource_profile: Default::default(),
            diagnostic_events: Vec::new(),
            auth_chains: Vec::new(),
        }
    }

    fn whatif_result(divergence_point: Option<usize>) -> WhatIfResult {
        WhatIfResult {
            original: trace("abc123"),
            modified: trace("abc123"),
            divergence_point,
            summary: "Argument 0 changed".to_string(),
        }
    }

    #[test]
    fn whatif_summary_shows_patch_count_and_divergence_point() {
        let summary = format_whatif_summary(&whatif_result(Some(3)), 2);
        assert!(summary.starts_with("Status: Complete | Tx: abc123 | Patches: 2 | Divergence: 3 |"));
    }

    #[test]
    fn whatif_summary_reports_no_divergence() {
        let summary = format_whatif_summary(&whatif_result(None), 1);
        assert!(summary.contains("| Divergence: none |"), "{summary}");
    }

    #[test]
    fn whatif_json_includes_both_runs() {
        let json = whatif_json(&whatif_result(Some(0)), 4);
        assert_eq!(json["tx_hash"], "abc123");
        assert_eq!(json["patch_count"], 4);
        assert_eq!(json["divergence_point"], 0);
        assert_eq!(json["summary"], "Argument 0 changed");
        assert_eq!(json["original"]["tx_hash"], "abc123");
        assert_eq!(json["modified"]["ledger_sequence"], 100);

        let none = whatif_json(&whatif_result(None), 1);
        assert!(none["divergence_point"].is_null());
    }
}
