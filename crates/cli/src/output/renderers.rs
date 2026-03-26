//! Shared terminal renderers for CLI output.

use colored::Colorize;
use prism_core::types::report::DiagnosticReport;

const BAR_WIDTH: usize = 10;

/// Render a boxed section header suitable for terminal report sections.
pub fn render_section_header(title: &str) -> String {
    SectionHeader::new(title).render()
}

/// Render an error card to display transaction errors prominently.
pub fn render_error_card(report: &DiagnosticReport) -> String {
    ErrorCard::new(report).render()
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

        // Create the border and content structure
        let category_badge = format!("[{}]", self.report.error_category.to_uppercase());
        let error_line = format!(
            " {} ({})",
            self.report.error_name, self.report.error_code
        );

        // Calculate width based on content
        let max_width = error_line.len().max(self.report.summary.len()).max(category_badge.len()) + 4;
        let border = "█".repeat(max_width);

        // Render with red color
        let border_colored = border.red().bold().to_string();
        let category_colored = category_badge.red().bold().to_string();
        let error_colored = error_line.red().bold().to_string();
        let summary_colored = self.report.summary.white().to_string();

        // Build the card
        output.push_str(&format!("{}\n", border_colored));
        output.push_str(&format!("{} {}\n", "█".red().bold(), category_colored));
        output.push_str(&format!("{} {}\n", "█".red().bold(), error_colored));

        // Add component info if it's a contract error
        if let Some(contract_error) = &self.report.contract_error {
            let component_line = format!("Component: {}", contract_error.contract_id);
            output.push_str(&format!("{} {}\n", "█".red().bold(), component_line.white()));
        }

        output.push_str(&format!("{} {}\n", "█".red().bold(), summary_colored));
        output.push_str(&format!("{}\n", border_colored));

        output
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
<<<<<<< HEAD
        let ratio = if self.limit == 0 {
            0.0f64
        } else {
            self.used as f64 / self.limit as f64
        };

        let filled = ((ratio * BAR_WIDTH as f64).round() as usize).min(BAR_WIDTH);
        let empty = BAR_WIDTH - filled;

        let bar_inner = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
        let bar = format!("[{}]", bar_inner);

        let colored_bar = if ratio >= 0.9 {
            bar.red().bold().to_string()
        } else if ratio >= 0.7 {
            bar.yellow().to_string()
        } else {
            bar.green().to_string()
        };

        let pct = (ratio * 100.0).round() as u64;
        format!(
            "{:>6}: {} {:>3}%  ({} / {})",
            self.label, colored_bar, pct, self.used, self.limit
        )
    }
}

/// Render a list of actionable fixes from a diagnostic report.
pub fn render_fix_list(report: &DiagnosticReport) -> String {
    if report.suggested_fixes.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    output.push_str("Actionable Fixes:\n");

    for (index, fix) in report.suggested_fixes.iter().enumerate() {
        let icon = get_fix_icon(fix);
        let difficulty_badge = get_difficulty_badge(&fix.difficulty);

        output.push_str(&format!(
            "  {} {}{}\n",
            icon, fix.description, difficulty_badge
        ));

        if fix.requires_upgrade {
            output.push_str("    ⚡ May require contract upgrade\n");
        }

        if let Some(example) = &fix.example {
            output.push_str(&format!("    📄 Example: {}\n", example));
        }

        if index < report.suggested_fixes.len() - 1 {
            output.push('\n');
        }
    }
=======
        let percentage = if self.limit > 0 {
            (self.used as f64 / self.limit as f64 * 100.0) as u64
        } else {
            0
        };

        let filled = (percentage / 10).min(BAR_WIDTH as u64) as usize;
        let empty = BAR_WIDTH - filled;
        let bar = format!(
            "[{}{}]",
            "█".repeat(filled),
            "░".repeat(empty)
        );
>>>>>>> 71d530f ( Implement ErrorCard for terminal)

        let bar_colored = if percentage > 90 {
            bar.red().to_string()
        } else if percentage > 70 {
            bar.yellow().to_string()
        } else {
            bar.green().to_string()
        };

        format!(
            "{:8} {} {}/{} ({:3}%)",
            self.label,
            bar_colored,
            self.used,
            self.limit,
            percentage
        )
    }
}

#[cfg(test)]
mod tests {
<<<<<<< HEAD
    use super::{get_difficulty_badge, get_fix_icon, render_fix_list};
    use super::{render_section_header, BudgetBar, SectionHeader};
    use prism_core::types::report::{DiagnosticReport, SuggestedFix};

    fn make_fix(
        description: &str,
        difficulty: &str,
        requires_upgrade: bool,
        example: Option<&str>,
    ) -> SuggestedFix {
        SuggestedFix {
            description: description.to_string(),
            difficulty: difficulty.to_string(),
            requires_upgrade,
            example: example.map(|s| s.to_string()),
        }
    }

    fn create_test_report() -> DiagnosticReport {
        let mut report = DiagnosticReport::new("test", 0, "TestError", "test summary");
        report.suggested_fixes = vec![
            make_fix("Fix A", "easy", false, None),
            make_fix("Fix B", "medium", false, Some("example code")),
            make_fix("Fix C", "hard", true, None),
        ];
        report
    }

