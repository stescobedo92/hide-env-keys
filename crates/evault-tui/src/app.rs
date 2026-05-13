//! Deterministic state machine driving the dashboard.

use ratatui::widgets::TableState;

use crate::event::Action;
use crate::provider::{ProviderError, VarProvider, VarSummary};

/// In-session toast displayed at the bottom of the screen.
///
/// `Toast` is part of the *crate*-internal API. External callers
/// inspect toast state via [`AppState::toast_text`] /
/// [`AppState::toast_is_error`] and cannot construct or pattern-match
/// on the inner kind directly. Keeping these types private lets the
/// toast model evolve (e.g. severity levels, timeouts) without an API
/// break.
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Toast {
    pub(crate) text: String,
    pub(crate) kind: ToastKind,
}

/// Whether a toast represents a user-recoverable info message or an
/// error worth keeping on-screen until explicitly dismissed.
///
/// See [`Toast`] — crate-internal.
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastKind {
    /// Neutral, informational message. Auto-dismissed on the next
    /// non-`Noop` interaction so it does not pile up in front of the
    /// user.
    Info,
    /// Failure surfaced from the provider or another subsystem.
    /// Sticky: only dismissed by an explicit `Action::Dismiss` or
    /// `Action::Refresh` so the user has time to read the message
    /// even if they keep typing.
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
    /// populate the dashboard. Until then [`Self::selected_index`]
    /// returns `None` (rather than `Some(0)` pointing at non-existent
    /// row 0) so callers cannot accidentally index into an empty
    /// buffer.
    #[must_use]
    pub fn new() -> Self {
        // Selection begins at `None`. `clamp_selection` (run from
        // `refresh`) re-anchors to row 0 once rows are loaded.
        Self {
            rows: Vec::new(),
            table_state: TableState::default(),
            overlay: Overlay::None,
            toast: None,
            secrets_visible: false,
            quit: false,
        }
    }

    /// Re-read rows from `provider` and re-anchor the selection.
    ///
    /// On success the dashboard's row buffer is replaced. The cursor
    /// is clamped to `[0, rows.len())`: if the previously-selected
    /// index is still in range it survives; if the new row count is
    /// smaller the cursor pins to the last surviving row; if the
    /// dashboard is now empty the cursor is cleared to `None`.
    ///
    /// On failure the previous rows are preserved and the error is
    /// returned unchanged — the runtime decides whether to display it
    /// as a toast.
    ///
    /// Note: re-anchoring is *by index*, not by [`evault_core::model::VarId`].
    /// In phase 1 the dashboard is read-only, so external mutation
    /// during a refresh can silently shift the user's selection by one
    /// row. Phase 2 will track the selected `VarId` and re-anchor by
    /// identity once CRUD is wired.
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
    ///
    /// Toast lifecycle: *info* toasts auto-dismiss on any non-`Noop`
    /// interaction so they do not pile up; *error* toasts are sticky
    /// and only cleared by [`Action::Dismiss`] or [`Action::Refresh`]
    /// so a user typing fast cannot lose a failure notice they never
    /// had a chance to read.
    pub fn apply(&mut self, action: Action) {
        // Auto-dismiss INFO toasts on any non-Noop action *before*
        // dispatch so the current action can set its own toast which
        // will survive. Error toasts persist until explicitly handled.
        if !matches!(action, Action::Noop) && !self.toast_is_error() {
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
            Action::Refresh => {
                // The runtime owns the refresh side-effect (it owns
                // the provider). Here we just clear any toast so the
                // runtime's post-refresh toast — whether the success
                // confirmation or an error from the provider — is the
                // only message visible afterwards.
                self.toast = None;
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
            | Action::NextView => {
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

    fn dismiss(&mut self) {
        // Cascade: toast → overlay → quit. A sticky error toast must
        // be cleared first so users have a way to acknowledge it
        // without leaving the app.
        if self.toast.is_some() {
            self.toast = None;
            return;
        }
        if matches!(self.overlay, Overlay::Help) {
            self.overlay = Overlay::None;
            return;
        }
        self.quit = true;
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
    fn selection_is_none_before_first_refresh() {
        // Invariant: an `AppState` that has never been refreshed must
        // not advertise a selection (would point at non-existent row 0).
        let app = AppState::new();
        assert!(app.rows().is_empty());
        assert_eq!(app.selected_index(), None);
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
    fn info_toast_dismissed_on_next_interaction() {
        let mut app = AppState::new();
        app.refresh(&three_rows()).unwrap();
        app.set_info_toast("hi");
        app.apply(Action::MoveDown);
        assert!(app.toast_text().is_none());
    }

    /// Error toasts must survive navigation so a user typing fast
    /// (e.g. holding `j` to scroll) cannot lose a failure notice
    /// before reading it. Only explicit `Dismiss` / `Refresh` clears
    /// an error.
    #[test]
    fn error_toast_survives_navigation_and_help_toggle() {
        let mut app = AppState::new();
        app.refresh(&three_rows()).unwrap();
        app.set_error_toast("backend exploded");
        app.apply(Action::MoveDown);
        assert_eq!(app.toast_text(), Some("backend exploded"));
        app.apply(Action::ToggleHelp);
        assert_eq!(app.toast_text(), Some("backend exploded"));
        // Explicit dismiss clears it. Because the toast is present,
        // `Dismiss` consumes it instead of closing the help overlay,
        // so help stays visible.
        app.apply(Action::Dismiss);
        assert!(app.toast_text().is_none());
        assert!(app.help_visible());
    }

    /// `Action::Refresh` is *not* a stub: it MUST clear any toast so
    /// the runtime's post-refresh success/failure message is the only
    /// thing visible afterwards. Previously this action set a
    /// "not implemented" info toast that lingered on every successful
    /// runtime refresh.
    #[test]
    fn refresh_action_clears_pre_existing_toast() {
        let mut app = AppState::new();
        app.set_info_toast("stale info");
        app.apply(Action::Refresh);
        assert!(app.toast_text().is_none());

        app.set_error_toast("stale error");
        app.apply(Action::Refresh);
        assert!(
            app.toast_text().is_none(),
            "Refresh must also clear sticky error toasts"
        );
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
