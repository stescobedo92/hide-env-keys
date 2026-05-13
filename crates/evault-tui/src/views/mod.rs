//! Top-level views composed by the runtime.

pub mod dashboard;
pub mod help;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

use crate::app::AppState;
use crate::components::{statusbar, toast};
use crate::theme::Theme;

/// Render the full UI for the current `app` state.
///
/// Composes the dashboard, status bar, and (when active) the help
/// overlay. The runtime calls this from inside `terminal.draw(...)`.
pub fn render(frame: &mut Frame<'_>, app: &mut AppState, theme: &Theme) {
    let area = frame.area();

    // Layout: dashboard body | toast row (only when needed) | status.
    // The toast row is fixed-height 1 and pinned directly above the
    // status bar so it never clobbers the dashboard's bottom border
    // or the status hints below it.
    let show_toast = app.toast_text().is_some();
    let constraints: &[Constraint] = if show_toast {
        &[
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ]
    } else {
        &[Constraint::Min(1), Constraint::Length(1)]
    };
    let regions = Layout::vertical(constraints).split(area);
    // `split` always returns `constraints.len()` rects so the indexing
    // below is guaranteed in-bounds — but we still use `get` and a
    // graceful early-return to satisfy `clippy::indexing_slicing`.
    let Some(&body) = regions.first() else { return };
    let Some(&status) = regions.last() else {
        return;
    };

    dashboard::render(frame, body, app, theme);
    statusbar::render(frame, status, app, theme);

    if show_toast {
        if let Some(&toast_area) = regions.get(1) {
            toast::render(frame, toast_area, app, theme);
        }
    }

    if app.help_visible() {
        help::render(frame, centered(area, 60, 70), theme);
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
