//! Run-in-project form — centered modal popup.
//!
//! Renders three fields stacked vertically:
//!
//! ```text
//! ┌─ run in project ──────────────────────────────┐
//! │                                                │
//! │  Path:     ▌                                   │
//! │  Profile:  default                             │
//! │  Command:  npm start                           │
//! │                                                │
//! │  Tab cycle · Enter run · Esc cancel            │
//! └────────────────────────────────────────────────┘
//! ```
//!
//! The `Command` field tokenises on whitespace at submit time. Quoted
//! arguments are NOT supported — users with complex shell quoting
//! needs should fall back to the `evault run` CLI command.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, RunField, RunForm};
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState, theme: &Theme) {
    let Some(form) = app.current_run_form() else {
        return;
    };
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .title(" run in project ")
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
    draw(2, command_row(form, theme));
    draw(4, hints_row(theme));
}

fn path_row(form: &RunForm, theme: &Theme) -> Line<'static> {
    let focused = matches!(form.focus, RunField::Path);
    let label = field_label(" Path:     ", focused, theme);
    let mut spans = vec![label, Span::raw(form.path.clone())];
    if focused {
        spans.push(cursor_span(theme));
    }
    Line::from(spans)
}

fn profile_row(form: &RunForm, theme: &Theme) -> Line<'static> {
    let focused = matches!(form.focus, RunField::Profile);
    let label = field_label(" Profile:  ", focused, theme);
    let mut spans = vec![label, Span::raw(form.profile.clone())];
    if focused {
        spans.push(cursor_span(theme));
    }
    Line::from(spans)
}

fn command_row(form: &RunForm, theme: &Theme) -> Line<'static> {
    let focused = matches!(form.focus, RunField::Command);
    let label = field_label(" Command:  ", focused, theme);
    let mut spans = vec![label, Span::raw(form.command.clone())];
    if focused {
        spans.push(cursor_span(theme));
    }
    Line::from(spans)
}

fn hints_row(theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled("  Enter ", Style::new().fg(theme.accent)),
        Span::styled("run  \u{00b7}  ", theme.dim_cell()),
        Span::styled("Esc ", Style::new().fg(theme.accent)),
        Span::styled("cancel  \u{00b7}  ", theme.dim_cell()),
        Span::styled("Tab ", Style::new().fg(theme.accent)),
        Span::styled("cycle field", theme.dim_cell()),
    ])
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
