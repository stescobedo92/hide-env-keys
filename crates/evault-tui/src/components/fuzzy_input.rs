//! One-line fuzzy-search input pinned above the status bar.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::AppState;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(needle) = app.filter_needle() else {
        return;
    };
    // Clear the row so the dashboard's bottom border does not bleed
    // through long needles.
    frame.render_widget(Clear, area);

    let prompt = Span::styled(" filter > ", Style::new().fg(theme.accent));
    // While the input is active we render a block cursor at the end
    // of the needle so the user has visual confirmation they are in
    // input mode. Once the filter is committed the cursor goes away.
    let needle_span = Span::raw(needle.to_owned());
    let cursor_span = if app.is_filter_input_active() {
        Span::styled("▌", Style::new().fg(theme.accent))
    } else {
        Span::raw(" ")
    };
    let hint = if app.is_filter_input_active() {
        Span::styled("  (Esc cancel · Enter accept)", theme.dim_cell())
    } else {
        // After commit, Esc still clears the filter — the `dismiss`
        // cascade in `AppState` handles "filter → overlay → quit" in
        // that order, so a single Esc returns the user to the
        // unfiltered dashboard.
        Span::styled("  (Ctrl+F to edit · Esc clears filter)", theme.dim_cell())
    };
    let line = Line::from(vec![prompt, needle_span, cursor_span, hint]);
    frame.render_widget(Paragraph::new(line), area);
}
