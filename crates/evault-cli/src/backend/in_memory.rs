//! Ephemeral in-memory backend.
//!
//! Used for the `--demo` / `--ephemeral` CLI paths and for tests.
//! Nothing persists across invocations; every `InMemoryBackend::new`
//! starts from an empty registry.

use std::path::PathBuf;

use evault_core::model::{
    AuditAction, AuditEntry, Group, Profile, Project, ProjectId, ProjectVar, Var, VarFilter, VarId,
    VarKind,
};
use evault_core::service::RegistryService;
use evault_core::traits::{SystemClock, UuidV4IdGenerator};
use evault_store_memory::{MemoryAuditSink, MemoryMetadataStore, MemorySecretStore};
use evault_tui::{AuditProvider, ProviderError, VarDraft, VarMutator, VarProvider, VarSummary};
use secrecy::SecretString;

use super::{core_to_provider, format_core_chain, BackendOps};

/// Concrete in-memory registry shape — pinned here so adapter
/// methods don't have to repeat the generic params.
pub type MemoryRegistry = RegistryService<
    MemoryMetadataStore,
    MemorySecretStore,
    MemoryAuditSink,
    SystemClock,
    UuidV4IdGenerator,
>;

/// Adapter that owns a [`RegistryService`] backed by in-memory
/// stores and exposes the TUI's read/write trait surface plus the
/// CLI's [`BackendOps`].
///
/// Every CLI invocation gets a fresh `InMemoryBackend`; nothing is
/// persisted across runs. Real persistence is provided by
/// [`super::SqlCipherBackend`].
pub struct InMemoryBackend {
    registry: MemoryRegistry,
}

