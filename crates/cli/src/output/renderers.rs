//! Shared terminal renderers for CLI output.

#![allow(dead_code)]

use colored::Colorize;
use prism_core::types::report::TransactionContext;
use prism_core::types::trace::ResourceProfile;
use tabled::{Table, Tabled};

const BAR_WIDTH: usize = 10;

/// Render a boxed section header suitable for terminal report sections.
pub fn render_section_header(title: &str) -> String {
    SectionHeader::new(title).render()
}

/// Utility for rendering a clearly separated section heading.
pub struct SectionHeader<'a> {
    title: &'a str,
}

impl<'a> SectionHeader<'a> {
    pub fn new(title: &'a str) -> Self {
        Self { title }
    }

    pub fn render(&self) -> String {
        let normalized_title = self.title.trim().to_uppercase();
        let inner = format!(" {} ", normalized_title);
        let border = format!("+{}+", "-".repeat(inner.chars().count()));
        let middle = format!("|{}|", inner);

        let border = border.cyan().bold().to_string();
        let middle = middle.white().bold().to_string();

        format!("{}\n{}\n{}", border, middle, border)
    }
}

/// Renders a colored budget utilization bar for Soroban resource usage.
pub struct BudgetBar {
    label: &'static str,
    used: u64,
    limit: u64,
}

impl BudgetBar {
    pub fn new(label: &'static str, used: u64, limit: u64) -> Self {
        Self { label, used, limit }
    }

    pub fn render(&self) -> String {
        let pct = if self.limit > 0 {
            (self.used as f64 / self.limit as f64).min(1.0)
        } else {
            0.0
        };

        let filled = (pct * BAR_WIDTH as f64).round() as usize;
        let empty = BAR_WIDTH - filled;
        let bar_str = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

        let colored_bar = if pct >= 0.9 {
            bar_str.red().bold().to_string()
        } else if pct >= 0.7 {
            bar_str.yellow().to_string()
        } else {
            bar_str.green().to_string()
        };

        format!(
            "{:<6} [{}] {}/{} ({:.0}%)",
            self.label,
            colored_bar,
            self.used,
            self.limit,
            pct * 100.0
        )
    }
}

// Heatmap block characters ordered from coldest to hottest.
const HEAT_BLOCKS: [&str; 4] = ["░", "▒", "▓", "█"];

/// Map a 0.0–1.0 intensity to a colored block character.
fn heat_cell(intensity: f64) -> String {
    let block = if intensity >= 0.75 {
        HEAT_BLOCKS[3]
    } else if intensity >= 0.5 {
        HEAT_BLOCKS[2]
    } else if intensity >= 0.25 {
        HEAT_BLOCKS[1]
    } else {
        HEAT_BLOCKS[0]
    };

    // Repeat the block to fill a fixed cell width of 10 chars.
    let filled = (intensity * BAR_WIDTH as f64).round() as usize;
    let empty = BAR_WIDTH - filled;
    let cell = format!("{}{}", block.repeat(filled), "░".repeat(empty));

    if intensity >= 0.75 {
        cell.red().bold().to_string()
    } else if intensity >= 0.5 {
        cell.yellow().to_string()
    } else if intensity >= 0.25 {
        cell.cyan().to_string()
    } else {
        cell.dimmed().to_string()
    }
}

