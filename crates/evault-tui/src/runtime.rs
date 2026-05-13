//! Terminal lifecycle and event loop.

use std::time::Duration;

use ratatui::crossterm::event::{self, Event};
use ratatui::DefaultTerminal;

use crate::app::AppState;
use crate::error::TuiError;
use crate::event::Action;
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
/// # Errors
/// Returns [`TuiError::Terminal`] if terminal I/O fails (raw-mode
/// toggling, drawing, event reading). Returns [`TuiError::Provider`]
/// only if the *initial* refresh fails; subsequent refresh errors are
/// surfaced as a toast and the loop continues.
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
    let result = event_loop(&mut terminal, &provider);
    // `try_restore` returns its own io::Error; if the loop also failed
    // we keep the loop error and surface the restore error through
    // ratatui's default panic-hook reporter on stderr (which `restore`
    // already does internally).
    if let Err(restore_err) = ratatui::try_restore() {
        return match result {
            Ok(()) => Err(TuiError::Terminal(restore_err)),
            Err(prior) => Err(prior),
        };
    }
    result
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

        if event::poll(POLL_INTERVAL)? {
            if let Event::Key(key) = event::read()? {
                let action = Action::from_key(key);
                app.apply(action);
                if matches!(action, Action::Refresh) {
                    // For Refresh we re-fetch immediately; runtime
                    // failures become a toast so the user can keep
                    // working from the last-known-good buffer.
                    if let Err(e) = app.refresh(provider) {
                        app.set_error_toast(e.to_string());
                    }
                }
            }
        }
    }

    Ok(())
}
