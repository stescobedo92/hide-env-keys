//! Shared helper for the TUI's `R` (run in project) flow and the
//! CLI's `evault run` subcommand.
//!
//! Both `InMemoryBackend` and `SqlCipherBackend` reuse this function
//! from their `VarMutator::run_in_project` impls. The logic mirrors
//! the CLI's `evault run` subcommand: load the manifest, resolve
//! every binding for the chosen profile (secrets via `BackendOps`,
//! inline literals straight from the manifest), then spawn the child
//! process with the env overlay injected.

use std::collections::BTreeMap;
use std::path::PathBuf;

use evault_core::model::{BindingSource, Profile};
use evault_core::traits::{ManifestIo, ProcessRunner};
use evault_manifest::FileManifestIo;
use evault_runner::StdProcessRunner;
use evault_tui::ProviderError;
use secrecy::ExposeSecret;

use super::BackendOps;

const MANIFEST_FILENAME: &str = "evault.toml";

/// Spawn `program` with `args` injecting the project's resolved
/// environment overlay. Public entry point invoked from both backends'
/// `VarMutator::run_in_project` impls.
///
/// The terminal lifecycle is the caller's concern. The TUI runtime
/// restores the terminal before invoking this method and re-enters
/// raw mode + alternate screen after it returns, so this function
/// may safely block on the child without corrupting the parent's
/// screen.
///
/// Returns the child's exit code (`None` if killed by a signal).
pub fn run_in_project<B: BackendOps>(
    backend: &B,
    project_path: PathBuf,
    profile_raw: String,
    program: String,
    args: Vec<String>,
) -> Result<Option<i32>, ProviderError> {
    // 1. Canonicalise the path so the manifest is looked up relative
    //    to a stable absolute location and the child's cwd points at
    //    the project root (`evault run --project ./mi-app` from
    //    anywhere should behave the same as cd'ing in first).
    let project_path = project_path.canonicalize().map_err(|e| {
        ProviderError::Backend(format!("canonicalise '{}': {e}", project_path.display()))
    })?;

    // 2. Load the manifest. Missing manifest is the most common error,
    //    so the message names the file explicitly.
    let manifest_path = project_path.join(MANIFEST_FILENAME);
    let io = FileManifestIo;
    let snapshot = io.load(&manifest_path).map_err(|e| {
        ProviderError::Backend(format!("manifest load '{}': {e}", manifest_path.display()))
    })?;

    // 3. Resolve every binding under the requested profile into a
    //    flat `key -> value` map. Secret bindings hit the secret
    //    store; inline bindings come straight from the manifest text.
    let profile = if profile_raw.trim().is_empty() {
        Profile::default_profile()
    } else {
        Profile::try_named(profile_raw)
            .map_err(|e| ProviderError::Backend(format!("invalid profile: {e}")))?
    };
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    for binding in snapshot.effective_bindings(&profile) {
        let value = match &binding.source {
            BindingSource::Inline { value } => value.clone(),
            BindingSource::Registry { var_id } => {
                let secret = BackendOps::get_value(backend, *var_id)?.ok_or_else(|| {
                    ProviderError::Backend(format!(
                        "variable {var_id:?} referenced by manifest is missing its value"
                    ))
                })?;
                secret.expose_secret().to_owned()
            }
        };
        env.insert(binding.key.clone(), value);
    }

    // 4. Spawn. `StdProcessRunner` validates keys/values, strips
    //    EVAULT_* parent vars, and inherits stdio.
    let runner = StdProcessRunner;
    let outcome = runner
        .run(&program, &args, &project_path, &env)
        .map_err(|e| ProviderError::Backend(format!("run: {e}")))?;
    Ok(outcome.exit_code)
}
