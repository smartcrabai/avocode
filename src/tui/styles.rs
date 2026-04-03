//! Minimal, terminal-friendly style definitions.
//!
//! Replaces the previous multi-theme `Theme` system with a single, fixed
//! colour palette that relies on ANSI defaults where possible -- mirroring
//! the approach described in `codex-rs/tui/styles.md`.

use ratatui::style::Color;

/// Minimal colour palette for the simplified TUI.
///
/// Only three semantic colours are exposed:
/// - **foreground** - default text colour
/// - **accent**     - headings, prompts, borders
/// - **muted**      - key hints, secondary text
#[derive(Debug, Clone)]
pub struct Styles {
    pub foreground: Color,
    pub accent: Color,
    pub muted: Color,
}

impl Default for Styles {
    fn default() -> Self {
        Self::new()
    }
}

impl Styles {
    /// Create the default style palette.
    ///
    /// The palette intentionally uses ANSI colours rather than custom `Rgb`
    /// values so that the TUI looks acceptable on any terminal theme
    /// (light or dark), matching the `codex-rs` recommendation.
    #[must_use]
    pub fn new() -> Self {
        Self {
            foreground: Color::Reset,
            accent: Color::Cyan,
            muted: Color::DarkGray,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    // ---- Default values are terminal-friendly ANSI colours ----

    #[test]
    fn test_default_foreground_is_reset() {
        assert_eq!(Styles::new().foreground, Color::Reset);
    }

    #[test]
    fn test_default_accent_is_cyan() {
        assert_eq!(Styles::new().accent, Color::Cyan);
    }

    #[test]
    fn test_default_muted_is_dark_gray() {
        assert_eq!(Styles::new().muted, Color::DarkGray);
    }

    #[test]
    fn test_styles_has_three_color_fields() {
        // Compile-time proof: struct literal syntax requires all fields, so
        // adding a fourth field to Styles will break this test.
        let _ = Styles {
            foreground: Color::Reset,
            accent: Color::Cyan,
            muted: Color::DarkGray,
        };
    }
}
