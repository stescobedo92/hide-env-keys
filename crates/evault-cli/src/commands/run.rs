//! `evault run --project PATH [--profile P] -- CMD [ARGS...]` —
//! execute a child process with the project's environment injected.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use evault_core::model::{BindingSource, Profile};
use evault_core::traits::{ManifestIo, ProcessRunner};
use evault_manifest::FileManifestIo;
use evault_runner::StdProcessRunner;
use secrecy::ExposeSecret;

use crate::backend::BackendOps;
use crate::commands::link::manifest_path_for;
use crate::error::CliError;

/// Spawn `cmd` with `args`, injecting the project's resolved
/// environment overlay. Forwards the child's exit code so wrapper
/// scripts behave the same as without `evault run`.
///
/// # Errors
/// Returns [`CliError`] on missing manifest, value resolution
/// failure, spawn / IO failure, or non-zero child exit.
pub fn run<B: BackendOps>(
    backend: &B,
    project_path: &Path,
    profile: Profile,
    cmd: &str,
    args: &[String],
) -> Result<ExitCode, CliError> {
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

    let runner = StdProcessRunner;
    let outcome = runner
        .run(cmd, args, &project_path, &env)
        .map_err(|e| CliError::Io(std::io::Error::other(format!("run: {e}"))))?;

    Ok(match outcome.exit_code {
        Some(0) => ExitCode::SUCCESS,
        Some(c) => {
            // Map the child's i32 exit code into the u8 our shell
            // can carry. Negative codes (rare; Unix uses signals)
            // are folded by absolute-value-then-mask. Use 1 if the
            // resulting byte would be 0 (which would falsely
            // indicate success).
            let truncated = u8::try_from(c.unsigned_abs() & 0xFF).unwrap_or(1);
            ExitCode::from(if truncated == 0 { 1 } else { truncated })
        }
        None => ExitCode::FAILURE,
    })
}
