//! View-value modal — read-only popup showing a variable's
//! decrypted value (or its masked form). Reached via the `v` key.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;
use secrecy::ExposeSecret;

use crate::app::AppState;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(modal) = app.current_view_value() else {
        return;
    };
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .title(format!(" value of {} ", modal.name))
        .border_style(Style::new().fg(theme.warning));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // spacer
        Constraint::Min(1),    // value
        Constraint::Length(1), // hint
    ])
    .split(inner);

    let value_text = if modal.show {
        modal.value.expose_secret().to_owned()
    } else {
        "*".repeat(modal.value.expose_secret().chars().count())
    };

    if let Some(&r) = rows.get(1) {
        let para = Paragraph::new(value_text).wrap(Wrap { trim: false });
        frame.render_widget(para, r);
    }
    if let Some(&r) = rows.get(2) {
        let hint = Line::from(vec![
            Span::styled("  Ctrl+S ", Style::new().fg(theme.accent)),
            Span::styled(if modal.show { "hide" } else { "reveal" }, theme.dim_cell()),
            Span::styled("  \u{00b7}  ", theme.dim_cell()),
            Span::styled("Esc ", Style::new().fg(theme.accent)),
            Span::styled("close", theme.dim_cell()),
        ]);
        frame.render_widget(Paragraph::new(hint), r);
    }
}
