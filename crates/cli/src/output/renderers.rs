#![allow(dead_code)]

use crate::output::theme::ColorPalette;
use colored::Colorize;
use grat_core::types::report::{DiagnosticReport, FeeBreakdown, RootCause, TransactionContext};
use grat_core::types::trace::ResourceProfile;
use tabled::{Table, Tabled};

const BAR_WIDTH: usize = 10;
const HEAT_BLOCKS: [&str; 4] = ["░", "▒", "▓", "█"];

pub fn render_section_header(title: &str) -> String {
    SectionHeader::new(title).render()
}

pub fn render_error_card(report: &DiagnosticReport) -> String {
    ErrorCard::new(report).render()
}

pub fn render_fix_list(fixes: &[grat_core::types::report::SuggestedFix]) -> String {
    FixList::new(fixes).render()
}

pub fn render_cause_list(causes: &[RootCause]) -> String {
    CauseList::new(causes).render()
}

pub fn render_state_diff_table(diff: &grat_core::types::trace::StateDiff) -> String {
    StateDiffTable::new(diff).render()
}

pub struct SectionHeader<'a> {
    title: &'a str,
}

impl<'a> SectionHeader<'a> {
    pub fn new(title: &'a str) -> Self {
        Self { title }
    }

    pub fn render(&self) -> String {
        let normalized_title = self.title.trim().to_uppercase();
        let inner = format!(" {normalized_title} ");
        let border = format!("+{}+", "-".repeat(inner.chars().count()));
        let middle = format!("|{inner}|");

        let palette = ColorPalette::default();
        let border = palette.metadata_text(&border);
        let middle = palette.accent_text(&middle);

        format!("{border}\n{middle}\n{border}")
    }
}

/// Displays transaction errors with a bold red border and categorical labels.
pub struct ErrorCard<'a> {
    report: &'a DiagnosticReport,
}

impl<'a> ErrorCard<'a> {
    pub fn new(report: &'a DiagnosticReport) -> Self {
        Self { report }
    }

    pub fn render(&self) -> String {
        let mut output = String::new();

        let category_badge = format!("[{}]", self.report.error_category.to_uppercase());
        let error_line = format!(" {} ({})", self.report.error_name, self.report.error_code);

        let max_width = error_line
            .len()
            .max(self.report.summary.len())
            .max(category_badge.len())
            + 4;
        let border = "█".repeat(max_width);

        let border_colored = border.red().bold().to_string();
        let category_colored = category_badge.red().bold().to_string();
        let error_colored = error_line.red().bold().to_string();
        let summary_colored = self.report.summary.white().to_string();

        output.push_str(&format!("{border_colored}\n"));
        output.push_str(&format!("{} {}\n", "█".red().bold(), category_colored));
        output.push_str(&format!("{} {}\n", "█".red().bold(), error_colored));

        if let Some(contract_error) = &self.report.contract_error {
            let component_line = format!("Component: {}", contract_error.contract_id);
            output.push_str(&format!(
                "{} {}\n",
                "█".red().bold(),
                component_line.white()
            ));
        }

        output.push_str(&format!("{} {}\n", "█".red().bold(), summary_colored));
        output.push_str(&format!("{border_colored}\n"));

        output
    }
}

pub struct FixList<'a> {
    fixes: &'a [grat_core::types::report::SuggestedFix],
}

impl<'a> FixList<'a> {
    pub fn new(fixes: &'a [grat_core::types::report::SuggestedFix]) -> Self {
        Self { fixes }
    }

    pub fn render(&self) -> String {
        if self.fixes.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        let palette = ColorPalette::default();

        output.push_str(&palette.accent_text("SUGGESTED FIXES\n"));

        for fix in self.fixes {
            let fix_id = format!("[fix:{}]", fix.id).cyan();
            output.push_str(&format!("  • {} {}\n", fix_id, fix.description));

            if let Some(code) = &fix.remedy_code {
                let code_block = format!("    ```\n    {}\n    ```", code.trim())
                    .dimmed()
                    .to_string();
                output.push_str(&format!("{code_block}\n"));
            }
        }

        output
    }
}

pub struct CauseList<'a> {
    causes: &'a [RootCause],
}

