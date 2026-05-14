//! Persistent keybindings hint bar — two compact rows pinned just
//! above the status bar so every shortcut is visible at a glance.
//!
//! Each row stretches edge-to-edge by inflating the inter-pair
//! separators with spaces. If the natural content does NOT fit in
//! the available width (very narrow terminal), the row falls back
//! to a compact rendering with `wrap` so nothing is silently
//! truncated.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{AppState, View};
use crate::theme::Theme;

/// Number of rows the hint bar occupies. Reserved by `views::render`.
pub const HEIGHT: u16 = 2;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    if area.height == 0 {
        return;
    }
    let on_detail = matches!(app.current_view(), View::Detail);

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
    if area.height >= 2 {
        render_row(frame, area, 1, &row2, theme);
    }
}

/// One `(key, description)` cell.
struct Pair<'a>(&'a str, &'a str);

fn pair_chars(p: &Pair<'_>) -> usize {
    // "<key> <desc>" — key + 1 space + description.
    p.0.chars().count() + 1 + p.1.chars().count()
}

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
    if pairs.is_empty() || row.width == 0 {
        return;
    }

    // Width of every pair's text + the minimum separator (" \u{00b7} "
    // = 3 chars) between them. Reserve a 1-char gutter on each side.
    let row_w = usize::from(row.width);
    let natural: usize = pairs.iter().map(pair_chars).sum();
    let sep_count = pairs.len() - 1;
    let min_sep = 3_usize;
    let gutter = 1_usize;
    let min_total = natural + sep_count * min_sep + gutter * 2;

    if min_total > row_w {
        // Doesn't fit even at minimum spacing — render with wrap so
        // nothing is silently truncated. The Paragraph will spill
        // onto subsequent lines if `area` allows it.
        render_wrapped(frame, area, row_offset, pairs, theme);
        return;
    }

    // Distribute the extra horizontal space evenly across the
    // separators so the row stretches edge-to-edge.
    let extra = row_w - min_total;
    let per_sep = if sep_count == 0 { 0 } else { extra / sep_count };
    let remainder = if sep_count == 0 { 0 } else { extra % sep_count };

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(pairs.len() * 4);
    spans.push(Span::raw(" ".repeat(gutter)));
    for (i, pair) in pairs.iter().enumerate() {
        if i > 0 {
            // base separator + per-sep extra + (1 more for the first
            // `remainder` separators so the row is perfectly flush).
            let extra_here = per_sep + usize::from(i <= remainder);
            let pad_left = " ".repeat(1 + extra_here / 2);
            let pad_right = " ".repeat(1 + extra_here - extra_here / 2);
            spans.push(Span::styled(pad_left, theme.dim_cell()));
            spans.push(Span::styled("\u{00b7}", theme.dim_cell()));
            spans.push(Span::styled(pad_right, theme.dim_cell()));
        }
        spans.push(Span::styled(
            pair.0.to_owned(),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(pair.1.to_owned(), theme.dim_cell()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), row);
}

/// Fallback for narrow terminals: render the pairs as a wrapped
/// paragraph so they spill onto subsequent lines instead of being
/// truncated.
fn render_wrapped(
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
        height: area.height.saturating_sub(row_offset),
    };
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(pairs.len() * 4);
    spans.push(Span::raw(" "));
    for (i, pair) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" \u{00b7} ", theme.dim_cell()));
        }
        spans.push(Span::styled(
            pair.0.to_owned(),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(pair.1.to_owned(), theme.dim_cell()));
    }
    let para = Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false });
    frame.render_widget(para, row);
}
