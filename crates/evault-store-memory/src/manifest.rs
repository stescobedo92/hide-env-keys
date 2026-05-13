//! [`MemoryManifestIo`] — in-memory implementation of
//! [`evault_core::traits::ManifestIo`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use evault_core::error::ManifestError;
use evault_core::model::ManifestSnapshot;
use evault_core::traits::ManifestIo;

/// In-memory manifest IO. Snapshots are addressed by their `path` argument.
///
/// `load` returns [`ManifestError::NotFound`] when the path has never been
/// `save`d. `save` replaces any prior snapshot for that path.
#[derive(Debug, Default)]
pub struct MemoryManifestIo {
    inner: RwLock<HashMap<PathBuf, ManifestSnapshot>>,
}

impl MemoryManifestIo {
    /// Construct an empty in-memory manifest IO.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ManifestIo for MemoryManifestIo {
    fn load(&self, path: &Path) -> Result<ManifestSnapshot, ManifestError> {
        let guard = self
            .inner
            .read()
            .map_err(|_| ManifestError::Parse("in-memory manifest lock poisoned".into()))?;
        let result = guard
            .get(path)
            .cloned()
            .ok_or_else(|| ManifestError::NotFound(path.to_path_buf()));
        drop(guard);
        result
    }

    fn save(&self, path: &Path, snapshot: &ManifestSnapshot) -> Result<(), ManifestError> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| ManifestError::Write("in-memory manifest lock poisoned".into()))?;
        guard.insert(path.to_path_buf(), snapshot.clone());
        drop(guard);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evault_core::model::ProjectId;

    fn empty_snapshot() -> ManifestSnapshot {
        ManifestSnapshot {
            project_id: ProjectId::new_v4(),
            name: "x".into(),
            bindings: Vec::new(),
        }
    }

    #[test]
    fn save_then_load_roundtrips() {
        let io = MemoryManifestIo::new();
        let path = PathBuf::from("/tmp/evault.toml");
        let snap = empty_snapshot();
        io.save(&path, &snap).unwrap();
        let loaded = io.load(&path).unwrap();
        assert_eq!(loaded.name, "x");
    }

    #[test]
    fn load_missing_returns_not_found() {
        let io = MemoryManifestIo::new();
        let path = PathBuf::from("/tmp/missing.toml");
        match io.load(&path) {
            Err(ManifestError::NotFound(p)) => assert_eq!(p, path),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn save_replaces_prior_snapshot() {
        let io = MemoryManifestIo::new();
        let path = PathBuf::from("/tmp/evault.toml");
        let mut a = empty_snapshot();
        a.name = "first".into();
        let mut b = empty_snapshot();
        b.name = "second".into();
        io.save(&path, &a).unwrap();
        io.save(&path, &b).unwrap();
        assert_eq!(io.load(&path).unwrap().name, "second");
    }
}