/// Render a resource heatmap grid from a `ResourceProfile`.
///
/// Rows = hotspot locations (contract functions).
/// Columns = CPU, Memory, Reads, Writes.
/// Cell intensity is relative to the hottest value in each column.
pub fn render_heatmap(profile: &ResourceProfile) -> String {
    if profile.hotspots.is_empty() {
        return format!(
            "{}\n  {}\n",
            render_section_header("Resource Heatmap"),
            "No hotspot data available.".dimmed()
        );
    }

    // Column max values for normalisation.
    let max_cpu = profile
        .hotspots
        .iter()
        .map(|h| h.cpu_instructions)
        .max()
        .unwrap_or(1)
        .max(1);
    let max_mem = profile
        .hotspots
        .iter()
        .map(|h| h.memory_bytes)
        .max()
        .unwrap_or(1)
        .max(1);
    // Reads/writes aren't on ResourceHotspot yet, so we derive them from the
    // profile totals split evenly as a placeholder until the type is extended.
    let total_io = (profile.total_read_bytes + profile.total_write_bytes).max(1);

    // Label column width — pad to the longest location name.
    let label_width = profile
        .hotspots
        .iter()
        .map(|h| h.location.len())
        .max()
        .unwrap_or(8)
        .max(8);

    let col_width = BAR_WIDTH + 2; // cell + 2 spaces padding

    // Header row.
    let mut out = String::new();
    out.push_str(&render_section_header("Resource Heatmap"));
    out.push('\n');
    out.push_str(&format!(
        "  {:<lw$}  {:<cw$}  {:<cw$}  {:<cw$}  {:<cw$}\n",
        "Function",
        "CPU",
        "Memory",
        "Reads",
        "Writes",
        lw = label_width,
        cw = col_width,
    ));
    out.push_str(&format!(
        "  {}\n",
        "-".repeat(label_width + 4 * (col_width + 2) + 6)
    ));

    // Data rows.
    for hotspot in &profile.hotspots {
        let cpu_intensity = hotspot.cpu_instructions as f64 / max_cpu as f64;
        let mem_intensity = hotspot.memory_bytes as f64 / max_mem as f64;

        // Approximate read/write split: use cpu_percentage as a proxy weight
        // until ResourceHotspot gains dedicated read/write fields.
        let weight = hotspot.cpu_percentage / 100.0;
        let read_intensity = (profile.total_read_bytes as f64 * weight / total_io as f64).min(1.0);
        let write_intensity =
            (profile.total_write_bytes as f64 * weight / total_io as f64).min(1.0);

        let label = if hotspot.location.len() > label_width {
            format!("{}…", &hotspot.location[..label_width - 1])
        } else {
            hotspot.location.clone()
        };

        out.push_str(&format!(
            "  {:<lw$}  {}  {}  {}  {}\n",
            label,
            heat_cell(cpu_intensity),
            heat_cell(mem_intensity),
            heat_cell(read_intensity),
            heat_cell(write_intensity),
            lw = label_width,
        ));
    }

    // Legend.
    out.push('\n');
    out.push_str(&format!(
        "  Legend: {} cold  {} low  {} medium  {} hot\n",
        "░░░░░░░░░░".dimmed(),
        "▒▒▒▒▒▒▒▒▒▒".cyan(),
        "▓▓▓▓▓▓▓▓▓▓".yellow(),
        "██████████".red().bold(),
    ));

    out
}

/// A single row in the context table representing a decoded argument.
#[derive(Tabled)]
struct ArgumentRow {
    #[tabled(rename = "Argument")]
    index: usize,
    #[tabled(rename = "Value")]
    value: String,
}

