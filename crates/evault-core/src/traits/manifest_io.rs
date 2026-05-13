//! [`ManifestIo`] — load and save project manifests on disk.

use std::path::Path;

use crate::error::ManifestError;
use crate::model::ManifestSnapshot;

/// Read and write `evault.toml` project manifests.
///
/// The wire format is owned by `evault-manifest`; this trait abstracts the
/// IO and parsing so the rest of the workspace can speak in
/// [`ManifestSnapshot`] values.
pub trait ManifestIo: Send + Sync {
    /// Load the manifest at `path` and parse it into a snapshot.
    ///
    /// # Errors
    /// Returns [`ManifestError::NotFound`] if the file does not exist;
    /// [`ManifestError::Parse`] if the file is malformed;
    /// [`ManifestError::Invalid`] if the contents violate semantic rules.
    fn load(&self, path: &Path) -> Result<ManifestSnapshot, ManifestError>;

    /// Serialize `snapshot` and write it to `path`, atomically replacing any
    /// existing file at that location.
    ///
    /// # Errors
    /// Returns [`ManifestError::Write`] if the write or rename fails.
    fn save(&self, path: &Path, snapshot: &ManifestSnapshot) -> Result<(), ManifestError>;
}
