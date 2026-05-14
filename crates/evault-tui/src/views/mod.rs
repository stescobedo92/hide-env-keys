//! Top-level views composed by the runtime.

pub mod dashboard;
pub mod detail;
pub mod editor;
pub mod help;
pub mod link_form;
pub mod view_value;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

use crate::app::{AppState, View};
use crate::components::{fuzzy_input, keybindings, modal, statusbar, toast};
use crate::theme::Theme;

/// Render the full UI for the current `app` state.
///
/// Composes the dashboard, optional fuzzy-input strip, optional
/// toast strip, the persistent keybindings hint bar, the status
/// bar, and (when active) any modal overlays.
pub fn render(frame: &mut Frame<'_>, app: &mut AppState, theme: &Theme) {
    let area = frame.area();

    // Layout, top-down:
    //   - body (dashboard / detail)             — flexible (`Min(1)`)
    //   - fuzzy input (1 row, only when active)
    //   - toast (1 row, only when active)
    //   - keybindings hint bar (`HEIGHT` rows, always)
    //   - status bar (1 row, always)
    //
    // We compose the constraint slice dynamically so optional rows
    // do not steal vertical space when they are not on-screen.
    let show_filter = app.is_filter_active();
    let show_toast = app.toast_text().is_some();

    let mut constraints: Vec<Constraint> = vec![Constraint::Min(1)];
    if show_filter {
        constraints.push(Constraint::Length(1));
    }
    if show_toast {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(keybindings::HEIGHT));
    constraints.push(Constraint::Length(1)); // status bar

    let regions = Layout::vertical(constraints).split(area);
    let Some(&body) = regions.first() else { return };
    let Some(&status) = regions.last() else {
        return;
    };
    // Keybindings strip sits in the second-to-last slot. `split`
    // returns at least 2 entries (body + status) so the
    // `len() - 2` access is safe, but we use `get` for defence.
    let keybindings_idx = regions.len().saturating_sub(2);
    let Some(&keybindings_rect) = regions.get(keybindings_idx) else {
        return;
    };

    match app.current_view() {
        View::Dashboard => dashboard::render(frame, body, app, theme),
        View::Detail => detail::render(frame, body, app, theme),
    }
    keybindings::render(frame, keybindings_rect, app, theme);
    statusbar::render(frame, status, app, theme);

    // Optional rows between body and keybindings strip.
    let mut next = 1_usize;
    if show_filter {
        if let Some(&r) = regions.get(next) {
            fuzzy_input::render(frame, r, app, theme);
        }
        next += 1;
    }
    if show_toast {
        if let Some(&r) = regions.get(next) {
            toast::render(frame, r, app, theme);
        }
    }

    if app.help_visible() {
        help::render(frame, centered(area, 60, 70), theme);
    }

    // Editor form modal.
    if app.is_form_visible() {
        editor::render(frame, centered(area, 60, 35), app, theme);
    }

    // Link form modal.
    if app.is_link_form_visible() {
        link_form::render(frame, centered(area, 60, 30), app, theme);
    }

    // View-value modal.
    if app.is_view_value_visible() {
        view_value::render(frame, centered(area, 70, 40), app, theme);
    }

    // Confirm modal drawn last so it sits on top of every other
    // layer (dashboard / detail / filter input / toast / help /
    // editor / link / view-value).
    if let Some(req) = app.current_confirm() {
        modal::render(frame, centered(area, 50, 25), req, theme);
    }
}

/// A rectangle centered inside `outer`, sized as percentages.
fn centered(outer: Rect, pct_x: u16, pct_y: u16) -> Rect {
    let [_, mid_v, _] = Layout::vertical([
        Constraint::Percentage((100 - pct_y) / 2),
        Constraint::Percentage(pct_y),
        Constraint::Percentage((100 - pct_y) / 2),
    ])
    .areas(outer);
    let [_, mid_h, _] = Layout::horizontal([
        Constraint::Percentage((100 - pct_x) / 2),
        Constraint::Percentage(pct_x),
        Constraint::Percentage((100 - pct_x) / 2),
    ])
    .areas(mid_v);
    mid_h
}