/// Renders decoded contract arguments as a clean table.
///
/// Displays arguments in a grid format with columns for Argument and Value.
/// This makes it much easier to read than nested JSON when viewed in the terminal.
pub fn render_context_table(context: &TransactionContext) -> String {
    if context.arguments.is_empty() {
        return String::new();
    }

    let rows: Vec<ArgumentRow> = context
        .arguments
        .iter()
        .enumerate()
        .map(|(index, value)| ArgumentRow {
            index: index + 1,
            value: value.clone(),
        })
        .collect();

    let table = Table::new(rows).to_string();

    let mut output = String::new();
    if let Some(function_name) = &context.function_name {
        output.push_str(&format!("Function: {}\n", function_name));
    }
    output.push_str("Arguments:\n");
    output.push_str(&table);

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::types::report::{
        DiagnosticReport, FeeBreakdown, ResourceSummary, Severity, SuggestedFix,
    };
    use prism_core::types::trace::ResourceHotspot;

    fn make_profile(hotspots: Vec<ResourceHotspot>) -> ResourceProfile {
        ResourceProfile {
            total_cpu: hotspots.iter().map(|h| h.cpu_instructions).sum(),
            cpu_limit: 1_000_000,
            total_memory: hotspots.iter().map(|h| h.memory_bytes).sum(),
            memory_limit: 1_000_000,
            total_read_bytes: 0,
            total_write_bytes: 0,
            hotspots,
            warnings: vec![],
        }
    }

    fn create_test_report() -> DiagnosticReport {
        DiagnosticReport {
            error_category: "Budget".to_string(),
            error_code: 1,
            error_name: "cpu_limit_exceeded".to_string(),
            summary: "CPU usage exceeded limit".to_string(),
            detailed_explanation: "The contract used more CPU than allowed.".to_string(),
            severity: Severity::Error,
            root_causes: vec![],
            suggested_fixes: vec![
                SuggestedFix {
                    description: "Reduce the number of loop iterations".to_string(),
                    difficulty: "easy".to_string(),
                    requires_upgrade: false,
                    example: Some("Use for_each instead of iterate".to_string()),
                },
                SuggestedFix {
                    description: "Optimize your contract logic".to_string(),
                    difficulty: "medium".to_string(),
                    requires_upgrade: false,
                    example: None,
                },
                SuggestedFix {
                    description: "Upgrade to a newer contract version".to_string(),
                    difficulty: "hard".to_string(),
                    requires_upgrade: true,
                    example: None,
                },
            ],
            contract_error: None,
            transaction_context: None,
            related_errors: vec![],
        }
    }

    #[test]
    fn section_header_renders_boxed_uppercase_title() {
        let rendered = SectionHeader::new("Transaction Summary").render();
        assert!(rendered.contains("TRANSACTION SUMMARY"));
        assert!(rendered.contains("+"));
        assert!(rendered.contains("|"));
    }

    #[test]
    fn section_header_function_trims_title() {
        let rendered = render_section_header("  network info  ");
        assert!(rendered.contains("NETWORK INFO"));
    }

    #[test]
    fn budget_bar_renders_with_zero_limit() {
        let bar = BudgetBar::new("CPU", 0, 0).render();
        assert!(bar.contains("CPU"));
        assert!(bar.contains("0%"));
    }

    #[test]
    fn heatmap_empty_hotspots_shows_no_data_message() {
        let profile = make_profile(vec![]);
        let output = render_heatmap(&profile);
        assert!(output.contains("No hotspot data available."));
    }

    #[test]
    fn heatmap_renders_function_names() {
        let profile = make_profile(vec![
            ResourceHotspot {
                location: "transfer::invoke".to_string(),
                cpu_instructions: 800_000,
                cpu_percentage: 80.0,
                memory_bytes: 300_000,
                memory_percentage: 30.0,
            },
            ResourceHotspot {
                location: "storage::get".to_string(),
                cpu_instructions: 200_000,
                cpu_percentage: 20.0,
                memory_bytes: 100_000,
                memory_percentage: 10.0,
            },
        ]);
        let output = render_heatmap(&profile);
        assert!(output.contains("transfer::invoke"));
        assert!(output.contains("storage::get"));
        assert!(output.contains("CPU"));
        assert!(output.contains("Memory"));
        assert!(output.contains("Legend"));
    }

    #[test]
    fn test_render_context_table_with_arguments() {
        let context = TransactionContext {
            tx_hash: "abc123".to_string(),
            ledger_sequence: 12345,
            function_name: Some("transfer".to_string()),
            arguments: vec![
                "GABC123...".to_string(),
                "GDEF456...".to_string(),
                "1000".to_string(),
            ],
            fee: FeeBreakdown {
                inclusion_fee: 100,
                resource_fee: 50,
                refundable_fee: 25,
                non_refundable_fee: 25,
            },
            resources: ResourceSummary {
                cpu_instructions_used: 1000,
                cpu_instructions_limit: 10000,
                memory_bytes_used: 5000,
                memory_bytes_limit: 50000,
                read_bytes: 1000,
                write_bytes: 500,
            },
        };

        let output = render_context_table(&context);

        assert!(output.contains("Function: transfer"));
        assert!(output.contains("Arguments:"));
        assert!(output.contains("GABC123..."));
        assert!(output.contains("GDEF456..."));
        assert!(output.contains("1000"));
    }

    #[test]
    fn test_render_context_table_empty() {
        let context = TransactionContext {
            tx_hash: "abc123".to_string(),
            ledger_sequence: 12345,
            function_name: None,
            arguments: vec![],
            fee: FeeBreakdown {
                inclusion_fee: 100,
                resource_fee: 50,
                refundable_fee: 25,
                non_refundable_fee: 25,
            },
            resources: ResourceSummary {
                cpu_instructions_used: 1000,
                cpu_instructions_limit: 10000,
                memory_bytes_used: 5000,
                memory_bytes_limit: 50000,
                read_bytes: 1000,
                write_bytes: 500,
            },
        };

        let output = render_context_table(&context);

        assert!(output.is_empty());
    }
}
