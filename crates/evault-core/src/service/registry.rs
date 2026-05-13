//! [`RegistryService`] — the orchestration layer over the storage traits.
//!
//! `RegistryService` is the single entry point that the TUI and CLI use to
//! create, update, link, and delete variables. It composes a
//! [`MetadataStore`], a [`SecretStore`], an [`AuditSink`], a [`Clock`], and
//! an [`IdGenerator`] through generic type parameters — so the service is
//! testable with in-memory backends and deployable against `SQLCipher` + the
//! OS keyring without any code change.
//!
//! The business rules enforced here are:
//! - **One namespace per name**: creating a variable whose name already
//!   exists fails with [`MetadataError::DuplicateName`].
//! - **Two-tier value storage**: [`VarKind::Plain`] values go to the metadata
//!   store, [`VarKind::Secret`] values go to the secret store. The same
//!   service method (`update_value`) routes both cases.
//! - **Idempotent delete**: deleting an absent variable is `Ok(())`.
//! - **Audit-on-mutation**: every state-changing call emits an
//!   [`AuditEntry`] timestamped by the injected [`Clock`].

use crate::crypto::{ExposeSecret, SecretString};
use crate::error::{CoreError, MetadataError};
use crate::model::{
    AuditAction, AuditEntry, AuditId, Group, Profile, Project, ProjectId, ProjectVar, Var,
    VarFilter, VarId, VarKind,
};
use crate::traits::{
    AuditSink, Clock, IdGenerator, MetadataStore, SecretStore, SystemClock, UuidV4IdGenerator,
};

/// Composed registry orchestration.
///
/// Generic over the five infrastructure traits so tests can substitute fast,
/// deterministic backends; production binaries wire the `SQLCipher` /
/// `OsKeyring` / `SystemClock` / `UuidV4IdGenerator` quartet.
///
/// See `evault-store-memory/tests/registry_service.rs` for a full example
/// exercising the public API with the in-memory backends.
pub struct RegistryService<M, S, A, C, I>
where
    M: MetadataStore,
    S: SecretStore,
    A: AuditSink,
    C: Clock,
    I: IdGenerator,
{
    metadata: M,
    secrets: S,
    audit: A,
    clock: C,
    id_gen: I,
}

impl<M, S, A> RegistryService<M, S, A, SystemClock, UuidV4IdGenerator>
where
    M: MetadataStore,
    S: SecretStore,
    A: AuditSink,
{
    /// Construct a registry with the default real-time clock and v4 UUID
    /// generator.
    ///
    /// Convenience wrapper around [`Self::new`] for production wiring.
    pub const fn with_defaults(metadata: M, secrets: S, audit: A) -> Self {
        Self::new(metadata, secrets, audit, SystemClock, UuidV4IdGenerator)
    }
}