impl<'a> CauseList<'a> {
    pub fn new(causes: &'a [RootCause]) -> Self {
        Self { causes }
    }

    pub fn render(&self) -> String {
        if self.causes.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        let palette = ColorPalette::default();

        output.push_str(&palette.accent_text("COMMON CAUSES\n"));

        for cause in self.causes {
            let likelihood = format!("[{}]", cause.likelihood).cyan();
            output.push_str(&format!("  - {} {}\n", likelihood, cause.description));
        }

        output
    }
}

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
        let empty = BAR_WIDTH.saturating_sub(filled);
        let bar_str = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

        let palette = ColorPalette::default();
        let colored_bar = if pct >= 0.9 {
            palette.error_text(&bar_str)
        } else if pct >= 0.7 {
            palette.warning_text(&bar_str)
        } else {
            palette.success_text(&bar_str)
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

    let filled = (intensity * BAR_WIDTH as f64).round() as usize;
    let empty = BAR_WIDTH.saturating_sub(filled);
    let cell = format!("{}{}", block.repeat(filled), "░".repeat(empty));

    let palette = ColorPalette::default();
    if intensity >= 0.75 {
        palette.error_text(&cell)
    } else if intensity >= 0.5 {
        palette.warning_text(&cell)
    } else if intensity >= 0.25 {
        palette.metadata_text(&cell)
    } else {
        palette.muted_text(&cell)
    }
}

pub fn render_heatmap(profile: &ResourceProfile) -> String {
    if profile.hotspots.is_empty() {
        let palette = ColorPalette::default();
        return format!(
            "{}\n  {}\n",
            render_section_header("Resource Heatmap"),
            palette.muted_text("No hotspot data available.")
        );
    }

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
    let total_io = (profile.total_read_bytes + profile.total_write_bytes).max(1);

    let label_width = profile
        .hotspots
        .iter()
        .map(|h| h.location.len())
        .max()
        .unwrap_or(8)
        .max(8);

    let col_width = BAR_WIDTH + 2;

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

    for hotspot in &profile.hotspots {
        let cpu_intensity = hotspot.cpu_instructions as f64 / max_cpu as f64;
        let mem_intensity = hotspot.memory_bytes as f64 / max_mem as f64;
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

    out.push('\n');
    let palette = ColorPalette::default();
    out.push_str(&format!(
        "  Legend: {} cold  {} low  {} medium  {} hot\n",
        palette.muted_text("░░░░░░░░░░"),
        palette.metadata_text("▒▒▒▒▒▒▒▒▒▒"),
        palette.warning_text("▓▓▓▓▓▓▓▓▓▓"),
        palette.error_text("██████████"),
    ));

    out
}

#[derive(Tabled)]
struct ArgumentRow {
    #[tabled(rename = "Argument")]
    index: usize,
    #[tabled(rename = "Value")]
    value: String,
}

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
        output.push_str(&format!("Function: {function_name}\n"));
    }
    output.push_str("Arguments:\n");
    output.push_str(&table);

    output
}

#[derive(Tabled)]
struct DiffRow {
    #[tabled(rename = "Key")]
    key: String,
    #[tabled(rename = "Change")]
    change: String,
    #[tabled(rename = "Old Value")]
    old_value: String,
    #[tabled(rename = "New Value")]
    new_value: String,
}

pub struct StateDiffTable<'a> {
    diff: &'a grat_core::types::trace::StateDiff,
}

impl<'a> StateDiffTable<'a> {
    pub fn new(diff: &'a grat_core::types::trace::StateDiff) -> Self {
        Self { diff }
    }

    pub fn render(&self) -> String {
        if self.diff.entries.is_empty() {
            return String::new();
        }

        let palette = ColorPalette::default();
        let rows: Vec<DiffRow> = self
            .diff
            .entries
            .iter()
            .map(|entry| {
                let change = match entry.change_type {
                    grat_core::types::trace::DiffChangeType::Created => {
                        palette.success_text("Created")
                    }
                    grat_core::types::trace::DiffChangeType::Deleted => {
                        palette.error_text("Deleted")
                    }
                    grat_core::types::trace::DiffChangeType::Updated => {
                        palette.warning_text("Updated")
                    }
                    grat_core::types::trace::DiffChangeType::Unchanged => {
                        palette.muted_text("Unchanged")
                    }
                };

                DiffRow {
                    key: entry.key.clone(),
                    change,
                    old_value: entry.before.clone().unwrap_or_else(|| "-".to_string()),
                    new_value: entry.after.clone().unwrap_or_else(|| "-".to_string()),
                }
            })
            .collect();

        Table::new(rows).to_string()
    }
}

