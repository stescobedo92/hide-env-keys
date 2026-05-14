//! Editor form — modal popup for the `n` (new var) and `e` (edit
//! value) flows.
//!
//! Renders four fields stacked vertically:
//!
//! ```text
//! ┌─ new variable ─────────────────────┐
//! │  Name:   [API_KEY▌                ] │
//! │  Group:  < user >                   │
//! │  Kind:   < secret >                 │
//! │  Value:  [****▌                   ] │
//! │                                     │
//! │  Tab cycle · Enter submit · Esc     │
//! └─────────────────────────────────────┘
//! ```
//!
//! In `EditValue` mode the Name / Group / Kind cells are displayed
//! read-only (dimmed); only the Value cell accepts input.

use evault_core::model::VarKind;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, EditorForm, EditorMode, FormField, GROUP_CYCLE, KIND_CYCLE};
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(form) = app.current_form() else {
        return;
    };

    // Clear the area first so the dashboard doesn't bleed through.
    frame.render_widget(Clear, area);

    let title = match &form.mode {
        EditorMode::NewVar => " new variable ".to_owned(),
        EditorMode::EditValue { original_name, .. } => {
            format!(" edit value: {original_name} ")
        }
    };
    let block = Block::bordered()
        .title(title)
        .border_style(Style::new().fg(theme.accent));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // 4 field rows + spacer + hints row.
    let constraints = [
        Constraint::Length(1), // Name
        Constraint::Length(1), // Group
        Constraint::Length(1), // Kind
        Constraint::Length(1), // Value
        Constraint::Length(1), // spacer
        Constraint::Length(1), // hints
        Constraint::Min(0),
    ];
    let rows = Layout::vertical(constraints).split(inner);

    let mut render_row = |idx: usize, line: Line<'static>| {
        if let Some(&r) = rows.get(idx) {
            frame.render_widget(Paragraph::new(line), r);
        }
    };

    let read_only = matches!(form.mode, EditorMode::EditValue { .. });

    render_row(0, render_name_row(form, theme, read_only));
    render_row(1, render_group_row(form, theme, read_only));
    render_row(2, render_kind_row(form, theme, read_only));
    render_row(3, render_value_row(form, theme));
    render_row(5, render_hints(form, theme));
}

fn render_name_row(form: &EditorForm, theme: &Theme, read_only: bool) -> Line<'static> {
    let focused = matches!(form.focus, FormField::Name) && !read_only;
    let label = field_label(" Name:   ", focused, theme);
    let body_text = form.name.clone();
    let body_style = if read_only {
        theme.dim_cell()
    } else {
        Style::new()
    };
    let mut spans = vec![label, Span::styled(body_text, body_style)];
    if focused {
        spans.push(cursor_span(theme));
    }
    Line::from(spans)
}

fn render_group_row(form: &EditorForm, theme: &Theme, read_only: bool) -> Line<'static> {
    let focused = matches!(form.focus, FormField::Group) && !read_only;
    let label = field_label(" Group:  ", focused, theme);
    let current = GROUP_CYCLE
        .get(form.group_idx)
        .map_or("user", |g| g.as_str());
    let combo = combo_box(current, focused, read_only, theme);
    Line::from(vec![label, combo])
}

fn render_kind_row(form: &EditorForm, theme: &Theme, read_only: bool) -> Line<'static> {
    let focused = matches!(form.focus, FormField::Kind) && !read_only;
    let label = field_label(" Kind:   ", focused, theme);
    let current = KIND_CYCLE.get(form.kind_idx).map_or("secret", |k| match k {
        VarKind::Secret => "secret",
        VarKind::Plain => "plain",
    });
    let combo = combo_box(current, focused, read_only, theme);
    Line::from(vec![label, combo])
}

fn render_value_row(form: &EditorForm, theme: &Theme) -> Line<'static> {
    let focused = matches!(form.focus, FormField::Value);
    let label = field_label(" Value:  ", focused, theme);
    let body_text = if form.show_value {
        form.value.clone()
    } else {
        "*".repeat(form.value.chars().count())
    };
    let mut spans = vec![label, Span::raw(body_text)];
    if focused {
        spans.push(cursor_span(theme));
    }
    Line::from(spans)
}

fn render_hints(form: &EditorForm, theme: &Theme) -> Line<'static> {
    let read_only = matches!(form.mode, EditorMode::EditValue { .. });
    let mut hints: Vec<Span<'_>> = vec![
        Span::styled("  Enter ", Style::new().fg(theme.accent)),
        Span::styled("submit  \u{00b7}  ", theme.dim_cell()),
        Span::styled("Esc ", Style::new().fg(theme.accent)),
        Span::styled("cancel  \u{00b7}  ", theme.dim_cell()),
    ];
    if !read_only {
        hints.push(Span::styled("Tab ", Style::new().fg(theme.accent)));
        hints.push(Span::styled("cycle field  \u{00b7}  ", theme.dim_cell()));
    }
    if matches!(form.focus, FormField::Value) {
        hints.push(Span::styled("Ctrl+S ", Style::new().fg(theme.accent)));
        hints.push(Span::styled("show value", theme.dim_cell()));
    } else if matches!(form.focus, FormField::Group | FormField::Kind) && !read_only {
        hints.push(Span::styled(
            "\u{2190} \u{2192} ",
            Style::new().fg(theme.accent),
        ));
        hints.push(Span::styled("change option", theme.dim_cell()));
    }
    Line::from(
        hints
            .into_iter()
            .map(|s| Span::styled(s.content, s.style))
            .collect::<Vec<_>>(),
    )
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

fn combo_box(current: &str, focused: bool, read_only: bool, theme: &Theme) -> Span<'static> {
    let style = if read_only {
        theme.dim_cell()
    } else if focused {
        Style::new().fg(theme.warning).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    Span::styled(format!("< {current} >"), style)
}

fn cursor_span(theme: &Theme) -> Span<'static> {
    Span::styled("\u{258C}", Style::new().fg(theme.accent))
}
