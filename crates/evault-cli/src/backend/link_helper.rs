//! Shared helper for the TUI's `l` (link to project) flow.
//!
//! Both `InMemoryBackend` and `SqlCipherBackend` reuse this function
//! from their `VarMutator::link_to_project` impls. The logic mirrors
//! the CLI's `evault link` (+ optional `evault gen`) subcommands.

use std::collections::BTreeMap;
use std::path::PathBuf;

use evault_core::model::{BindingSource, ManifestBinding, ManifestSnapshot, Profile, VarId};
use evault_core::traits::{ManifestIo, Materializer};
use evault_manifest::FileManifestIo;
use evault_materializer::EnvFileMaterializer;
use evault_tui::ProviderError;
use secrecy::ExposeSecret;

use super::BackendOps;

const MANIFEST_FILENAME: &str = "evault.toml";

/// Link a variable to a project and (optionally) materialise the
/// project's `.env` immediately. Public entry point invoked from
/// both backends' `VarMutator::link_to_project` impls.
pub fn link_to_project<B: BackendOps>(
    backend: &B,
    var_id: VarId,
    var_name: String,
    project_path: PathBuf,
    profile_raw: String,
    materialize: bool,
) -> Result<(), ProviderError> {
    // 1. Canonicalise the path so the registry stores a stable
    //    absolute reference. If the path doesn't exist yet, create
    //    it so the user can link before scaffolding.
    if !project_path.exists() {
        std::fs::create_dir_all(&project_path).map_err(|e| {
            ProviderError::Backend(format!(
                "create project dir '{}': {e}",
                project_path.display()
            ))
        })?;
    }
    let project_path = project_path.canonicalize().map_err(|e| {
        ProviderError::Backend(format!("canonicalise '{}': {e}", project_path.display()))
    })?;

    // 2. Resolve or create the project record.
    let existing = backend.find_project_by_path(&project_path)?;
    let project_id = if let Some(project) = existing {
        project.id()
    } else {
        let name = project_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("project")
            .to_owned();
        backend.create_project(&name, project_path.clone())?
    };

    let profile = if profile_raw.is_empty() {
        Profile::default_profile()
    } else {
        Profile::named(profile_raw)
    };

    // 3. Record the link in the registry (idempotent: best-effort).
    let _ = backend.link_var(project_id, var_id, profile.clone(), None);

    // 4. Read or create the manifest, upsert the binding.
    let manifest_path = project_path.join(MANIFEST_FILENAME);
    let io = FileManifestIo;
    let mut snapshot = if manifest_path.exists() {
        io.load(&manifest_path)
            .map_err(|e| ProviderError::Backend(format!("manifest load: {e}")))?
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
    let key = var_name;
    snapshot
        .bindings
        .retain(|b| !(b.key == key && b.profile == profile));
    snapshot.bindings.push(ManifestBinding {
        key,
        source: BindingSource::Registry { var_id },
        profile: profile.clone(),
    });
    io.save(&manifest_path, &snapshot)
        .map_err(|e| ProviderError::Backend(format!("manifest save: {e}")))?;

    // 5. Optionally materialise `.env`.
    if materialize {
        let mut env: BTreeMap<String, String> = BTreeMap::new();
        for binding in snapshot.effective_bindings(&profile) {
            let value = match &binding.source {
                BindingSource::Inline { value } => value.clone(),
                BindingSource::Registry { var_id } => {
                    let secret = backend.get_value(*var_id)?.ok_or_else(|| {
                        ProviderError::Backend(format!(
                            "value missing for var referenced by manifest binding `{}`",
                            binding.key
                        ))
                    })?;
                    secret.expose_secret().to_owned()
                }
            };
            env.insert(binding.key.clone(), value);
        }
        let out = project_path.join(".env");
        let materializer = EnvFileMaterializer;
        materializer
            .materialize(&out, &env)
            .map_err(|e| ProviderError::Backend(format!("materialize: {e}")))?;
    }
    Ok(())
}
