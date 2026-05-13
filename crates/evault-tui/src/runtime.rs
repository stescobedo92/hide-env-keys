//! Terminal lifecycle and event loop.

use std::io;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event};
use ratatui::DefaultTerminal;

use crate::app::{AppState, DispatchOutcome};
use crate::error::TuiError;
use crate::provider::VarProvider;
use crate::theme::Theme;
use crate::views;

/// How long to block waiting for a key event before re-drawing.
///
/// Short enough that resize events feel snappy (50 ms ≈ 20 fps when
/// idle) but long enough that the runtime spends most of its time
/// asleep, not redrawing the same frame.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Run the TUI against `provider`.
///
/// Owns the terminal lifecycle: enters raw mode + the alternate
/// screen, installs a panic hook that restores them on unwind, and
/// guarantees the restore happens whether the loop returns `Ok`,
/// returns `Err`, or panics.
///
/// `provider` is **consumed** for the duration of the session: the
/// TUI takes ownership and drops it on return, so callers cannot
/// reuse the same instance for a second session. Change the signature
/// to `&P` if you need a longer-lived provider.
///
/// # Errors
/// Returns [`TuiError::Terminal`] if terminal I/O fails (raw-mode
/// toggling, drawing, event reading). Transient `ErrorKind::Interrupted`
/// errors (e.g. signal-interrupted `poll`) are retried internally and
/// do not surface. Returns [`TuiError::Provider`] only if the *initial*
/// refresh fails; subsequent refresh errors are surfaced as a sticky
/// error toast and the loop continues.
///
/// # Examples
///
/// ```no_run
/// use evault_tui::{run_tui, ProviderError, VarProvider, VarSummary};
///
/// struct Empty;
/// impl VarProvider for Empty {
///     fn list(&self) -> Result<Vec<VarSummary>, ProviderError> { Ok(Vec::new()) }
/// }
///
/// run_tui(Empty).unwrap();
/// ```
#[allow(clippy::needless_pass_by_value)]
pub fn run_tui<P: VarProvider>(provider: P) -> Result<(), TuiError> {
    let mut terminal = ratatui::try_init()?;
    let loop_result = event_loop(&mut terminal, &provider);

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

fn event_loop<P: VarProvider + ?Sized>(
    terminal: &mut DefaultTerminal,
    provider: &P,
) -> Result<(), TuiError> {
    let mut app = AppState::new();
    let theme = Theme::dark();

    // Initial load. A first-load failure is a hard error: the user
    // sees an empty TUI and has no way to recover.
    app.refresh(provider)?;

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
            Event::Key(key) => {
                if matches!(app.dispatch_key(key), DispatchOutcome::RefreshRequested) {
                    // `dispatch_key` already cleared any toast. We
                    // re-fetch here so the side-effect lives at the
                    // boundary that owns the provider. On success
                    // surface a positive confirmation; on failure the
                    // error toast is sticky and survives further input.
                    match app.refresh(provider) {
                        Ok(()) => {
                            app.set_info_toast(format!("refreshed ({} vars)", app.rows().len()));
                        }
                        Err(e) => app.set_error_toast(e.to_string()),
                    }
                }
            }
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
