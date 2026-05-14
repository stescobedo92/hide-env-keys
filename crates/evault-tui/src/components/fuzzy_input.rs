//! Fuzzy-filter modal popup — captures the needle the user is
//! typing while the dashboard live-filters underneath.
//!
//! Rendered as a centered modal (same shape as the editor / link /
//! confirm modals) so it doesn't compete with the keybindings strip
//! for the bottom of the screen.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::app::AppState;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(needle) = app.filter_needle() else {
        return;
    };
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .title(" fuzzy filter ")
        .border_style(Style::new().fg(theme.accent));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // spacer
        Constraint::Length(1), // input line
        Constraint::Length(1), // spacer
        Constraint::Length(1), // hints
        Constraint::Min(0),
    ])
    .split(inner);

    let prompt = Span::styled(" filter > ", Style::new().fg(theme.accent));
    let body = Span::raw(needle.to_owned());
    let cursor = if app.is_filter_input_active() {
        Span::styled("\u{258C}", Style::new().fg(theme.accent))
    } else {
        Span::raw(" ")
    };
    if let Some(&r) = rows.get(1) {
        frame.render_widget(Paragraph::new(Line::from(vec![prompt, body, cursor])), r);
    }
    if let Some(&r) = rows.get(3) {
        let hint = if app.is_filter_input_active() {
            Line::from(vec![
                Span::styled("  Esc ", Style::new().fg(theme.accent)),
                Span::styled("cancel  \u{00b7}  ", theme.dim_cell()),
                Span::styled("Enter ", Style::new().fg(theme.accent)),
                Span::styled("accept (filter stays applied)", theme.dim_cell()),
            ])
        } else {
            Line::from(vec![
                Span::styled("  Ctrl+F ", Style::new().fg(theme.accent)),
                Span::styled("edit  \u{00b7}  ", theme.dim_cell()),
                Span::styled("Esc ", Style::new().fg(theme.accent)),
                Span::styled("clear filter", theme.dim_cell()),
            ])
        };
        frame.render_widget(Paragraph::new(hint), r);
    }
}
