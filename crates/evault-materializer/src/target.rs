//! Target path helpers for materialized `.env` files.

use std::path::{Path, PathBuf};

use evault_core::error::MaterializerError;

/// Build the materialized env file path for a project directory.
///
/// `environment = None` keeps the historical `<project>/.env` target.
/// `Some("Production")` yields `<project>/.env.Production` after validating
/// that the suffix is a safe filename segment.
///
/// # Errors
/// Returns [`MaterializerError::Backend`] when the environment label is empty
/// or unsafe for a single filename segment.
pub fn env_file_path(
    project_path: &Path,
    environment: Option<&str>,
) -> Result<PathBuf, MaterializerError> {
    let file_name = environment.map_or_else(
        || Ok(String::from(".env")),
        |label| validate_environment_label(label).map(|safe| format!(".env.{safe}")),
    )?;
    Ok(project_path.join(file_name))
}

/// Validate a user-supplied environment label.
///
/// The returned value is trimmed but otherwise preserves casing so
/// `.env.Production`, `.env.testing`, and custom labels all round-trip in the
/// shape the user requested.
///
/// # Errors
/// Returns [`MaterializerError::Backend`] with a non-secret validation message
/// if the label is empty or contains characters that could escape the project
/// directory or create awkward cross-platform filenames.
pub fn validate_environment_label(label: &str) -> Result<String, MaterializerError> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err(MaterializerError::Backend(
            "environment label is empty".into(),
        ));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(MaterializerError::Backend(
            "environment label cannot be a dot segment".into(),
        ));
    }
    if trimmed.len() > 64 {
        return Err(MaterializerError::Backend(
            "environment label longer than 64 characters".into(),
        ));
    }
    for (offset, c) in trimmed.char_indices() {
        let valid = c.is_ascii_alphanumeric() || matches!(c, '_' | '-');
        if !valid {
            let reason = if c == '/' || c == '\\' {
                "path separator"
            } else if c == ':' {
                "drive or stream separator"
            } else if c.is_control() {
                "control character"
            } else {
                "unsupported character"
            };
            return Err(MaterializerError::Backend(format!(
                "environment label contains a {reason} at byte offset {offset}"
            )));
        }
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_file_path_without_environment_targets_dotenv() {
        assert_eq!(
            env_file_path(Path::new("app"), None).unwrap(),
            Path::new("app").join(".env")
        );
    }

    #[test]
    fn env_file_path_accepts_builtin_and_custom_labels() {
        assert_eq!(
            env_file_path(Path::new("app"), Some("Production")).unwrap(),
            Path::new("app").join(".env.Production")
        );
        assert_eq!(
            env_file_path(Path::new("app"), Some("qa-preview_1")).unwrap(),
            Path::new("app").join(".env.qa-preview_1")
        );
    }

    #[test]
    fn validate_environment_label_trims_and_preserves_case() {
        assert_eq!(
            validate_environment_label("  testing  ").unwrap(),
            "testing"
        );
    }

    #[test]
    fn validate_environment_label_rejects_unsafe_segments() {
        for label in ["", "   ", ".", "..", "prod/local", "C:prod", "bad\\path"] {
            assert!(
                validate_environment_label(label).is_err(),
                "expected {label:?} to be rejected"
            );
        }
    }
}
