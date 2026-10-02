use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCategory {
    Success,
    Info,
    Warning,
    Error,
}

impl DiagnosticCategory {
    pub fn icon(&self) -> &'static str {
        match self {
            DiagnosticCategory::Success => "✔",
            DiagnosticCategory::Info => "ℹ",
            DiagnosticCategory::Warning => "⚠",
            DiagnosticCategory::Error => "✖",
        }
    }

    pub fn fallback(&self) -> &'static str {
        match self {
            DiagnosticCategory::Success => "[SUCCESS]",
            DiagnosticCategory::Info => "[INFO]",
            DiagnosticCategory::Warning => "[WARN]",
            DiagnosticCategory::Error => "[ERROR]",
        }
    }

    pub fn color_code(&self) -> &'static str {
        match self {
            DiagnosticCategory::Success => "\x1b[32m",
            DiagnosticCategory::Info => "\x1b[34m",
            DiagnosticCategory::Warning => "\x1b[33m",
            DiagnosticCategory::Error => "\x1b[31m",
        }
    }
}

pub struct DiagnosticBadge {
    pub category: DiagnosticCategory,
    pub supports_unicode: bool,
}

impl DiagnosticBadge {
    pub fn new(category: DiagnosticCategory, supports_unicode: bool) -> Self {
        Self {
            category,
            supports_unicode,
        }
    }

    pub fn format_message(&self, message: &str) -> String {
        let badge_text = if self.supports_unicode {
            self.category.icon()
        } else {
            self.category.fallback()
        };

        format!(
            "{}{} \x1b[0m{}",
            self.category.color_code(),
            badge_text,
            message
        )
    }
}

impl fmt::Display for DiagnosticBadge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let badge_text = if self.supports_unicode {
            self.category.icon()
        } else {
            self.category.fallback()
        };

        write!(f, "{}{}\x1b[0m", self.category.color_code(), badge_text)
    }
}