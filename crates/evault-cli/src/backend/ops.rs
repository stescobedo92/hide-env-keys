//! `BackendOps` — the subset of CLI-internal operations every backend
//! must provide.
//!
//! The TUI consumes a backend through `evault_tui::VarProvider` +
//! `VarMutator`. CLI subcommands need a broader surface (create,
//! get-value, audit, project linking, …) than the TUI ever exposes
//! through a key chord. Rather than enlarge the public TUI traits,
//! we group the extra operations into a local trait that both
//! backends implement.

use std::path::PathBuf;

use evault_core::model::{
    AuditEntry, Group, Profile, Project, ProjectId, ProjectVar, Var, VarId, VarKind,
};
use evault_tui::{ProviderError, VarSummary};
use secrecy::SecretString;

/// Operations the CLI subcommands invoke on a backend, beyond what
/// `VarProvider` / `VarMutator` already provide.
pub trait BackendOps {
    /// Resolve a variable by exact name.
    ///
    /// # Errors
    /// Returns [`ProviderError`] on storage failure.
    fn find_var_by_name(&self, name: &str) -> Result<Option<Var>, ProviderError>;

    /// Create a new variable and return its assigned id.
    ///
    /// # Errors
    /// Returns [`ProviderError`] if the name is invalid / duplicate
    /// or the underlying stores reject the write.
    fn create_var(
        &self,
        name: &str,
        group: Group,
        kind: VarKind,
        value: SecretString,
    ) -> Result<VarId, ProviderError>;

    /// Replace a variable's value (in whichever tier its kind dictates).
    ///
    /// # Errors
    /// Returns [`ProviderError`] on validation or write failure.
    fn update_value(&self, id: VarId, value: SecretString) -> Result<(), ProviderError>;

    /// Read the variable's value. Returns `None` if the value is not
    /// present in its expected tier (rare; mostly indicates external
    /// drift between metadata and value tiers).
    ///
    /// # Errors
    /// Returns [`ProviderError`] on storage failure.
    fn get_value(&self, id: VarId) -> Result<Option<SecretString>, ProviderError>;

    /// List all known projects.
    ///
    /// # Errors
    /// Returns [`ProviderError`] on storage failure.
    fn list_projects(&self) -> Result<Vec<Project>, ProviderError>;

    /// Find a project by absolute path. The CLI uses this to
    /// idempotently upsert a project record from `link` / `gen` /
    /// `run`.
    ///
    /// # Errors
    /// Returns [`ProviderError`] on storage failure.
    fn find_project_by_path(
        &self,
        path: &std::path::Path,
    ) -> Result<Option<Project>, ProviderError>;

    /// Create a new project pointing at `path`.
    ///
    /// # Errors
    /// Returns [`ProviderError`] on validation or write failure.
    fn create_project(&self, name: &str, path: PathBuf) -> Result<ProjectId, ProviderError>;

    /// Link a variable to a project under a chosen profile + alias
    /// (alias defaults to the variable's own name when absent).
    ///
    /// # Errors
    /// Returns [`ProviderError`] if either side is unknown or the
    /// link already exists.
    fn link_var(
        &self,
        project_id: ProjectId,
        var_id: VarId,
        profile: Profile,
        alias: Option<String>,
    ) -> Result<(), ProviderError>;

    /// Links registered against a project, in stable order.
    ///
    /// # Errors
    /// Returns [`ProviderError`] on storage failure.
    fn links_for_project(&self, project_id: ProjectId) -> Result<Vec<ProjectVar>, ProviderError>;

    /// Recent audit entries — newest first, capped at `limit`.
    ///
    /// # Errors
    /// Returns [`ProviderError`] on storage failure.
    fn recent_audit(&self, limit: usize) -> Result<Vec<AuditEntry>, ProviderError>;

    /// Convenience: project a [`Var`] into the dashboard's
    /// [`VarSummary`] shape (with current link count). Implemented
    /// once per backend so subcommands don't have to plumb the
    /// internals.
    ///
    /// # Errors
    /// Returns [`ProviderError`] if the link count cannot be read.
    fn summarise(&self, var: &Var) -> Result<VarSummary, ProviderError>;
}
