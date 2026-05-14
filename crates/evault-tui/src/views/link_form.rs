//! Link-to-project form — centered modal popup.
//!
//! Renders three fields stacked vertically:
//!
//! ```text
//! ┌─ link API_KEY to project ──────────────┐
//! │                                         │
//! │  Path:        ▌                         │
//! │  Profile:     default                   │
//! │  Materialize: < no >                    │
//! │                                         │
//! │  Tab cycle · Enter submit · Esc cancel  │
//! └─────────────────────────────────────────┘
//! ```

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, LinkField, LinkForm};
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(form) = app.current_link_form() else {
        return;
    };
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .title(format!(" link {} to project ", form.var_name))
        .border_style(Style::new().fg(theme.accent));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    let mut draw = |idx: usize, line: Line<'static>| {
        if let Some(&r) = rows.get(idx) {
            frame.render_widget(Paragraph::new(line), r);
        }
    };

    draw(0, path_row(form, theme));
    draw(1, profile_row(form, theme));
    draw(2, materialize_row(form, theme));
    draw(4, hints_row(form, theme));
}

fn path_row(form: &LinkForm, theme: &Theme) -> Line<'static> {
    let focused = matches!(form.focus, LinkField::Path);
    let label = field_label(" Path:        ", focused, theme);
    let mut spans = vec![label, Span::raw(form.path.clone())];
    if focused {
        spans.push(cursor_span(theme));
    }
    Line::from(spans)
}

fn profile_row(form: &LinkForm, theme: &Theme) -> Line<'static> {
    let focused = matches!(form.focus, LinkField::Profile);
    let label = field_label(" Profile:     ", focused, theme);
    let mut spans = vec![label, Span::raw(form.profile.clone())];
    if focused {
        spans.push(cursor_span(theme));
    }
    Line::from(spans)
}

fn materialize_row(form: &LinkForm, theme: &Theme) -> Line<'static> {
    let focused = matches!(form.focus, LinkField::Materialize);
    let label = field_label(" Materialize: ", focused, theme);
    let opt = if form.materialize {
        "< yes >"
    } else {
        "< no >"
    };
    let style = if focused {
        Style::new().fg(theme.warning).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    Line::from(vec![label, Span::styled(opt.to_owned(), style)])
}

fn hints_row(form: &LinkForm, theme: &Theme) -> Line<'static> {
    let mut hints = vec![
        Span::styled("  Enter ", Style::new().fg(theme.accent)),
        Span::styled("submit  \u{00b7}  ", theme.dim_cell()),
        Span::styled("Esc ", Style::new().fg(theme.accent)),
        Span::styled("cancel  \u{00b7}  ", theme.dim_cell()),
        Span::styled("Tab ", Style::new().fg(theme.accent)),
        Span::styled("cycle field", theme.dim_cell()),
    ];
    if matches!(form.focus, LinkField::Materialize) {
        hints.push(Span::styled("  \u{00b7}  ", theme.dim_cell()));
        hints.push(Span::styled(
            "Space / y / n ",
            Style::new().fg(theme.accent),
        ));
        hints.push(Span::styled("toggle", theme.dim_cell()));
    }
    Line::from(hints)
}

fn field_label(text: &str, focused: bool, theme: &Theme) -> Span<'static> {
    if focused {
        Span::styled(
            text.to_owned(),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(text.to_owned(), theme.dim_cell())
    }
}

fn cursor_span(theme: &Theme) -> Span<'static> {
    Span::styled("\u{258C}", Style::new().fg(theme.accent))
}
