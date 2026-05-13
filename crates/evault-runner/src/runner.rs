//! [`StdProcessRunner`] — spawns a child via [`std::process::Command`]
//! with the parent stdio inherited and an injected env overlay.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use evault_core::error::RunnerError;
use evault_core::model::Var;
use evault_core::traits::{ProcessOutcome, ProcessRunner};

/// Variables whose name starts with this prefix are stripped from the
/// parent environment before spawning the child. Internal evault
/// runtime configuration must not bleed into untrusted child processes.
pub const EVAULT_ENV_PREFIX: &str = "EVAULT_";

/// Production [`ProcessRunner`] implementation backed by
/// [`std::process::Command`].
#[derive(Debug, Default, Clone, Copy)]
pub struct StdProcessRunner;

impl StdProcessRunner {
    /// Construct a fresh `StdProcessRunner`. The struct is stateless.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ProcessRunner for StdProcessRunner {
    fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: &Path,
        env: &BTreeMap<String, String>,
    ) -> Result<ProcessOutcome, RunnerError> {
        // Validate every key/value BEFORE touching `Command`. A rejected
        // entry never reaches the spawn syscall.
        validate_env(env)?;

        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.current_dir(cwd);

        // Strip every parent variable whose name starts with EVAULT_.
        // We do NOT call `env_clear()` — the user wants PATH and similar
        // to propagate. Iterating `std::env::vars_os` gives the parent's
        // view of the environment as it was when this Rust process started.
        //
        // Windows env var names are case-insensitive: `evault_master` and
        // `EVAULT_MASTER` refer to the same variable. On Unix they are
        // case-sensitive. The prefix match below is platform-aware so the
        // strip honours the trait contract on every host.
        for (key, _) in std::env::vars_os() {
            if matches_evault_prefix(&key) {
                cmd.env_remove(&key);
            }
        }

        // Apply the overlay last so it wins over any non-stripped parent
        // variable with the same name.
        for (k, v) in env {
            cmd.env(k, v);
        }

        // Stdin/stdout/stderr are inherited by default; we deliberately do
        // not override them.

        let status = cmd.status().map_err(|e| {
            let kind = e.kind();
            if kind == std::io::ErrorKind::NotFound {
                RunnerError::Spawn(format!("program not found: {program:?}"))
            } else {
                // Include the io::Error Display so OS-level diagnostic
                // context survives (e.g. "permission denied (os error 5)"
                // on Windows). No secret material is in scope here:
                // io::Error Display for OS errors is bounded to the
                // syscall name and the errno description.
                RunnerError::Spawn(format!("spawn: {kind:?}: {e}"))
            }
        })?;
        Ok(ProcessOutcome {
            exit_code: status.code(),
        })
    }
}

/// Returns `true` if `key` starts with the `EVAULT_` prefix under the
/// host platform's env-var name semantics.
///
/// - Unix: case-sensitive prefix match.
/// - Windows: case-insensitive prefix match. The OS treats env var names
///   as case-insensitive, so a parent var named `evault_master` is the
///   same variable as `EVAULT_MASTER` and must be stripped too.
///
/// Non-UTF-8 OS strings (rare on every supported platform) are skipped
/// — they cannot collide with the ASCII `EVAULT_` prefix.
fn matches_evault_prefix(key: &std::ffi::OsStr) -> bool {
    let Some(s) = key.to_str() else {
        return false;
    };
    if s.len() < EVAULT_ENV_PREFIX.len() {
        return false;
    }
    #[cfg(windows)]
    {
        s[..EVAULT_ENV_PREFIX.len()].eq_ignore_ascii_case(EVAULT_ENV_PREFIX)
    }
    #[cfg(not(windows))]
    {
        s.starts_with(EVAULT_ENV_PREFIX)
    }
}

/// Validate every key and value before they reach the OS env block.
///
/// Keys must satisfy the variable-name rules; values must not contain a
/// NUL byte. Errors carry the key (not secret per evault's design) and
/// a stable category label, never the surrounding value text.
fn validate_env(env: &BTreeMap<String, String>) -> Result<(), RunnerError> {
    for (key, value) in env {
        Var::validate_name(key)
            .map_err(|_| RunnerError::Invalid(format!("invalid env key: {key:?}")))?;
        if value.contains('\0') {
            return Err(RunnerError::Invalid(format!(
                "value for env key {key:?} contains a NUL byte; \
                 NUL terminates the OS environment block prematurely"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(k: &str, v: &str) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert(k.to_owned(), v.to_owned());
        m
    }

    #[test]
    fn validate_env_accepts_empty_overlay() {
        assert!(validate_env(&BTreeMap::new()).is_ok());
    }

    #[test]
    fn validate_env_accepts_valid_entries() {
        let mut env = BTreeMap::new();
        env.insert("DATABASE_URL".into(), "postgres://localhost".into());
        env.insert("NODE_ENV".into(), "production".into());
        assert!(validate_env(&env).is_ok());
    }

    #[test]
    fn validate_env_rejects_invalid_key_name() {
        match validate_env(&pair("with space", "v")) {
            Err(RunnerError::Invalid(msg)) => assert!(msg.contains("invalid env key")),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn validate_env_rejects_value_with_nul_byte() {
        match validate_env(&pair("TOKEN", "abc\0xyz")) {
            Err(RunnerError::Invalid(msg)) => {
                assert!(msg.contains("NUL byte"));
                // CRITICAL: no part of the value text should appear in
                // the error string — would leak secret material.
                assert!(!msg.contains("xyz"), "value text leaked: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn matches_evault_prefix_uppercase() {
        assert!(matches_evault_prefix(std::ffi::OsStr::new("EVAULT_MASTER")));
        assert!(matches_evault_prefix(std::ffi::OsStr::new("EVAULT_")));
    }

    #[test]
    fn matches_evault_prefix_rejects_non_prefix() {
        assert!(!matches_evault_prefix(std::ffi::OsStr::new("PATH")));
        assert!(!matches_evault_prefix(std::ffi::OsStr::new("VAULT_FOO")));
        // Shorter than the prefix.
        assert!(!matches_evault_prefix(std::ffi::OsStr::new("EVAULT")));
    }

    #[cfg(windows)]
    #[test]
    fn matches_evault_prefix_is_case_insensitive_on_windows() {
        // Windows env var names are case-insensitive: any casing of the
        // prefix must be detected and stripped.
        assert!(matches_evault_prefix(std::ffi::OsStr::new("evault_master")));
        assert!(matches_evault_prefix(std::ffi::OsStr::new("Evault_Master")));
        assert!(matches_evault_prefix(std::ffi::OsStr::new("eVaUlT_X")));
    }

    #[cfg(not(windows))]
    #[test]
    fn matches_evault_prefix_is_case_sensitive_on_unix() {
        // POSIX env var names are case-sensitive. Lower-case must NOT
        // match — a parent that intentionally has `evault_master`
        // (distinct from `EVAULT_MASTER`) is left alone.
        assert!(!matches_evault_prefix(std::ffi::OsStr::new(
            "evault_master"
        )));
        assert!(!matches_evault_prefix(std::ffi::OsStr::new(
            "Evault_Master"
        )));
    }
}
