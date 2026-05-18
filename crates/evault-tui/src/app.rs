//! Deterministic state machine driving the dashboard.

use evault_core::model::{AuditEntry, Group, VarId, VarKind};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::TableState;
use secrecy::SecretString;

use crate::event::Action;
use crate::filter::FilterState;
use crate::provider::{AuditProvider, ProviderError, VarDraft, VarProvider, VarSummary};

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

/// Top-level view currently displayed by the runtime.
///
/// Views are mutually exclusive: at any moment the dashboard is
/// either showing the table or the per-variable detail screen, not
/// both. Overlays (help, modals) layer on top of *either* view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Variable list — the default screen.
    Dashboard,
    /// Read-only inspection of the row that was selected when the
    /// user pressed Enter.
    Detail,
    /// Recent audit entries.
    Audit,
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
    filter: Option<FilterState>,
    view: View,
    audit_rows: Vec<AuditEntry>,
    active_profile: String,
    /// `Some(id)` while `view == Detail`. Tracking the inspected
    /// variable by id (rather than by selection index) keeps the
    /// Detail screen from silently re-pointing at a different row
    /// when an external mutation reshuffles the row buffer. Cleared
    /// on every return to the dashboard.
    detail_target: Option<VarId>,
    /// Modal confirmation request currently focused. When `Some`,
    /// [`Self::dispatch_key`] routes all keys to the confirm-modal
    /// handler instead of the normal Action / filter paths.
    confirm: Option<ConfirmRequest>,
    /// Editor form (modal popup) currently focused. When `Some`,
    /// [`Self::dispatch_key`] routes typed characters and editing
    /// keys to this form and bypasses the Action / filter / modal
    /// paths.
    form: Option<EditorForm>,
    /// Link-to-project form (modal popup) currently focused. Same
    /// focus-stealing semantics as `form` above.
    link_form: Option<LinkForm>,
    /// Run-in-project form (modal popup) currently focused. Captures
    /// the project path, the profile, and the command line to spawn.
    run_form: Option<RunForm>,
    /// Active-profile switch prompt currently focused.
    profile_form: Option<ProfileForm>,
    /// Read-only view-value modal currently focused. Shows a
    /// variable's decrypted value; closed by Esc.
    view_value: Option<ViewValueModal>,
    /// Error modal currently focused. Surfaces an action failure
    /// with a contextual hint. Dismissed by Esc / Enter.
    error_modal: Option<ErrorModal>,
}

/// Outcome of [`AppState::dispatch_key`]: signals whether the caller
/// (the runtime) should perform an I/O side effect after the state
/// has been updated.
#[derive(Debug, Clone)]
pub enum DispatchOutcome {
    /// Continue the event loop without further side effects.
    Continue,
    /// Re-fetch rows from the provider; the dispatch translated a
    /// `Refresh` intent. The runtime owns the provider and is
    /// responsible for the actual call.
    RefreshRequested,
    /// The user confirmed deletion of the variable identified by
    /// `id`; the runtime should call
    /// [`VarMutator::delete`](crate::VarMutator::delete) and then
    /// refresh. `name` is carried along for the post-delete success
    /// toast so the runtime does not have to look it up after the row
    /// is gone.
    DeleteRequested {
        /// Variable identifier the user confirmed deleting.
        id: VarId,
        /// Human-readable name, for use in the post-delete toast.
        name: String,
    },
    /// The user submitted the new-var prompt; the runtime should call
    /// [`crate::VarMutator::create`] and refresh on success.
    CreateRequested(VarDraft),
    /// The user submitted the edit-value prompt; the runtime should
    /// call [`crate::VarMutator::update_value`] and refresh on success.
    /// `name` is carried for the post-update toast.
    UpdateValueRequested {
        /// Variable to update.
        id: VarId,
        /// New value.
        value: SecretString,
        /// Human-readable name for the success toast.
        name: String,
    },
    /// The user submitted the link form; the runtime should call
    /// [`crate::VarMutator::link_to_project`] and refresh on success.
    LinkRequested {
        /// Variable being linked.
        id: VarId,
        /// Variable's display name (for the success toast).
        name: String,
        /// Project path the user typed.
        project_path: std::path::PathBuf,
        /// Profile name to use for the binding.
        profile: String,
        /// Whether to also materialize `.env` after linking.
        materialize: bool,
    },
    /// The user asked to view the value of a variable. The runtime
    /// should fetch via [`crate::VarProvider::get_value`] and then
    /// call [`AppState::show_value_modal`] with the result.
    ViewValueRequested {
        /// Variable id whose value to fetch.
        id: VarId,
        /// Display name for the modal title.
        name: String,
    },
    /// The user asked to copy a value. The runtime fetches the value, writes
    /// it to the OS clipboard, and records an audit entry without surfacing
    /// the secret in the UI state.
    CopyValueRequested {
        /// Variable id whose value to copy.
        id: VarId,
        /// Display name for the success toast.
        name: String,
    },
    /// The user submitted the run-in-project form. The runtime should
    /// restore the terminal, call
    /// [`crate::VarMutator::run_in_project`], and re-init the TUI
    /// afterwards.
    RunRequested {
        /// Project path to load the manifest from.
        project_path: std::path::PathBuf,
        /// Profile to resolve bindings under.
        profile: String,
        /// Program to spawn.
        program: String,
        /// Arguments forwarded to the program.
        args: Vec<String>,
    },
    /// The user submitted a new active profile for TUI defaults.
    ProfileSwitchRequested {
        /// Profile name.
        profile: String,
    },
}

/// Active editor form — modal popup for the `n` (new var) and `e`
/// (edit value) flows. Replaces the bottom-strip prompt with a
/// centered window that has multiple fields (name / group / kind /
/// value).
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone)]
pub(crate) struct EditorForm {
    /// What the form is collecting.
    pub(crate) mode: EditorMode,
    /// Variable name. Editable only in `NewVar` mode.
    pub(crate) name: String,
    /// Variable value (the secret material). Always editable.
    pub(crate) value: String,
    /// Index into [`GROUP_CYCLE`].
    pub(crate) group_idx: usize,
    /// Index into [`KIND_CYCLE`].
    pub(crate) kind_idx: usize,
    /// Currently-focused field — receives typing / arrow input.
    pub(crate) focus: FormField,
    /// Whether the value field should be rendered verbatim (true)
    /// or as `*` characters (false). Defaults to false for Secret
    /// kind, true for Plain.
    pub(crate) show_value: bool,
}

/// What the editor form is collecting.
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone)]
pub(crate) enum EditorMode {
    /// Creating a new variable. All four fields are editable.
    NewVar,
    /// Replacing the value of an existing variable. The name /
    /// group / kind are display-only; only the value field accepts
    /// input.
    EditValue {
        /// Target variable id (snapshotted at form-open time).
        id: VarId,
        /// Original variable name, shown read-only.
        original_name: String,
    },
}

/// Field currently focused inside [`EditorForm`].
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormField {
    Name,
    Group,
    Kind,
    Value,
}

/// Groups exposed by the TUI cycler. Custom groups remain available
/// via the CLI's `--group` flag.
#[allow(clippy::redundant_pub_crate)]
pub(crate) const GROUP_CYCLE: &[Group] = &[Group::User, Group::System, Group::Project];

/// Kinds exposed by the TUI cycler.
#[allow(clippy::redundant_pub_crate)]
pub(crate) const KIND_CYCLE: &[VarKind] = &[VarKind::Secret, VarKind::Plain];

/// Link-to-project form — modal popup for the `l` flow.
///
/// Captures the project path, the profile name, and whether to
/// materialise the project's `.env` immediately after linking.
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone)]
pub(crate) struct LinkForm {
    /// Variable being linked (id snapshotted at form-open time).
    pub(crate) var_id: VarId,
    /// Display name for the form title + success toast.
    pub(crate) var_name: String,
    /// Filesystem path the user has typed.
    pub(crate) path: String,
    /// Profile name (defaults to `default`).
    pub(crate) profile: String,
    /// Whether to materialise `.env` right after linking.
    pub(crate) materialize: bool,
    /// Field currently focused.
    pub(crate) focus: LinkField,
}

/// Field currently focused inside [`LinkForm`].
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkField {
    Path,
    Profile,
    Materialize,
}

/// Run-in-project form — modal popup for the `R` flow.
///
/// Captures the project path, the profile name, and the command line
/// (program + args) to spawn with the project's resolved environment
/// overlay injected.
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone)]
pub(crate) struct RunForm {
    /// Filesystem path the user has typed.
    pub(crate) path: String,
    /// Profile name (defaults to `default`).
    pub(crate) profile: String,
    /// Raw command line as typed by the user.
    ///
    /// Tokenised by whitespace at submit time. Quoted arguments are
    /// NOT supported — users with complex shell quoting needs should
    /// fall back to the `evault run` CLI command.
    pub(crate) command: String,
    /// Field currently focused.
    pub(crate) focus: RunField,
}

