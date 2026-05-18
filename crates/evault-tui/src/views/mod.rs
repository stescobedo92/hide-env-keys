//! Top-level views composed by the runtime.

pub mod audit;
pub mod dashboard;
pub mod detail;
pub mod editor;
pub mod error_modal;
pub mod help;
pub mod link_form;
pub mod profile_form;
pub mod run_form;
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
    let show_toast = app.toast_text().is_some();
    let regions = layout_regions(area, show_toast);

    match app.current_view() {
        View::Dashboard => dashboard::render(frame, regions.body, app, theme),
        View::Detail => detail::render(frame, regions.body, app, theme),
        View::Audit => audit::render(frame, regions.body, app, theme),
    }
    statusbar::render(frame, regions.status, app, theme);
    keybindings::render(frame, regions.keybindings, app, theme);

    if show_toast {
        if let Some(r) = regions.toast {
            toast::render(frame, r, app, theme);
        }
    }

    // Fuzzy filter modal — only while the input is being typed.
    // Once committed (Enter), the modal closes and the filter stays
    // applied (the dashboard title shows `vars (matched/total)`).
    if app.is_filter_input_active() {
        fuzzy_input::render(frame, centered(area, 50, 25), app, theme);
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

    // Run-in-project form modal.
    if app.is_run_form_visible() {
        run_form::render(frame, centered(area, 60, 30), app, theme);
    }

    // Active profile form modal.
    if app.is_profile_form_visible() {
        profile_form::render(frame, centered(area, 50, 24), app, theme);
    }

    // View-value modal.
    if app.is_view_value_visible() {
        view_value::render(frame, centered(area, 70, 40), app, theme);
    }

    // Confirm modal drawn ABOVE the regular stack so a delete
    // confirmation always sits on top.
    if let Some(req) = app.current_confirm() {
        modal::render(frame, centered(area, 50, 25), req, theme);
    }

    // Error modal sits at the TOP of the layer stack — when an
    // action fails the user must acknowledge before anything else
    // can be interacted with. Sized generously (60% x 50%) so a
    // multi-line hint with bullets fits comfortably.
    if app.is_error_modal_visible() {
        error_modal::render(frame, centered(area, 60, 50), app, theme);
    }
}

pub struct UiRegions {
    pub status: Rect,
    pub body: Rect,
    pub toast: Option<Rect>,
    pub keybindings: Rect,
}

/// Layout shared by rendering and mouse hit-testing.
///
/// Top status is deliberately outside the bottom keybinding strip so the
/// context line does not visually collide with shortcuts.
pub fn layout_regions(area: Rect, show_toast: bool) -> UiRegions {
    let mut constraints: Vec<Constraint> = vec![Constraint::Length(1), Constraint::Min(1)];
    if show_toast {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(keybindings::HEIGHT));

    let regions = Layout::vertical(constraints).split(area);
    let status = regions[0];
    let body = regions[1];
    let toast = if show_toast {
        regions.get(2).copied()
    } else {
        None
    };
    let keybindings = *regions.last().unwrap_or(&area);
    UiRegions {
        status,
        body,
        toast,
        keybindings,
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
