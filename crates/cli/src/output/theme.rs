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
    use std::sync::{Mutex, MutexGuard};

    use super::*;

    /// Serializes every test that touches the process-wide color flag so the
    /// tests never observe each other's flag state. Each test acquires this
    /// lock exactly once and holds it for its whole body.
    static COLOR_FLAG_LOCK: Mutex<()> = Mutex::new(());

    fn lock_color_flag() -> MutexGuard<'static, ()> {
        COLOR_FLAG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn palette() -> ColorPalette {
        ColorPalette::default()
    }

    /// Toggles the color flag for the lifetime of the guard and restores the
    /// previous value on drop, even when an assertion fails. Unlike the
    /// mutex, guards may be nested freely: the innermost guard restores the
    /// state captured when it was created.
    struct ColorFlagGuard {
        previous: bool,
    }

    impl ColorFlagGuard {
        fn set(enabled: bool) -> Self {
            let previous = colors_enabled();
            set_color_enabled(enabled);
            Self { previous }
        }
    }

    impl Drop for ColorFlagGuard {
        fn drop(&mut self) {
            set_color_enabled(self.previous);
        }
    }

    #[test]
    fn disabled_colors_return_raw_text_from_every_helper() {
        let _lock = lock_color_flag();
        let _guard = ColorFlagGuard::set(false);
        let palette = palette();

        assert_eq!(palette.error_text("plain text"), "plain text");
        assert_eq!(palette.warning_text("plain text"), "plain text");
        assert_eq!(palette.success_text("plain text"), "plain text");
        assert_eq!(palette.metadata_text("plain text"), "plain text");
        assert_eq!(palette.muted_text("plain text"), "plain text");
        assert_eq!(palette.accent_text("plain text"), "plain text");
    }

    #[test]
    fn disabled_colors_never_emit_esc_sequences() {
        let _lock = lock_color_flag();
        let _guard = ColorFlagGuard::set(false);
        let palette = palette();

        let inputs = ["", "boom", "multi\nline", "unicode ✓ ✗ — é"];

        for input in inputs {
            for out in [
                palette.error_text(input),
                palette.warning_text(input),
                palette.success_text(input),
                palette.metadata_text(input),
                palette.muted_text(input),
                palette.accent_text(input),
            ] {
                assert_eq!(out, input, "expected raw text for {input:?}, got {out:?}");
                assert!(
                    !out.contains('\u{1b}'),
                    "ESC sequence leaked for {input:?}: {out:?}"
                );
            }
        }
    }

    #[test]
    fn enabled_colors_restore_ansi_output_for_every_helper() {
        let _lock = lock_color_flag();
        let _guard = ColorFlagGuard::set(true);
        let palette = palette();

        for text in ["boom", "warn", "ok"] {
            for out in [
                palette.error_text(text),
                palette.warning_text(text),
                palette.success_text(text),
                palette.metadata_text(text),
                palette.muted_text(text),
                palette.accent_text(text),
            ] {
                assert!(
                    out.contains('\u{1b}'),
                    "expected ANSI escapes with colors enabled for {text:?}: {out:?}"
                );
                assert!(
                    out.contains(text),
                    "styled output lost the original text {text:?}: {out:?}"
                );
            }
        }
    }

    #[test]
    fn toggling_colors_round_trips_between_raw_and_styled_output() {
        let _lock = lock_color_flag();
        let _outer = ColorFlagGuard::set(true);
        let palette = palette();
        assert!(colors_enabled(), "outer guard should have enabled colors");

        {
            let _inner = ColorFlagGuard::set(false);
            assert!(!colors_enabled());
            assert_eq!(palette.error_text("raw"), "raw");
        }

        // The inner guard restored the flag value captured when it was
        // created, so styled output must work again without manual cleanup.
        assert!(colors_enabled(), "flag was not restored after inner guard");
        assert!(
            palette.error_text("raw").contains('\u{1b}'),
            "styled output not restored after re-enabling colors"
        );
    }

    #[test]
    fn guard_restores_previous_flag_value_even_when_it_was_disabled() {
        let _lock = lock_color_flag();
        let _outer = ColorFlagGuard::set(false);

        {
            let _inner = ColorFlagGuard::set(true);
            assert!(colors_enabled());
            assert!(palette().error_text("raw").contains('\u{1b}'));
        }

        assert!(
            !colors_enabled(),
            "inner guard should have restored the disabled state"
        );
    }
}