/// Active profile prompt — modal popup for the `p` flow.
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone)]
pub(crate) struct ProfileForm {
    /// Profile name to use as the default for future link/run forms.
    pub(crate) profile: String,
}

/// Field currently focused inside [`RunForm`].
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunField {
    Path,
    Profile,
    Command,
}

/// Error modal — focused popup that surfaces an action failure
/// (failed create / edit / delete / link) with an explanatory hint.
///
/// Replaces a sticky error toast for cases where the user needs to
/// actively acknowledge the failure (the toast is too easy to miss
/// when an action they just initiated fails).
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone)]
pub(crate) struct ErrorModal {
    /// Short title, e.g. `"create failed"` or `"link failed"`.
    pub(crate) title: String,
    /// The raw error message from the backend.
    pub(crate) message: String,
    /// Optional contextual hint explaining the failure and how to
    /// fix it (e.g. naming rules when the create rejected the name).
    pub(crate) hint: Option<String>,
}

/// View-value modal — popup showing a variable's decrypted value
/// after the user pressed `v`. The runtime fetches the value and
/// calls [`AppState::show_value_modal`] which inserts this struct.
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug)]
pub(crate) struct ViewValueModal {
    /// Variable name for the modal title.
    pub(crate) name: String,
    /// The decrypted value. Held in `SecretString` so it gets
    /// zeroized on drop.
    pub(crate) value: SecretString,
    /// Whether the value is currently rendered verbatim (`true`)
    /// or as `*` characters. Toggle with `Ctrl+S` while open.
    pub(crate) show: bool,
}

/// Modal confirmation request — internal state for the y/n overlay.
///
/// Crate-private: external callers don't construct these; they are
/// raised by [`AppState`] in response to user input and rendered by
/// the views layer.
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfirmRequest {
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) action: PendingAction,
}

