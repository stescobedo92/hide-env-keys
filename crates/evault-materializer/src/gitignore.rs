//! Append the materialized basename to a sibling `.gitignore` if it
//! isn't already listed.
//!
//! This guards against the common accident of `git add .` picking up a
//! freshly-generated `.env`. The function is idempotent: re-running it
//! after the entry exists does nothing.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// Ensure that `parent/.gitignore` contains an entry matching
/// `materialized.file_name()`. Creates `.gitignore` if needed.
///
/// # Errors
/// Propagates I/O failures. The caller surfaces these as
/// [`MaterializerError::Backend`](evault_core::error::MaterializerError::Backend)
/// without including the path or OS message.
pub fn ensure_gitignore_entry(parent: &Path, materialized: &Path) -> io::Result<()> {
    let Some(basename) = materialized.file_name().and_then(|s| s.to_str()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "materialized path has no UTF-8 filename",
        ));
    };
    // Always store the leading-slash form so the pattern only matches
    // the file at the project root, not any matching basename in a
    // subdirectory. Git treats both `/foo` and `foo` the same in a
    // top-level `.gitignore`, but the leading slash is the explicit form.
    let pattern = format!("/{basename}");

    let gitignore_path = parent.join(".gitignore");
    let existing = match fs::read_to_string(&gitignore_path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };

    if has_pattern(&existing, &pattern) || has_pattern(&existing, basename) {
        return Ok(());
    }

    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&gitignore_path)?;
    // If the existing file doesn't end in a newline, prepend one so the
    // appended pattern starts on its own line. Empty file: no leading
    // newline needed.
    let needs_leading_newline = !existing.is_empty() && !existing.ends_with('\n');
    if needs_leading_newline {
        file.write_all(b"\n")?;
    }
    writeln!(file, "# Added by evault — generated env file.")?;
    writeln!(file, "{pattern}")?;
    Ok(())
}

/// Returns `true` if `text` contains `pattern` on its own line (ignoring
/// leading/trailing whitespace), and the line is not a comment.
fn has_pattern(text: &str, pattern: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return false;
        }
        trimmed == pattern
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_pattern_matches_exact_line() {
        assert!(has_pattern("a\n/.env\nb\n", "/.env"));
        assert!(has_pattern("/.env", "/.env"));
        assert!(!has_pattern("# /.env\n", "/.env"));
        assert!(!has_pattern("env\n", "/.env"));
    }
}
