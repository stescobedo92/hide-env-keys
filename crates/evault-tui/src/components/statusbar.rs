//! One-line status bar pinned to the bottom of the dashboard.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{AppState, View};
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let key_style = Style::new().fg(theme.accent);
    let dim = theme.dim_cell();

    let secrets_marker = if app.secrets_visible() {
        "shown"
    } else {
        "masked"
    };
    let view_name = match app.current_view() {
        View::Dashboard => "dashboard",
        View::Detail => "detail",
        View::Audit => "audit",
    };

    let line = Line::from(vec![
        Span::styled(" evault ", key_style),
        Span::styled("\u{2022} ", dim),
        Span::styled(format!("{view_name} "), dim),
        Span::styled("\u{2022} ", dim),
        Span::styled(format!("rows: {} ", app.rows().len()), dim),
        Span::styled("\u{2022} ", dim),
        Span::styled(format!("profile: {} ", app.active_profile()), dim),
        Span::styled("\u{2022} ", dim),
        Span::styled(format!("secrets: {secrets_marker}"), dim),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}
