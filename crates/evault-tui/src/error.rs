//! Error type surfaced by the TUI runtime.

use thiserror::Error;

use crate::provider::ProviderError;

/// Top-level error type returned by [`crate::run_tui`].
///
/// Wraps the two failure surfaces of the runtime: terminal I/O
/// (raw-mode toggling, drawing, event reading) and the caller-supplied
/// [`crate::VarProvider`].
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum TuiError {
    /// Terminal I/O failed — raw-mode toggling, drawing to the
    /// framebuffer, or reading an event.
    #[error("terminal io error: {0}")]
    Terminal(#[from] std::io::Error),

    /// The data provider returned an error while the dashboard was
    /// refreshing.
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
}