pub fn render_fee_breakdown(fee: &FeeBreakdown) -> String {
    let palette = ColorPalette::default();
    let mut out = String::new();

    out.push_str(&render_section_header("Fee Breakdown"));
    out.push('\n');

    let format_fee = |value: i64| format!("{value} stroops");
    let format_opt_fee = |value: Option<i64>| match value {
        Some(v) => format!("{v} stroops"),
        None => "N/A".to_string(),
    };

    out.push_str(&format!(
        "  Bid Fee:            {}\n",
        palette.metadata_text(&format_opt_fee(fee.bid_fee))
    ));
    out.push_str(&format!(
        "  Total Charged Fee:  {}\n",
        palette.accent_text(&format_fee(fee.total_charged_fee))
    ));
    out.push_str(&format!(
        "  Inclusion Fee:      {}\n",
        palette.success_text(&format_fee(fee.inclusion_fee))
    ));
    out.push_str(&format!(
        "  Resource Fee:       {}\n",
        palette.warning_text(&format_fee(fee.resource_fee))
    ));

    if fee.resource_fee > 0 {
        out.push_str(&format!(
            "    Refundable Resource Fee:  {}\n",
            palette.muted_text(&format_fee(fee.refundable_resource_fee))
        ));
        out.push_str(&format!(
            "    Non-Refundable:           {}\n",
            palette.muted_text(&format_fee(fee.non_refundable_fee))
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use grat_core::types::report::{
        ContractErrorInfo, FeeBreakdown, ResourceSummary, Severity, TransactionContext,
    };
    use grat_core::types::trace::{ResourceHotspot, ResourceProfile};

    fn make_profile(hotspots: Vec<ResourceHotspot>) -> ResourceProfile {
        ResourceProfile {
            total_cpu: hotspots.iter().map(|h| h.cpu_instructions).sum(),
            cpu_limit: 1_000_000,
            total_memory: hotspots.iter().map(|h| h.memory_bytes).sum(),
            memory_limit: 1_000_000,
            total_read_bytes: 0,
            total_write_bytes: 0,
            read_limit: 0,
            write_limit: 0,
            hotspots,
            warnings: vec![],
        }
    }

    fn create_test_report() -> DiagnosticReport {
        DiagnosticReport {
            error_category: "Contract".to_string(),
            error_code: 1,
            error_name: "InsufficientBalance".to_string(),
            summary: "The account does not have enough balance to complete this transaction."
                .to_string(),
            detailed_explanation: String::new(),
            severity: Severity::Error,
            root_causes: Vec::new(),
            suggested_fixes: Vec::new(),
            contract_error: Some(ContractErrorInfo {
                contract_id: "CBDLTOJWR2YX2U6BR3P5C4UXKWHE5DJW3JPSIOEXTW2E7D5JUDPQULE7".to_string(),
                error_code: 1,
                error_name: Some("InsufficientBalance".to_string()),
                doc_comment: Some("User attempted transfer with insufficient balance".to_string()),
                learn_more: String::new(),
            }),
            transaction_context: None,
            related_errors: Vec::new(),
            cross_contract_attribution: None,
            auth_signatures: Vec::new(),
            auth_entries: Vec::new(),
            failing_contract_id: None,
            call_chain: None,
            resource_diagnostics: None,
            operation_index: None,
            operation_count: None,
            learn_more: "https://developers.stellar.org/docs/learn/smart-contracts/errors"
                .to_string(),
        }
    }

    #[test]
    fn section_header_renders_boxed_uppercase_title() {
        let rendered = SectionHeader::new("Transaction Summary").render();
        assert!(rendered.contains("TRANSACTION SUMMARY"));
        assert!(rendered.contains('+'));
        assert!(rendered.contains('|'));
    }

    #[test]
    fn budget_bar_renders_low_usage() {
        let bar = BudgetBar::new("CPU", 100, 1000);
        let rendered = bar.render();
        assert!(rendered.contains("CPU"));
        assert!(rendered.contains("10%"));
    }

    #[test]
    fn heatmap_renders_function_names() {
        let profile = make_profile(vec![ResourceHotspot {
            location: "transfer::invoke".to_string(),
            cpu_instructions: 800_000,
            cpu_percentage: 80.0,
            memory_bytes: 300_000,
            memory_percentage: 30.0,
        }]);
        let output = render_heatmap(&profile);
        assert!(output.contains("transfer::invoke"));
    }

    #[test]
    fn error_card_renders_basic_error() {
        let report = create_test_report();
        let rendered = render_error_card(&report);
        assert!(rendered.contains("InsufficientBalance"));
        assert!(rendered.contains("[CONTRACT]"));
        assert!(rendered.contains("does not have enough balance"));
    }

    #[test]
    fn cause_list_renders_common_causes() {
        let causes = vec![RootCause {
            description: "The transaction was submitted with an undersized resource budget."
                .to_string(),
            likelihood: "high".to_string(),
        }];

        let rendered = render_cause_list(&causes);

        assert!(rendered.contains("COMMON CAUSES"));
        assert!(rendered.contains("undersized resource budget"));
        assert!(rendered.contains("high"));
    }

    #[test]
    fn render_context_table_with_arguments() {
        let context = TransactionContext {
            tx_hash: "abc123".to_string(),
            ledger_sequence: 12345,
            function_name: Some("transfer".to_string()),
            arguments: vec!["GABC".to_string(), "100".to_string()],
            fee: FeeBreakdown {
                total_charged_fee: 150,
                inclusion_fee: 100,
                resource_fee: 50,
                refundable_resource_fee: 25,
                refundable_fee: 25,
                non_refundable_fee: 25,
                bid_fee: Some(150),
            },
            resources: ResourceSummary {
                cpu_instructions_used: 1000,
                cpu_instructions_limit: 10000,
                memory_bytes_used: 5000,
                memory_bytes_limit: 50000,
                read_bytes: 1000,
                read_bytes_limit: 10000,
                write_bytes: 500,
            },
            return_value: None,
            operation_index: None,
            operation_count: None,
        };

        let output = render_context_table(&context);
        assert!(output.contains("Function: transfer"));
        assert!(output.contains("Arguments:"));
        assert!(output.contains("GABC"));
    }

    #[test]
    fn render_context_table_with_empty_arguments() {
        let context = TransactionContext {
            tx_hash: "abc123".to_string(),
            ledger_sequence: 12345,
            operation_count: Some(0),
            operation_index: Some(0),
            function_name: Some("transfer".to_string()),
            arguments: vec![],
            fee: FeeBreakdown {
                total_charged_fee: 150,
                inclusion_fee: 100,
                resource_fee: 50,
                refundable_resource_fee: 25,
                refundable_fee: 25,
                non_refundable_fee: 25,
                bid_fee: Some(150),
            },
            resources: ResourceSummary {
                cpu_instructions_used: 1000,
                cpu_instructions_limit: 10000,
                memory_bytes_used: 5000,
                memory_bytes_limit: 50000,
                read_bytes: 1000,
                read_bytes_limit: 10000,
                write_bytes: 500,
            },
            return_value: None,
        };

        let output = render_context_table(&context);
        assert!(output.is_empty());
    }

    #[test]
    fn render_fee_breakdown_works() {
        let fee = FeeBreakdown {
            total_charged_fee: 150,
            inclusion_fee: 100,
            resource_fee: 50,
            refundable_resource_fee: 25,
            refundable_fee: 25,
            non_refundable_fee: 25,
            bid_fee: Some(150),
        };
        let output = render_fee_breakdown(&fee);
        assert!(output.contains("FEE BREAKDOWN"));
        assert!(output.contains("Total Charged Fee:"));
        assert!(output.contains("150 stroops"));
        assert!(output.contains("Refundable Resource Fee:"));
    }
}