    #[test]
    fn test_render_fix_list_with_fixes() {
        let report = create_test_report();
        let output = render_fix_list(&report);

        assert!(output.contains("Actionable Fixes:"));
        assert!(output.contains("🔧"));
        assert!(output.contains("📋"));
        assert!(output.contains("🔒"));
        assert!(output.contains("[easy]"));
        assert!(output.contains("[medium]"));
        assert!(output.contains("[hard]"));
        assert!(output.contains("May require contract upgrade"));
    }

    #[test]
    fn test_render_fix_list_empty() {
        let mut report = create_test_report();
        report.suggested_fixes = vec![];
        let output = render_fix_list(&report);

        assert!(output.is_empty());
    }

    #[test]
    fn test_get_fix_icon() {
        assert_eq!(
            get_fix_icon(&make_fix("T", "easy", false, Some("code"))),
            "📋"
        );
        assert_eq!(get_fix_icon(&make_fix("T", "easy", true, None)), "🔒");
        assert_eq!(get_fix_icon(&make_fix("T", "easy", false, None)), "🔧");
    }

    #[test]
    fn test_get_difficulty_badge() {
        assert_eq!(get_difficulty_badge("easy"), " [easy]");
        assert_eq!(get_difficulty_badge("medium"), " [medium]");
        assert_eq!(get_difficulty_badge("hard"), " [hard]");
        assert_eq!(get_difficulty_badge("unknown"), "");
=======
    use super::*;
    use prism_core::types::report::{ContractErrorInfo, Severity};

    fn create_test_report() -> DiagnosticReport {
        DiagnosticReport {
            error_category: "Contract".to_string(),
            error_code: 1,
            error_name: "InsufficientBalance".to_string(),
            summary: "The account does not have enough balance to complete this transaction.".to_string(),
            detailed_explanation: String::new(),
            severity: Severity::Error,
            root_causes: Vec::new(),
            suggested_fixes: Vec::new(),
            contract_error: Some(ContractErrorInfo {
                contract_id: "CBDLTOJWR2YX2U6BR3P5C4UXKWHE5DJW3JPSIOEXTW2E7D5JUDPQULE7".to_string(),
                error_code: 1,
                error_name: Some("InsufficientBalance".to_string()),
                doc_comment: Some("User attempted transfer with insufficient balance".to_string()),
            }),
            transaction_context: None,
            related_errors: Vec::new(),
        }
>>>>>>> 71d530f ( Implement ErrorCard for terminal)
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
<<<<<<< HEAD
    fn budget_bar_renders_green_when_low_usage() {
        let bar = BudgetBar::new("CPU", 100, 1000).render();
        assert!(bar.contains("CPU"));
        assert!(bar.contains("10%"));
=======
    fn error_card_renders_basic_error() {
        let report = create_test_report();
        let rendered = render_error_card(&report);

        assert!(rendered.contains("❌"));
        assert!(rendered.contains("InsufficientBalance"));
        assert!(rendered.contains("1"));
        assert!(rendered.contains("[CONTRACT]"));
        assert!(rendered.contains("does not have enough balance"));
    }

    #[test]
    fn error_card_includes_contract_component() {
        let report = create_test_report();
        let rendered = render_error_card(&report);

        assert!(rendered.contains("Component:"));
        assert!(rendered.contains("CBDLTOJWR2YX2U6BR3P5C4UXKWHE5DJW3JPSIOEXTW2E7D5JUDPQULE7"));
    }

    #[test]
    fn error_card_excludes_component_without_contract_error() {
        let mut report = create_test_report();
        report.contract_error = None;
        let rendered = render_error_card(&report);

        assert!(!rendered.contains("Component:"));
    }

    #[test]
    fn error_card_uses_red_styling() {
        let report = create_test_report();
        let rendered = render_error_card(&report);

        // The card should contain the border characters
        assert!(rendered.contains("█"));
    }

    #[test]
    fn budget_bar_renders_low_usage() {
        let bar = BudgetBar::new("CPU", 100, 1000);
        let rendered = bar.render();

        assert!(rendered.contains("CPU"));
        assert!(rendered.contains("100/1000"));
        assert!(rendered.contains("10%"));
    }

    #[test]
    fn budget_bar_renders_high_usage() {
        let bar = BudgetBar::new("Memory", 950, 1000);
        let rendered = bar.render();

        assert!(rendered.contains("Memory"));
        assert!(rendered.contains("950/1000"));
        assert!(rendered.contains("95%"));
>>>>>>> 71d530f ( Implement ErrorCard for terminal)
    }

    #[test]
    fn budget_bar_handles_zero_limit() {
<<<<<<< HEAD
        let bar = BudgetBar::new("MEM", 0, 0).render();
        assert!(bar.contains("MEM"));
=======
        let bar = BudgetBar::new("CPU", 100, 0);
        let rendered = bar.render();

        assert!(rendered.contains("CPU"));
        assert!(rendered.contains("0%"));
    }

    #[test]
    fn budget_bar_shows_full_usage() {
        let bar = BudgetBar::new("Disk", 1000, 1000);
        let rendered = bar.render();

        assert!(rendered.contains("Disk"));
        assert!(rendered.contains("1000/1000"));
        assert!(rendered.contains("100%"));
>>>>>>> 71d530f ( Implement ErrorCard for terminal)
    }
}
