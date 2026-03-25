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

    let mut output = String::new();
    output.push_str("Actionable Fixes:\n");

    for (index, fix) in report.suggested_fixes.iter().enumerate() {
        let icon = get_fix_icon(fix);
        let difficulty_badge = get_difficulty_badge(&fix.difficulty);
        
        output.push_str(&format!("  {} {}{}\n", icon, fix.description, difficulty_badge));
        
        if fix.requires_upgrade {
            output.push_str("    ⚡ May require contract upgrade\n");
        }
        
        if let Some(example) = &fix.example {
            output.push_str(&format!("    📄 Example: {}\n", example));
        }
        
        // Add a blank line between fixes except for the last one
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
    use super::{render_section_header, BudgetBar, SectionHeader};

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
        let fix_with_example = SuggestedFix {
            description: "Test".to_string(),
            difficulty: "easy".to_string(),
            requires_upgrade: false,
            example: Some("code".to_string()),
        };
        assert_eq!(get_fix_icon(&fix_with_example), "📋");

        let fix_requires_upgrade = SuggestedFix {
            description: "Test".to_string(),
            difficulty: "easy".to_string(),
            requires_upgrade: true,
            example: None,
        };
        assert_eq!(get_fix_icon(&fix_requires_upgrade), "🔒");

        let fix_standard = SuggestedFix {
            description: "Test".to_string(),
            difficulty: "easy".to_string(),
            requires_upgrade: false,
            example: None,
        };
        assert_eq!(get_fix_icon(&fix_standard), "🔧");
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
}
