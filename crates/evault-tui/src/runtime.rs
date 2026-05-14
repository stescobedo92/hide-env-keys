//! Terminal lifecycle and event loop.

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::time::Duration;

use ratatui::crossterm::event::{self, Event};
use ratatui::DefaultTerminal;

use crate::app::{AppState, DispatchOutcome, View};
use crate::error::TuiError;
use crate::provider::{VarMutator, VarProvider};
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
/// use evault_core::model::VarId;
/// use evault_tui::{run_tui, ProviderError, VarMutator, VarProvider, VarSummary};
///
/// struct Empty;
/// impl VarProvider for Empty {
///     fn list(&self) -> Result<Vec<VarSummary>, ProviderError> { Ok(Vec::new()) }
/// }
/// impl VarMutator for Empty {
///     fn delete(&self, _id: VarId) -> Result<(), ProviderError> { Ok(()) }
/// }
///
/// run_tui(Empty).unwrap();
/// ```
#[allow(clippy::needless_pass_by_value)]
pub fn run_tui<B>(backend: B) -> Result<(), TuiError>
where
    B: VarProvider + VarMutator,
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
    B: VarProvider + VarMutator + ?Sized,
{
    let mut app = AppState::new();
    let theme = Theme::dark();

    // Initial load. A first-load failure is a hard error: the user
    // sees an empty TUI and has no way to recover.
    app.refresh(backend)?;

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
                            app.set_error_toast("create crashed: backend panicked");
                        }
                        Ok(Err(e)) => {
                            app.set_error_toast(format!("create failed: {e}"));
                        }
                        Ok(Ok(_id)) => {
                            if let Err(e) = app.refresh(backend) {
                                app.set_error_toast(format!(
                                    "created `{name}` but refresh failed: {e}"
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
                            app.set_error_toast("update crashed: backend panicked");
                        }
                        Ok(Err(e)) => {
                            app.set_error_toast(format!("update failed: {e}"));
                        }
                        Ok(Ok(())) => {
                            if let Err(e) = app.refresh(backend) {
                                app.set_error_toast(format!(
                                    "updated `{name}` but refresh failed: {e}"
                                ));
                            } else {
                                app.set_info_toast(format!("updated `{name}`"));
                            }
                        }
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
                            app.set_error_toast("delete crashed: backend panicked");
                        }
                        Ok(Err(e)) => {
                            app.set_error_toast(format!("delete failed: {e}"));
                        }
                        Ok(Ok(())) => {
                            if matches!(app.current_view(), View::Detail) {
                                app.return_to_dashboard();
                            }
                            match app.refresh(backend) {
                                Ok(()) => {
                                    app.set_info_toast(format!("deleted `{name}`"));
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
