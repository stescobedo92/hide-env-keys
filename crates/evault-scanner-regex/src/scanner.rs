//! [`RegexCodeScanner`] — walks a tree and applies per-language regex
//! patterns to every source file.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use evault_core::error::ScannerError;
use evault_core::traits::{CodeScanner, ScanHit};

use crate::patterns::{language_specs, LanguageSpec};

/// Directory names that the walker prunes wholesale.
///
/// These are the canonical build / cache / vendor / VCS directories.
/// Stepping into them would produce thousands of hits in generated
/// code, vendored dependencies, or compiled binaries — none of which
/// reflect what the *user* wrote.
pub const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    "dist",
    "build",
    "out",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    ".idea",
    ".vscode",
];

/// Regex-based [`CodeScanner`].
///
/// Patterns are compiled lazily at construction. The scanner is
/// stateless beyond the compiled patterns, so it is safe to share
/// across threads (`Send + Sync` via the trait bound).
pub struct RegexCodeScanner {
    /// Extension → compiled patterns. We index by lowercased extension
    /// so the per-file lookup is O(1).
    by_extension: HashMap<&'static str, Vec<regex::Regex>>,
}

impl std::fmt::Debug for RegexCodeScanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegexCodeScanner").finish_non_exhaustive()
    }
}

impl Default for RegexCodeScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl RegexCodeScanner {
    /// Construct a scanner with the built-in pattern bank.
    #[must_use]
    pub fn new() -> Self {
        let specs = language_specs();
        let mut by_extension: HashMap<&'static str, Vec<regex::Regex>> = HashMap::new();
        for LanguageSpec {
            extensions,
            patterns,
        } in specs
        {
            for ext in extensions {
                by_extension
                    .entry(ext)
                    .or_default()
                    .extend(patterns.clone());
            }
        }
        Self { by_extension }
    }
}

impl CodeScanner for RegexCodeScanner {
    fn scan(&self, root: &Path) -> Result<Vec<ScanHit>, ScannerError> {
        let mut hits = Vec::new();
        let walker = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_skipped(e.file_name().to_str()));
        for entry in walker {
            let entry = entry.map_err(|e| ScannerError::Io(e.into()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let Some(ext) = path
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase)
            else {
                continue;
            };
            let Some(patterns) = self.by_extension.get(ext.as_str()) else {
                continue;
            };
            scan_file(path, patterns, &mut hits)?;
        }
        Ok(hits)
    }
}

fn is_skipped(name: Option<&str>) -> bool {
    name.is_some_and(|s| SKIPPED_DIRS.contains(&s))
}

fn scan_file(
    path: &Path,
    patterns: &[regex::Regex],
    hits: &mut Vec<ScanHit>,
) -> Result<(), ScannerError> {
    let text = match fs::read_to_string(path) {
        Ok(s) => s,
        // Per the trait contract: silently skip files whose contents
        // are not valid UTF-8 (binaries, latin-1 sources, etc.).
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
        Err(e) => return Err(ScannerError::Io(e)),
    };
    for (line_idx, line) in text.lines().enumerate() {
        let line_number = line_idx + 1;
        for pattern in patterns {
            for caps in pattern.captures_iter(line) {
                if let Some(name_match) = caps.name("name") {
                    hits.push(ScanHit {
                        path: PathBuf::from(path),
                        line: line_number,
                        // `start()` returns a byte offset on the line.
                        // We add 1 to match the conventional 1-based
                        // column number that editors use.
                        column: name_match.start() + 1,
                        name: name_match.as_str().to_owned(),
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_skipped_recognizes_common_dirs() {
        assert!(is_skipped(Some("target")));
        assert!(is_skipped(Some("node_modules")));
        assert!(is_skipped(Some(".git")));
        assert!(!is_skipped(Some("src")));
        assert!(!is_skipped(None));
    }

    #[test]
    fn extension_index_covers_every_language() {
        let scanner = RegexCodeScanner::new();
        for ext in ["js", "ts", "py", "rs", "go", "sh", "bash"] {
            assert!(
                scanner.by_extension.contains_key(ext),
                "missing extension index for {ext}"
            );
        }
    }
}
