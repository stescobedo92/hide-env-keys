//! [`AuditEntry`] records every state-changing operation on the registry.

use std::fmt;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::model::{ProjectId, VarId};

/// Stable identifier of an [`AuditEntry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AuditId(Uuid);

impl AuditId {
    /// Generate a fresh identifier.
    #[must_use]
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing [`Uuid`].
    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Borrow the inner [`Uuid`].
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for AuditId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Classification of state-changing operations.
///
/// Each variant maps to a high-level user-visible action; the dashboard's
/// audit view groups entries by action.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuditAction {
    /// A new [`crate::model::Var`] was created.
    Created,
    /// An existing [`crate::model::Var`] was modified.
    Updated,
    /// A [`crate::model::Var`] was deleted.
    Deleted,
    /// A [`crate::model::Var`] was linked to a [`crate::model::Project`].
    Linked,
    /// A [`crate::model::Var`] was unlinked from a [`crate::model::Project`].
    Unlinked,
    /// A `.env` file was materialized for a project.
    Materialized,
    /// `evault run` injected env vars into a child process.
    Run,
    /// A [`crate::model::Var`] value was copied to the clipboard.
    Copied,
    /// A custom action recorded by an extension.
    Custom(String),
}

impl AuditAction {
    /// Returns the canonical short name used for display.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Deleted => "deleted",
            Self::Linked => "linked",
            Self::Unlinked => "unlinked",
            Self::Materialized => "materialized",
            Self::Run => "run",
            Self::Copied => "copied",
            Self::Custom(s) => s.as_str(),
        }
    }
}

/// A single auditable event.
///
/// Audit entries are append-only. They are persisted in the
/// [`AuditSink`](crate::traits::AuditSink) and may be displayed by the TUI.
///
/// # Examples
/// ```
/// use evault_core::model::{AuditAction, AuditEntry, VarId};
/// let entry = AuditEntry::for_var(VarId::new_v4(), AuditAction::Created);
/// assert_eq!(entry.action(), &AuditAction::Created);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    id: AuditId,
    action: AuditAction,
    var_id: Option<VarId>,
    project_id: Option<ProjectId>,
    note: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    at: OffsetDateTime,
}

impl AuditEntry {
    /// Create a new audit entry referencing a single variable.
    #[must_use]
    pub fn for_var(var_id: VarId, action: AuditAction) -> Self {
        Self {
            id: AuditId::new_v4(),
            action,
            var_id: Some(var_id),
            project_id: None,
            note: None,
            at: OffsetDateTime::now_utc(),
        }
    }

    /// Create a new audit entry referencing a project (and optionally a var).
    #[must_use]
    pub fn for_project(project_id: ProjectId, var_id: Option<VarId>, action: AuditAction) -> Self {
        Self {
            id: AuditId::new_v4(),
            action,
            var_id,
            project_id: Some(project_id),
            note: None,
            at: OffsetDateTime::now_utc(),
        }
    }

    /// Attach a free-form note (e.g. the command that was executed).
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Rehydrate an [`AuditEntry`] from already-stored fields.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn from_parts(
        id: AuditId,
        action: AuditAction,
        var_id: Option<VarId>,
        project_id: Option<ProjectId>,
        note: Option<String>,
        at: OffsetDateTime,
    ) -> Self {
        Self {
            id,
            action,
            var_id,
            project_id,
            note,
            at,
        }
    }

    /// Returns the entry's stable identifier.
    #[must_use]
    pub const fn id(&self) -> AuditId {
        self.id
    }

    /// Returns the action recorded by this entry.
    #[must_use]
    pub const fn action(&self) -> &AuditAction {
        &self.action
    }

    /// Returns the referenced variable, if any.
    #[must_use]
    pub const fn var_id(&self) -> Option<VarId> {
        self.var_id
    }

    /// Returns the referenced project, if any.
    #[must_use]
    pub const fn project_id(&self) -> Option<ProjectId> {
        self.project_id
    }

    /// Returns the free-form note, if any.
    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Returns the timestamp the entry was recorded (UTC).
    #[must_use]
    pub const fn at(&self) -> OffsetDateTime {
        self.at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_var_records_action_and_var() {
        let v = VarId::new_v4();
        let e = AuditEntry::for_var(v, AuditAction::Created);
        assert_eq!(e.var_id(), Some(v));
        assert_eq!(e.project_id(), None);
        assert_eq!(e.action(), &AuditAction::Created);
    }

    #[test]
    fn for_project_records_both_ids() {
        let p = ProjectId::new_v4();
        let v = VarId::new_v4();
        let e = AuditEntry::for_project(p, Some(v), AuditAction::Linked);
        assert_eq!(e.project_id(), Some(p));
        assert_eq!(e.var_id(), Some(v));
    }

    #[test]
    fn with_note_attaches_text() {
        let e =
            AuditEntry::for_var(VarId::new_v4(), AuditAction::Updated).with_note("changed length");
        assert_eq!(e.note(), Some("changed length"));
    }

    #[test]
    fn action_as_str_uses_canonical_names() {
        assert_eq!(AuditAction::Created.as_str(), "created");
        assert_eq!(AuditAction::Custom("imported".into()).as_str(), "imported");
    }
}
