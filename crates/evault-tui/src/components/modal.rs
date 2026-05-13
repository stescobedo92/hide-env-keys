//! Confirmation modal — focused overlay with [y / n] choices.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::ConfirmRequest;
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, req: &ConfirmRequest, theme: &Theme) {
    // Clear so nothing underneath bleeds through the modal.
    frame.render_widget(Clear, area);

    let body_lines: Vec<Line<'_>> = req
        .body
        .lines()
        .map(|s| Line::from(Span::raw(s.to_owned())))
        .collect();

    let mut lines = vec![Line::raw("")];
    lines.extend(body_lines);
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            "  [y / Enter] yes ",
            Style::new().fg(theme.warning).add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::styled("[n / Esc] no", theme.dim_cell()),
    ]));

    let block = Block::bordered()
        .title(format!(" {} ", req.title))
        .border_style(Style::new().fg(theme.warning));
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}
