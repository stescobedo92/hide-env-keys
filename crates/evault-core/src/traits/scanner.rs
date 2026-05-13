//! [`CodeScanner`] — find references to environment variables in source code.

use std::path::{Path, PathBuf};

use crate::error::ScannerError;

/// Where a scanner found a variable reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScanHit {
    /// File the reference was found in.
    pub path: PathBuf,
    /// 1-based line number of the reference.
    ///
    /// Lines are split on `\n` and `\r\n`. Files that use only bare `\r`
    /// line endings (legacy Mac OS Classic) are treated as a single line;
    /// implementations should document this in their own docs if relevant.
    pub line: usize,
    /// 1-based **character** column where the reference begins (Unicode
    /// scalar values, not bytes). Editors universally count columns in
    /// characters, so a hit at column `N` lands where a user clicking
    /// `goto N` would expect, even if the line contains multi-byte UTF-8
    /// before the match.
    pub column: usize,
    /// Variable name as it appears in the code (e.g. `DATABASE_URL`).
    pub name: String,
}

/// Walk a directory tree looking for references to environment variables.
///
/// Implementations should:
/// - skip common build/output directories (`target`, `node_modules`, `dist`, …);
/// - skip binary files (no UTF-8) without error;
/// - return one [`ScanHit`] per reference (a variable referenced N times in
///   one file produces N hits).
pub trait CodeScanner: Send + Sync {
    /// Scan the directory rooted at `root` and return all hits.
    ///
    /// # Errors
    /// Returns [`ScannerError::Io`] if the file system cannot be walked;
    /// [`ScannerError::Pattern`] if the configured patterns are invalid.
    fn scan(&self, root: &Path) -> Result<Vec<ScanHit>, ScannerError>;
}
