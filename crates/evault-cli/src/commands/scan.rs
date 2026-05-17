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
    ci: bool,
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

    // Query each unique referenced name and check existence.
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

    // Unused: registry names not in `referenced`. BackendOps doesn't expose
    // list directly; VarProvider does, so reuse that dashboard-facing shape.
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

    if ci && (!orphans.is_empty() || !unused.is_empty()) {
        return Err(CliError::Io(std::io::Error::other(format!(
            "scan --ci found {} orphaned and {} unused variables",
            orphans.len(),
            unused.len()
        ))));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    use evault_core::model::{Group, Profile, Project, ProjectId, ProjectVar, Var, VarId, VarKind};
    use evault_tui::{ProviderError, VarSummary};
    use secrecy::SecretString;
    use tempfile::TempDir;
    use time::OffsetDateTime;

    #[derive(Default)]
    struct FakeBackend {
        names: BTreeSet<String>,
    }

    impl FakeBackend {
        fn with_names(names: &[&str]) -> Self {
            Self {
                names: names.iter().map(|name| (*name).to_owned()).collect(),
            }
        }
    }

    impl BackendOps for FakeBackend {
        fn find_var_by_name(&self, name: &str) -> Result<Option<Var>, ProviderError> {
            if !self.names.contains(name) {
                return Ok(None);
            }
            Ok(Some(Var::from_trusted_parts(
                VarId::new_v4(),
                name.to_owned(),
                Group::User,
                VarKind::Plain,
                Vec::new(),
                1,
                OffsetDateTime::UNIX_EPOCH,
                OffsetDateTime::UNIX_EPOCH,
            )))
        }

        fn create_var(
            &self,
            _name: &str,
            _group: Group,
            _kind: VarKind,
            _value: SecretString,
        ) -> Result<VarId, ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn update_value(&self, _id: VarId, _value: SecretString) -> Result<(), ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn get_value(&self, _id: VarId) -> Result<Option<SecretString>, ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn list_projects(&self) -> Result<Vec<Project>, ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn find_project_by_path(
            &self,
            _path: &std::path::Path,
        ) -> Result<Option<Project>, ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn create_project(
            &self,
            _name: &str,
            _path: std::path::PathBuf,
        ) -> Result<ProjectId, ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn link_var(
            &self,
            _project_id: ProjectId,
            _var_id: VarId,
            _profile: Profile,
            _alias: Option<String>,
        ) -> Result<(), ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn unlink_var(
            &self,
            _project_id: ProjectId,
            _var_id: VarId,
            _profile: &Profile,
        ) -> Result<(), ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn links_for_project(
            &self,
            _project_id: ProjectId,
        ) -> Result<Vec<ProjectVar>, ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn recent_audit(
            &self,
            _limit: usize,
        ) -> Result<Vec<evault_core::model::AuditEntry>, ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn record_var_action(
            &self,
            _id: VarId,
            _action: evault_core::model::AuditAction,
        ) -> Result<(), ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }

        fn summarise(&self, _var: &Var) -> Result<VarSummary, ProviderError> {
            Err(ProviderError::Backend("unused test stub".into()))
        }
    }

    impl evault_tui::VarProvider for FakeBackend {
        fn list(&self) -> Result<Vec<VarSummary>, ProviderError> {
            Ok(self
                .names
                .iter()
                .map(|name| VarSummary {
                    id: VarId::new_v4(),
                    name: name.clone(),
                    group: Group::User,
                    kind: VarKind::Plain,
                    value_len: 1,
                    linked_projects: 0,
                    updated_at: OffsetDateTime::UNIX_EPOCH,
                })
                .collect())
        }

        fn get_value(&self, _id: VarId) -> Result<Option<SecretString>, ProviderError> {
            Ok(None)
        }
    }

    #[test]
    fn scan_ci_fails_when_orphans_or_unused_exist() {
        let dir = TempDir::new().expect("tmpdir");
        std::fs::write(
            dir.path().join("app.js"),
            "const db = process.env.DATABASE_URL;\n",
        )
        .expect("write");
        let backend = FakeBackend::with_names(&["UNUSED_KEY"]);

        let err = run(&backend, dir.path(), true).expect_err("ci should fail");
        assert!(err.to_string().contains("io error"));
    }

    #[test]
    fn scan_ci_succeeds_when_registry_matches_references() {
        let dir = TempDir::new().expect("tmpdir");
        std::fs::write(dir.path().join("app.js"), "process.env.DATABASE_URL;\n").expect("write");
        let backend = FakeBackend::with_names(&["DATABASE_URL"]);

        run(&backend, dir.path(), true).expect("matching registry should pass");
    }
}
