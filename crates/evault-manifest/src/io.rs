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
            // Carry only the IO error kind label; never the path or the
            // error's own message (which on some platforms can echo the
            // path).
            Err(e) => {
                return Err(ManifestError::Parse(format!("read failed: {:?}", e.kind())));
            }
        };
        let file: ManifestFile = toml::from_str(&text).map_err(|e| {
            // `toml::de::Error` Display can include the offending line/column
            // text. The line/column is operational signal worth preserving;
            // the surrounding text is not. We carry the parser's own message
            // here because it's bounded by the structural error (token,
            // expected/found) and does not embed inline-binding values
            // unless the user genuinely put a malformed string at that
            // exact position — at which point the user supplied it.
            ManifestError::Parse(format!("toml: {e}"))
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
        fs::write(&tmp, body.as_bytes())
            .map_err(|e| ManifestError::Write(format!("write tmp: {:?}", e.kind())))?;
        if let Err(e) = fs::rename(&tmp, path) {
            // Best-effort cleanup; ignore the secondary error so the user
            // sees the original failure.
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
