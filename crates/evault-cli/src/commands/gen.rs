//! `evault gen --project PATH [--profile P]` — materialize a `.env`
//! from the project manifest.

use std::collections::BTreeMap;
use std::path::Path;

use evault_core::model::{BindingSource, Profile};
use evault_core::traits::{ManifestIo, Materializer};
use evault_manifest::FileManifestIo;
use evault_materializer::{env_file_path, EnvFileMaterializer};
use secrecy::ExposeSecret;

use crate::backend::BackendOps;
use crate::commands::link::manifest_path_for;
use crate::error::CliError;
use crate::presentation;

/// Default basename of the materialized file (`.env`).
const DEFAULT_OUTPUT: &str = ".env";

/// Resolve the project manifest under `profile`, fetch each
/// registry-referenced value, and write `.env` (atomic, gitignored).
///
/// # Errors
/// Returns [`CliError`] on missing manifest, unresolved var
/// reference, or materializer IO failure.
pub fn run<B: BackendOps>(
    backend: &B,
    project_path: &Path,
    profile: Profile,
    environment: Option<&str>,
) -> Result<(), CliError> {
    let project_path = project_path.canonicalize().map_err(CliError::Io)?;
    let manifest_path = manifest_path_for(&project_path);
    let io = FileManifestIo;
    let snapshot = io
        .load(&manifest_path)
        .map_err(|e| CliError::Io(std::io::Error::other(format!("manifest load: {e}"))))?;

    let mut env: BTreeMap<String, String> = BTreeMap::new();
    for binding in snapshot.effective_bindings(&profile) {
        let value = match &binding.source {
            BindingSource::Inline { value } => value.clone(),
            BindingSource::Registry { var_id } => {
                let secret = backend
                    .get_value(*var_id)
                    .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?
                    .ok_or_else(|| {
                        CliError::Io(std::io::Error::other(format!(
                            "variable {var_id:?} referenced by manifest is missing its value"
                        )))
                    })?;
                secret.expose_secret().to_owned()
            }
        };
        env.insert(binding.key.clone(), value);
    }

    let out = match environment {
        Some(label) => env_file_path(&project_path, Some(label))
            .map_err(|e| CliError::Io(std::io::Error::other(format!("environment: {e}"))))?,
        None => project_path.join(DEFAULT_OUTPUT),
    };
    let materializer = EnvFileMaterializer;
    materializer
        .materialize(&out, &env)
        .map_err(|e| CliError::Io(std::io::Error::other(format!("materialize: {e}"))))?;

    println!(
        "{}",
        presentation::success(format!("wrote {} ({} variables)", out.display(), env.len()))
    );
    Ok(())
}
