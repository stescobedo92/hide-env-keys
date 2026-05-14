//! Translation from raw [`KeyEvent`] to high-level [`Action`].

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// High-level UI intent.
///
/// `Action` decouples view code from crossterm internals. Every
/// keybinding lives in exactly one place ([`Action::from_key`]) so the
/// keymap is auditable and testable without a terminal.
///
/// Unknown / unbound keys translate to [`Action::Noop`] so callers can
/// route them to view-local handlers (e.g. a text-input widget).
///
/// **Help overlay parity:** every binding here must have a matching
/// row in `crate::views::help`. The two tables are maintained by
/// hand; keep them in sync when adding or renaming keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Request the program to exit immediately.
    Quit,
    /// Close the topmost overlay, or quit if none is open.
    Dismiss,
    /// Move selection up by one row.
    MoveUp,
    /// Move selection down by one row.
    MoveDown,
    /// Jump to the first row.
    MoveTop,
    /// Jump to the last row.
    MoveBottom,
    /// Scroll one viewport up.
    PageUp,
    /// Scroll one viewport down.
    PageDown,
    /// Open the detail view for the selected row.
    OpenDetail,
    /// Start the "new variable" flow.
    NewVar,
    /// Edit the selected variable.
    EditVar,
    /// Delete the selected variable (with confirm).
    DeleteVar,
    /// Link the selected variable to a project.
    LinkVar,
    /// Copy the selected variable's value to the clipboard.
    CopyValue,
    /// Reveal the selected variable's value in a centered modal.
    ViewValue,
    /// Toggle showing / masking secret values.
    ToggleSecretVisibility,
    /// Open the run-in-project form.
    RunInProject,
    /// Open the fuzzy-search overlay.
    StartFuzzy,
    /// Switch the active profile.
    SwitchProfile,
    /// Move to the next top-level view.
    NextView,
    /// Toggle the help overlay.
    ToggleHelp,
    /// Re-read data from the provider.
    Refresh,
    /// Unbound / unrecognised key.
    Noop,
}

impl Action {
    /// Translate a [`KeyEvent`] into the corresponding [`Action`].
    ///
    /// Filters out non-`Press` events so Windows — which reports
    /// `Press` *and* `Release` for every key — does not fire each
    /// action twice. Keys with no binding return [`Action::Noop`].
    ///
    /// # Examples
    /// ```
    /// use evault_tui::Action;
    /// use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    ///
    /// let press = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    /// assert_eq!(Action::from_key(press), Action::Quit);
    /// ```
    #[must_use]
    pub fn from_key(key: KeyEvent) -> Self {
        if key.kind != KeyEventKind::Press {
            return Self::Noop;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            // Ctrl-C is the universal "I want out" gesture. Binding
            // it to Quit also prevents the parent shell's SIGINT
            // handler from delivering the signal in scenarios where
            // crossterm has not captured Ctrl-C — which would leave
            // raw mode enabled and corrupt the terminal.
            KeyCode::Char('c') if ctrl => Self::Quit,
            KeyCode::Char('q') => Self::Quit,
            KeyCode::Esc => Self::Dismiss,
            KeyCode::Char('j') | KeyCode::Down => Self::MoveDown,
            KeyCode::Char('k') | KeyCode::Up => Self::MoveUp,
            KeyCode::Char('g') => Self::MoveTop,
            KeyCode::Char('G') => Self::MoveBottom,
            KeyCode::PageDown => Self::PageDown,
            KeyCode::PageUp => Self::PageUp,
            KeyCode::Enter => Self::OpenDetail,
            KeyCode::Char('n') => Self::NewVar,
            KeyCode::Char('e') => Self::EditVar,
            KeyCode::Char('d') => Self::DeleteVar,
            KeyCode::Char('l') => Self::LinkVar,
            KeyCode::Char('y') => Self::CopyValue,
            KeyCode::Char('v') => Self::ViewValue,
            KeyCode::Char('s') => Self::ToggleSecretVisibility,
            // Shift-R = "Run in project" (lower-case `r` is bound to
            // Refresh; capitalising it follows the `g`/`G` precedent
            // for related-but-distinct actions).
            KeyCode::Char('R') => Self::RunInProject,
            KeyCode::Char('f') if ctrl => Self::StartFuzzy,
            KeyCode::Char('p') => Self::SwitchProfile,
            KeyCode::Tab => Self::NextView,
            KeyCode::Char('?') => Self::ToggleHelp,
            KeyCode::Char('r') => Self::Refresh,
            _ => Self::Noop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn release(code: KeyCode) -> KeyEvent {
        let mut e = KeyEvent::new(code, KeyModifiers::NONE);
        e.kind = KeyEventKind::Release;
        e
    }

    #[test]
    fn release_events_are_dropped() {
        // Critical Windows correctness: release events must not fire
        // any action, otherwise every keypress fires twice.
        assert_eq!(Action::from_key(release(KeyCode::Char('q'))), Action::Noop);
        assert_eq!(Action::from_key(release(KeyCode::Down)), Action::Noop);
    }

    #[test]
    fn basic_navigation_keys() {
        assert_eq!(
            Action::from_key(press(KeyCode::Char('j'))),
            Action::MoveDown
        );
        assert_eq!(Action::from_key(press(KeyCode::Down)), Action::MoveDown);
        assert_eq!(Action::from_key(press(KeyCode::Char('k'))), Action::MoveUp);
        assert_eq!(Action::from_key(press(KeyCode::Up)), Action::MoveUp);
        assert_eq!(Action::from_key(press(KeyCode::Char('g'))), Action::MoveTop);
        assert_eq!(
            Action::from_key(press(KeyCode::Char('G'))),
            Action::MoveBottom
        );
    }

    #[test]
    fn quit_and_dismiss_are_distinct() {
        assert_eq!(Action::from_key(press(KeyCode::Char('q'))), Action::Quit);
        assert_eq!(Action::from_key(press(KeyCode::Esc)), Action::Dismiss);
    }

    #[test]
    fn ctrl_f_starts_fuzzy_search() {
        let ctrl_f = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL);
        assert_eq!(Action::from_key(ctrl_f), Action::StartFuzzy);
        // Plain 'f' without ctrl is unbound.
        assert_eq!(Action::from_key(press(KeyCode::Char('f'))), Action::Noop);
    }

    #[test]
    fn ctrl_c_quits() {
        // Universal "I want out" gesture. Must always quit cleanly
        // through the regular state path so raw mode and the
        // alternate screen are properly torn down.
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(Action::from_key(ctrl_c), Action::Quit);
        // Plain 'c' is still unbound.
        assert_eq!(Action::from_key(press(KeyCode::Char('c'))), Action::Noop);
    }

    #[test]
    fn shift_r_is_run_in_project_lowercase_r_is_refresh() {
        // The two are deliberately separate actions: `r` refreshes
        // the dashboard, `R` launches a child process with the
        // project's env overlay.
        assert_eq!(Action::from_key(press(KeyCode::Char('r'))), Action::Refresh);
        assert_eq!(
            Action::from_key(press(KeyCode::Char('R'))),
            Action::RunInProject
        );
    }

    #[test]
    fn unknown_key_is_noop() {
        assert_eq!(Action::from_key(press(KeyCode::Char('z'))), Action::Noop);
        assert_eq!(Action::from_key(press(KeyCode::F(7))), Action::Noop);
    }
}
