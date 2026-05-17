//! Terminal lifecycle and event loop.

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::time::Duration;

use ratatui::crossterm::event::{self, Event};
use ratatui::DefaultTerminal;
use secrecy::ExposeSecret;

use crate::app::{AppState, DispatchOutcome, View};
use crate::error::TuiError;
use crate::provider::{AuditProvider, VarMutator, VarProvider};
use crate::theme::Theme;
use crate::views;

/// How long to block waiting for a key event before re-drawing.
///
/// Short enough that resize events feel snappy (50 ms ≈ 20 fps when
/// idle) but long enough that the runtime spends most of its time
/// asleep, not redrawing the same frame.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Run the TUI against `backend`.
///
/// The single `backend` argument implements both [`VarProvider`]
/// (read side: the dashboard refreshes from it) and [`VarMutator`]
/// (write side: the confirm-modal delete flow calls into it). Phase
/// 2c will extend [`VarMutator`] with create / update / link
/// without breaking this signature.
///
/// Owns the terminal lifecycle: enters raw mode + the alternate
/// screen, installs a panic hook that restores them on unwind, and
/// guarantees the restore happens whether the loop returns `Ok`,
/// returns `Err`, or panics.
///
/// `backend` is **consumed** for the duration of the session: the
/// TUI takes ownership and drops it on return, so callers cannot
/// reuse the same instance for a second session.
///
/// # Errors
/// Returns [`TuiError::Terminal`] if terminal I/O fails (raw-mode
/// toggling, drawing, event reading). Transient `ErrorKind::Interrupted`
/// errors (e.g. signal-interrupted `poll`) are retried internally and
/// do not surface. Returns [`TuiError::Provider`] only if the *initial*
/// refresh fails; subsequent refresh errors are surfaced as a sticky
/// error toast and the loop continues. Delete failures are surfaced
/// as a sticky error toast — they never propagate.
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use evault_core::model::VarId;
/// use evault_tui::{AuditProvider, run_tui, ProviderError, VarDraft, VarMutator, VarProvider, VarSummary};
/// use secrecy::SecretString;
///
/// struct Empty;
/// impl VarProvider for Empty {
///     fn list(&self) -> Result<Vec<VarSummary>, ProviderError> { Ok(Vec::new()) }
///     fn get_value(&self, _id: VarId) -> Result<Option<SecretString>, ProviderError> {
///         Ok(None)
///     }
/// }
/// impl VarMutator for Empty {
///     fn delete(&self, _id: VarId) -> Result<(), ProviderError> { Ok(()) }
///     fn create(&self, _draft: VarDraft) -> Result<VarId, ProviderError> {
///         Ok(VarId::new_v4())
///     }
///     fn update_value(&self, _id: VarId, _value: SecretString) -> Result<(), ProviderError> {
///         Ok(())
///     }
///     fn record_copy(&self, _id: VarId) -> Result<(), ProviderError> { Ok(()) }
///     fn link_to_project(
///         &self,
///         _var_id: VarId,
///         _var_name: String,
///         _project_path: PathBuf,
///         _profile: String,
///         _materialize: bool,
///     ) -> Result<(), ProviderError> { Ok(()) }
///     fn run_in_project(
///         &self,
///         _project_path: PathBuf,
///         _profile: String,
///         _program: String,
///         _args: Vec<String>,
///     ) -> Result<Option<i32>, ProviderError> { Ok(Some(0)) }
/// }
/// impl AuditProvider for Empty {
///     fn recent_audit(
///         &self,
///         _limit: usize,
///     ) -> Result<Vec<evault_core::model::AuditEntry>, ProviderError> {
///         Ok(Vec::new())
///     }
/// }
///
/// run_tui(Empty).unwrap();
/// ```
#[allow(clippy::needless_pass_by_value)]
pub fn run_tui<B>(backend: B) -> Result<(), TuiError>
where
    B: VarProvider + VarMutator + AuditProvider,
{
    let mut terminal = ratatui::try_init()?;
    let loop_result = event_loop(&mut terminal, &backend);

    // ALWAYS attempt to restore. The restore-error precedence policy
    // is: if the loop succeeded, surface a restore failure; if the
    // loop already failed, log the restore failure to stderr (raw
    // mode is presumably broken anyway, so the print will reach the
    // user's reset shell) and propagate the *original* loop error so
    // the user sees the real cause.
    match (loop_result, ratatui::try_restore()) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(restore_err)) => Err(TuiError::Terminal(restore_err)),
        (Err(loop_err), Ok(())) => Err(loop_err),
        (Err(loop_err), Err(restore_err)) => {
            // Best-effort visibility: the user's terminal may be in
            // an inconsistent state. We use stderr because logging
            // crates are not a dependency of this layer.
            #[allow(clippy::print_stderr)]
            {
                eprintln!("evault-tui: terminal restore failed after loop error: {restore_err}");
            }
            Err(loop_err)
        }
    }
}

