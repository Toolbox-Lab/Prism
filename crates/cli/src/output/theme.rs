//! Semantic terminal color palette for CLI output.

use std::sync::atomic::{AtomicBool, Ordering};

use owo_colors::{OwoColorize, Style};

static COLOR_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_color_enabled(enabled: bool) {
    COLOR_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn colors_enabled() -> bool {
    COLOR_ENABLED.load(Ordering::Relaxed)
}

#[derive(Clone, Copy)]
pub struct ColorPalette {
    pub error: Style,
    pub warning: Style,
    pub success: Style,
    pub metadata: Style,
    pub muted: Style,
    pub accent: Style,
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self {
            error: Style::new().red().bold(),
            warning: Style::new().yellow().bold(),
            success: Style::new().green().bold(),
            metadata: Style::new().cyan(),
            muted: Style::new().dimmed(),
            accent: Style::new().white().bold(),
        }
    }
}

impl ColorPalette {
    fn paint(&self, text: &str, style: Style) -> String {
        if colors_enabled() {
            format!("{}", text.style(style))
        } else {
            text.to_string()
        }
    }

    pub fn error_text(&self, text: &str) -> String {
        self.paint(text, self.error)
    }

    pub fn warning_text(&self, text: &str) -> String {
        self.paint(text, self.warning)
    }

    pub fn success_text(&self, text: &str) -> String {
        self.paint(text, self.success)
    }

    pub fn metadata_text(&self, text: &str) -> String {
        self.paint(text, self.metadata)
    }

    pub fn muted_text(&self, text: &str) -> String {
        self.paint(text, self.muted)
    }

    pub fn accent_text(&self, text: &str) -> String {
        self.paint(text, self.accent)
    }
}
