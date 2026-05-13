//! Semantic colour palette used by every view.
//!
//! Views never pick colours directly — they ask the [`Theme`] for a
//! named role (`ok`, `error`, `warning`, `accent`, …). Centralising
//! the mapping makes it trivial to swap palettes or add a light theme
//! without touching individual widgets.

use ratatui::style::{Color, Modifier, Style};

/// Semantic palette.
///
/// Field names describe the *role* a colour plays, not the colour
/// itself. The [`Self::dark`] constructor wires a default dark-theme
/// palette tuned for legibility on a typical terminal background.
///
/// # Examples
/// ```
/// use evault_tui::Theme;
/// let theme = Theme::dark();
/// let _header_style = theme.header();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Success — green.
    pub ok: Color,
    /// Failure / validation error — red.
    pub error: Color,
    /// Soft warning (missing link, unset secret) — yellow.
    pub warning: Color,
    /// Highlight for rows added during this session — cyan.
    pub new: Color,
    /// Highlight for rows modified during this session — magenta.
    pub modified: Color,
    /// Indicator colour for masked / secret values — dark gray.
    pub secret: Color,
    /// Background colour applied to the selected row.
    pub selected: Color,
    /// Secondary text — timestamps, hints, metadata.
    pub dim: Color,
    /// Accent — titles, separators, key chord hints.
    pub accent: Color,
}

impl Theme {
    /// Default dark-theme palette.
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            ok: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            new: Color::Cyan,
            modified: Color::Magenta,
            secret: Color::DarkGray,
            selected: Color::Indexed(238),
            dim: Color::Gray,
            accent: Color::Blue,
        }
    }

    /// Style applied to the dashboard header row.
    #[must_use]
    pub const fn header(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    /// Style applied to the currently selected table row.
    #[must_use]
    pub const fn selected_row(&self) -> Style {
        Style::new().bg(self.selected).add_modifier(Modifier::BOLD)
    }

    /// Style for a value that is hidden because it is a secret.
    #[must_use]
    pub const fn secret_cell(&self) -> Style {
        Style::new().fg(self.secret).add_modifier(Modifier::DIM)
    }

    /// Style for a dim / metadata cell (timestamps, counts).
    #[must_use]
    pub const fn dim_cell(&self) -> Style {
        Style::new().fg(self.dim)
    }

    /// Style for an error toast (red foreground on default background).
    #[must_use]
    pub const fn error_toast(&self) -> Style {
        Style::new().fg(self.error).add_modifier(Modifier::BOLD)
    }

    /// Style for an informational toast.
    #[must_use]
    pub const fn info_toast(&self) -> Style {
        Style::new().fg(self.ok)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_equals_dark() {
        assert_eq!(Theme::default(), Theme::dark());
    }

    #[test]
    fn dark_palette_distinguishes_ok_from_error() {
        let t = Theme::dark();
        assert_ne!(t.ok, t.error);
        assert_ne!(t.warning, t.error);
    }
}