#[allow(clippy::too_many_lines)]
fn event_loop<B>(terminal: &mut DefaultTerminal, backend: &B) -> Result<(), TuiError>
where
    B: VarProvider + VarMutator + AuditProvider + ?Sized,
{
    let mut app = AppState::new();
    let theme = Theme::dark();

    // Initial load. A first-load failure is a hard error: the user
    // sees an empty TUI and has no way to recover.
    app.refresh(backend)?;
    if let Err(e) = app.refresh_audit(backend) {
        app.set_error_toast(format!("audit load failed: {e}"));
    }

    while !app.quit_requested() {
        terminal.draw(|frame| views::render(frame, &mut app, &theme))?;

        let polled = match event::poll(POLL_INTERVAL) {
            Ok(b) => b,
            // EINTR is non-fatal: a signal arrived during `poll`
            // (SIGWINCH on resize, SIGCONT after a stop, debugger
            // attach). Treat the interruption as "no event"; the
            // outer loop will re-draw and try again.
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(TuiError::Terminal(e)),
        };
        if !polled {
            continue;
        }
        let ev = match event::read() {
            Ok(ev) => ev,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(TuiError::Terminal(e)),
        };

        // We match all variants explicitly so the choice to ignore
        // resize / mouse / focus / paste is auditable rather than an
        // implicit drop via `if let Event::Key(_) = ...`.
        #[allow(clippy::match_same_arms)]
        match ev {
            Event::Key(key) => match app.dispatch_key(key) {
                DispatchOutcome::Continue => {}
                DispatchOutcome::RefreshRequested => {
                    // `dispatch_key` already cleared any toast. We
                    // re-fetch here so the side-effect lives at the
                    // boundary that owns the provider. On success
                    // surface a positive confirmation; on failure the
                    // error toast is sticky and survives further input.
                    match app.refresh(backend) {
                        Ok(()) => {
                            if let Err(e) = app.refresh_audit(backend) {
                                app.set_error_toast(format!("audit refresh failed: {e}"));
                                continue;
                            }
                            // When a filter is applied the dashboard
                            // title reads `vars (matched/total)`. The
                            // toast mirrors that format so a user with
                            // an active filter does not see two
                            // contradicting counts.
                            let total = app.rows().len();
                            let msg = if app.is_filter_active() {
                                let matched = app.visible_row_indices().len();
                                format!("refreshed ({matched}/{total} vars)")
                            } else {
                                format!("refreshed ({total} vars)")
                            };
                            app.set_info_toast(msg);
                        }
                        Err(e) => app.set_error_toast(e.to_string()),
                    }
                }
                DispatchOutcome::CreateRequested(draft) => {
                    let name = draft.name.clone();
                    let create_result =
                        panic::catch_unwind(AssertUnwindSafe(|| backend.create(draft)));
                    match create_result {
                        Err(_) => {
                            app.show_error_modal(
                                "create failed",
                                "backend panicked while creating the variable",
                                Some(
                                    "this is a bug in the backend; restart and \
                                     report the issue if it persists."
                                        .into(),
                                ),
                            );
                        }
                        Ok(Err(e)) => {
                            let msg = e.to_string();
                            let hint = create_hint(&msg);
                            app.show_error_modal("create failed", msg, hint);
                        }
                        Ok(Ok(_id)) => {
                            if let Err(e) = app.refresh(backend) {
                                app.set_error_toast(format!(
                                    "created `{name}` but refresh failed: {e}"
                                ));
                            } else if let Err(e) = app.refresh_audit(backend) {
                                app.set_error_toast(format!(
                                    "created `{name}` but audit refresh failed: {e}"
                                ));
                            } else {
                                app.set_info_toast(format!("created `{name}`"));
                            }
                        }
                    }
                }
                DispatchOutcome::UpdateValueRequested { id, value, name } => {
                    let update_result =
                        panic::catch_unwind(AssertUnwindSafe(|| backend.update_value(id, value)));
                    match update_result {
                        Err(_) => {
                            app.show_error_modal(
                                "update failed",
                                "backend panicked while updating the value",
                                Some(
                                    "this is a bug in the backend; restart \
                                     and report the issue if it persists."
                                        .into(),
                                ),
                            );
                        }
                        Ok(Err(e)) => {
                            let msg = e.to_string();
                            let hint = update_hint(&msg);
                            app.show_error_modal("update failed", msg, hint);
                        }
                        Ok(Ok(())) => {
                            if let Err(e) = app.refresh(backend) {
                                app.set_error_toast(format!(
                                    "updated `{name}` but refresh failed: {e}"
                                ));
                            } else if let Err(e) = app.refresh_audit(backend) {
                                app.set_error_toast(format!(
                                    "updated `{name}` but audit refresh failed: {e}"
                                ));
                            } else {
                                app.set_info_toast(format!("updated `{name}`"));
                            }
                        }
                    }
                }
                DispatchOutcome::LinkRequested {
                    id,
                    name,
                    project_path,
                    profile,
                    materialize,
                } => {
                    let result = panic::catch_unwind(AssertUnwindSafe(|| {
                        backend.link_to_project(
                            id,
                            name.clone(),
                            project_path.clone(),
                            profile.clone(),
                            materialize,
                        )
                    }));
                    match result {
                        Err(_) => {
                            app.show_error_modal(
                                "link failed",
                                "backend panicked while linking the variable",
                                Some(
                                    "this is a bug in the backend; restart \
                                     and report the issue if it persists."
                                        .into(),
                                ),
                            );
                        }
                        Ok(Err(e)) => {
                            let msg = e.to_string();
                            let hint = link_hint(&msg);
                            app.show_error_modal("link failed", msg, hint);
                        }
                        Ok(Ok(())) => {
                            let suffix = if materialize { " + .env" } else { "" };
                            if let Err(e) = app.refresh(backend) {
                                app.set_error_toast(format!(
                                    "linked `{name}` to {}{suffix} but refresh failed: {e}",
                                    project_path.display()
                                ));
                            } else if let Err(e) = app.refresh_audit(backend) {
                                app.set_error_toast(format!(
                                    "linked `{name}` to {}{suffix} but audit refresh failed: {e}",
                                    project_path.display()
                                ));
                            } else {
                                app.set_info_toast(format!(
                                    "linked `{name}` to {}{suffix}",
                                    project_path.display()
                                ));
                            }
                        }
                    }
                }
                DispatchOutcome::RunRequested {
                    project_path,
                    profile,
                    program,
                    args,
                } => {
                    // The child process must inherit a NORMAL terminal
                    // (no raw mode, no alternate screen). We tear the
                    // TUI down, spawn synchronously, and re-init when
                    // the child returns. `try_restore` failures are
                    // surfaced — without them, the child would print
                    // into the alternate buffer and never appear.
                    if let Err(e) = ratatui::try_restore() {
                        app.show_error_modal(
                            "run failed",
                            format!("could not restore the terminal before spawning: {e}"),
                            None,
                        );
                        continue;
                    }
                    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
                        backend.run_in_project(
                            project_path.clone(),
                            profile.clone(),
                            program.clone(),
                            args.clone(),
                        )
                    }));
                    // Re-enter raw mode + alternate screen regardless
                    // of the outcome below. Failing to re-init leaves
                    // the user stranded in a half-cooked terminal —
                    // bail out hard so the panic-restore path runs.
                    *terminal = ratatui::try_init().map_err(TuiError::Terminal)?;
                    match outcome {
                        Err(_) => {
                            app.show_error_modal(
                                "run failed",
                                "backend panicked while running the command",
                                Some(
                                    "this is a bug in the backend; restart \
                                     and report the issue if it persists."
                                        .into(),
                                ),
                            );
                        }
                        Ok(Err(e)) => {
                            let msg = e.to_string();
                            let hint = run_hint(&msg);
                            app.show_error_modal("run failed", msg, hint);
                        }
                        Ok(Ok(code)) => {
                            let cmd_repr = if args.is_empty() {
                                program.clone()
                            } else {
                                format!("{program} {}", args.join(" "))
                            };
                            let msg = match code {
                                Some(0) => format!("ran `{cmd_repr}` (exit 0)"),
                                Some(c) => format!("ran `{cmd_repr}` (exit {c})"),
                                None => format!("ran `{cmd_repr}` (killed by signal)"),
                            };
                            if let Err(e) = app.refresh(backend) {
                                app.set_error_toast(format!("{msg} but refresh failed: {e}"));
                            } else if let Err(e) = app.refresh_audit(backend) {
                                app.set_error_toast(format!(
                                    "{msg} but audit refresh failed: {e}"
                                ));
                            } else {
                                app.set_info_toast(msg);
                            }
                        }
                    }
                }
                DispatchOutcome::ViewValueRequested { id, name } => {
                    let result = panic::catch_unwind(AssertUnwindSafe(|| backend.get_value(id)));
                    match result {
                        Err(_) => {
                            app.set_error_toast("view value crashed: backend panicked");
                        }
                        Ok(Err(e)) => {
                            app.set_error_toast(format!("view value failed: {e}"));
                        }
                        Ok(Ok(None)) => {
                            app.set_error_toast(format!("value missing for `{name}`"));
                        }
                        Ok(Ok(Some(value))) => {
                            app.show_value_modal(name, value);
                        }
                    }
                }
                DispatchOutcome::CopyValueRequested { id, name } => {
                    let result = panic::catch_unwind(AssertUnwindSafe(|| backend.get_value(id)));
                    match result {
                        Err(_) => {
                            app.set_error_toast("copy failed: backend panicked");
                        }
                        Ok(Err(e)) => {
                            app.set_error_toast(format!("copy failed: {e}"));
                        }
                        Ok(Ok(None)) => {
                            app.set_error_toast(format!("value missing for `{name}`"));
                        }
                        Ok(Ok(Some(value))) => {
                            let copied = arboard::Clipboard::new()
                                .and_then(|mut clipboard| clipboard.set_text(value.expose_secret().to_owned()));
                            match copied {
                                Err(e) => {
                                    app.show_error_modal(
                                        "copy failed",
                                        format!("clipboard unavailable: {e}"),
                                        Some(
                                            "The value was read but not copied. Try again from a graphical session or use `evault export --mask` for inspection."
                                                .to_owned(),
                                        ),
                                    );
                                }
                                Ok(()) => match panic::catch_unwind(AssertUnwindSafe(|| {
                                    backend.record_copy(id)
                                })) {
                                    Err(_) => {
                                        app.set_error_toast(
                                            "copied value but audit recording panicked",
                                        );
                                    }
                                    Ok(Err(e)) => {
                                        app.set_error_toast(format!(
                                            "copied `{name}` but audit failed: {e}"
                                        ));
                                    }
                                    Ok(Ok(())) => {
                                        if let Err(e) = app.refresh_audit(backend) {
                                            app.set_error_toast(format!(
                                                "copied `{name}` but audit refresh failed: {e}"
                                            ));
                                        } else {
                                            app.set_info_toast(format!(
                                                "copied `{name}` to clipboard"
                                            ));
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
                DispatchOutcome::ProfileSwitchRequested { profile } => {
                    match evault_core::model::Profile::try_named(profile.clone()) {
                        Ok(valid) => {
                            app.set_active_profile(valid.as_str().to_owned());
                            app.set_info_toast(format!(
                                "active profile: {}",
                                valid.as_str()
                            ));
                        }
                        Err(e) => app.show_error_modal(
                            "profile failed",
                            e.to_string(),
                            Some(
                                "Use 1-32 ASCII letters, digits, '-' or '_', and include at least one non-digit character."
                                    .to_owned(),
                            ),
                        ),
                    }
                }
                DispatchOutcome::DeleteRequested { id, name } => {
                    // Side-effect at the runtime boundary that owns
                    // the backend. We guard against three failure
                    // modes:
                    //
                    // 1. The backend's `delete` panics — without a
                    //    `catch_unwind` the panic hook would tear
                    //    the terminal down with no actionable toast
                    //    for the user. We wrap the call and surface
                    //    the panic as an error toast instead.
                    // 2. `delete` returns `Err` — surfaced verbatim.
                    // 3. `delete` succeeds but `refresh` fails:
                    //    locally splice the deleted row out so the
                    //    dashboard does not show a ghost entry that
                    //    would re-fire on a second `d` keypress.
                    //
                    // On the happy path we return to Dashboard *only*
                    // if the user was inspecting the deleted row;
                    // refresh's stale-target guard would otherwise
                    // surface "removed elsewhere" for a self-initiated
                    // delete (a lie).
                    let delete_result =
                        panic::catch_unwind(AssertUnwindSafe(|| backend.delete(id)));
                    match delete_result {
                        Err(_) => {
                            app.show_error_modal(
                                "delete failed",
                                "backend panicked while deleting the variable",
                                Some(
                                    "this is a bug in the backend; restart \
                                     and report the issue if it persists."
                                        .into(),
                                ),
                            );
                        }
                        Ok(Err(e)) => {
                            let msg = e.to_string();
                            app.show_error_modal("delete failed", msg, None);
                        }
                        Ok(Ok(())) => {
                            if matches!(app.current_view(), View::Detail) {
                                app.return_to_dashboard();
                            }
                            match app.refresh(backend) {
                                Ok(()) => {
                                    if let Err(e) = app.refresh_audit(backend) {
                                        app.set_error_toast(format!(
                                            "deleted `{name}` but audit refresh failed: {e}"
                                        ));
                                    } else {
                                        app.set_info_toast(format!("deleted `{name}`"));
                                    }
                                }
                                Err(e) => {
                                    // Refresh failed — splice the
                                    // deleted row out so the user
                                    // doesn't see a ghost.
                                    app.splice_out_row(id);
                                    app.set_error_toast(format!(
                                        "deleted `{name}` but refresh failed: {e}"
                                    ));
                                }
                            }
                        }
                    }
                }
            },
            // Resize: the loop redraws on every iteration anyway, so
            // the new dimensions are picked up on the next `draw()`.
            Event::Resize(_, _) => {}
            // Mouse, focus, paste, etc. — not yet bound. Phase 2
            // will wire mouse selection and paste-into-editor.
            _ => {}
        }
    }

    Ok(())
}

/// Contextual hint for a failed `create` action.
///
/// Inspects the backend's error message and returns a plain-English
/// explanation. Multi-line hints use `\n` between bullets — the
/// error modal renders each line separately.
fn create_hint(msg: &str) -> Option<String> {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("invalid character") || lower.contains("invalid name") {
        return Some(
            "Variable names have these rules:\n\
             \u{2022} Start with a letter (A-Z or a-z) or an underscore (_)\n\
             \u{2022} After the first character, use only letters, digits, \
             or underscores\n\
             \u{2022} Maximum 64 characters\n\
             \u{2022} Not allowed: dashes, spaces, dots, accents, or other \
             punctuation\n\
             \n\
             Try a name like API_KEY, DATABASE_URL, or my_token."
                .to_owned(),
        );
    }
    if lower.contains("duplicate") || lower.contains("already exists") {
        return Some(
            "A variable with that name already exists. Pick a different \
             name, or press e on the existing row to update its value."
                .to_owned(),
        );
    }
    if lower.contains("empty") {
        return Some("The value field cannot be empty.".to_owned());
    }
    if lower.contains("too long") {
        return Some(
            "Names are limited to 64 characters. Values typically cap \
             around 1 MB depending on the storage backend."
                .to_owned(),
        );
    }
    None
}

/// Contextual hint for a failed `update_value` action.
fn update_hint(msg: &str) -> Option<String> {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("empty") {
        return Some("The new value cannot be empty.".to_owned());
    }
    if lower.contains("not found") || lower.contains("no variable") {
        return Some(
            "The variable was deleted by another process before the \
             update could complete. Press r to refresh the dashboard."
                .to_owned(),
        );
    }
    None
}

/// Contextual hint for a failed `run_in_project` action.
fn run_hint(msg: &str) -> Option<String> {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("manifest") || lower.contains("evault.toml") || lower.contains("no such file")
    {
        return Some(
            "The project must have an evault.toml manifest before \
             it can be run. Link a variable to the project first \
             (press l on a row), or run `evault link` from the \
             shell."
                .to_owned(),
        );
    }
    if lower.contains("program not found") || lower.contains("not found") {
        return Some(
            "The program was not found on PATH inside the project \
             directory. Check the spelling, or use an absolute path \
             (for example `./node_modules/.bin/jest` or \
             `C:\\Program Files\\app\\app.exe`)."
                .to_owned(),
        );
    }
    if lower.contains("permission") {
        return Some(
            "The OS refused to spawn the program. On Unix, mark the \
             file executable with `chmod +x`. On Windows, check that \
             the file is not blocked by `Unblock-File`."
                .to_owned(),
        );
    }
    None
}

/// Contextual hint for a failed `link_to_project` action.
fn link_hint(msg: &str) -> Option<String> {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("create project dir") || lower.contains("permission") {
        return Some(
            "Could not create the project directory. Check that the \
             path is writable and try again with a different path."
                .to_owned(),
        );
    }
    if lower.contains("canonicalise") || lower.contains("canonicalize") {
        return Some(
            "Could not resolve the project path. Check that the path \
             syntax is valid for your platform (use forward slashes on \
             Linux/macOS, backslashes or forward slashes on Windows)."
                .to_owned(),
        );
    }
    if lower.contains("manifest") {
        return Some(
            "Could not read or write the project's evault.toml file. \
             Check filesystem permissions on the project directory."
                .to_owned(),
        );
    }
    if lower.contains("materialize") {
        return Some(
            "Linking succeeded but writing the .env file failed. The \
             binding is recorded; you can retry materialization later \
             with evault gen --project PATH from the shell."
                .to_owned(),
        );
    }
    None
}
