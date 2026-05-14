//! Error modal — focused popup that surfaces an action failure with
//! an explanatory hint. Replaces the bottom-bar toast for cases
//! where the user just initiated the failing action and a quiet
//! toast would be too easy to miss.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::AppState;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(modal) = app.current_error_modal() else {
        return;
    };
    frame.render_widget(Clear, area);

    // Bordered block in the error palette + a clear "Error" title.
    let block = Block::bordered()
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "\u{2716}",
                Style::new().fg(theme.error).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                modal.title.clone(),
                Style::new().fg(theme.error).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]))
        .border_style(Style::new().fg(theme.error));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // spacer
        Constraint::Min(2),    // message (wraps)
        Constraint::Length(1), // spacer
        Constraint::Min(1),    // hint (wraps; can be empty)
        Constraint::Length(1), // spacer
        Constraint::Length(1), // dismiss hint
    ])
    .split(inner);

    // Message line(s) — leading error icon then the raw backend
    // message. Wrapped so long messages don't get truncated.
    if let Some(&r) = rows.get(1) {
        let lines = vec![Line::from(vec![
            Span::styled(
                " \u{2716} ",
                Style::new().fg(theme.error).add_modifier(Modifier::BOLD),
            ),
            Span::styled(modal.message.clone(), Style::new().fg(theme.error)),
        ])];
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), r);
    }

    // Optional hint — rendered in dim style so it's clearly
    // secondary to the error message itself.
    if let (Some(&r), Some(hint)) = (rows.get(3), modal.hint.as_deref()) {
        let para = Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(hint.to_owned(), theme.dim_cell()),
        ]))
        .wrap(Wrap { trim: false });
        frame.render_widget(para, r);
    }

    if let Some(&r) = rows.get(5) {
        let hint = Line::from(vec![
            Span::styled(" Enter ", Style::new().fg(theme.accent)),
            Span::styled("acknowledge  \u{00b7}  ", theme.dim_cell()),
            Span::styled("Esc ", Style::new().fg(theme.accent)),
            Span::styled("dismiss", theme.dim_cell()),
        ]);
        frame.render_widget(Paragraph::new(hint), r);
    }
}
