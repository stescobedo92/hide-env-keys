//! Error type returned by the binary's top-level dispatch.
//!
//! Splitting "subcommand not implemented" from "TUI crashed" lets
//! `main` exit with distinct codes so shell scripts can tell the two
//! apart — POSIX convention is `2` for misuse / unimplemented features
//! and `1` for actual runtime failures.

use thiserror::Error;

use evault_tui::TuiError;

use crate::backend::SqlCipherOpenError;

/// Top-level CLI error.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum CliError {
    /// A subcommand was recognised by the parser but not yet wired.
    /// Reserved for Batch D's still-stubbed subcommands (`link`,
    /// `gen`, `run`, `scan`, `import`, `export`); `ls` / `add` / `rm`
    /// landed in Batch B.
    #[allow(dead_code)]
    #[error("'{name}' is not yet implemented in this build")]
    SubcommandUnimplemented {
        /// Subcommand the user invoked (verbatim — `ls`, `add`, `rm`,
        /// …). Surfaced in the error message so users know which one
        /// is unavailable.
        name: String,
    },
    /// The TUI returned an error. The inner [`TuiError`] is exposed
    /// for top-level chain walking so the user sees the underlying
    /// cause (terminal I/O, provider failure) rather than just
    /// "TUI error".
    #[error("TUI error")]
    Tui(#[from] TuiError),
    /// The persistent backend could not be opened. The inner error
    /// carries the chain walker through its `source()` implementations.
    #[error("backend open error")]
    BackendOpen(#[from] SqlCipherOpenError),
    /// Generic I/O failure (terminal prompt, stdout, stdin) from a
    /// non-TUI subcommand path.
    #[error("io error")]
    Io(#[from] std::io::Error),
}

/// Format an error and its full `source()` chain into a single line
/// of the form `outer: middle: innermost`. Each layer's `Display` is
/// joined with `: `. Stops at the first link without a source.
///
/// Lets the top-level `main` surface forensic detail without pulling
/// in a structured logging dependency. Phase 3 will replace this
/// with `tracing::error!` and structured events.
#[must_use]
pub fn format_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut msg = err.to_string();
    let mut source = err.source();
    while let Some(inner) = source {
        msg.push_str(": ");
        msg.push_str(&inner.to_string());
        source = inner.source();
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic error with a chain so we can exercise `format_chain`
    /// without depending on whatever inner errors `TuiError` happens
    /// to expose today.
    #[derive(Debug)]
    struct Outer(Inner);
    #[derive(Debug)]
    struct Inner(Bottom);
    #[derive(Debug)]
    struct Bottom;

    impl std::fmt::Display for Outer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("outer")
        }
    }
    impl std::fmt::Display for Inner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("inner")
        }
    }
    impl std::fmt::Display for Bottom {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bottom")
        }
    }
    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }
    impl std::error::Error for Inner {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }
    impl std::error::Error for Bottom {}

    #[test]
    fn format_chain_walks_all_levels() {
        let err = Outer(Inner(Bottom));
        assert_eq!(format_chain(&err), "outer: inner: bottom");
    }

    #[test]
    fn format_chain_on_leaf_error_returns_only_message() {
        assert_eq!(format_chain(&Bottom), "bottom");
    }

    #[test]
    fn subcommand_unimplemented_carries_the_name() {
        let e = CliError::SubcommandUnimplemented { name: "ls".into() };
        assert!(e.to_string().contains("'ls'"));
    }
}
