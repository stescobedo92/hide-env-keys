//! `evault diff --project PATH [--profile P]` — compare manifest and registry.

use std::collections::HashSet;
use std::path::Path;

use evault_core::model::{BindingSource, Profile, VarId};
use evault_core::traits::ManifestIo;
use evault_manifest::FileManifestIo;
use evault_materializer::env_file_path;

use crate::backend::BackendOps;
use crate::commands::link::manifest_path_for;
use crate::error::CliError;
use crate::presentation;

/// Compare the effective manifest bindings with registry state.
///
/// # Errors
/// Returns [`CliError`] on manifest IO or backend failures.
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

    let out = env_file_path(&project_path, environment)
        .map_err(|e| CliError::Io(std::io::Error::other(format!("environment: {e}"))))?;
    println!("{}", presentation::accent("evault diff"));
    println!("  project:      {}", project_path.display());
    println!("  manifest:     {}", manifest_path.display());
    println!("  profile:      {}", profile.as_str());
    println!("  materialized: {}", out.display());
    println!(
        "  env file:     {}",
        if out.exists() {
            presentation::success("present")
        } else {
            presentation::muted("not present")
        }
    );

    let mut effective_refs: HashSet<VarId> = HashSet::new();
    let mut missing_values: Vec<String> = Vec::new();
    let mut inline_keys: Vec<String> = Vec::new();
    for binding in snapshot.effective_bindings(&profile) {
        match &binding.source {
            BindingSource::Inline { .. } => inline_keys.push(binding.key.clone()),
            BindingSource::Registry { var_id } => {
                effective_refs.insert(*var_id);
                let value = backend
                    .get_value(*var_id)
                    .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
                if value.is_none() {
                    missing_values.push(binding.key.clone());
                }
            }
        }
    }

    let mut registry_links_missing_manifest: Vec<String> = Vec::new();
    if let Some(project) = backend
        .find_project_by_path(&project_path)
        .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?
    {
        let links = backend
            .links_for_project(project.id())
            .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
        for link in links.into_iter().filter(|link| link.profile == profile) {
            if !effective_refs.contains(&link.var_id) {
                registry_links_missing_manifest.push(short_var(link.var_id));
            }
        }
    }

    print_section("inline bindings", &inline_keys);
    print_section("registry refs missing values", &missing_values);
    print_section(
        "registry links missing from manifest",
        &registry_links_missing_manifest,
    );

    if missing_values.is_empty() && registry_links_missing_manifest.is_empty() {
        println!("{}", presentation::success("manifest and registry agree"));
    } else {
        println!("{}", presentation::warning("differences found"));
    }
    Ok(())
}

fn print_section(title: &str, values: &[String]) {
    println!();
    println!("{}", presentation::accent(title));
    if values.is_empty() {
        println!("  {}", presentation::muted("none"));
        return;
    }
    for value in values {
        println!("  {value}");
    }
}

fn short_var(id: VarId) -> String {
    id.as_uuid()
        .as_hyphenated()
        .to_string()
        .chars()
        .take(8)
        .collect()
}
