//! [`Project`] entity and its [`Var`](crate::model::Var) linkage.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{Profile, VarId};

/// Stable identifier of a [`Project`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(Uuid);

impl ProjectId {
    /// Generate a fresh identifier using [`Uuid::new_v4`].
    #[must_use]
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing [`Uuid`] (used when rehydrating from a backend).
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

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// A project that has variables linked to it via an `evault.toml` manifest.
///
/// `Project` records the location of the manifest and a human-friendly name.
/// Linkage details live in [`ProjectVar`] records.
///
/// # Examples
/// ```
/// use evault_core::model::Project;
/// use std::path::PathBuf;
///
/// let p = Project::new("my-app", PathBuf::from("./my-app"));
/// assert_eq!(p.name(), "my-app");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    id: ProjectId,
    name: String,
    path: PathBuf,
}

impl Project {
    /// Create a new [`Project`] with a fresh identifier.
    ///
    /// `path` is the directory that contains (or will contain) the
    /// `evault.toml` manifest.
    pub fn new(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            id: ProjectId::new_v4(),
            name: name.into(),
            path,
        }
    }

    /// Rehydrate a [`Project`] from already-stored fields.
    #[must_use]
    pub const fn from_parts(id: ProjectId, name: String, path: PathBuf) -> Self {
        Self { id, name, path }
    }

    /// Returns the project's identifier.
    #[must_use]
    pub const fn id(&self) -> ProjectId {
        self.id
    }

    /// Returns the project name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the path that contains the project's `evault.toml`.
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        self.path.as_path()
    }
}

/// Linkage record between a [`Project`] and a [`crate::model::Var`].
///
/// Each link can optionally rename the variable in the project context
/// (`alias`) and is scoped to a [`Profile`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectVar {
    /// Owning project.
    pub project_id: ProjectId,
    /// Linked variable in the central registry.
    pub var_id: VarId,
    /// Optional rename: the project sees the variable under this name.
    pub alias: Option<String>,
    /// Profile this link applies to (`default`, `dev`, `prod`, etc.).
    pub profile: Profile,
}

impl ProjectVar {
    /// Construct a linkage for the default profile with no alias.
    #[must_use]
    pub fn new(project_id: ProjectId, var_id: VarId) -> Self {
        Self {
            project_id,
            var_id,
            alias: None,
            profile: Profile::default_profile(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_has_fresh_id() {
        let a = Project::new("a", PathBuf::from("./a"));
        let b = Project::new("b", PathBuf::from("./b"));
        assert_ne!(a.id(), b.id());
    }

    #[test]
    fn project_var_default_profile_no_alias() {
        let pid = ProjectId::new_v4();
        let vid = VarId::new_v4();
        let pv = ProjectVar::new(pid, vid);
        assert_eq!(pv.project_id, pid);
        assert_eq!(pv.var_id, vid);
        assert!(pv.alias.is_none());
        assert!(pv.profile.is_default());
    }

    #[test]
    fn project_id_display_matches_uuid() {
        let id = ProjectId::new_v4();
        assert_eq!(id.to_string(), id.as_uuid().to_string());
    }
}
