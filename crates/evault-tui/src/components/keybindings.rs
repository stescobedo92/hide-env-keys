//! Persistent keybindings hint bar — two compact rows pinned just
//! above the status bar so every shortcut is visible at a glance.
//!
//! Renders only what's reachable from the current view + state (no
//! point showing `Enter detail` on the detail view, for instance).

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{AppState, View};
use crate::theme::Theme;

/// Number of rows the hint bar occupies. Reserved by `views::render`.
pub const HEIGHT: u16 = 2;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let area_height = area.height;
    if area_height == 0 {
        return;
    }
    let on_detail = matches!(app.current_view(), View::Detail);

    // Row 1: navigation + CRUD actions on the selected row.
    let row1: Vec<Pair<'static>> = if on_detail {
        vec![
            Pair("Esc", "back"),
            Pair("s", "toggle secret"),
            Pair("e", "edit value"),
            Pair("d", "delete"),
            Pair("l", "link to project"),
            Pair("v", "view value"),
            Pair("?", "help"),
        ]
    } else {
        vec![
            Pair("j/k \u{2195}", "move"),
            Pair("Enter", "detail"),
            Pair("n", "new var"),
            Pair("e", "edit value"),
            Pair("d", "delete"),
            Pair("l", "link to project"),
            Pair("v", "view value"),
        ]
    };

    // Row 2: filter / mask / meta actions.
    let row2: Vec<Pair<'static>> = vec![
        Pair("Ctrl+F", "fuzzy filter"),
        Pair("s", "mask/show secrets"),
        Pair("r", "refresh"),
        Pair("?", "help overlay"),
        Pair("Esc", "back / dismiss"),
        Pair("q", "quit"),
        Pair("Ctrl+C", "quit"),
    ];

    render_row(frame, area, 0, &row1, theme);
    if area_height >= 2 {
        render_row(frame, area, 1, &row2, theme);
    }
}

/// One (key, description) cell.
struct Pair<'a>(&'a str, &'a str);

fn render_row(
    frame: &mut Frame<'_>,
    area: Rect,
    row_offset: u16,
    pairs: &[Pair<'static>],
    theme: &Theme,
) {
    let row = Rect {
        x: area.x,
        y: area.y + row_offset,
        width: area.width,
        height: 1,
    };
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(pairs.len() * 3);
    spans.push(Span::raw(" ")); // leading gutter
    for (i, Pair(key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  \u{00b7}  ", theme.dim_cell()));
        }
        spans.push(Span::styled(
            (*key).to_owned(),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*desc).to_owned(), theme.dim_cell()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), row);
}
