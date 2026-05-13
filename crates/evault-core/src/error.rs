//! Error types for `evault-core`.
//!
//! Each subsystem has its own [`thiserror`]-derived enum so that callers can
//! match precisely on what went wrong. [`CoreError`] is a convenience
//! aggregator for code that consumes several subsystems through a single
//! result type.
//!
//! All error types are `#[non_exhaustive]` so that adding new variants is not
//! a breaking change for downstream crates that pattern-match on them.

use std::path::PathBuf;

use thiserror::Error;

use crate::model::{ProjectId, VarId, VarKind};

/// Errors that can occur while interacting with a
/// [`MetadataStore`](crate::traits::MetadataStore).
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum MetadataError {
    /// A variable with the given name already exists in the registry.
    #[error("variable named {0:?} already exists")]
    DuplicateName(String),

    /// No variable with the given identifier was found.
    #[error("variable {0} not found")]
    VarNotFound(VarId),

    /// No project with the given identifier was found.
    #[error("project {0} not found")]
    ProjectNotFound(ProjectId),

    /// A value supplied to the metadata layer failed a domain invariant.
    #[error("invalid input: {0}")]
    Invalid(String),

    /// An operation that requires a specific [`VarKind`] (e.g. storing a
    /// plain value) was invoked on a variable of the wrong kind.
    ///
    /// This guards against secrets accidentally flowing into the plain-value
    /// side table — every backend must enforce this rule, never silently
    /// accept a kind mismatch.
    #[error("variable {id} has kind {actual:?} but {expected:?} was required")]
    KindMismatch {
        /// Identifier of the offending variable.
        id: VarId,
        /// The kind the operation expected.
        expected: VarKind,
        /// The kind the variable actually has.
        actual: VarKind,
    },

    /// The underlying storage backend reported an unexpected failure.
    ///
    /// **Backend implementors MUST NOT** include secret material, variable
    /// values, or absolute paths that disclose the user's home directory in
    /// this string. Wrap raw backend errors with a category label (e.g.
    /// `"sqlite write failed"`) rather than `format!("{e}")` that may
    /// propagate parameter values from a failed query.
    #[error("backend error: {0}")]
    Backend(String),
}

/// Errors that can occur while interacting with a
/// [`SecretStore`](crate::traits::SecretStore).
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum SecretError {
    /// The underlying OS keyring or fallback backend rejected the operation.
    ///
    /// **Backend implementors MUST NOT** include the secret value, the
    /// keyring service/account names containing user paths, or any data that
    /// could disclose the secret in this string.
    #[error("secret backend error: {0}")]
    Backend(String),

    /// The platform does not provide secure storage and no fallback was
    /// configured by the caller.
    #[error("no secure storage available on this platform")]
    Unavailable,
}

/// Errors that can occur while loading or saving an `evault.toml` manifest.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum ManifestError {
    /// The manifest file does not exist at the supplied path.
    ///
    /// Note: the path is the exact string the caller supplied and may
    /// disclose the user's home directory if the resulting error is shipped
    /// off-host. Strip or redact before forwarding to remote sinks.
    #[error("manifest path does not exist: {0}")]
    NotFound(PathBuf),

    /// The manifest contents could not be parsed.
    ///
    /// **Backend implementors MUST NOT** include rendered manifest content,
    /// the inline values of `BindingSource::Inline` bindings, or absolute
    /// paths that disclose the user's home directory in this string. Quote
    /// only the structural error (expected token, line/column) — never the
    /// surrounding source text.
    #[error("manifest parse failed: {0}")]
    Parse(String),

    /// The manifest could not be written back to disk.
    ///
    /// **Backend implementors MUST NOT** include rendered manifest content,
    /// the inline values being serialized, or absolute paths that disclose
    /// the user's home directory in this string.
    #[error("manifest write failed: {0}")]
    Write(String),

    /// The manifest parsed but violated a structural rule (e.g. duplicate var).
    ///
    /// **Backend implementors MUST NOT** include rendered inline values or
    /// secret material in this string. Quote variable names by all means;
    /// never their values.
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

/// Errors that can occur while materializing a `.env` file from a manifest.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum MaterializerError {
    /// I/O error while reading or writing files.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The manifest referenced a variable that is missing from the registry.
    #[error("variable referenced by manifest not found in registry: {0}")]
    MissingVar(String),

    /// The materializer backend reported an unexpected failure.
    ///
    /// **Backend implementors MUST NOT** include the rendered values, the
    /// secret material, or absolute path components in this string.
    #[error("materializer backend error: {0}")]
    Backend(String),
}

/// Errors that can occur while running a child process with an injected
/// environment.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum RunnerError {
    /// Spawning the child process failed.
    #[error("failed to spawn process: {0}")]
    Spawn(String),

    /// The child exited with a non-zero status code.
    #[error("process exited with non-zero status: {0}")]
    NonZeroExit(i32),

    /// I/O error while interacting with the child process.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors that can occur while scanning source code for environment-variable
/// references.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum ScannerError {
    /// I/O error while walking the file tree.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// One of the configured patterns failed to compile.
    #[error("scanner pattern error: {0}")]
    Pattern(String),
}

/// Convenience aggregator for code that consumes multiple subsystems through a
/// single result type.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum CoreError {
    /// See [`MetadataError`].
    #[error(transparent)]
    Metadata(#[from] MetadataError),

    /// See [`SecretError`].
    #[error(transparent)]
    Secret(#[from] SecretError),

    /// See [`ManifestError`].
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// See [`MaterializerError`].
    #[error(transparent)]
    Materializer(#[from] MaterializerError),

    /// See [`RunnerError`].
    #[error(transparent)]
    Runner(#[from] RunnerError),

    /// See [`ScannerError`].
    #[error(transparent)]
    Scanner(#[from] ScannerError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_name_message_contains_name() {
        let e = MetadataError::DuplicateName("API_KEY".to_owned());
        assert!(e.to_string().contains("API_KEY"));
    }

    #[test]
    fn core_error_from_metadata_via_question_mark() {
        fn inner() -> Result<(), MetadataError> {
            Err(MetadataError::Invalid("bad".into()))
        }
        fn outer() -> Result<(), CoreError> {
            inner()?;
            Ok(())
        }
        let err = outer().expect_err("expected error");
        assert!(matches!(err, CoreError::Metadata(_)));
    }

    #[test]
    fn io_error_converts_into_materializer_error() {
        let io = std::io::Error::other("disk full");
        let me: MaterializerError = io.into();
        assert!(me.to_string().contains("disk full"));
    }
}
