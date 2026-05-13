//! [`FileManifestIo`] — reads and writes `evault.toml` on the local
//! filesystem.
//!
//! `save` writes the manifest through a temporary sibling file and renames
//! it into place so a crash or out-of-space mid-write cannot leave the
//! caller staring at a half-rewritten manifest.

use std::fs;
use std::path::{Path, PathBuf};

use evault_core::error::ManifestError;
use evault_core::model::ManifestSnapshot;
use evault_core::traits::ManifestIo;

use crate::wire::ManifestFile;

/// Filesystem-backed implementation of [`ManifestIo`].
#[derive(Debug, Default, Clone, Copy)]
pub struct FileManifestIo;

impl FileManifestIo {
    /// Construct a fresh `FileManifestIo`. The struct is stateless.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ManifestIo for FileManifestIo {
    fn load(&self, path: &Path) -> Result<ManifestSnapshot, ManifestError> {
        let text = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ManifestError::NotFound(path.to_path_buf()));
            }
            // I/O failure (permission denied, disk error, path is a
            // directory, etc). Route through the dedicated `Io` variant so
            // the user can distinguish a filesystem problem from malformed
            // TOML. Carry only the ErrorKind label; never the OS message.
            Err(e) => {
                return Err(ManifestError::Io(format!("read: {:?}", e.kind())));
            }
        };
        let file: ManifestFile = toml::from_str(&text).map_err(|e| {
            // `toml::de::Error` Display renders a snippet of the source
            // line with a caret. That snippet can include inline binding
            // values, which violates the `ManifestError::Parse` doc
            // contract ("Quote only the structural error … never the
            // surrounding source text"). Extract only the structural
            // message + line/column so no source text reaches the error.
            let location = e
                .span()
                .map(|range| format!(" at bytes {}..{}", range.start, range.end))
                .unwrap_or_default();
            ManifestError::Parse(format!("toml: {}{location}", e.message()))
        })?;
        file.into_snapshot()
    }

    fn save(&self, path: &Path, snapshot: &ManifestSnapshot) -> Result<(), ManifestError> {
        let file = ManifestFile::from_snapshot(snapshot);
        let body = toml::to_string_pretty(&file)
            .map_err(|e| ManifestError::Write(format!("toml encode: {e}")))?;

        // Resolve parent directory. The caller is expected to pass a path
        // ending in a filename; if `path` has no parent (`/foo` on Unix)
        // we treat the current dir as parent.
        let parent: PathBuf = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

        // Write to a sibling tempfile then rename so partial writes never
        // become visible at `path`. The tempfile lives in the same
        // directory so the rename stays on the same filesystem (Windows
        // requires same-volume renames for atomicity).
        let tmp = parent.join(make_tmp_name(path)?);
        if let Err(e) = fs::write(&tmp, body.as_bytes()) {
            // A failed `fs::write` may have created a truncated tempfile.
            // Best-effort cleanup so we don't accumulate orphan files in
            // the manifest's parent directory; ignore the secondary error
            // so the user sees the original failure.
            let _ = fs::remove_file(&tmp);
            return Err(ManifestError::Write(format!("write tmp: {:?}", e.kind())));
        }
        if let Err(e) = fs::rename(&tmp, path) {
            // Same best-effort cleanup as above.
            let _ = fs::remove_file(&tmp);
            return Err(ManifestError::Write(format!(
                "rename failed: {:?}",
                e.kind()
            )));
        }
        Ok(())
    }
}

fn make_tmp_name(path: &Path) -> Result<String, ManifestError> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let base = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ManifestError::Write("path has no UTF-8 filename".into()))?;
    // Process id + a monotonic nanosecond suffix gives a per-process
    // unique name without bringing in the `tempfile` crate as a runtime
    // dep. Concurrent saves to the same path are not a supported scenario.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let pid = std::process::id();
    Ok(format!(".{base}.tmp.{pid}.{nanos}"))
}