/// Action to perform when a [`ConfirmRequest`] is accepted.
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PendingAction {
    /// Delete the named variable; the runtime resolves via
    /// [`VarMutator::delete`](crate::VarMutator::delete).
    DeleteVar { id: VarId, name: String },
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
            filter: None,
            view: View::Dashboard,
            audit_rows: Vec::new(),
            active_profile: "default".to_owned(),
            detail_target: None,
            confirm: None,
            form: None,
            link_form: None,
            run_form: None,
            profile_form: None,
            view_value: None,
            error_modal: None,
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
        self.rebuild_filter();
        self.clamp_selection();
        // Detail-target validation. If the user is inspecting a
        // variable that just disappeared (external delete, profile
        // switch, etc.) auto-return to the dashboard with a loud
        // error toast. Without this check the Detail pane would
        // silently re-anchor by index and start showing a
        // *different* variable under the same screen header — the
        // exact silent-failure mode the audit charter forbids.
        if matches!(self.view, View::Detail) && !self.detail_target_is_present() {
            self.view = View::Dashboard;
            self.detail_target = None;
            self.set_error_toast("variable removed elsewhere \u{2014} returned to dashboard");
        }
        Ok(())
    }

    /// Re-read recent audit entries for the audit view.
    ///
    /// # Errors
    /// Propagates whatever [`ProviderError`] the audit provider returns.
    pub fn refresh_audit<P: AuditProvider + ?Sized>(
        &mut self,
        provider: &P,
    ) -> Result<(), ProviderError> {
        self.audit_rows = provider.recent_audit(100)?;
        Ok(())
    }

    fn detail_target_is_present(&self) -> bool {
        let Some(target) = self.detail_target else {
            return false;
        };
        self.rows.iter().any(|v| v.id == target)
    }

    /// Dispatch a raw key event.
    ///
    /// When the filter input is active, characters and Backspace edit
    /// the needle, Enter accepts (filter stays applied but the input
    /// is closed), and Esc cancels the filter entirely. Navigation
    /// keys (Up / Down / `PageUp` / `PageDown`) and Ctrl-C remain bound
    /// so the user can scroll through results and quit even while
    /// typing.
    ///
    /// When the filter input is **not** active, the key is translated
    /// via [`Action::from_key`] and dispatched to [`Self::apply`].
    ///
    /// Returns [`DispatchOutcome::RefreshRequested`] when the user
    /// pressed `r` (or otherwise triggered `Action::Refresh`); the
    /// runtime is responsible for the actual provider call.
    pub fn dispatch_key(&mut self, key: KeyEvent) -> DispatchOutcome {
        if key.kind != KeyEventKind::Press {
            return DispatchOutcome::Continue;
        }
        // Error modal: takes priority over everything else so the
        // user has to acknowledge an action failure before continuing.
        if self.error_modal.is_some() {
            return self.dispatch_error_modal_key(key);
        }
        // Modal confirm steals focus from everything else: when the
        // user is being asked "are you sure?", any other action would
        // be ambiguous.
        if self.confirm.is_some() {
            return self.dispatch_confirm_key(key);
        }
        // View-value modal: read-only popup, only Esc / Ctrl+S / Ctrl+C
        // make sense while it's focused.
        if self.view_value.is_some() {
            return self.dispatch_view_value_key(key);
        }
        // Link form: modal popup capturing path + profile + materialize.
        if self.link_form.is_some() {
            return self.dispatch_link_form_key(key);
        }
        // Run form: modal popup capturing path + profile + command line.
        if self.run_form.is_some() {
            return self.dispatch_run_form_key(key);
        }
        // Profile form: modal popup capturing the active profile.
        if self.profile_form.is_some() {
            return self.dispatch_profile_form_key(key);
        }
        // Editor form: typed characters go to its fields.
        if self.form.is_some() {
            return self.dispatch_form_key(key);
        }
        if self.is_filter_input_active() {
            return self.dispatch_filter_input_key(key);
        }
        let action = Action::from_key(key);
        // `ViewValue` is special: it needs to emit a runtime request
        // carrying the selected row's id, which `apply` cannot
        // express. We intercept it here.
        if matches!(action, Action::ViewValue) {
            if let Some((id, name)) = self.request_view_value() {
                return DispatchOutcome::ViewValueRequested { id, name };
            }
            return DispatchOutcome::Continue;
        }
        if matches!(action, Action::CopyValue) {
            if let Some((id, name)) = self.request_view_value() {
                return DispatchOutcome::CopyValueRequested { id, name };
            }
            return DispatchOutcome::Continue;
        }
        self.apply(action);
        if matches!(action, Action::Refresh) {
            DispatchOutcome::RefreshRequested
        } else {
            DispatchOutcome::Continue
        }
    }

    /// Handle a key while the editor form modal is focused.
    ///
    /// - `Tab` / `Shift+Tab` — cycle focus across the four fields.
    /// - `Enter` — submit (validates; on failure leaves the form
    ///   open and surfaces a sticky error toast).
    /// - `Esc` — cancel.
    /// - `Ctrl+C` — quit (escape hatch).
    /// - On `Name` / `Value` focus: typed characters append, Backspace
    ///   pops.
    /// - On `Group` / `Kind` focus: Left / Right arrows + Space cycle
    ///   the option.
    /// - `s` on `Value` focus toggles secret-value masking (kept as
    ///   typed but rendered with `*`).
    fn dispatch_form_key(&mut self, key: KeyEvent) -> DispatchOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if matches!(key.code, KeyCode::Char('c')) && ctrl {
            self.quit = true;
            return DispatchOutcome::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                self.form = None;
                DispatchOutcome::Continue
            }
            KeyCode::Enter => self.submit_form(),
            KeyCode::Tab => {
                if let Some(form) = self.form.as_mut() {
                    form.focus = next_focus(form.focus, &form.mode);
                }
                DispatchOutcome::Continue
            }
            KeyCode::BackTab => {
                if let Some(form) = self.form.as_mut() {
                    form.focus = prev_focus(form.focus, &form.mode);
                }
                DispatchOutcome::Continue
            }
            _ => {
                if let Some(form) = self.form.as_mut() {
                    handle_field_key(form, key);
                }
                DispatchOutcome::Continue
            }
        }
    }

    /// Validate the editor form's current contents and emit the
    /// matching outcome. On validation failure, surfaces a sticky
    /// error toast and leaves the form open with the entered data
    /// preserved.
    fn submit_form(&mut self) -> DispatchOutcome {
        let Some(form) = self.form.take() else {
            return DispatchOutcome::Continue;
        };
        // Reach into the cycle slices to recover the typed options.
        // Indices are bounded at construction + key handling, so
        // out-of-range here is unreachable; we clamp defensively.
        let group = GROUP_CYCLE
            .get(form.group_idx.min(GROUP_CYCLE.len() - 1))
            .cloned()
            .unwrap_or(Group::User);
        let kind = *KIND_CYCLE
            .get(form.kind_idx.min(KIND_CYCLE.len() - 1))
            .unwrap_or(&VarKind::Secret);

        match form.mode.clone() {
            EditorMode::NewVar => {
                if form.name.trim().is_empty() {
                    self.set_error_toast("name must be non-empty (Esc to cancel)");
                    self.form = Some(EditorForm {
                        focus: FormField::Name,
                        ..form
                    });
                    return DispatchOutcome::Continue;
                }
                if form.value.is_empty() {
                    self.set_error_toast("value must be non-empty (Esc to cancel)");
                    self.form = Some(EditorForm {
                        focus: FormField::Value,
                        ..form
                    });
                    return DispatchOutcome::Continue;
                }
                DispatchOutcome::CreateRequested(VarDraft {
                    name: form.name.trim().to_owned(),
                    group,
                    kind,
                    value: SecretString::new(form.value.into()),
                })
            }
            EditorMode::EditValue { id, original_name } => {
                if form.value.is_empty() {
                    self.set_error_toast("value must be non-empty (Esc to cancel)");
                    self.form = Some(EditorForm {
                        focus: FormField::Value,
                        ..form
                    });
                    return DispatchOutcome::Continue;
                }
                DispatchOutcome::UpdateValueRequested {
                    id,
                    value: SecretString::new(form.value.into()),
                    name: original_name,
                }
            }
        }
    }

    /// Handle a key while the view-value modal is focused.
    /// `Esc` closes; `Ctrl+S` toggles masking; `Ctrl+C` quits.
    fn dispatch_view_value_key(&mut self, key: KeyEvent) -> DispatchOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if matches!(key.code, KeyCode::Char('c')) && ctrl {
            self.quit = true;
            return DispatchOutcome::Continue;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.view_value = None;
            }
            KeyCode::Char('s') if ctrl => {
                if let Some(modal) = self.view_value.as_mut() {
                    modal.show = !modal.show;
                }
            }
            _ => {}
        }
        DispatchOutcome::Continue
    }

    /// Handle a key while the link form modal is focused.
    fn dispatch_link_form_key(&mut self, key: KeyEvent) -> DispatchOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if matches!(key.code, KeyCode::Char('c')) && ctrl {
            self.quit = true;
            return DispatchOutcome::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                self.link_form = None;
                DispatchOutcome::Continue
            }
            KeyCode::Enter => self.submit_link_form(),
            KeyCode::Tab => {
                if let Some(form) = self.link_form.as_mut() {
                    form.focus = match form.focus {
                        LinkField::Path => LinkField::Profile,
                        LinkField::Profile => LinkField::Materialize,
                        LinkField::Materialize => LinkField::Path,
                    };
                }
                DispatchOutcome::Continue
            }
            KeyCode::BackTab => {
                if let Some(form) = self.link_form.as_mut() {
                    form.focus = match form.focus {
                        LinkField::Path => LinkField::Materialize,
                        LinkField::Profile => LinkField::Path,
                        LinkField::Materialize => LinkField::Profile,
                    };
                }
                DispatchOutcome::Continue
            }
            _ => {
                if let Some(form) = self.link_form.as_mut() {
                    handle_link_field_key(form, key);
                }
                DispatchOutcome::Continue
            }
        }
    }

    fn submit_link_form(&mut self) -> DispatchOutcome {
        let Some(form) = self.link_form.take() else {
            return DispatchOutcome::Continue;
        };
        let path = form.path.trim();
        if path.is_empty() {
            self.set_error_toast("project path must be non-empty (Esc to cancel)");
            self.link_form = Some(LinkForm {
                focus: LinkField::Path,
                ..form
            });
            return DispatchOutcome::Continue;
        }
        let profile = if form.profile.trim().is_empty() {
            "default".to_owned()
        } else {
            form.profile.trim().to_owned()
        };
        DispatchOutcome::LinkRequested {
            id: form.var_id,
            name: form.var_name,
            project_path: std::path::PathBuf::from(path),
            profile,
            materialize: form.materialize,
        }
    }

    /// Handle a key while the run-in-project form modal is focused.
    fn dispatch_run_form_key(&mut self, key: KeyEvent) -> DispatchOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if matches!(key.code, KeyCode::Char('c')) && ctrl {
            self.quit = true;
            return DispatchOutcome::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                self.run_form = None;
                DispatchOutcome::Continue
            }
            KeyCode::Enter => self.submit_run_form(),
            KeyCode::Tab => {
                if let Some(form) = self.run_form.as_mut() {
                    form.focus = match form.focus {
                        RunField::Path => RunField::Profile,
                        RunField::Profile => RunField::Command,
                        RunField::Command => RunField::Path,
                    };
                }
                DispatchOutcome::Continue
            }
            KeyCode::BackTab => {
                if let Some(form) = self.run_form.as_mut() {
                    form.focus = match form.focus {
                        RunField::Path => RunField::Command,
                        RunField::Profile => RunField::Path,
                        RunField::Command => RunField::Profile,
                    };
                }
                DispatchOutcome::Continue
            }
            _ => {
                if let Some(form) = self.run_form.as_mut() {
                    handle_run_field_key(form, key);
                }
                DispatchOutcome::Continue
            }
        }
    }

    /// Validate the form's contents and emit a [`DispatchOutcome::RunRequested`]
    /// or — if validation fails — re-open the form with an info toast.
    fn submit_run_form(&mut self) -> DispatchOutcome {
        let Some(form) = self.run_form.take() else {
            return DispatchOutcome::Continue;
        };
        let path = form.path.trim();
        if path.is_empty() {
            self.set_error_toast("project path must be non-empty (Esc to cancel)");
            self.run_form = Some(RunForm {
                focus: RunField::Path,
                ..form
            });
            return DispatchOutcome::Continue;
        }
        let command_trim = form.command.trim();
        if command_trim.is_empty() {
            self.set_error_toast("command line must be non-empty (Esc to cancel)");
            self.run_form = Some(RunForm {
                focus: RunField::Command,
                ..form
            });
            return DispatchOutcome::Continue;
        }
        let mut tokens = command_trim.split_whitespace();
        // `command_trim` is non-empty, so the first token exists.
        let program = tokens.next().unwrap_or("").to_owned();
        let args: Vec<String> = tokens.map(str::to_owned).collect();
        let profile = if form.profile.trim().is_empty() {
            "default".to_owned()
        } else {
            form.profile.trim().to_owned()
        };
        DispatchOutcome::RunRequested {
            project_path: std::path::PathBuf::from(path),
            profile,
            program,
            args,
        }
    }

    /// Open the run-in-project form modal. Unlike the link form, the
    /// run form is per-project rather than per-var, so no row needs to
    /// be selected — the user types the project path explicitly.
    fn open_run_form(&mut self) {
        self.run_form = Some(RunForm {
            path: String::new(),
            profile: self.active_profile.clone(),
            command: String::new(),
            focus: RunField::Path,
        });
    }

    /// Whether the run-in-project form modal is currently focused.
    #[must_use]
    pub const fn is_run_form_visible(&self) -> bool {
        self.run_form.is_some()
    }

    /// Read-only access to the focused run-form (for the views layer).
    pub(crate) const fn current_run_form(&self) -> Option<&RunForm> {
        self.run_form.as_ref()
    }

    fn dispatch_profile_form_key(&mut self, key: KeyEvent) -> DispatchOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if matches!(key.code, KeyCode::Char('c')) && ctrl {
            self.quit = true;
            return DispatchOutcome::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                self.profile_form = None;
                DispatchOutcome::Continue
            }
            KeyCode::Enter => self.submit_profile_form(),
            KeyCode::Backspace => {
                if let Some(form) = self.profile_form.as_mut() {
                    form.profile.pop();
                }
                DispatchOutcome::Continue
            }
            KeyCode::Char(c) if is_text_input(key) => {
                if let Some(form) = self.profile_form.as_mut() {
                    form.profile.push(c);
                }
                DispatchOutcome::Continue
            }
            _ => DispatchOutcome::Continue,
        }
    }

    fn submit_profile_form(&mut self) -> DispatchOutcome {
        let Some(form) = self.profile_form.take() else {
            return DispatchOutcome::Continue;
        };
        let profile = form.profile.trim();
        if profile.is_empty() {
            self.set_error_toast("profile must be non-empty (Esc to cancel)");
            self.profile_form = Some(form);
            return DispatchOutcome::Continue;
        }
        DispatchOutcome::ProfileSwitchRequested {
            profile: profile.to_owned(),
        }
    }

    fn open_profile_form(&mut self) {
        self.profile_form = Some(ProfileForm {
            profile: self.active_profile.clone(),
        });
    }

    /// Whether the active-profile modal is currently focused.
    #[must_use]
    pub const fn is_profile_form_visible(&self) -> bool {
        self.profile_form.is_some()
    }

    /// Read-only access to the focused profile form (for the views layer).
    pub(crate) const fn current_profile_form(&self) -> Option<&ProfileForm> {
        self.profile_form.as_ref()
    }

    /// Set the active TUI profile after the runtime validates the request.
    pub fn set_active_profile(&mut self, profile: impl Into<String>) {
        self.active_profile = profile.into();
    }

    /// Open the link-form modal for the currently-targeted variable.
    /// No-op (with info toast) if there is no row selected.
    fn open_link_form(&mut self) {
        let target = match self.view {
            View::Dashboard => self.selected_row(),
            View::Detail => self.detail_row(),
            View::Audit => None,
        };
        let Some(var) = target else {
            if !self.toast_is_error() {
                self.set_info_toast("no row selected");
            }
            return;
        };
        self.link_form = Some(LinkForm {
            var_id: var.id,
            var_name: var.name.clone(),
            path: String::new(),
            profile: self.active_profile.clone(),
            materialize: false,
            focus: LinkField::Path,
        });
    }

    /// Trigger a value-view request for the currently-targeted row.
    /// Returns the outcome the runtime should act on; called from
    /// `apply(Action::ViewValue)` via the special path.
    fn request_view_value(&mut self) -> Option<(VarId, String)> {
        let target = match self.view {
            View::Dashboard => self.selected_row(),
            View::Detail => self.detail_row(),
            View::Audit => None,
        };
        let Some(var) = target else {
            if !self.toast_is_error() {
                self.set_info_toast("no row selected");
            }
            return None;
        };
        Some((var.id, var.name.clone()))
    }

    /// Show the value modal. Runtime calls this after fetching the
    /// secret material via [`crate::VarProvider::get_value`].
    pub fn show_value_modal(&mut self, name: String, value: SecretString) {
        self.view_value = Some(ViewValueModal {
            name,
            value,
            show: false,
        });
    }

    /// Raise a focused error modal so the user has to acknowledge
    /// an action failure before continuing. Preferred over
    /// [`Self::set_error_toast`] when the user just initiated the
    /// failing action (create / edit / delete / link) — the toast
    /// is too easy to miss.
    pub fn show_error_modal(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        hint: Option<String>,
    ) {
        self.error_modal = Some(ErrorModal {
            title: title.into(),
            message: message.into(),
            hint,
        });
    }

    /// Whether the error modal is currently focused.
    #[must_use]
    pub const fn is_error_modal_visible(&self) -> bool {
        self.error_modal.is_some()
    }

    /// Read-only access to the focused error modal (for the views layer).
    pub(crate) const fn current_error_modal(&self) -> Option<&ErrorModal> {
        self.error_modal.as_ref()
    }

    /// Handle a key while the error modal is focused. Only `Esc`,
    /// `Enter`, and `Ctrl+C` are recognised; the modal swallows
    /// everything else so the user can't accidentally trigger a new
    /// action while still reading the error.
    fn dispatch_error_modal_key(&mut self, key: KeyEvent) -> DispatchOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if matches!(key.code, KeyCode::Char('c')) && ctrl {
            self.quit = true;
            return DispatchOutcome::Continue;
        }
        if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ')) {
            self.error_modal = None;
        }
        DispatchOutcome::Continue
    }

    /// Whether the link form modal is currently focused.
    #[must_use]
    pub const fn is_link_form_visible(&self) -> bool {
        self.link_form.is_some()
    }

    /// Read-only access to the link form for the views layer.
    pub(crate) const fn current_link_form(&self) -> Option<&LinkForm> {
        self.link_form.as_ref()
    }

    /// Whether the view-value modal is currently focused.
    #[must_use]
    pub const fn is_view_value_visible(&self) -> bool {
        self.view_value.is_some()
    }

    /// Read-only access to the view-value modal for the views layer.
    pub(crate) const fn current_view_value(&self) -> Option<&ViewValueModal> {
        self.view_value.as_ref()
    }

    /// Handle a key while a confirmation modal is focused.
    ///
    /// Recognised keys:
    /// - `y` / `Y` / `Enter` — accept; consume the pending action
    ///   and surface it as a [`DispatchOutcome`] for the runtime.
    /// - `n` / `N` / `Esc` — cancel; clear the modal.
    /// - `Ctrl+C` — quit (overrides the modal so the user can always
    ///   escape).
    /// - Any other key is ignored and the modal stays focused.
    fn dispatch_confirm_key(&mut self, key: KeyEvent) -> DispatchOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if matches!(key.code, KeyCode::Char('c')) && ctrl {
            self.quit = true;
            return DispatchOutcome::Continue;
        }
        let accept = matches!(key.code, KeyCode::Char('y' | 'Y') | KeyCode::Enter);
        let reject = matches!(key.code, KeyCode::Char('n' | 'N') | KeyCode::Esc);
        if !accept && !reject {
            return DispatchOutcome::Continue;
        }
        let Some(req) = self.confirm.take() else {
            return DispatchOutcome::Continue;
        };
        if reject {
            // Cascade the dismissal: if the user opened help and then
            // raised a modal, an Esc press should back them out of
            // *both* layers in one go rather than leaving help still
            // visible. The user opened help BEFORE the modal (modals
            // steal focus so `?` cannot reach the Action path), so
            // closing it here matches the "Esc means back to the
            // dashboard root" invariant.
            if matches!(self.overlay, Overlay::Help) {
                self.overlay = Overlay::None;
            }
            return DispatchOutcome::Continue;
        }
        match req.action {
            PendingAction::DeleteVar { id, name } => DispatchOutcome::DeleteRequested { id, name },
        }
    }

    fn dispatch_filter_input_key(&mut self, key: KeyEvent) -> DispatchOutcome {
        // Mirror `apply()`'s contract: any active interaction in the
        // filter input clears a stale info toast so the user never
        // sees a "refreshed (5 vars)" sitting on screen while they
        // type a needle that contradicts it. Error toasts remain
        // sticky and survive — same policy as the Action path.
        if !self.toast_is_error() {
            self.toast = None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            // Universal exit gesture remains bound.
            KeyCode::Char('c') if ctrl => {
                self.quit = true;
            }
            // Cancel — clear the filter, restore full row set.
            KeyCode::Esc => self.close_filter(),
            // Accept — keep filter applied, hide the input box.
            KeyCode::Enter => {
                if let Some(filter) = self.filter.as_mut() {
                    filter.commit();
                }
            }
            // Edit the needle.
            KeyCode::Backspace => self.filter_pop(),
            KeyCode::Char(c) if !ctrl => self.filter_push(c),
            // Navigation through filtered results — arrow / page keys
            // only; j/k are valid needle characters and must not steal
            // the keystroke while the input is active.
            KeyCode::Up => self.select_prev(),
            KeyCode::Down => self.select_next(),
            KeyCode::PageUp => self.page(false),
            KeyCode::PageDown => self.page(true),
            _ => {}
        }
        DispatchOutcome::Continue
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
            Action::StartFuzzy => self.open_filter(),
            Action::OpenDetail => self.open_detail(),
            Action::DeleteVar => self.request_delete_confirmation(),
            Action::NewVar => self.open_new_var_prompt(),
            Action::EditVar => self.open_edit_value_prompt(),
            Action::LinkVar => self.open_link_form(),
            Action::RunInProject => self.open_run_form(),
            Action::SwitchProfile => self.open_profile_form(),
            Action::NextView => self.next_view(),
            // `ViewValue` is handled in `dispatch_key` directly
            // because its outcome needs to leave the apply path. We
            // accept it here as a no-op for tests that bypass
            // `dispatch_key`. Same for `Noop`.
            Action::ViewValue | Action::CopyValue | Action::Noop => {}
        }
    }

    /// Open the fuzzy filter overlay. If a filter is already active
    /// the input box is re-opened so the user can keep typing without
    /// losing the existing needle.
    /// Open the fuzzy filter overlay.
    ///
    /// If a filter is already applied, the input box is re-opened so
    /// the user can continue editing the existing needle. The cursor
    /// position is **preserved** in both first-open and re-open paths
    /// so the user's mental "what is selected" pointer survives a
    /// roundtrip through the input.
    pub fn open_filter(&mut self) {
        if let Some(filter) = self.filter.as_mut() {
            filter.reopen_input();
            return;
        }
        self.filter = Some(FilterState::new(self.rows.len()));
        // The fresh `FilterState` already matches all rows in original
        // order, and the visible row count is unchanged, so the
        // existing selection index remains valid. Clamp defensively
        // in case the dashboard was empty.
        self.clamp_selection();
    }

    /// Clear the filter and restore the full row set.
    pub fn close_filter(&mut self) {
        self.filter = None;
        self.clamp_selection();
    }

    /// Switch to [`View::Detail`] for the currently-selected row.
    ///
    /// The selected row's `VarId` is snapshotted into
    /// [`detail_target`](Self::detail_row) so the Detail screen looks
    /// up its data by identity rather than index — this prevents the
    /// pane from silently re-pointing at a different row when a
    /// concurrent refresh reshuffles the row buffer.
    ///
    /// If no row is selected, the call is a no-op aside from a brief
    /// `"no row selected"` info toast. A pre-existing error toast is
    /// **never** clobbered (errors are sticky for a reason; an
    /// accidental Enter must not erase a refresh-failure notice the
    /// user has not yet read).
    pub fn open_detail(&mut self) {
        if let Some(var) = self.selected_row() {
            self.detail_target = Some(var.id);
            self.view = View::Detail;
            return;
        }
        // Preserve sticky error toasts; only show the info hint when
        // the toast slot is empty or already an info toast (which
        // `apply` already cleared before dispatch in the normal path).
        if !self.toast_is_error() {
            self.set_info_toast("no row selected");
        }
    }

    /// Return to the dashboard from any other view. Clears the
    /// Detail target so a subsequent re-entry re-snapshots fresh.
    pub const fn return_to_dashboard(&mut self) {
        self.view = View::Dashboard;
        self.detail_target = None;
    }

    const fn next_view(&mut self) {
        match self.view {
            View::Dashboard | View::Detail => {
                self.view = View::Audit;
                self.detail_target = None;
            }
            View::Audit => {
                self.view = View::Dashboard;
                self.detail_target = None;
            }
        }
    }

    /// Splice the given variable id out of the row buffer locally
    /// without going through the provider.
    ///
    /// Used by the runtime when a `delete` succeeded but the
    /// subsequent `refresh` failed: keeping the deleted row visible
    /// would let the user press `d` a second time on a ghost entry,
    /// producing a confusing `NotFound` error or a "deleted twice"
    /// success. Splicing locally restores the user's mental model.
    ///
    /// No-op if the id is not in the buffer. Re-ranks any active
    /// filter and clamps the selection cursor.
    pub fn splice_out_row(&mut self, id: VarId) {
        let before = self.rows.len();
        self.rows.retain(|v| v.id != id);
        if self.rows.len() == before {
            return;
        }
        self.rebuild_filter();
        self.clamp_selection();
        // If the user was inspecting the just-spliced variable,
        // return to the dashboard so the detail target does not
        // dangle.
        if self.detail_target == Some(id) {
            self.return_to_dashboard();
        }
    }

    /// Open the editor form modal for creating a new variable.
    /// Fields default to: empty name + empty value, group=user,
    /// kind=secret (matches the most common case).
    fn open_new_var_prompt(&mut self) {
        self.form = Some(EditorForm {
            mode: EditorMode::NewVar,
            name: String::new(),
            value: String::new(),
            group_idx: 0,
            kind_idx: 0,
            focus: FormField::Name,
            show_value: false,
        });
    }

    /// Open the editor form modal for editing the value of the
    /// currently-targeted variable. The name / group / kind fields
    /// are displayed but read-only; only the value is editable.
    fn open_edit_value_prompt(&mut self) {
        let target = match self.view {
            View::Dashboard => self.selected_row(),
            View::Detail => self.detail_row(),
            View::Audit => None,
        };
        let Some(var) = target else {
            if !self.toast_is_error() {
                self.set_info_toast("no row selected");
            }
            return;
        };
        let group_idx = GROUP_CYCLE
            .iter()
            .position(|g| g == &var.group)
            .unwrap_or(0);
        let kind_idx = KIND_CYCLE.iter().position(|k| *k == var.kind).unwrap_or(0);
        self.form = Some(EditorForm {
            mode: EditorMode::EditValue {
                id: var.id,
                original_name: var.name.clone(),
            },
            name: var.name.clone(),
            value: String::new(),
            group_idx,
            kind_idx,
            focus: FormField::Value,
            show_value: !matches!(var.kind, VarKind::Secret),
        });
    }

    /// Whether the editor form modal is currently focused.
    #[must_use]
    pub const fn is_form_visible(&self) -> bool {
        self.form.is_some()
    }

    /// Read-only access to the focused editor form (for the views layer).
    pub(crate) const fn current_form(&self) -> Option<&EditorForm> {
        self.form.as_ref()
    }

    /// Raise a confirmation modal for deleting the currently-targeted
    /// variable (the selected row on the Dashboard, or
    /// [`Self::detail_row`] when on Detail). Surfaces an info toast
    /// instead if there is no target — without clobbering a sticky
    /// error toast.
    fn request_delete_confirmation(&mut self) {
        debug_assert!(
            self.confirm.is_none(),
            "request_delete_confirmation called with a focused modal — \
             dispatch_key routes confirm-mode keys to dispatch_confirm_key \
             before any Action::DeleteVar can reach here"
        );
        let target = match self.view {
            View::Dashboard => self.selected_row(),
            View::Detail => self.detail_row(),
            View::Audit => None,
        };
        let Some(var) = target else {
            if !self.toast_is_error() {
                self.set_info_toast("no row selected");
            }
            return;
        };
        let kind = match var.kind {
            evault_core::model::VarKind::Secret => "secret",
            evault_core::model::VarKind::Plain => "plain",
        };
        self.confirm = Some(ConfirmRequest {
            title: "delete variable".to_owned(),
            body: format!("Delete `{}` ({kind})?\nThis cannot be undone.", var.name),
            action: PendingAction::DeleteVar {
                id: var.id,
                name: var.name.clone(),
            },
        });
    }

    /// Whether a confirmation modal is currently focused.
    #[must_use]
    pub const fn is_confirm_visible(&self) -> bool {
        self.confirm.is_some()
    }

    /// Read-only access to the focused confirm request (for the
    /// views layer). Crate-private so external callers stay on the
    /// observation-only API ([`Self::is_confirm_visible`] et al.).
    pub(crate) const fn current_confirm(&self) -> Option<&ConfirmRequest> {
        self.confirm.as_ref()
    }

    /// Programmatic dismissal of a focused modal. Returns `true` if a
    /// modal was actually cleared. Used by the runtime to flush state
    /// after the user-initiated delete it triggered has completed.
    pub fn dismiss_confirm(&mut self) -> bool {
        let was_set = self.confirm.is_some();
        self.confirm = None;
        was_set
    }

    /// The variable currently displayed by the Detail view, looked
    /// up by identity rather than by selection index.
    ///
    /// Returns `None` when no Detail view is active, or when the
    /// inspected variable has been removed from the row buffer
    /// between Detail entry and the current frame.
    #[must_use]
    pub fn detail_row(&self) -> Option<&VarSummary> {
        let id = self.detail_target?;
        self.rows.iter().find(|v| v.id == id)
    }

    fn filter_push(&mut self, c: char) {
        let haystacks: Vec<&str> = self.rows.iter().map(|v| v.name.as_str()).collect();
        if let Some(filter) = self.filter.as_mut() {
            filter.push(c, &haystacks);
        }
        self.clamp_selection();
    }

    fn filter_pop(&mut self) {
        let haystacks: Vec<&str> = self.rows.iter().map(|v| v.name.as_str()).collect();
        if let Some(filter) = self.filter.as_mut() {
            filter.pop(&haystacks);
        }
        self.clamp_selection();
    }

    /// Re-rank the existing filter against the (possibly changed) row
    /// buffer. Called from [`Self::refresh`]; no-op when no filter is
    /// active.
    ///
    /// `FilterState::rerank` is pure in `(needle, haystacks)`, so
    /// replaying the needle char-by-char against the new haystacks
    /// produces the same ranking that live typing would. We rebuild
    /// rather than expose a public rerank to keep `FilterState`'s
    /// surface minimal.
    fn rebuild_filter(&mut self) {
        let Some(filter) = self.filter.as_mut() else {
            return;
        };
        let needle = filter.needle().to_owned();
        let input_active = filter.input_active();
        let haystacks: Vec<&str> = self.rows.iter().map(|v| v.name.as_str()).collect();
        let mut fresh = FilterState::new(self.rows.len());
        for c in needle.chars() {
            fresh.push(c, &haystacks);
        }
        if !input_active {
            fresh.commit();
        }
        *filter = fresh;
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

    /// Read-only access to the full row buffer (filter-independent).
    /// Use [`Self::visible_row_indices`] / [`Self::visible_rows`] when
    /// you need the rows currently rendered by the dashboard.
    #[must_use]
    pub fn rows(&self) -> &[VarSummary] {
        &self.rows
    }

    /// Indices into [`Self::rows`] of the rows currently rendered.
    ///
    /// When a filter is applied the indices are in match-score order
    /// (best score first). Without a filter they are simply
    /// `0..rows.len()`.
    #[must_use]
    pub fn visible_row_indices(&self) -> Vec<usize> {
        self.filter.as_ref().map_or_else(
            || (0..self.rows.len()).collect(),
            |f| f.visible_indices().to_vec(),
        )
    }

    /// Iterator over the rows currently rendered by the dashboard.
    pub fn visible_rows(&self) -> impl Iterator<Item = &VarSummary> {
        self.visible_row_indices()
            .into_iter()
            .filter_map(move |i| self.rows.get(i))
    }

    /// Whether the fuzzy-filter input box is currently capturing
    /// keystrokes. While `true` characters edit the needle instead of
    /// firing actions.
    #[must_use]
    pub fn is_filter_input_active(&self) -> bool {
        self.filter.as_ref().is_some_and(FilterState::input_active)
    }

    /// Whether a filter is currently applied (regardless of whether
    /// the input box is still open).
    #[must_use]
    pub const fn is_filter_active(&self) -> bool {
        self.filter.is_some()
    }

    /// The current filter needle, if any. Empty string when the user
    /// has opened the filter but not typed anything yet.
    #[must_use]
    pub fn filter_needle(&self) -> Option<&str> {
        self.filter.as_ref().map(FilterState::needle)
    }

    /// Visible-row index of the currently-selected row, if any. This
    /// is the index inside [`Self::visible_rows`], not into
    /// [`Self::rows`]. Use [`Self::selected_row`] to dereference to
    /// the underlying [`VarSummary`].
    #[must_use]
    pub const fn selected_index(&self) -> Option<usize> {
        self.table_state.selected()
    }

    /// The currently selected [`VarSummary`], if any, resolved through
    /// the active filter.
    #[must_use]
    pub fn selected_row(&self) -> Option<&VarSummary> {
        let visible_idx = self.table_state.selected()?;
        let absolute_idx = match self.filter.as_ref() {
            Some(f) => *f.visible_indices().get(visible_idx)?,
            None => visible_idx,
        };
        self.rows.get(absolute_idx)
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

    /// Select a row by visible dashboard index.
    ///
    /// Used by mouse support. The index is in the same coordinate space as
    /// [`Self::visible_rows`], so active filters are respected.
    pub fn select_visible_index(&mut self, index: usize) {
        if index < self.visible_len() {
            self.table_state.select(Some(index));
        }
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

    /// Top-level view currently displayed.
    #[must_use]
    pub const fn current_view(&self) -> View {
        self.view
    }

    /// Active profile used as the default for link/run forms.
    #[must_use]
    pub fn active_profile(&self) -> &str {
        &self.active_profile
    }

    /// Recent audit rows shown in the audit view.
    #[must_use]
    pub fn audit_rows(&self) -> &[AuditEntry] {
        &self.audit_rows
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
        // Cascade: toast → filter → secondary view → overlay → quit.
        // Each level is dismissed in turn so the user has a
        // predictable Esc path back to the dashboard root before the
        // app exits. Modals and similar focus-stealing overlays will
        // be inserted ahead of `toast` in subsequent phases.
        if self.toast.is_some() {
            self.toast = None;
            return;
        }
        if self.is_filter_active() {
            self.close_filter();
            return;
        }
        if !matches!(self.view, View::Dashboard) {
            self.view = View::Dashboard;
            self.detail_target = None;
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

    /// Visible-row count: the rendered length of the dashboard.
    /// When a filter is applied this is the count of *matching* rows;
    /// otherwise it equals `rows.len()`. Navigation operates in this
    /// space so the cursor never lands on a hidden row.
    fn visible_len(&self) -> usize {
        self.filter
            .as_ref()
            .map_or_else(|| self.rows.len(), |f| f.visible_indices().len())
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            self.table_state.select(None);
            return;
        }
        let max = len - 1;
        let cur = self.table_state.selected().unwrap_or(0).min(max);
        self.table_state.select(Some(cur));
    }

    fn select_next(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            return;
        }
        let next = self.table_state.selected().map_or(0, |i| (i + 1) % len);
        self.table_state.select(Some(next));
    }

    fn select_prev(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            return;
        }
        let prev = self
            .table_state
            .selected()
            .map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
        self.table_state.select(Some(prev));
    }

    #[allow(clippy::missing_const_for_fn)]
    fn select_first(&mut self) {
        if self.visible_len() > 0 {
            self.table_state.select(Some(0));
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    fn select_last(&mut self) {
        if let Some(last) = self.visible_len().checked_sub(1) {
            self.table_state.select(Some(last));
        }
    }

    fn page(&mut self, down: bool) {
        // A "page" is intentionally a fixed stride; the runtime does
        // not know the viewport size at action-translation time. Ten
        // rows is a sensible compromise that works on small and large
        // terminals alike.
        const STRIDE: usize = 10;
        let len = self.visible_len();
        if len == 0 {
            return;
        }
        let cur = self.table_state.selected().unwrap_or(0);
        let new = if down {
            cur.saturating_add(STRIDE).min(len - 1)
        } else {
            cur.saturating_sub(STRIDE)
        };
        self.table_state.select(Some(new));
    }
}

/// Return the next field in the editor form's focus cycle. In
/// `EditValue` mode the Name / Group / Kind fields are display-only
/// so focus stays on `Value`.
const fn next_focus(current: FormField, mode: &EditorMode) -> FormField {
    if matches!(mode, EditorMode::EditValue { .. }) {
        return FormField::Value;
    }
    match current {
        FormField::Name => FormField::Group,
        FormField::Group => FormField::Kind,
        FormField::Kind => FormField::Value,
        FormField::Value => FormField::Name,
    }
}

/// Return the previous field in the editor form's focus cycle.
const fn prev_focus(current: FormField, mode: &EditorMode) -> FormField {
    if matches!(mode, EditorMode::EditValue { .. }) {
        return FormField::Value;
    }
    match current {
        FormField::Name => FormField::Value,
        FormField::Group => FormField::Name,
        FormField::Kind => FormField::Group,
        FormField::Value => FormField::Kind,
    }
}

/// Apply a non-Tab / non-Esc / non-Enter / non-Ctrl-C key to the
/// currently-focused field of the editor form.
fn handle_field_key(form: &mut EditorForm, key: KeyEvent) {
    let read_only_metadata = matches!(form.mode, EditorMode::EditValue { .. });
    match form.focus {
        FormField::Name => {
            if read_only_metadata {
                return;
            }
            match key.code {
                KeyCode::Backspace => {
                    form.name.pop();
                }
                KeyCode::Char(c) if is_text_input(key) => form.name.push(c),
                _ => {}
            }
        }
        FormField::Group => {
            if read_only_metadata {
                return;
            }
            match key.code {
                KeyCode::Left => {
                    form.group_idx = (form.group_idx + GROUP_CYCLE.len() - 1) % GROUP_CYCLE.len();
                }
                KeyCode::Right | KeyCode::Char(' ') => {
                    form.group_idx = (form.group_idx + 1) % GROUP_CYCLE.len();
                }
                _ => {}
            }
        }
        FormField::Kind => {
            if read_only_metadata {
                return;
            }
            match key.code {
                KeyCode::Left => {
                    form.kind_idx = (form.kind_idx + KIND_CYCLE.len() - 1) % KIND_CYCLE.len();
                }
                KeyCode::Right | KeyCode::Char(' ') => {
                    form.kind_idx = (form.kind_idx + 1) % KIND_CYCLE.len();
                }
                _ => {}
            }
        }
        FormField::Value => match key.code {
            KeyCode::Backspace => {
                form.value.pop();
            }
            // `Ctrl+S` toggles "show value" so the user can verify what
            // they typed.
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                form.show_value = !form.show_value;
            }
            KeyCode::Char(c) if is_text_input(key) => form.value.push(c),
            _ => {}
        },
    }
}

/// Apply a key to the currently-focused field of the link form.
fn handle_link_field_key(form: &mut LinkForm, key: KeyEvent) {
    match form.focus {
        LinkField::Path => match key.code {
            KeyCode::Backspace => {
                form.path.pop();
            }
            KeyCode::Char(c) if is_text_input(key) => form.path.push(c),
            _ => {}
        },
        LinkField::Profile => match key.code {
            KeyCode::Backspace => {
                form.profile.pop();
            }
            KeyCode::Char(c) if is_text_input(key) => form.profile.push(c),
            _ => {}
        },
        LinkField::Materialize => match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ' | 'y' | 'Y' | 'n' | 'N') => {
                form.materialize = !form.materialize;
            }
            _ => {}
        },
    }
}

