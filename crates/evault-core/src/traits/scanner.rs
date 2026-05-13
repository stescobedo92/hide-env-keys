//! [`CodeScanner`] — find references to environment variables in source code.

use std::path::{Path, PathBuf};

use crate::error::ScannerError;

/// Where a scanner found a variable reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScanHit {
    /// File the reference was found in.
    pub path: PathBuf,
    /// 1-based line number of the reference.
    pub line: usize,
    /// 1-based column number where the reference begins.
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
