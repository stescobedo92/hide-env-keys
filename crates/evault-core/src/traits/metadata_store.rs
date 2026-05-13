//! [`MetadataStore`] — persistent CRUD over variable and project metadata.
//!
//! Implementations must:
//! - Be safe to call concurrently from the same process. (For SQLite-based
//!   backends, the WAL journal mode is sufficient.)
//! - Never store secret variable values — those go to a
//!   [`SecretStore`](crate::traits::SecretStore).
//! - Preserve referential integrity between projects, variables, and their
//!   linkage records.

use crate::error::MetadataError;
use crate::model::{Profile, Project, ProjectId, ProjectVar, Var, VarFilter, VarId};

/// Persistent storage of variable and project metadata.
///
/// All methods take a shared reference, so implementations must rely on
/// interior mutability (typically a connection pool or a `Mutex`).
pub trait MetadataStore: Send + Sync {
    // ----- Var -------------------------------------------------------------

    /// Create or update a [`Var`] by id.
    ///
    /// # Errors
    /// Returns [`MetadataError::DuplicateName`] if a different variable with
    /// the same name already exists; [`MetadataError::Backend`] for storage
    /// failures.
    fn upsert_var(&self, var: &Var) -> Result<(), MetadataError>;

    /// Fetch a [`Var`] by id, or `Ok(None)` if not found.
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn get_var(&self, id: VarId) -> Result<Option<Var>, MetadataError>;

    /// Fetch a [`Var`] by its canonical name, or `Ok(None)` if not found.
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn find_var_by_name(&self, name: &str) -> Result<Option<Var>, MetadataError>;

    /// List variables matching `filter`.
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn list_vars(&self, filter: &VarFilter) -> Result<Vec<Var>, MetadataError>;

    /// Delete a [`Var`] by id. No-op if the variable does not exist.
    ///
    /// **Implementations MUST atomically cascade** the deletion to the
    /// plain-value side table and to every project↔var linkage that
    /// references this variable. The service layer relies on this atomicity
    /// to guarantee that a variable becomes invisible to every read path in
    /// a single observable step.
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn delete_var(&self, id: VarId) -> Result<(), MetadataError>;

    // ----- Plain-value side table (for VarKind::Plain) ---------------------

    /// Fetch the plain (non-secret) value associated with a variable, or
    /// `Ok(None)` if the variable does not exist or is a secret.
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn get_plain_value(&self, id: VarId) -> Result<Option<String>, MetadataError>;

    /// Set the plain value of a variable.
    ///
    /// Callers must only invoke this for [`crate::model::VarKind::Plain`]
    /// variables. Secret values must go to a
    /// [`SecretStore`](crate::traits::SecretStore).
    ///
    /// **Implementations MUST enforce this rule**: a call with a
    /// [`VarKind::Secret`](crate::model::VarKind::Secret) variable is a
    /// silent-failure surface (the value would be stored in the wrong tier),
    /// so backends must reject it with
    /// [`MetadataError::KindMismatch`] instead of silently writing.
    ///
    /// # Errors
    /// Returns [`MetadataError::VarNotFound`] if the variable does not exist;
    /// [`MetadataError::KindMismatch`] if the variable is not
    /// [`VarKind::Plain`](crate::model::VarKind::Plain);
    /// [`MetadataError::Backend`] on storage failures.
    fn set_plain_value(&self, id: VarId, value: &str) -> Result<(), MetadataError>;

    // ----- Project ---------------------------------------------------------

    /// Create or update a [`Project`].
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn upsert_project(&self, project: &Project) -> Result<(), MetadataError>;

    /// Fetch a [`Project`] by id, or `Ok(None)` if not found.
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn get_project(&self, id: ProjectId) -> Result<Option<Project>, MetadataError>;

    /// List every known [`Project`].
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn list_projects(&self) -> Result<Vec<Project>, MetadataError>;

    /// Delete a project and all of its linkage records. No-op if the project
    /// does not exist.
    ///
    /// **Implementations MUST atomically cascade** the deletion to every
    /// linkage that references this project, matching the
    /// [`Self::delete_var`] contract.
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn delete_project(&self, id: ProjectId) -> Result<(), MetadataError>;

    // ----- Project ↔ Var linkage ------------------------------------------

    /// Create or update a [`ProjectVar`] linkage.
    ///
    /// # Errors
    /// Returns [`MetadataError::ProjectNotFound`] or [`MetadataError::VarNotFound`]
    /// if either side does not exist; [`MetadataError::Backend`] on storage
    /// failures.
    fn upsert_link(&self, link: &ProjectVar) -> Result<(), MetadataError>;

    /// Delete a single linkage. Idempotent.
    ///
    /// Returns `Ok(true)` when a linkage was actually removed and `Ok(false)`
    /// when the triple was already absent. The service layer uses this to
    /// avoid emitting `Unlinked` audit entries for no-op calls.
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn delete_link(
        &self,
        project_id: ProjectId,
        var_id: VarId,
        profile: &Profile,
    ) -> Result<bool, MetadataError>;

    /// List every linkage attached to a project (across profiles).
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn list_links_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectVar>, MetadataError>;

    /// List every linkage that references a given variable (used by the UI to
    /// show "where is this used").
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failures.
    fn list_links_for_var(&self, var_id: VarId) -> Result<Vec<ProjectVar>, MetadataError>;
}
