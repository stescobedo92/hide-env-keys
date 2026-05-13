//! Help overlay listing every key binding.
//!
//! **Keep in sync with [`crate::event::Action::from_key`].** The two
//! tables are maintained by hand; renaming or adding a binding
//! requires updating both.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    // Clear the area first so the dashboard does not bleed through.
    frame.render_widget(Clear, area);

    let entries: &[(&str, &str)] = &[
        ("j / ↓", "move down"),
        ("k / ↑", "move up"),
        ("g", "jump to top"),
        ("G", "jump to bottom"),
        ("PgDn", "page down"),
        ("PgUp", "page up"),
        ("Enter", "open detail view (Esc returns)"),
        ("n", "new variable"),
        ("e", "edit variable"),
        ("d", "delete (with [y/n] confirm)"),
        ("l", "link to project"),
        ("y", "copy value"),
        ("s", "toggle secret visibility"),
        ("Ctrl+F", "fuzzy search"),
        ("p", "switch profile"),
        ("Tab", "next view"),
        ("r", "refresh"),
        ("?", "toggle this help"),
        ("Esc", "dismiss toast / overlay (or quit)"),
        ("q", "quit"),
        ("Ctrl+C", "quit"),
    ];

    let key_style = Style::new().fg(theme.accent);
    let dim_style = theme.dim_cell();

    let lines: Vec<Line<'_>> = entries
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(format!("  {key:<8}"), key_style),
                Span::raw("  "),
                Span::styled((*desc).to_string(), dim_style),
            ])
        })
        .collect();

    let block = Block::bordered().title(" help ");
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}
