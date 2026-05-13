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
///
/// The list is intentionally broad (covers Node, Python, Rust, Go,
/// JVM, .NET, Terraform, AWS CDK, and JS framework caches) but is not
/// exhaustive: callers with project-specific generated directories
/// should layer additional filtering on top of [`RegexCodeScanner`].
pub const SKIPPED_DIRS: &[&str] = &[
    // VCS.
    ".git",
    ".hg",
    ".svn",
    // Rust.
    "target",
    // Node / JS.
    "node_modules",
    "bower_components",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".parcel-cache",
    ".cache",
    ".turbo",
    "coverage",
    ".nyc_output",
    // Python.
    "__pycache__",
    ".venv",
    "venv",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    // Editors.
    ".idea",
    ".vscode",
    // Vendor / generic.
    "vendor",
    // Infra-as-code caches.
    ".terraform",
    ".terragrunt-cache",
    "cdk.out",
    // JVM.
    ".gradle",
    ".mvn",
    // macOS / iOS.
    "Pods",
];

/// Regex-based [`CodeScanner`].
///
/// Patterns are compiled lazily at construction. The scanner is
/// stateless beyond the compiled patterns, so it is safe to share
/// across threads (`Send + Sync` via the trait bound).
///
/// # Limitations
///
/// The scanner is *line-oriented* and not language-aware:
///
/// - Hits inside comments and string literals are reported. A line
///   like `// process.env.DEPRECATED — do not use` produces a hit.
/// - Lines are split on `\n` and `\r\n`. Files using bare `\r` (legacy
///   Mac OS Classic) collapse into a single line, so reported lines
///   and columns will be off.
/// - Shell brace-expansion modifiers `${VAR#prefix}`, `${VAR%suffix}`,
///   `${VAR^^}`, `${VAR,,}`, `${VAR/foo/bar}`, `${VAR[0]}`, `${!VAR}`
///   *are* recognized (all standard parameter-expansion terminators
///   are matched), but indirect expansion via `${!prefix*}` and the
///   like is not.
/// - Single-quoted shell strings are not honored: `echo '$FOO'` is
///   reported even though the shell would treat it as a literal.
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
    ///
    /// # Panics
    /// Panics if the built-in patterns are malformed. The pattern bank is
    /// fixed at build time and validated by unit tests, so a panic here
    /// means the source was edited incorrectly. Use [`Self::try_new`] in
    /// contexts where a panic is unacceptable.
    ///
    /// # Examples
    /// ```
    /// use evault_scanner_regex::RegexCodeScanner;
    /// let _scanner = RegexCodeScanner::new();
    /// ```
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn new() -> Self {
        Self::try_new().expect("built-in pattern bank must be valid")
    }

    /// Fallible constructor.
    ///
    /// Validates that every compiled pattern exposes the required `name`
    /// capture group. Without this group the scanner would silently
    /// produce zero hits for that pattern, so the construction-time
    /// check turns a silent-failure mode into a loud one.
    ///
    /// # Errors
    /// Returns [`ScannerError::Pattern`] if any pattern lacks the
    /// `name` named capture group.
    pub fn try_new() -> Result<Self, ScannerError> {
        let specs = language_specs();
        let mut by_extension: HashMap<&'static str, Vec<regex::Regex>> = HashMap::new();
        for LanguageSpec {
            extensions,
            patterns,
        } in specs
        {
            for pattern in &patterns {
                if !pattern.capture_names().any(|n| n == Some("name")) {
                    return Err(ScannerError::Pattern(format!(
                        "pattern `{}` is missing required `name` capture group",
                        pattern.as_str()
                    )));
                }
            }
            for ext in extensions {
                by_extension
                    .entry(ext)
                    .or_default()
                    .extend(patterns.clone());
            }
        }
        Ok(Self { by_extension })
    }
}

impl CodeScanner for RegexCodeScanner {
    /// Scan `root` recursively.
    ///
    /// # Errors
    /// Returns [`ScannerError::Io`] if the directory cannot be walked or
    /// a readable source file cannot be read for a reason *other than*
    /// invalid UTF-8 (which is silently skipped per the trait contract).
    /// On error, **no partial results are returned** — any hits
    /// collected before the failure are discarded.
    ///
    /// Walking errors include the path that triggered them so the
    /// caller can diagnose permission issues, broken paths, etc.
    fn scan(&self, root: &Path) -> Result<Vec<ScanHit>, ScannerError> {
        let mut hits = Vec::new();
        let walker = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_skipped(e.file_name().to_str()));
        for entry in walker {
            let entry = entry.map_err(|e| walk_error_to_scanner_error(&e))?;
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

/// Convert a [`walkdir::Error`] into a [`ScannerError::Io`] that
/// preserves the offending path.
///
/// `walkdir::Error::into` would lose `err.path()` and `err.depth()`;
/// here we re-wrap into an `io::Error` whose Display includes the path
/// so users can see *which* entry failed (typically a permission
/// problem on a deeply nested directory).
fn walk_error_to_scanner_error(err: &walkdir::Error) -> ScannerError {
    let path = err.path().map(Path::to_path_buf);
    let kind = err
        .io_error()
        .map_or(std::io::ErrorKind::Other, std::io::Error::kind);
    let msg = path.map_or_else(
        || format!("walking: {err}"),
        |p| format!("walking '{}': {err}", p.display()),
    );
    ScannerError::Io(std::io::Error::new(kind, msg))
}

fn scan_file(
    path: &Path,
    patterns: &[regex::Regex],
    hits: &mut Vec<ScanHit>,
) -> Result<(), ScannerError> {
    let text = match fs::read_to_string(path) {
        Ok(s) => s,
        // Per the trait contract: silently skip files whose contents
        // are not valid UTF-8 (binaries, latin-1 sources, etc.). No log
        // line is emitted; callers wanting per-file diagnostics should
        // pre-filter the tree before invoking the scanner.
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
        Err(e) => {
            return Err(ScannerError::Io(std::io::Error::new(
                e.kind(),
                format!("reading '{}': {e}", path.display()),
            )))
        }
    };
    for (line_idx, line) in text.lines().enumerate() {
        let line_number = line_idx + 1;
        for pattern in patterns {
            for caps in pattern.captures_iter(line) {
                let Some(name_match) = caps.name("name") else {
                    // Defense-in-depth: `try_new` rejects patterns
                    // missing the `name` group, so this branch is
                    // unreachable in practice. Skip rather than panic.
                    continue;
                };
                let byte_start = name_match.start();
                // Char-column: count Unicode scalar values BEFORE the
                // match start. This matches what editors mean by
                // "column N" and stays stable across multi-byte UTF-8.
                let column = line
                    .get(..byte_start)
                    .map_or(byte_start, |prefix| prefix.chars().count())
                    + 1;
                hits.push(ScanHit {
                    path: PathBuf::from(path),
                    line: line_number,
                    column,
                    name: name_match.as_str().to_owned(),
                });
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
        assert!(is_skipped(Some(".terraform")));
        assert!(is_skipped(Some(".next")));
        assert!(is_skipped(Some("coverage")));
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

    #[test]
    fn try_new_accepts_builtin_patterns() {
        let scanner = RegexCodeScanner::try_new().expect("built-in patterns must validate");
        assert!(!scanner.by_extension.is_empty());
    }
}
