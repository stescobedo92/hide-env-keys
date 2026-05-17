//! Small presentation helpers for human CLI output.
//!
//! The helpers emit ANSI color only when stdout is a terminal and `NO_COLOR`
//! is not set. Data-oriented commands such as `export` can keep printing raw
//! machine-readable content by not using this module.

use std::io::IsTerminal;

#[derive(Clone, Copy)]
enum Tone {
    Success,
    Warning,
    Accent,
    Muted,
}

/// Human-friendly success line.
#[must_use]
pub fn success(message: impl AsRef<str>) -> String {
    format!("{} {}", paint(Tone::Success, "OK"), message.as_ref())
}

/// Human-friendly warning line.
#[must_use]
pub fn warning(message: impl AsRef<str>) -> String {
    format!("{} {}", paint(Tone::Warning, "WARN"), message.as_ref())
}

/// Accent text for table headers and labels.
#[must_use]
pub fn accent(text: impl AsRef<str>) -> String {
    paint(Tone::Accent, text.as_ref())
}

/// Muted text for secondary context.
#[must_use]
pub fn muted(text: impl AsRef<str>) -> String {
    paint(Tone::Muted, text.as_ref())
}

fn paint(tone: Tone, text: &str) -> String {
    if !should_color() {
        return text.to_owned();
    }
    let code = match tone {
        Tone::Success => "32;1",
        Tone::Warning => "33;1",
        Tone::Accent => "36;1",
        Tone::Muted => "2",
    };
    format!("\x1b[{code}m{text}\x1b[0m")
}

fn should_color() -> bool {
    should_color_from(
        std::env::var_os("NO_COLOR").is_none(),
        std::io::stdout().is_terminal(),
    )
}

const fn should_color_from(no_color_absent: bool, stdout_is_terminal: bool) -> bool {
    no_color_absent && stdout_is_terminal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_requires_terminal_and_no_no_color_env() {
        assert!(should_color_from(true, true));
        assert!(!should_color_from(false, true));
        assert!(!should_color_from(true, false));
    }
}
