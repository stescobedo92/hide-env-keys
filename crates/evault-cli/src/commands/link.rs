//! `evault link NAME --project PATH [--profile P] [--alias A]` —
//! link a variable to a project's manifest.
//!
//! Idempotently:
//! 1. Finds (or creates) a Project record for `PATH` in the registry.
//! 2. Calls `BackendOps::link_var` so the registry's link table
//!    records the relationship.
//! 3. Reads or creates `PATH/evault.toml` and adds a binding pointing
//!    at the registry var.

use std::path::{Path, PathBuf};

use evault_core::model::{BindingSource, ManifestBinding, ManifestSnapshot, Profile};
use evault_core::traits::ManifestIo;
use evault_manifest::FileManifestIo;

use crate::backend::BackendOps;
use crate::error::CliError;
use crate::presentation;

/// Manifest filename inside the project root.
const MANIFEST_FILENAME: &str = "evault.toml";

/// Link `var_name` into the project at `project_path` under the given
/// `profile` and optional `alias`.
///
/// # Errors
/// Returns [`CliError`] on missing variable, missing project path,
/// backend failure, or manifest IO failure.
pub fn run<B: BackendOps>(
    backend: &B,
    var_name: &str,
    project_path: &Path,
    profile: Profile,
    alias: Option<String>,
) -> Result<(), CliError> {
    // Canonicalise the project path so the registry stores a stable
    // absolute reference rather than a relative one that changes
    // meaning between invocations.
    let project_path = project_path.canonicalize().map_err(CliError::Io)?;

    let var = backend
        .find_var_by_name(var_name)
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?
        .ok_or_else(|| {
            CliError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no variable named '{var_name}'"),
            ))
        })?;

    // Resolve or create the project record.
    let existing = backend
        .find_project_by_path(&project_path)
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
    let project_id = if let Some(project) = existing {
        project.id()
    } else {
        let name = project_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("project")
            .to_owned();
        backend
            .create_project(&name, project_path.clone())
            .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?
    };

    backend
        .link_var(project_id, var.id(), profile.clone(), alias.clone())
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;

    // Update the on-disk manifest. We read what's there, append (or
    // replace) the binding for this var, then save back.
    let manifest_path = project_path.join(MANIFEST_FILENAME);
    let io = FileManifestIo;
    let mut snapshot = if manifest_path.exists() {
        io.load(&manifest_path)
            .map_err(|e| CliError::Io(std::io::Error::other(format!("manifest load: {e}"))))?
    } else {
        ManifestSnapshot {
            project_id,
            name: project_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("project")
                .to_owned(),
            bindings: Vec::new(),
        }
    };

    let key = alias.unwrap_or_else(|| var.name().to_owned());
    // Replace any existing binding for (key, profile); otherwise append.
    snapshot
        .bindings
        .retain(|b| !(b.key == key && b.profile == profile));
    snapshot.bindings.push(ManifestBinding {
        key: key.clone(),
        source: BindingSource::Registry { var_id: var.id() },
        profile,
    });

    io.save(&manifest_path, &snapshot)
        .map_err(|e| CliError::Io(std::io::Error::other(format!("manifest save: {e}"))))?;

    println!(
        "{}",
        presentation::success(format!(
            "linked {} -> {} ({})",
            var.name(),
            project_path.display(),
            manifest_path.display(),
        ))
    );
    Ok(())
}

/// Helper: load or create the canonical manifest path for `project_path`.
/// Exposed for `gen` / `run` reuse.
#[must_use]
pub fn manifest_path_for(project_path: &Path) -> PathBuf {
    project_path.join(MANIFEST_FILENAME)
}
