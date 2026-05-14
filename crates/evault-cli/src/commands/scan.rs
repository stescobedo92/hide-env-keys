//! `evault scan PATH` — find env-var references in a source tree
//! and report orphans (referenced in code but missing from registry)
//! plus unused (in registry but not referenced anywhere).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use evault_core::traits::CodeScanner;
use evault_scanner_regex::RegexCodeScanner;

use crate::backend::BackendOps;
use crate::error::CliError;

/// Run the regex scanner over `path` and cross-reference with the
/// backend's registry.
///
/// Output format:
///
/// ```text
/// ORPHANS  (referenced in code but missing from registry)
///   FOO    src/main.rs:14
///   BAR    config/settings.py:7
///
/// UNUSED   (in registry but not referenced anywhere under PATH)
///   STALE_KEY
///
/// REFERENCED (in code AND registry)
///   DATABASE_URL  src/main.rs:3
/// ```
///
/// # Errors
/// Returns [`CliError`] on scanner IO failure or backend list failure.
pub fn run<B: BackendOps + evault_tui::VarProvider>(
    backend: &B,
    path: &Path,
) -> Result<(), CliError> {
    let scanner = RegexCodeScanner::new();
    let hits = scanner
        .scan(path)
        .map_err(|e| CliError::Io(std::io::Error::other(format!("scan: {e}"))))?;

    // Group hits by name.
    let mut hits_by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for hit in &hits {
        hits_by_name
            .entry(hit.name.clone())
            .or_default()
            .push(format!("{}:{}", hit.path.display(), hit.line));
    }

    // Registry side.
    let registry_vars: BTreeSet<String> = backend
        .list_projects()
        .map_err(|_| ())
        .into_iter()
        .flatten()
        .map(|_| String::new())
        .collect::<BTreeSet<_>>(); // placeholder — see below

    // Use BackendOps::summarise indirectly via find_var_by_name —
    // simpler: query each unique name and check existence.
    let referenced: BTreeSet<&str> = hits_by_name.keys().map(String::as_str).collect();
    let mut orphans: Vec<&str> = Vec::new();
    let mut matched: Vec<&str> = Vec::new();
    for name in &referenced {
        let exists = backend
            .find_var_by_name(name)
            .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?
            .is_some();
        if exists {
            matched.push(name);
        } else {
            orphans.push(name);
        }
    }

    // Unused: registry names not in `referenced`. We approximate by
    // probing find_var_by_name for each match candidate above — for
    // a full report we'd want list_vars. The backend exposes that
    // via VarProvider::list; reuse here.
    let _ = registry_vars; // silence the placeholder; see follow-up
    let all_registry_names: BTreeSet<String> = {
        // Round-trip through VarProvider (BackendOps doesn't expose
        // list directly — it lives on VarProvider).
        let summaries = <B as evault_tui::VarProvider>::list(backend)
            .map_err(|e| CliError::Tui(evault_tui::TuiError::Provider(e)))?;
        summaries.into_iter().map(|s| s.name).collect()
    };
    let unused: Vec<&str> = all_registry_names
        .iter()
        .filter(|n| !referenced.contains(n.as_str()))
        .map(String::as_str)
        .collect();

    if !orphans.is_empty() {
        println!("ORPHANS  (referenced in code but missing from registry)");
        for name in &orphans {
            let locations = hits_by_name
                .get(*name)
                .map(|v| v.join(", "))
                .unwrap_or_default();
            println!("  {name}  {locations}");
        }
        println!();
    }

    if !unused.is_empty() {
        println!(
            "UNUSED   (in registry but not referenced anywhere under {})",
            path.display()
        );
        for name in &unused {
            println!("  {name}");
        }
        println!();
    }

    if !matched.is_empty() {
        println!("REFERENCED (in code AND registry)");
        for name in &matched {
            let locations = hits_by_name
                .get(*name)
                .map(|v| v.join(", "))
                .unwrap_or_default();
            println!("  {name}  {locations}");
        }
    }

    Ok(())
}
