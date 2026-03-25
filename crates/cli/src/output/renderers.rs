//! Shared terminal renderers for CLI output.

use colored::{ColoredString, Colorize};

const BAR_WIDTH: usize = 10;

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
    use super::BudgetBar;

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
}
