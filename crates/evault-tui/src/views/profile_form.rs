//! Active-profile modal.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::AppState;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(form) = app.current_profile_form() else {
        return;
    };
    let block = Block::bordered()
        .title(" active profile ")
        .border_style(Style::new().fg(theme.accent));
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "Profile  ",
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw(form.profile.clone()),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Enter", Style::new().fg(theme.accent)),
            Span::styled(" apply   ", theme.dim_cell()),
            Span::styled("Esc", Style::new().fg(theme.accent)),
            Span::styled(" cancel", theme.dim_cell()),
        ]),
    ];
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}
