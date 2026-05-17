//! `evault unlink NAME --project PATH [--profile P]` — remove a project link.

use std::path::Path;

use evault_core::model::{BindingSource, Profile};
use evault_core::traits::ManifestIo;
use evault_manifest::FileManifestIo;

use crate::backend::BackendOps;
use crate::commands::link::manifest_path_for;
use crate::error::CliError;
use crate::presentation;

/// Remove a variable link from the registry and the project's manifest.
///
/// # Errors
/// Returns [`CliError`] on missing variable/project, manifest IO failure, or
/// backend failure.
pub fn run<B: BackendOps>(
    backend: &B,
    var_name: &str,
    project_path: &Path,
    profile: Profile,
    key: Option<String>,
) -> Result<(), CliError> {
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
    let project = backend
        .find_project_by_path(&project_path)
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?
        .ok_or_else(|| {
            CliError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("project not linked: {}", project_path.display()),
            ))
        })?;

    backend
        .unlink_var(project.id(), var.id(), &profile)
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;

    let manifest_path = manifest_path_for(&project_path);
    let io = FileManifestIo;
    let mut removed = 0_usize;
    if manifest_path.exists() {
        let mut snapshot = io
            .load(&manifest_path)
            .map_err(|e| CliError::Io(std::io::Error::other(format!("manifest load: {e}"))))?;
        let key = key.unwrap_or_else(|| var.name().to_owned());
        let before = snapshot.bindings.len();
        snapshot.bindings.retain(|binding| {
            if binding.profile != profile {
                return true;
            }
            let key_match = binding.key == key;
            let var_match = matches!(
                binding.source,
                BindingSource::Registry { var_id } if var_id == var.id()
            );
            !(key_match || var_match)
        });
        removed = before.saturating_sub(snapshot.bindings.len());
        io.save(&manifest_path, &snapshot)
            .map_err(|e| CliError::Io(std::io::Error::other(format!("manifest save: {e}"))))?;
    }

    let manifest_note = if removed == 0 {
        presentation::muted("manifest already clean")
    } else {
        format!("manifest bindings removed: {removed}")
    };
    println!(
        "{}",
        presentation::success(format!(
            "unlinked {} from {} ({})",
            var.name(),
            project_path.display(),
            manifest_note
        ))
    );
    Ok(())
}