impl<M, S, A, C, I> RegistryService<M, S, A, C, I>
where
    M: MetadataStore,
    S: SecretStore,
    A: AuditSink,
    C: Clock,
    I: IdGenerator,
{
    /// Construct a registry from its trait dependencies.
    pub const fn new(metadata: M, secrets: S, audit: A, clock: C, id_gen: I) -> Self {
        Self {
            metadata,
            secrets,
            audit,
            clock,
            id_gen,
        }
    }

    // -------------------------------------------------------------------
    // Variables
    // -------------------------------------------------------------------

    /// Create a new variable, route its value to the correct storage tier,
    /// and emit an audit entry.
    ///
    /// The name is validated, then uniqueness is checked. If both succeed,
    /// the variable's id is generated through the injected
    /// [`IdGenerator`] and its timestamps through the injected [`Clock`].
    ///
    /// # Atomicity (v1)
    /// The implementation performs a best-effort compensation on
    /// value-tier failure: if the metadata row was written and the value
    /// write then fails, the metadata row is rolled back so the name is
    /// not permanently reserved. A failure of the audit append after a
    /// successful create is **not** compensated — the variable exists,
    /// only the audit row is missing — and surfaces as
    /// [`CoreError::Metadata`]. Full cross-tier atomic commit is on the
    /// roadmap for the `SQLCipher` backend.
    ///
    /// # Errors
    /// Returns [`CoreError::Metadata`] for validation, uniqueness, or
    /// storage failures; [`CoreError::Secret`] if writing to the secret
    /// store fails. Empty `value` is rejected with
    /// [`MetadataError::Invalid`].
    pub fn create_var(
        &self,
        name: &str,
        group: Group,
        kind: VarKind,
        value: SecretString,
    ) -> Result<VarId, CoreError> {
        Var::validate_name(name)?;
        if value.expose_secret().is_empty() {
            return Err(MetadataError::Invalid("value is empty".into()).into());
        }
        if self.metadata.find_var_by_name(name)?.is_some() {
            return Err(MetadataError::DuplicateName(name.to_owned()).into());
        }

        let id = VarId::from_uuid(self.id_gen.next());
        let now = self.clock.now();
        let length = value.expose_secret().len();
        let var = Var::from_trusted_parts(
            id,
            name.to_owned(),
            group,
            kind,
            Vec::new(),
            length,
            now,
            now,
        );
        self.metadata.upsert_var(&var)?;

        // Route the value to the correct tier. On failure, roll back the
        // metadata row we just wrote so the name does not get permanently
        // reserved on a half-created variable.
        let value_write: Result<(), CoreError> = match kind {
            VarKind::Plain => self
                .metadata
                .set_plain_value(id, value.expose_secret())
                .map_err(Into::into),
            VarKind::Secret => self.secrets.put(id, value).map_err(Into::into),
        };
        if let Err(e) = value_write {
            // Best-effort rollback. The metadata.delete_var call also
            // cascades plain values + links, which is harmless here.
            let _ = self.metadata.delete_var(id);
            return Err(e);
        }

        self.audit_var(id, AuditAction::Created)?;
        Ok(id)
    }

    /// Update the value of an existing variable.
    ///
    /// Routes the new value to the same storage tier the variable was
    /// created with. The metadata record's `length` is refreshed but the
    /// `kind` is preserved.
    ///
    /// The value-tier write happens **before** the metadata length update
    /// so that a partial failure cannot leave metadata advertising a
    /// length that no value in storage actually has.
    ///
    /// # Errors
    /// Returns [`MetadataError::VarNotFound`] if the variable does not
    /// exist, [`MetadataError::Invalid`] if the value is empty, or any
    /// error from the underlying storage tier.
    pub fn update_value(&self, id: VarId, value: SecretString) -> Result<(), CoreError> {
        if value.expose_secret().is_empty() {
            return Err(MetadataError::Invalid("value is empty".into()).into());
        }
        let Some(mut var) = self.metadata.get_var(id)? else {
            return Err(MetadataError::VarNotFound(id).into());
        };
        let length = value.expose_secret().len();

        // Write the value FIRST. If this fails, no metadata change has
        // landed and the stored state still describes the prior value.
        match var.kind() {
            VarKind::Plain => self.metadata.set_plain_value(id, value.expose_secret())?,
            VarKind::Secret => self.secrets.put(id, value)?,
        }
        // Now refresh the length and persist.
        var.set_length(length);
        self.metadata.upsert_var(&var)?;
        self.audit_var(id, AuditAction::Updated)?;
        Ok(())
    }

    /// Retrieve the value of a variable regardless of storage tier.
    ///
    /// If the metadata record claims one [`VarKind`] but the value is
    /// found in the opposite tier, this method returns
    /// [`CoreError::TierMismatch`] rather than silently treating the
    /// situation as "value missing". This surfaces corruption that
    /// bypassed the service layer's normal routing.
    ///
    /// # Errors
    /// Returns any propagated storage error. Returns `Ok(None)` if the
    /// variable does not exist or has no value in the expected tier and
    /// none in the opposite tier either.
    pub fn get_value(&self, id: VarId) -> Result<Option<SecretString>, CoreError> {
        let Some(var) = self.metadata.get_var(id)? else {
            return Ok(None);
        };
        let kind = var.kind();
        match kind {
            VarKind::Plain => {
                if let Some(plain) = self.metadata.get_plain_value(id)? {
                    return Ok(Some(SecretString::from(plain)));
                }
                // Plain-tier miss: probe the opposite tier to surface
                // corruption before returning the indistinguishable None.
                if self.secrets.get(id)?.is_some() {
                    return Err(CoreError::TierMismatch {
                        id,
                        expected: VarKind::Plain,
                        found: VarKind::Secret,
                    });
                }
                Ok(None)
            }
            VarKind::Secret => {
                if let Some(value) = self.secrets.get(id)? {
                    return Ok(Some(value));
                }
                if self.metadata.get_plain_value(id)?.is_some() {
                    return Err(CoreError::TierMismatch {
                        id,
                        expected: VarKind::Secret,
                        found: VarKind::Plain,
                    });
                }
                Ok(None)
            }
        }
    }

    /// Fetch a variable by id.
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn get_var(&self, id: VarId) -> Result<Option<Var>, CoreError> {
        Ok(self.metadata.get_var(id)?)
    }

    /// Fetch a variable by name.
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn find_var_by_name(&self, name: &str) -> Result<Option<Var>, CoreError> {
        Ok(self.metadata.find_var_by_name(name)?)
    }

    /// List every variable matching the supplied filter.
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn list_vars(&self, filter: &VarFilter) -> Result<Vec<Var>, CoreError> {
        Ok(self.metadata.list_vars(filter)?)
    }

    /// Delete a variable and every value stored for it across all tiers.
    /// Idempotent: deleting an absent variable is a successful no-op.
    ///
    /// Order of operations: metadata first (cascading plain values + links),
    /// then a defensive `secrets.delete` for *both* kinds. The metadata-first
    /// order guarantees that a transient secret-tier failure cannot leave a
    /// "zombie" variable that the user can see but never read; the
    /// defensive secret delete on `Plain` kind closes a corruption-recovery
    /// path. A secret-tier failure after metadata has been deleted is
    /// returned to the caller but the variable is, from any reader's
    /// perspective, gone.
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn delete_var(&self, id: VarId) -> Result<(), CoreError> {
        if self.metadata.get_var(id)?.is_none() {
            return Ok(());
        }
        // Metadata first: cascades plain values + links atomically (per
        // backend contract) and removes the variable from every read path.
        self.metadata.delete_var(id)?;
        // Defensive: try the secret tier even when the kind was Plain. The
        // call is idempotent per the trait contract.
        self.secrets.delete(id)?;
        self.audit_var(id, AuditAction::Deleted)?;
        Ok(())
    }

    // -------------------------------------------------------------------
    // Projects
    // -------------------------------------------------------------------

    /// Create a new project.
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn create_project(
        &self,
        name: impl Into<String>,
        path: std::path::PathBuf,
    ) -> Result<ProjectId, CoreError> {
        let id = ProjectId::from_uuid(self.id_gen.next());
        let project = Project::from_parts(id, name.into(), path);
        self.metadata.upsert_project(&project)?;
        self.audit_project(id, None, AuditAction::Created)?;
        Ok(id)
    }

    /// Fetch a project by id.
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn get_project(&self, id: ProjectId) -> Result<Option<Project>, CoreError> {
        Ok(self.metadata.get_project(id)?)
    }

    /// List every project, sorted by the backend's contract (usually by
    /// name for deterministic output).
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
        Ok(self.metadata.list_projects()?)
    }

    /// Delete a project and every linkage it owns.
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn delete_project(&self, id: ProjectId) -> Result<(), CoreError> {
        if self.metadata.get_project(id)?.is_none() {
            return Ok(());
        }
        self.metadata.delete_project(id)?;
        self.audit_project(id, None, AuditAction::Deleted)?;
        Ok(())
    }

    // -------------------------------------------------------------------
    // Linkage
    // -------------------------------------------------------------------

    /// Link a variable to a project under the given profile, optionally
    /// renaming it for the project's context.
    ///
    /// # Errors
    /// Returns [`MetadataError::ProjectNotFound`] or
    /// [`MetadataError::VarNotFound`] if either side is missing.
    pub fn link_var(
        &self,
        project_id: ProjectId,
        var_id: VarId,
        profile: Profile,
        alias: Option<String>,
    ) -> Result<(), CoreError> {
        let link = ProjectVar {
            project_id,
            var_id,
            alias,
            profile,
        };
        self.metadata.upsert_link(&link)?;
        self.audit_project(project_id, Some(var_id), AuditAction::Linked)?;
        Ok(())
    }

    /// Remove a linkage. Idempotent: removing an absent link is `Ok(())`.
    ///
    /// Emits an [`AuditAction::Unlinked`] entry **only when a linkage was
    /// actually removed**. A call on an absent triple is a successful
    /// no-op and is not audited (avoids audit-log pollution that would
    /// degrade forensic value).
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn unlink_var(
        &self,
        project_id: ProjectId,
        var_id: VarId,
        profile: &Profile,
    ) -> Result<(), CoreError> {
        let removed = self.metadata.delete_link(project_id, var_id, profile)?;
        if removed {
            self.audit_project(project_id, Some(var_id), AuditAction::Unlinked)?;
        }
        Ok(())
    }

    /// Every linkage owned by a project.
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn links_for_project(&self, project_id: ProjectId) -> Result<Vec<ProjectVar>, CoreError> {
        Ok(self.metadata.list_links_for_project(project_id)?)
    }

    /// Every linkage that references a given variable.
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn links_for_var(&self, var_id: VarId) -> Result<Vec<ProjectVar>, CoreError> {
        Ok(self.metadata.list_links_for_var(var_id)?)
    }

    // -------------------------------------------------------------------
    // Audit
    // -------------------------------------------------------------------

    /// Return the newest `limit` audit entries.
    ///
    /// # Errors
    /// Propagates storage failures.
    pub fn recent_audit(&self, limit: usize) -> Result<Vec<AuditEntry>, CoreError> {
        Ok(self.audit.list(limit)?)
    }

    // -------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------

    fn audit_var(&self, var_id: VarId, action: AuditAction) -> Result<(), MetadataError> {
        let entry = AuditEntry::from_parts(
            AuditId::from_uuid(self.id_gen.next()),
            action,
            Some(var_id),
            None,
            None,
            self.clock.now(),
        );
        self.audit.append(&entry)
    }

    fn audit_project(
        &self,
        project_id: ProjectId,
        var_id: Option<VarId>,
        action: AuditAction,
    ) -> Result<(), MetadataError> {
        let entry = AuditEntry::from_parts(
            AuditId::from_uuid(self.id_gen.next()),
            action,
            var_id,
            Some(project_id),
            None,
            self.clock.now(),
        );
        self.audit.append(&entry)
    }
}
