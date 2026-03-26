//! Shared terminal renderers for CLI output.

use colored::{ColoredString, Colorize};

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
        if self.limit == 0 {
            return format!("{}: [n/a] 0%", self.label);
        }

        let percent = self.percent();
        let filled = ((percent as usize) * BAR_WIDTH + 50) / 100;
        let filled = filled.min(BAR_WIDTH);
        let bar = format!(
            "{}{}",
            "█".repeat(filled),
            "░".repeat(BAR_WIDTH.saturating_sub(filled))
        );

        format!(
            "{}: [{}] {}% ({}/{})",
            self.label,
            self.colorize(bar),
            percent,
            self.used,
            self.limit
        )
    }

    fn percent(&self) -> u64 {
        if self.limit == 0 {
            return 0;
        }

        ((self.used.saturating_mul(100)) / self.limit).min(100)
    }

    fn colorize(&self, bar: String) -> ColoredString {
        match self.percent() {
            0..=69 => bar.green(),
            70..=89 => bar.yellow(),
            _ => bar.red(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{render_section_header, BudgetBar, SectionHeader};

    #[test]
    fn renders_expected_percentage() {
        let rendered = BudgetBar::new("CPU", 60, 100).render();

        assert!(rendered.contains("CPU:"));
        assert!(rendered.contains("60%"));
        assert!(rendered.contains("██████"));
    }

    #[test]
    fn clamps_over_limit_usage_to_full_bar() {
        let rendered = BudgetBar::new("RAM", 150, 100).render();

        assert!(rendered.contains("100%"));
        assert!(rendered.contains("██████████"));
    }

    #[test]
    fn renders_na_for_missing_limit() {
        let rendered = BudgetBar::new("CPU", 0, 0).render();

        assert_eq!(rendered, "CPU: [n/a] 0%");
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
}