/// Apply a key to the currently-focused field of the run form.
fn handle_run_field_key(form: &mut RunForm, key: KeyEvent) {
    let target = match form.focus {
        RunField::Path => &mut form.path,
        RunField::Profile => &mut form.profile,
        RunField::Command => &mut form.command,
    };
    match key.code {
        KeyCode::Backspace => {
            target.pop();
        }
        KeyCode::Char(c) if is_text_input(key) => target.push(c),
        _ => {}
    }
}

/// Whether a key event represents a typeable character (no Ctrl /
/// Alt modifier; Shift is OK).
const fn is_text_input(key: KeyEvent) -> bool {
    !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT)
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
        fn get_value(&self, _: VarId) -> Result<Option<SecretString>, ProviderError> {
            Ok(None)
        }
    }

    struct FailingProvider;
    impl VarProvider for FailingProvider {
        fn list(&self) -> Result<Vec<VarSummary>, ProviderError> {
            Err(ProviderError::Backend("synthetic".into()))
        }
        fn get_value(&self, _: VarId) -> Result<Option<SecretString>, ProviderError> {
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

    // ─── Phase 2a: fuzzy filter ───────────────────────────────────

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn five_rows() -> StaticProvider {
        StaticProvider(vec![
            summary("DATABASE_URL"),
            summary("API_KEY"),
            summary("DB_HOST"),
            summary("NODE_ENV"),
            summary("PORT"),
        ])
    }

    #[test]
    fn start_fuzzy_opens_filter_with_empty_needle() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::StartFuzzy);
        assert!(app.is_filter_active());
        assert!(app.is_filter_input_active());
        assert_eq!(app.filter_needle(), Some(""));
        // Empty needle shows every row.
        assert_eq!(app.visible_rows().count(), 5);
    }

    #[test]
    fn typing_filter_chars_narrows_visible_rows() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::StartFuzzy);
        // Dispatch each char through dispatch_key so the filter-input
        // routing path is exercised.
        app.dispatch_key(press(KeyCode::Char('d')));
        app.dispatch_key(press(KeyCode::Char('b')));
        let visible: Vec<_> = app.visible_rows().map(|v| v.name.clone()).collect();
        assert!(visible.contains(&"DATABASE_URL".to_string()));
        assert!(visible.contains(&"DB_HOST".to_string()));
        assert!(!visible.contains(&"API_KEY".to_string()));
        assert!(!visible.contains(&"PORT".to_string()));
        assert_eq!(app.filter_needle(), Some("db"));
    }

    #[test]
    fn backspace_pops_needle_and_widens_visible_set() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::StartFuzzy);
        app.dispatch_key(press(KeyCode::Char('x')));
        assert_eq!(app.visible_rows().count(), 0);
        app.dispatch_key(press(KeyCode::Backspace));
        assert_eq!(app.filter_needle(), Some(""));
        assert_eq!(app.visible_rows().count(), 5);
    }

    #[test]
    fn enter_commits_filter_input_but_keeps_filter_applied() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::StartFuzzy);
        app.dispatch_key(press(KeyCode::Char('p')));
        app.dispatch_key(press(KeyCode::Enter));
        assert!(app.is_filter_active());
        assert!(!app.is_filter_input_active());
        // The filter is still narrowing the view.
        assert!(app.visible_rows().count() < 5);
        // Char keys now go through the Action path again.
        app.dispatch_key(press(KeyCode::Char('s')));
        assert!(app.secrets_visible());
    }

    #[test]
    fn esc_clears_the_filter_entirely() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::StartFuzzy);
        app.dispatch_key(press(KeyCode::Char('p')));
        app.dispatch_key(press(KeyCode::Esc));
        assert!(!app.is_filter_active());
        assert_eq!(app.visible_rows().count(), 5);
    }

    #[test]
    fn selection_clamps_to_visible_count_on_narrow() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::MoveBottom); // select row 4 (PORT)
        assert_eq!(app.selected_index(), Some(4));
        app.apply(Action::StartFuzzy);
        // Type something that filters down to only a handful of rows.
        app.dispatch_key(press(KeyCode::Char('d')));
        app.dispatch_key(press(KeyCode::Char('b')));
        // Visible count is 2; selection must clamp.
        let visible = app.visible_rows().count();
        assert!(visible <= 2);
        assert!(app.selected_index().is_some_and(|i| i < visible));
    }

    #[test]
    fn selected_row_resolves_through_filter() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::StartFuzzy);
        // Filter narrows to rows containing "API".
        app.dispatch_key(press(KeyCode::Char('a')));
        app.dispatch_key(press(KeyCode::Char('p')));
        // The selected row should now be API_KEY (the best match for
        // "ap") rather than whatever absolute index 0 points to.
        let selected = app.selected_row().expect("a row should be selected");
        assert_eq!(selected.name, "API_KEY");
    }

    #[test]
    fn ctrl_c_still_quits_while_filter_input_active() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::StartFuzzy);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        app.dispatch_key(ctrl_c);
        assert!(app.quit_requested());
    }

    #[test]
    fn refresh_request_is_signalled_when_filter_is_off() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        let outcome = app.dispatch_key(press(KeyCode::Char('r')));
        assert!(matches!(outcome, DispatchOutcome::RefreshRequested));
    }

    #[test]
    fn dismiss_closes_an_active_filter_before_overlay_or_quit() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        // Open + commit the filter so input is no longer active.
        app.apply(Action::StartFuzzy);
        app.dispatch_key(press(KeyCode::Char('d')));
        app.dispatch_key(press(KeyCode::Enter));
        assert!(app.is_filter_active());
        // Esc with a committed filter must close the filter, NOT quit.
        app.apply(Action::Dismiss);
        assert!(!app.is_filter_active());
        assert!(!app.quit_requested());
        // A second Esc with no overlay/filter quits.
        app.apply(Action::Dismiss);
        assert!(app.quit_requested());
    }

    #[test]
    fn typing_in_filter_input_clears_pre_existing_info_toast() {
        // Regression: previously the filter-input path never touched
        // the toast, so a stale "refreshed (5 vars)" sat on screen
        // while the user typed a needle that contradicted it.
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.set_info_toast("refreshed (5 vars)");
        app.apply(Action::StartFuzzy);
        app.dispatch_key(press(KeyCode::Char('d')));
        assert!(app.toast_text().is_none());
    }

    #[test]
    fn typing_in_filter_input_preserves_error_toasts() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.set_error_toast("backend failure");
        app.apply(Action::StartFuzzy);
        app.dispatch_key(press(KeyCode::Char('d')));
        assert_eq!(app.toast_text(), Some("backend failure"));
    }

    // ─── Phase 2b1: detail view ───────────────────────────────────

    #[test]
    fn open_detail_switches_view_when_a_row_is_selected() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        assert_eq!(app.current_view(), View::Dashboard);
        app.apply(Action::OpenDetail);
        assert_eq!(app.current_view(), View::Detail);
    }

    #[test]
    fn open_detail_on_empty_dashboard_keeps_view_and_toasts() {
        let mut app = AppState::new();
        app.refresh(&StaticProvider(Vec::new())).unwrap();
        app.apply(Action::OpenDetail);
        assert_eq!(app.current_view(), View::Dashboard);
        assert_eq!(app.toast_text(), Some("no row selected"));
    }

    #[test]
    fn dismiss_returns_from_detail_to_dashboard() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::OpenDetail);
        assert_eq!(app.current_view(), View::Detail);
        app.apply(Action::Dismiss);
        assert_eq!(app.current_view(), View::Dashboard);
        assert!(!app.quit_requested());
    }

    #[test]
    fn detail_view_survives_secret_toggle_and_help_open() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::OpenDetail);
        // Action applied while on Detail view should not auto-return.
        app.apply(Action::ToggleSecretVisibility);
        assert_eq!(app.current_view(), View::Detail);
        assert!(app.secrets_visible());
        app.apply(Action::ToggleHelp);
        assert!(app.help_visible());
        assert_eq!(app.current_view(), View::Detail);
    }

    #[test]
    fn detail_row_resolves_by_identity_after_row_reorder() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        // Select API_KEY (index 1) and open detail.
        app.apply(Action::MoveDown);
        let target_id = app.selected_row().expect("selection").id;
        app.apply(Action::OpenDetail);
        assert_eq!(app.current_view(), View::Detail);
        assert_eq!(
            app.detail_row().map(|v| v.id),
            Some(target_id),
            "Detail must resolve to the originally inspected var"
        );

        // Now refresh with the SAME rows but in reverse order. By
        // index alone the Detail pane would silently jump to a
        // different variable.
        let reversed_rows: Vec<VarSummary> = {
            let mut tmp = app.rows().to_vec();
            tmp.reverse();
            tmp
        };
        app.refresh(&StaticProvider(reversed_rows)).unwrap();
        assert_eq!(app.current_view(), View::Detail);
        assert_eq!(
            app.detail_row().map(|v| v.id),
            Some(target_id),
            "Detail target must follow identity through a row reorder"
        );
    }

    #[test]
    fn refresh_returns_from_detail_when_inspected_var_is_gone() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::MoveDown); // select API_KEY
        let target_id = app.selected_row().expect("selection").id;
        app.apply(Action::OpenDetail);
        assert_eq!(app.current_view(), View::Detail);

        // External delete: rebuild rows without the inspected target.
        let surviving: Vec<VarSummary> = app
            .rows()
            .iter()
            .filter(|v| v.id != target_id)
            .cloned()
            .collect();
        app.refresh(&StaticProvider(surviving)).unwrap();

        assert_eq!(
            app.current_view(),
            View::Dashboard,
            "must auto-return to dashboard when the inspected var disappears"
        );
        assert!(
            app.toast_text()
                .is_some_and(|t| t.contains("removed elsewhere")),
            "must surface a loud error toast"
        );
        assert!(app.toast_is_error());
    }

    #[test]
    fn open_detail_does_not_clobber_sticky_error_toast() {
        let mut app = AppState::new();
        app.refresh(&StaticProvider(Vec::new())).unwrap();
        app.set_error_toast("backend failure");
        // Apply via `apply` so the pre-dispatch info-clear runs:
        // error toasts must survive that step AND the open_detail
        // empty-selection branch.
        app.apply(Action::OpenDetail);
        assert_eq!(app.toast_text(), Some("backend failure"));
        assert!(app.toast_is_error());
        assert_eq!(app.current_view(), View::Dashboard);
    }

    #[test]
    fn return_to_dashboard_clears_detail_target() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::OpenDetail);
        assert!(app.detail_row().is_some());
        app.apply(Action::Dismiss);
        assert_eq!(app.current_view(), View::Dashboard);
        // Internal invariant: re-opening Detail must re-snapshot
        // against the current selection, not retain the prior target.
        app.apply(Action::MoveDown);
        let new_target = app.selected_row().expect("selection").id;
        app.apply(Action::OpenDetail);
        assert_eq!(app.detail_row().map(|v| v.id), Some(new_target));
    }

    #[test]
    fn dismiss_cascade_priority_is_toast_filter_view_overlay() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        // Build a stacked context: filter committed, view = Detail,
        // overlay = Help, plus an ERROR toast on top (info toasts
        // would auto-clear before the dismiss cascade and merge two
        // steps into one — error toasts are sticky and exercise the
        // explicit-toast-dismiss step on its own).
        app.apply(Action::ToggleHelp);
        app.apply(Action::StartFuzzy);
        app.dispatch_key(press(KeyCode::Char('a')));
        app.dispatch_key(press(KeyCode::Enter));
        app.apply(Action::OpenDetail);
        app.set_error_toast("scratch");
        // 1) toast first
        app.apply(Action::Dismiss);
        assert!(app.toast_text().is_none());
        assert!(app.is_filter_active());
        // 2) filter
        app.apply(Action::Dismiss);
        assert!(!app.is_filter_active());
        assert_eq!(app.current_view(), View::Detail);
        // 3) view (back to dashboard)
        app.apply(Action::Dismiss);
        assert_eq!(app.current_view(), View::Dashboard);
        assert!(app.help_visible());
        // 4) overlay (help)
        app.apply(Action::Dismiss);
        assert!(!app.help_visible());
        // 5) finally quit
        app.apply(Action::Dismiss);
        assert!(app.quit_requested());
    }

    // ─── Phase 2b2: confirm modal + delete flow ───────────────────

    #[test]
    fn delete_action_opens_confirm_modal_for_selected_row() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        assert!(!app.is_confirm_visible());
        app.apply(Action::MoveDown); // select index 1 (API_KEY)
        let target_id = app.selected_row().expect("selection").id;
        app.apply(Action::DeleteVar);
        assert!(app.is_confirm_visible());
        let req = app.current_confirm().expect("confirm set");
        assert!(req.body.contains("API_KEY"));
        match &req.action {
            PendingAction::DeleteVar { id, name } => {
                assert_eq!(*id, target_id);
                assert_eq!(name, "API_KEY");
            }
        }
    }

    #[test]
    fn delete_action_on_empty_dashboard_does_not_open_modal() {
        let mut app = AppState::new();
        app.refresh(&StaticProvider(Vec::new())).unwrap();
        app.apply(Action::DeleteVar);
        assert!(!app.is_confirm_visible());
        assert_eq!(app.toast_text(), Some("no row selected"));
    }

    #[test]
    fn delete_action_on_detail_view_targets_inspected_var() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::MoveDown);
        let target_id = app.selected_row().expect("selection").id;
        app.apply(Action::OpenDetail);
        app.apply(Action::DeleteVar);
        assert!(app.is_confirm_visible());
        let req = app.current_confirm().expect("confirm set");
        match &req.action {
            PendingAction::DeleteVar { id, .. } => assert_eq!(*id, target_id),
        }
    }

    #[test]
    fn confirm_modal_steals_focus_from_filter_and_actions() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::DeleteVar);
        assert!(app.is_confirm_visible());

        // Char keys like 's' that normally fire ToggleSecretVisibility
        // must NOT take effect while a confirm is focused.
        let s = press(KeyCode::Char('s'));
        let outcome = app.dispatch_key(s);
        assert!(matches!(outcome, DispatchOutcome::Continue));
        assert!(!app.secrets_visible(), "modal must steal focus from `s`");
        assert!(app.is_confirm_visible());

        // Arrow keys must not navigate either.
        let down = press(KeyCode::Down);
        app.dispatch_key(down);
        assert!(app.is_confirm_visible());
    }

    #[test]
    fn modal_n_or_esc_cancels_without_side_effects() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::DeleteVar);
        let outcome = app.dispatch_key(press(KeyCode::Char('n')));
        assert!(matches!(outcome, DispatchOutcome::Continue));
        assert!(!app.is_confirm_visible());

        app.apply(Action::DeleteVar);
        let outcome = app.dispatch_key(press(KeyCode::Esc));
        assert!(matches!(outcome, DispatchOutcome::Continue));
        assert!(!app.is_confirm_visible());
    }

    #[test]
    fn modal_y_emits_delete_requested_with_id_and_name() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::MoveDown); // API_KEY
        let target_id = app.selected_row().expect("selection").id;
        app.apply(Action::DeleteVar);
        let outcome = app.dispatch_key(press(KeyCode::Char('y')));
        assert!(!app.is_confirm_visible(), "modal must clear after accept");
        match outcome {
            DispatchOutcome::DeleteRequested { id, name } => {
                assert_eq!(id, target_id);
                assert_eq!(name, "API_KEY");
            }
            other => panic!("expected DeleteRequested, got {other:?}"),
        }
    }

    #[test]
    fn modal_enter_also_accepts() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::DeleteVar);
        let outcome = app.dispatch_key(press(KeyCode::Enter));
        assert!(!app.is_confirm_visible());
        assert!(matches!(outcome, DispatchOutcome::DeleteRequested { .. }));
    }

    #[test]
    fn ctrl_c_quits_even_with_modal_focused() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::DeleteVar);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        app.dispatch_key(ctrl_c);
        assert!(app.quit_requested());
    }

    #[test]
    fn modal_plain_c_does_not_quit_or_dismiss() {
        // Regression: only Ctrl-C exits the modal; a plain `c`
        // keystroke is an unrecognised input that must leave the
        // modal focused.
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::DeleteVar);
        app.dispatch_key(press(KeyCode::Char('c')));
        assert!(app.is_confirm_visible());
        assert!(!app.quit_requested());
    }

    #[test]
    fn modal_reject_also_closes_help_overlay() {
        // The modal steals focus from `?`, so help can only be open
        // BEFORE the modal is raised. Rejecting the modal with Esc
        // should cascade and close help too — otherwise the user
        // sees nothing visible change when they hit Esc the first
        // time (modal disappears but help still covers everything).
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::ToggleHelp);
        assert!(app.help_visible());
        app.apply(Action::DeleteVar);
        assert!(app.is_confirm_visible());
        app.dispatch_key(press(KeyCode::Esc));
        assert!(!app.is_confirm_visible());
        assert!(!app.help_visible(), "Esc cascade must close help too");
    }

    #[test]
    fn splice_out_row_removes_local_entry_and_rebuilds_filter() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        // Snapshot the API_KEY id.
        app.apply(Action::MoveDown);
        let target_id = app.selected_row().expect("selection").id;

        // Apply a filter that matches `target_id` so we can confirm
        // the filter buffer is also re-ranked after the splice.
        app.apply(Action::StartFuzzy);
        app.dispatch_key(press(KeyCode::Char('a')));
        let before = app.visible_rows().count();
        assert!(before >= 1);

        app.splice_out_row(target_id);
        assert!(app.rows().iter().all(|v| v.id != target_id));
        assert!(app.visible_rows().count() < before);
    }

    #[test]
    fn splice_out_row_on_inspected_var_returns_to_dashboard() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::MoveDown);
        let target_id = app.selected_row().expect("selection").id;
        app.apply(Action::OpenDetail);
        assert_eq!(app.current_view(), View::Detail);

        app.splice_out_row(target_id);
        assert_eq!(
            app.current_view(),
            View::Dashboard,
            "splice of the inspected var must return to dashboard"
        );
        assert!(app.detail_row().is_none());
    }

    #[test]
    fn splice_out_row_on_unknown_id_is_a_noop() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        let before = app.rows().len();
        // Random VarId — must not be one of the five we just loaded.
        let bogus = VarId::new_v4();
        app.splice_out_row(bogus);
        assert_eq!(app.rows().len(), before);
    }

    #[test]
    fn unknown_keys_keep_modal_focused() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::DeleteVar);
        // Letter neither y/Y/n/N nor Enter/Esc — must not dismiss.
        app.dispatch_key(press(KeyCode::Char('q')));
        assert!(app.is_confirm_visible());
        assert!(!app.quit_requested());
    }

    #[test]
    fn refresh_rebuilds_filter_against_new_rows() {
        let mut app = AppState::new();
        app.refresh(&five_rows()).unwrap();
        app.apply(Action::StartFuzzy);
        app.dispatch_key(press(KeyCode::Char('d')));
        let before = app.visible_rows().count();
        // Shrink the underlying data and refresh.
        let shrunk = StaticProvider(vec![summary("DATABASE_URL")]);
        app.refresh(&shrunk).unwrap();
        // Filter must still be applied and re-ranked against the new rows.
        assert!(app.is_filter_active());
        assert_eq!(app.filter_needle(), Some("d"));
        let after = app.visible_rows().count();
        assert!(after <= before);
    }
}