impl InMemoryBackend {
    /// Construct a backend with empty stores.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: RegistryService::with_defaults(
                MemoryMetadataStore::new(),
                MemorySecretStore::new(),
                MemoryAuditSink::new(),
            ),
        }
    }

    /// Construct a backend pre-populated with a handful of demo
    /// variables so the TUI has something to interact with on a
    /// fresh launch.
    ///
    /// # Panics
    /// Panics if any seed `create_var` fails — the inputs are
    /// static and known-valid; a panic here means the in-memory
    /// store impl regressed.
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn with_demo_data() -> Self {
        let backend = Self::new();
        let seed: &[(&str, Group, VarKind, &str)] = &[
            (
                "DATABASE_URL",
                Group::User,
                VarKind::Secret,
                "postgres://localhost:5432/dev",
            ),
            ("NODE_ENV", Group::User, VarKind::Plain, "development"),
            ("API_KEY", Group::User, VarKind::Secret, "sk_live_abcdef"),
            ("LOG_LEVEL", Group::User, VarKind::Plain, "info"),
            (
                "AWS_ACCESS_KEY_ID",
                Group::System,
                VarKind::Secret,
                "AKIAEXAMPLE",
            ),
            (
                "AWS_SECRET_ACCESS_KEY",
                Group::System,
                VarKind::Secret,
                "wJalrXUtnFEMIxxK7MDENG",
            ),
            ("PORT", Group::Project, VarKind::Plain, "3000"),
            (
                "REDIS_URL",
                Group::User,
                VarKind::Secret,
                "redis://localhost:6379",
            ),
            (
                "FEATURE_FLAGS",
                Group::Project,
                VarKind::Plain,
                "new_ui=on,beta_login=off",
            ),
            (
                "SENTRY_DSN",
                Group::User,
                VarKind::Secret,
                "https://abc@sentry.io/123",
            ),
        ];
        for (name, group, kind, value) in seed {
            backend
                .registry
                .create_var(
                    name,
                    group.clone(),
                    *kind,
                    SecretString::new((*value).to_owned().into()),
                )
                .expect("demo seed must succeed against an empty in-memory backend");
        }
        backend
    }

    /// Borrow the underlying registry. Currently used only by tests;
    /// subcommands go through [`BackendOps`].
    #[must_use]
    pub const fn registry(&self) -> &MemoryRegistry {
        &self.registry
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VarProvider for InMemoryBackend {
    fn list(&self) -> Result<Vec<VarSummary>, ProviderError> {
        let vars = self
            .registry
            .list_vars(&VarFilter::new())
            .map_err(|e| core_to_provider(&e))?;
        let mut summaries = Vec::with_capacity(vars.len());
        for var in vars {
            summaries.push(self.summarise(&var)?);
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(summaries)
    }

    fn get_value(&self, id: VarId) -> Result<Option<SecretString>, ProviderError> {
        BackendOps::get_value(self, id)
    }
}

impl AuditProvider for InMemoryBackend {
    fn recent_audit(&self, limit: usize) -> Result<Vec<AuditEntry>, ProviderError> {
        BackendOps::recent_audit(self, limit)
    }
}

impl VarMutator for InMemoryBackend {
    fn delete(&self, id: VarId) -> Result<(), ProviderError> {
        self.registry
            .delete_var(id)
            .map_err(|e| core_to_provider(&e))
    }

    fn create(&self, draft: VarDraft) -> Result<VarId, ProviderError> {
        BackendOps::create_var(self, &draft.name, draft.group, draft.kind, draft.value)
    }

    fn update_value(&self, id: VarId, value: SecretString) -> Result<(), ProviderError> {
        BackendOps::update_value(self, id, value)
    }

    fn record_copy(&self, id: VarId) -> Result<(), ProviderError> {
        BackendOps::record_var_action(self, id, AuditAction::Copied)
    }

    fn link_to_project(
        &self,
        var_id: VarId,
        var_name: String,
        project_path: PathBuf,
        profile: String,
        materialize: bool,
    ) -> Result<(), ProviderError> {
        super::link_helper::link_to_project(
            self,
            var_id,
            var_name,
            project_path,
            profile,
            materialize,
        )
    }

    fn run_in_project(
        &self,
        project_path: PathBuf,
        profile: String,
        program: String,
        args: Vec<String>,
    ) -> Result<Option<i32>, ProviderError> {
        super::run_helper::run_in_project(self, project_path, profile, program, args)
    }
}

impl BackendOps for InMemoryBackend {
    fn find_var_by_name(&self, name: &str) -> Result<Option<Var>, ProviderError> {
        self.registry
            .find_var_by_name(name)
            .map_err(|e| core_to_provider(&e))
    }

    fn create_var(
        &self,
        name: &str,
        group: Group,
        kind: VarKind,
        value: SecretString,
    ) -> Result<VarId, ProviderError> {
        self.registry
            .create_var(name, group, kind, value)
            .map_err(|e| core_to_provider(&e))
    }

    fn update_value(&self, id: VarId, value: SecretString) -> Result<(), ProviderError> {
        self.registry
            .update_value(id, value)
            .map_err(|e| core_to_provider(&e))
    }

    fn get_value(&self, id: VarId) -> Result<Option<SecretString>, ProviderError> {
        self.registry
            .get_value(id)
            .map_err(|e| core_to_provider(&e))
    }

    fn list_projects(&self) -> Result<Vec<Project>, ProviderError> {
        self.registry
            .list_projects()
            .map_err(|e| core_to_provider(&e))
    }

    fn find_project_by_path(
        &self,
        path: &std::path::Path,
    ) -> Result<Option<Project>, ProviderError> {
        let projects = self.list_projects()?;
        Ok(projects.into_iter().find(|p| p.path() == path))
    }

    fn create_project(&self, name: &str, path: PathBuf) -> Result<ProjectId, ProviderError> {
        self.registry
            .create_project(name, path)
            .map_err(|e| core_to_provider(&e))
    }

    fn link_var(
        &self,
        project_id: ProjectId,
        var_id: VarId,
        profile: Profile,
        alias: Option<String>,
    ) -> Result<(), ProviderError> {
        self.registry
            .link_var(project_id, var_id, profile, alias)
            .map_err(|e| core_to_provider(&e))
    }

    fn unlink_var(
        &self,
        project_id: ProjectId,
        var_id: VarId,
        profile: &Profile,
    ) -> Result<(), ProviderError> {
        self.registry
            .unlink_var(project_id, var_id, profile)
            .map_err(|e| core_to_provider(&e))
    }

    fn links_for_project(&self, project_id: ProjectId) -> Result<Vec<ProjectVar>, ProviderError> {
        self.registry
            .links_for_project(project_id)
            .map_err(|e| core_to_provider(&e))
    }

    fn recent_audit(&self, limit: usize) -> Result<Vec<AuditEntry>, ProviderError> {
        self.registry
            .recent_audit(limit)
            .map_err(|e| core_to_provider(&e))
    }

    fn record_var_action(&self, id: VarId, action: AuditAction) -> Result<(), ProviderError> {
        self.registry
            .record_var_action(id, action)
            .map_err(|e| core_to_provider(&e))
    }

    fn summarise(&self, var: &Var) -> Result<VarSummary, ProviderError> {
        let links = self.registry.links_for_var(var.id()).map_err(|e| {
            let chain = format_core_chain(&e);
            ProviderError::Backend(format!("links for {}: {chain}", var.name()))
        })?;
        Ok(VarSummary {
            id: var.id(),
            name: var.name().to_owned(),
            group: var.group().clone(),
            kind: var.kind(),
            value_len: var.length(),
            linked_projects: links.len(),
            updated_at: var.updated_at(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_backend_lists_no_vars() {
        let backend = InMemoryBackend::new();
        let rows = backend.list().expect("list");
        assert!(rows.is_empty());
    }

    #[test]
    fn create_and_list_roundtrip() {
        let backend = InMemoryBackend::new();
        backend
            .create_var(
                "ALPHA",
                Group::User,
                VarKind::Plain,
                SecretString::new("v1".to_owned().into()),
            )
            .expect("create");
        let rows = backend.list().expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "ALPHA");
    }

    #[test]
    fn delete_round_trip() {
        let backend = InMemoryBackend::new();
        let id = backend
            .create_var(
                "DOOMED",
                Group::User,
                VarKind::Plain,
                SecretString::new("x".to_owned().into()),
            )
            .expect("create");
        VarMutator::delete(&backend, id).expect("delete");
        assert!(backend.list().expect("list").is_empty());
    }

    #[test]
    fn get_value_returns_stored_secret() {
        use secrecy::ExposeSecret;
        let backend = InMemoryBackend::new();
        let id = backend
            .create_var(
                "DB_URL",
                Group::User,
                VarKind::Secret,
                SecretString::new("postgres://x".to_owned().into()),
            )
            .expect("create");
        let value = BackendOps::get_value(&backend, id)
            .expect("get_value")
            .expect("value present");
        assert_eq!(value.expose_secret(), "postgres://x");
    }
}
