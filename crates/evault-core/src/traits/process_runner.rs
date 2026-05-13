//! [`ProcessRunner`] — execute a child process with a custom environment.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::RunnerError;

/// Result of running a child process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutcome {
    /// Exit status reported by the OS. `None` if the process was terminated
    /// by a signal (Unix).
    pub exit_code: Option<i32>,
}

impl ProcessOutcome {
    /// Returns `true` when the process exited normally with code `0`.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self.exit_code, Some(0))
    }
}

/// Spawn a child process with an injected environment.
///
/// The intent is `evault run --project X -- npm start`: the runner takes the
/// resolved environment, spawns `npm start`, waits for it to exit, and
/// propagates the exit code. The environment is **not** materialized to disk.
pub trait ProcessRunner: Send + Sync {
    /// Run `program` with the given `args`, working directory `cwd`, and an
    /// extra environment overlay `env`.
    ///
    /// The overlay is **added to** (not replacing) the parent process's
    /// environment so that `PATH` and similar variables remain available to
    /// the child. Keys in `env` override identically-named keys from the
    /// parent.
    ///
    /// **Implementors MUST** strip variables matching the `EVAULT_*` prefix
    /// from the parent environment before applying the overlay, so that
    /// internal runtime configuration (master key paths, telemetry knobs)
    /// does not leak into untrusted child processes.
    ///
    /// **Implementors MUST** validate every supplied key (same shape as a
    /// variable name) and reject values containing a NUL byte before
    /// touching `std::process::Command` — a NUL would terminate the OS
    /// environment block early and is non-recoverable.
    ///
    /// # Errors
    /// Returns [`RunnerError::Invalid`] when the env overlay contains a
    /// malformed key or a value with a NUL byte;
    /// [`RunnerError::Spawn`] if the process could not start;
    /// [`RunnerError::Io`] for IO failure during execution.
    fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: &Path,
        env: &BTreeMap<String, String>,
    ) -> Result<ProcessOutcome, RunnerError>;
}
