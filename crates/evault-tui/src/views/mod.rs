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
    let [body, status] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);

    dashboard::render(frame, body, app, theme);
    statusbar::render(frame, status, app, theme);

    if app.help_visible() {
        help::render(frame, centered(area, 60, 70), theme);
    }

    if app.toast_text().is_some() {
        let toast_area = bottom_strip(area, 1);
        toast::render(frame, toast_area, app, theme);
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

/// A 1-line strip pinned to the bottom of `outer`, above the status
/// bar so the toast does not clobber it.
fn bottom_strip(outer: Rect, lines: u16) -> Rect {
    let height = lines.saturating_add(1).min(outer.height);
    Rect {
        x: outer.x,
        y: outer.y + outer.height.saturating_sub(height),
        width: outer.width,
        height: lines.min(outer.height),
    }
}
