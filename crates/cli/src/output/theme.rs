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
    fn paint(text: &str, style: Style) -> String {
        if colors_enabled() {
            format!("{}", text.style(style))
        } else {
            text.to_string()
        }
    }

    pub fn error_text(&self, text: &str) -> String {
        Self::paint(text, self.error)
    }

    pub fn warning_text(&self, text: &str) -> String {
        Self::paint(text, self.warning)
    }

    pub fn success_text(&self, text: &str) -> String {
        Self::paint(text, self.success)
    }

    pub fn metadata_text(&self, text: &str) -> String {
        Self::paint(text, self.metadata)
    }

    pub fn muted_text(&self, text: &str) -> String {
        Self::paint(text, self.muted)
    }

    pub fn accent_text(&self, text: &str) -> String {
        Self::paint(text, self.accent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A guard that serializes tests touching the global COLOR_ENABLED flag and
    /// restores its original value when dropped, so parallel test runners cannot
    /// interfere with each other.
    static LOCK: Mutex<()> = Mutex::new(());

    struct ColorGuard {
        original: bool,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl ColorGuard {
        fn acquire(enabled: bool) -> Self {
            // Poison recovery: if a previous test panicked while holding the
            // lock the mutex becomes poisoned; we recover and continue.
            let guard = match LOCK.lock() {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
            let original = colors_enabled();
            set_color_enabled(enabled);
            ColorGuard { original, _guard: guard }
        }
    }

    impl Drop for ColorGuard {
        fn drop(&mut self) {
            set_color_enabled(self.original);
        }
    }

    // ------------------------------------------------------------------ //
    // Helper                                                               //
    // ------------------------------------------------------------------ //

    fn has_ansi(s: &str) -> bool {
        s.contains('\x1b')
    }

    // ------------------------------------------------------------------ //
    // Non-TTY (colors disabled) tests                                     //
    // ------------------------------------------------------------------ //

    #[test]
    fn error_text_no_ansi_when_colors_disabled() {
        let _g = ColorGuard::acquire(false);
        let palette = ColorPalette::default();
        let result = palette.error_text("something failed");
        assert_eq!(result, "something failed");
        assert!(!has_ansi(&result), "expected no ANSI codes, got: {:?}", result);
    }

    #[test]
    fn warning_text_no_ansi_when_colors_disabled() {
        let _g = ColorGuard::acquire(false);
        let palette = ColorPalette::default();
        let result = palette.warning_text("low disk space");
        assert_eq!(result, "low disk space");
        assert!(!has_ansi(&result), "expected no ANSI codes, got: {:?}", result);
    }

    #[test]
    fn success_text_no_ansi_when_colors_disabled() {
        let _g = ColorGuard::acquire(false);
        let palette = ColorPalette::default();
        let result = palette.success_text("transaction confirmed");
        assert_eq!(result, "transaction confirmed");
        assert!(!has_ansi(&result), "expected no ANSI codes, got: {:?}", result);
    }

    #[test]
    fn metadata_text_no_ansi_when_colors_disabled() {
        let _g = ColorGuard::acquire(false);
        let palette = ColorPalette::default();
        let result = palette.metadata_text("ledger: 12345");
        assert_eq!(result, "ledger: 12345");
        assert!(!has_ansi(&result), "expected no ANSI codes, got: {:?}", result);
    }

    #[test]
    fn muted_text_no_ansi_when_colors_disabled() {
        let _g = ColorGuard::acquire(false);
        let palette = ColorPalette::default();
        let result = palette.muted_text("optional detail");
        assert_eq!(result, "optional detail");
        assert!(!has_ansi(&result), "expected no ANSI codes, got: {:?}", result);
    }

    #[test]
    fn accent_text_no_ansi_when_colors_disabled() {
        let _g = ColorGuard::acquire(false);
        let palette = ColorPalette::default();
        let result = palette.accent_text("highlighted");
        assert_eq!(result, "highlighted");
        assert!(!has_ansi(&result), "expected no ANSI codes, got: {:?}", result);
    }

    #[test]
    fn all_methods_return_raw_string_when_colors_disabled() {
        let _g = ColorGuard::acquire(false);
        let palette = ColorPalette::default();
        let input = "test string";

        for result in [
            palette.error_text(input),
            palette.warning_text(input),
            palette.success_text(input),
            palette.metadata_text(input),
            palette.muted_text(input),
            palette.accent_text(input),
        ] {
            assert_eq!(result, input, "expected raw string, got: {:?}", result);
            assert!(!has_ansi(&result), "unexpected ANSI in: {:?}", result);
        }
    }

    // ------------------------------------------------------------------ //
    // TTY (colors enabled) tests                                          //
    // ------------------------------------------------------------------ //

    #[test]
    fn error_text_contains_ansi_when_colors_enabled() {
        let _g = ColorGuard::acquire(true);
        let palette = ColorPalette::default();
        let result = palette.error_text("something failed");
        assert!(has_ansi(&result), "expected ANSI codes, got: {:?}", result);
    }

    #[test]
    fn warning_text_contains_ansi_when_colors_enabled() {
        let _g = ColorGuard::acquire(true);
        let palette = ColorPalette::default();
        let result = palette.warning_text("low disk space");
        assert!(has_ansi(&result), "expected ANSI codes, got: {:?}", result);
    }

    #[test]
    fn success_text_contains_ansi_when_colors_enabled() {
        let _g = ColorGuard::acquire(true);
        let palette = ColorPalette::default();
        let result = palette.success_text("transaction confirmed");
        assert!(has_ansi(&result), "expected ANSI codes, got: {:?}", result);
    }

    #[test]
    fn metadata_text_contains_ansi_when_colors_enabled() {
        let _g = ColorGuard::acquire(true);
        let palette = ColorPalette::default();
        let result = palette.metadata_text("ledger: 12345");
        assert!(has_ansi(&result), "expected ANSI codes, got: {:?}", result);
    }

    #[test]
    fn muted_text_contains_ansi_when_colors_enabled() {
        let _g = ColorGuard::acquire(true);
        let palette = ColorPalette::default();
        let result = palette.muted_text("optional detail");
        assert!(has_ansi(&result), "expected ANSI codes, got: {:?}", result);
    }

    #[test]
    fn accent_text_contains_ansi_when_colors_enabled() {
        let _g = ColorGuard::acquire(true);
        let palette = ColorPalette::default();
        let result = palette.accent_text("highlighted");
        assert!(has_ansi(&result), "expected ANSI codes, got: {:?}", result);
    }

    // ------------------------------------------------------------------ //
    // Toggle / restore tests                                              //
    // ------------------------------------------------------------------ //

    #[test]
    fn toggling_colors_off_then_on_restores_colored_output() {
        let _g = ColorGuard::acquire(true);
        let palette = ColorPalette::default();

        // Sanity-check: colors are on.
        let with_color = palette.error_text("err");
        assert!(has_ansi(&with_color), "expected ANSI when enabled: {:?}", with_color);

        // Disable colors.
        set_color_enabled(false);
        let without_color = palette.error_text("err");
        assert_eq!(without_color, "err");
        assert!(!has_ansi(&without_color), "expected no ANSI when disabled: {:?}", without_color);

        // Re-enable colors — output should contain ANSI again.
        set_color_enabled(true);
        let restored = palette.error_text("err");
        assert!(has_ansi(&restored), "expected ANSI restored: {:?}", restored);
    }

    #[test]
    fn set_color_enabled_false_reflects_in_colors_enabled() {
        let _g = ColorGuard::acquire(false);
        assert!(!colors_enabled(), "colors_enabled() should return false after set_color_enabled(false)");
    }

    #[test]
    fn set_color_enabled_true_reflects_in_colors_enabled() {
        let _g = ColorGuard::acquire(true);
        assert!(colors_enabled(), "colors_enabled() should return true after set_color_enabled(true)");
    }
}
