//! Deterministic state machine driving the dashboard.

use ratatui::widgets::TableState;

use crate::event::Action;
use crate::provider::{ProviderError, VarProvider, VarSummary};

/// In-session toast displayed at the bottom of the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub(crate) text: String,
    pub(crate) kind: ToastKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// Neutral, informational message.
    Info,
    /// Failure surfaced from the provider or another subsystem.
    Error,
}

/// Overlay currently on top of the main view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    Help,
}

/// Dashboard state.
///
/// `AppState` is a pure value: every public method is a function of
/// `(state, input) -> state'`, so the whole UI can be exercised in
/// unit tests without ever touching a terminal. The runtime
/// ([`crate::run_tui`]) is the only piece that performs I/O.
#[derive(Debug)]
pub struct AppState {
    rows: Vec<VarSummary>,
    table_state: TableState,
    overlay: Overlay,
    toast: Option<Toast>,
    secrets_visible: bool,
    quit: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Construct an empty state with no rows loaded yet.
    ///
    /// Call [`Self::refresh`] before rendering for the first time to
    /// populate the dashboard.
    #[must_use]
    pub fn new() -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        Self {
            rows: Vec::new(),
            table_state,
            overlay: Overlay::None,
            toast: None,
            secrets_visible: false,
            quit: false,
        }
    }

    /// Re-read rows from `provider` and clamp the selection.
    ///
    /// On success the dashboard's row buffer is replaced. On failure
    /// the previous rows are preserved and the error is returned
    /// unchanged — the runtime decides whether to display it as a
    /// toast.
    ///
    /// # Errors
    /// Propagates whatever [`ProviderError`] the provider returns.
    pub fn refresh<P: VarProvider + ?Sized>(&mut self, provider: &P) -> Result<(), ProviderError> {
        let rows = provider.list()?;
        self.rows = rows;
        self.clamp_selection();
        Ok(())
    }

    /// Apply one [`Action`] to the state.
    ///
    /// Side-effect-free: only mutates `self`. The runtime drives this
    /// in a tight loop with each translated key event.
    pub fn apply(&mut self, action: Action) {
        // Any meaningful interaction dismisses a stale toast.
        if !matches!(action, Action::Noop) {
            self.toast = None;
        }
        match action {
            Action::Quit => self.quit = true,
            Action::Dismiss => self.dismiss(),
            Action::MoveDown => self.select_next(),
            Action::MoveUp => self.select_prev(),
            Action::MoveTop => self.select_first(),
            Action::MoveBottom => self.select_last(),
            Action::PageDown => self.page(true),
            Action::PageUp => self.page(false),
            Action::ToggleHelp => self.toggle_help(),
            Action::ToggleSecretVisibility => {
                self.secrets_visible = !self.secrets_visible;
            }
            Action::Noop => {}
            // Phase-1 surfaces: not yet wired to a registry. We surface
            // a toast so the user knows the key was *received* but the
            // operation is not yet implemented.
            Action::OpenDetail
            | Action::NewVar
            | Action::EditVar
            | Action::DeleteVar
            | Action::LinkVar
            | Action::CopyValue
            | Action::StartFuzzy
            | Action::SwitchProfile
            | Action::NextView
            | Action::Refresh => {
                self.set_info_toast("not implemented in this build");
            }
        }
    }

    /// Set an informational toast (auto-dismissed on the next action).
    pub fn set_info_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some(Toast {
            text: msg.into(),
            kind: ToastKind::Info,
        });
    }

    /// Set an error toast (rendered in the `error` palette).
    pub fn set_error_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some(Toast {
            text: msg.into(),
            kind: ToastKind::Error,
        });
    }

    /// `true` if the runtime should exit after the current frame.
    #[must_use]
    pub const fn quit_requested(&self) -> bool {
        self.quit
    }

    /// Read-only access to the row buffer.
    #[must_use]
    pub fn rows(&self) -> &[VarSummary] {
        &self.rows
    }

    /// Currently selected row index, if any.
    #[must_use]
    pub const fn selected_index(&self) -> Option<usize> {
        self.table_state.selected()
    }

    /// Read-only access to the [`TableState`]. Useful for tests that
    /// want to inspect the cursor without rendering.
    #[must_use]
    pub const fn table_state(&self) -> &TableState {
        &self.table_state
    }

    /// Mutable access to the [`TableState`]. The dashboard view
    /// uses this when calling `render_stateful_widget`.
    pub const fn table_state_mut(&mut self) -> &mut TableState {
        &mut self.table_state
    }

    /// Whether the help overlay is currently visible.
    #[must_use]
    pub const fn help_visible(&self) -> bool {
        matches!(self.overlay, Overlay::Help)
    }

    /// Whether secret values should be rendered (otherwise masked).
    #[must_use]
    pub const fn secrets_visible(&self) -> bool {
        self.secrets_visible
    }

    /// The currently-displayed toast text, if any.
    #[must_use]
    pub fn toast_text(&self) -> Option<&str> {
        self.toast.as_ref().map(|t| t.text.as_str())
    }

    /// Whether the current toast is an error (vs informational).
    #[must_use]
    pub fn toast_is_error(&self) -> bool {
        matches!(self.toast.as_ref().map(|t| t.kind), Some(ToastKind::Error))
    }

    pub(crate) const fn current_toast(&self) -> Option<&Toast> {
        self.toast.as_ref()
    }

    const fn dismiss(&mut self) {
        if matches!(self.overlay, Overlay::Help) {
            self.overlay = Overlay::None;
        } else {
            self.quit = true;
        }
    }

    const fn toggle_help(&mut self) {
        self.overlay = match self.overlay {
            Overlay::Help => Overlay::None,
            Overlay::None => Overlay::Help,
        };
    }

    fn clamp_selection(&mut self) {
        if self.rows.is_empty() {
            self.table_state.select(None);
            return;
        }
        let max = self.rows.len().saturating_sub(1);
        let cur = self.table_state.selected().unwrap_or(0).min(max);
        self.table_state.select(Some(cur));
    }

    fn select_next(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len();
        let next = self.table_state.selected().map_or(0, |i| (i + 1) % len);
        self.table_state.select(Some(next));
    }

    fn select_prev(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len();
        let prev = self
            .table_state
            .selected()
            .map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
        self.table_state.select(Some(prev));
    }

    #[allow(clippy::missing_const_for_fn)]
    fn select_first(&mut self) {
        if !self.rows.is_empty() {
            self.table_state.select(Some(0));
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    fn select_last(&mut self) {
        if let Some(last) = self.rows.len().checked_sub(1) {
            self.table_state.select(Some(last));
        }
    }

    fn page(&mut self, down: bool) {
        // A "page" is intentionally a fixed stride; the runtime does
        // not know the viewport size at action-translation time. Ten
        // rows is a sensible compromise that works on small and large
        // terminals alike.
        const STRIDE: usize = 10;
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len();
        let cur = self.table_state.selected().unwrap_or(0);
        let new = if down {
            cur.saturating_add(STRIDE).min(len - 1)
        } else {
            cur.saturating_sub(STRIDE)
        };
        self.table_state.select(Some(new));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evault_core::model::{Group, VarId, VarKind};
    use time::OffsetDateTime;

    struct StaticProvider(Vec<VarSummary>);
    impl VarProvider for StaticProvider {
        fn list(&self) -> Result<Vec<VarSummary>, ProviderError> {
            Ok(self.0.clone())
        }
    }

    struct FailingProvider;
    impl VarProvider for FailingProvider {
        fn list(&self) -> Result<Vec<VarSummary>, ProviderError> {
            Err(ProviderError::Backend("synthetic".into()))
        }
    }

    fn summary(name: &str) -> VarSummary {
        VarSummary {
            id: VarId::new_v4(),
            name: name.into(),
            group: Group::User,
            kind: VarKind::Plain,
            value_len: name.len(),
            linked_projects: 0,
            updated_at: OffsetDateTime::now_utc(),
        }
    }

    fn three_rows() -> StaticProvider {
        StaticProvider(vec![summary("ALPHA"), summary("BETA"), summary("GAMMA")])
    }

    #[test]
    fn refresh_populates_rows() {
        let mut app = AppState::new();
        app.refresh(&three_rows()).unwrap();
        assert_eq!(app.rows().len(), 3);
        assert_eq!(app.selected_index(), Some(0));
    }

    #[test]
    fn refresh_with_empty_provider_clears_selection() {
        let mut app = AppState::new();
        app.refresh(&three_rows()).unwrap();
        app.refresh(&StaticProvider(Vec::new())).unwrap();
        assert!(app.rows().is_empty());
        assert_eq!(app.selected_index(), None);
    }

    #[test]
    fn refresh_propagates_provider_error() {
        let mut app = AppState::new();
        let err = app.refresh(&FailingProvider).unwrap_err();
        assert!(matches!(err, ProviderError::Backend(_)));
    }

    #[test]
    fn selection_wraps_around() {
        let mut app = AppState::new();
        app.refresh(&three_rows()).unwrap();
        app.apply(Action::MoveUp); // wrap from 0 to last
        assert_eq!(app.selected_index(), Some(2));
        app.apply(Action::MoveDown); // wrap from last back to 0
        assert_eq!(app.selected_index(), Some(0));
        app.apply(Action::MoveBottom);
        assert_eq!(app.selected_index(), Some(2));
        app.apply(Action::MoveTop);
        assert_eq!(app.selected_index(), Some(0));
    }

    #[test]
    fn navigation_on_empty_rows_does_nothing() {
        let mut app = AppState::new();
        app.refresh(&StaticProvider(Vec::new())).unwrap();
        app.apply(Action::MoveDown);
        app.apply(Action::MoveUp);
        app.apply(Action::MoveTop);
        app.apply(Action::MoveBottom);
        assert_eq!(app.selected_index(), None);
    }

    #[test]
    fn quit_action_sets_quit_flag() {
        let mut app = AppState::new();
        app.apply(Action::Quit);
        assert!(app.quit_requested());
    }

    #[test]
    fn dismiss_closes_help_overlay_first_then_quits() {
        let mut app = AppState::new();
        app.apply(Action::ToggleHelp);
        assert!(app.help_visible());
        app.apply(Action::Dismiss);
        assert!(!app.help_visible());
        assert!(!app.quit_requested());
        app.apply(Action::Dismiss);
        assert!(app.quit_requested());
    }

    #[test]
    fn toggle_secret_visibility_round_trips() {
        let mut app = AppState::new();
        assert!(!app.secrets_visible());
        app.apply(Action::ToggleSecretVisibility);
        assert!(app.secrets_visible());
        app.apply(Action::ToggleSecretVisibility);
        assert!(!app.secrets_visible());
    }

    #[test]
    fn toasts_distinguish_info_and_error() {
        let mut app = AppState::new();
        app.set_info_toast("hello");
        assert_eq!(app.toast_text(), Some("hello"));
        assert!(!app.toast_is_error());
        app.set_error_toast("boom");
        assert_eq!(app.toast_text(), Some("boom"));
        assert!(app.toast_is_error());
    }

    #[test]
    fn toast_dismissed_on_next_interaction() {
        let mut app = AppState::new();
        app.refresh(&three_rows()).unwrap();
        app.set_info_toast("hi");
        app.apply(Action::MoveDown);
        assert!(app.toast_text().is_none());
    }

    #[test]
    fn noop_action_preserves_toast() {
        let mut app = AppState::new();
        app.set_info_toast("hi");
        app.apply(Action::Noop);
        assert_eq!(app.toast_text(), Some("hi"));
    }

    #[test]
    fn page_navigation_is_bounded() {
        let mut app = AppState::new();
        app.refresh(&three_rows()).unwrap();
        app.apply(Action::PageDown);
        // Only 3 rows; PageDown should pin at the last.
        assert_eq!(app.selected_index(), Some(2));
        app.apply(Action::PageUp);
        assert_eq!(app.selected_index(), Some(0));
    }
}
