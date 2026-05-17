//! Persistent keybindings hint bar — two compact rows pinned just
//! above the status bar so every shortcut is visible at a glance.
//!
//! The two rows are rendered as a **column grid**: each pair sits in
//! a cell whose width is the max of the natural widths of the two
//! pairs at that column index. Separators (`·`) therefore line up
//! vertically between row 1 and row 2.
//!
//! If the natural content does NOT fit in the available width (very
//! narrow terminal), the rows fall back to a wrapped rendering so
//! nothing is silently truncated.

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
    let row1: Vec<Pair<'static>> = match app.current_view() {
        View::Detail => vec![
            Pair("Esc", "back"),
            Pair("s", "toggle secret"),
            Pair("e", "edit value"),
            Pair("d", "delete"),
            Pair("l", "link to project"),
            Pair("v", "view value"),
            Pair("y", "copy value"),
        ],
        View::Audit => vec![
            Pair("Tab", "dashboard"),
            Pair("r", "refresh"),
            Pair("p", "profile"),
            Pair("?", "help"),
            Pair("Esc", "back"),
            Pair("q", "quit"),
        ],
        View::Dashboard => vec![
            Pair("j/k \u{2195}", "move"),
            Pair("Enter", "detail"),
            Pair("n", "new var"),
            Pair("e", "edit value"),
            Pair("d", "delete"),
            Pair("l", "link to project"),
            Pair("v/y", "view/copy"),
        ],
    };

    let row2: Vec<Pair<'static>> = vec![
        Pair("Ctrl+F", "fuzzy filter"),
        Pair("s", "mask/show secrets"),
        Pair("r", "refresh"),
        Pair("R", "run in project"),
        Pair("p", "profile"),
        Pair("Tab", "audit"),
        Pair("?", "help overlay"),
        Pair("Esc", "back / dismiss"),
        Pair("q", "quit"),
    ];

    render_aligned(frame, area, &row1, &row2, theme);
}

/// One `(key, description)` cell.
struct Pair<'a>(&'a str, &'a str);

fn pair_chars(p: &Pair<'_>) -> usize {
    // "<key> <desc>" — key + 1 space + description.
    p.0.chars().count() + 1 + p.1.chars().count()
}

/// Render two rows as a column-aligned grid. Column `i` is the max
/// of the natural widths of `row1[i]` and `row2[i]`; the separators
/// between columns use identical padding on both rows so the `·`
/// characters line up vertically.
fn render_aligned(
    frame: &mut Frame<'_>,
    area: Rect,
    row1: &[Pair<'static>],
    row2: &[Pair<'static>],
    theme: &Theme,
) {
    if area.width == 0 || row1.is_empty() || row2.is_empty() {
        return;
    }

    let columns = row1.len().max(row2.len());
    let widths: Vec<usize> = (0..columns)
        .map(|i| {
            let w1 = row1.get(i).map_or(0, pair_chars);
            let w2 = row2.get(i).map_or(0, pair_chars);
            w1.max(w2)
        })
        .collect();

    let row_w = usize::from(area.width);
    let natural: usize = widths.iter().sum();
    let sep_count = columns.saturating_sub(1);
    let min_sep = 3_usize; // " · "
    let gutter = 1_usize;
    let min_total = natural + sep_count * min_sep + gutter * 2;

    if min_total > row_w {
        // Doesn't fit even at minimum spacing — render each row with
        // wrap so nothing is silently truncated.
        render_wrapped(frame, area, 0, row1, theme);
        if area.height >= 2 {
            render_wrapped(frame, area, 1, row2, theme);
        }
        return;
    }

    let extra = row_w - min_total;
    let (per_sep, remainder) = if sep_count == 0 {
        (0, 0)
    } else {
        (extra / sep_count, extra % sep_count)
    };

    render_aligned_row(
        frame, area, 0, row1, &widths, per_sep, remainder, gutter, theme,
    );
    if area.height >= 2 {
        render_aligned_row(
            frame, area, 1, row2, &widths, per_sep, remainder, gutter, theme,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_aligned_row(
    frame: &mut Frame<'_>,
    area: Rect,
    row_offset: u16,
    pairs: &[Pair<'static>],
    widths: &[usize],
    per_sep: usize,
    remainder: usize,
    gutter: usize,
    theme: &Theme,
) {
    let row = Rect {
        x: area.x,
        y: area.y + row_offset,
        width: area.width,
        height: 1,
    };

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(widths.len() * 5);
    spans.push(Span::raw(" ".repeat(gutter)));

    for (i, target_w) in widths.iter().enumerate() {
        if i > 0 {
            // Same separator padding on both rows so the `·` aligns
            // vertically. base 1 space + per_sep extra, with the
            // first `remainder` separators getting one extra each.
            let extra_here = per_sep + usize::from(i <= remainder);
            let pad_left = " ".repeat(1 + extra_here / 2);
            let pad_right = " ".repeat(1 + extra_here - extra_here / 2);
            spans.push(Span::styled(pad_left, theme.dim_cell()));
            spans.push(Span::styled("\u{00b7}", theme.dim_cell()));
            spans.push(Span::styled(pad_right, theme.dim_cell()));
        }
        if let Some(pair) = pairs.get(i) {
            spans.push(Span::styled(
                pair.0.to_owned(),
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(pair.1.to_owned(), theme.dim_cell()));
            let natural = pair_chars(pair);
            if *target_w > natural {
                spans.push(Span::raw(" ".repeat(target_w - natural)));
            }
        } else {
            // Empty cell — pad with spaces so the next separator
            // still aligns with the other row.
            spans.push(Span::raw(" ".repeat(*target_w)));
        }
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
