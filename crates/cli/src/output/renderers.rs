//! Shared terminal renderers for CLI output.

use colored::Colorize;
use prism_core::types::report::{DiagnosticReport, SuggestedFix};

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

    output
}

/// Returns the appropriate icon for a suggested fix based on its characteristics.
fn get_fix_icon(fix: &SuggestedFix) -> &'static str {
    if fix.requires_upgrade {
        "🔒"
    } else if fix.example.is_some() {
        "📋"
    } else {
        "🔧"
    }
}

/// Returns a badge indicating the difficulty level of the fix.
fn get_difficulty_badge(difficulty: &str) -> String {
    match difficulty.to_lowercase().as_str() {
        "easy" => " [easy]".to_string(),
        "medium" => " [medium]".to_string(),
        "hard" => " [hard]".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
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
    fn budget_bar_renders_green_when_low_usage() {
        let bar = BudgetBar::new("CPU", 100, 1000).render();
        assert!(bar.contains("CPU"));
        assert!(bar.contains("10%"));
    }

    #[test]
    fn budget_bar_handles_zero_limit() {
        let bar = BudgetBar::new("MEM", 0, 0).render();
        assert!(bar.contains("MEM"));
    }
}
