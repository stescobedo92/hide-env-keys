//! Project manifest types.
//!
//! The on-disk manifest format is owned by `evault-manifest` (TOML), but the
//! in-memory representation is part of the domain model so that services
//! (e.g. materializer, runner) can consume it without depending on a
//! particular file format.

use serde::{Deserialize, Serialize};

use crate::model::{Profile, ProjectId, VarId};

/// In-memory snapshot of a project manifest.
///
/// A snapshot captures everything required to materialize a `.env` or inject
/// env vars into a child process: which keys the project exposes, where each
/// value comes from, and under which profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestSnapshot {
    /// Identifier of the owning [`crate::model::Project`].
    pub project_id: ProjectId,

    /// Human-friendly project name (kept in the manifest for portability).
    pub name: String,

    /// Variable bindings declared by the project, across all profiles.
    pub bindings: Vec<ManifestBinding>,
}

impl ManifestSnapshot {
    /// Filter the bindings to those that apply for the given profile.
    ///
    /// Default-profile bindings are always included; named-profile bindings
    /// override default ones when they share a key.
    #[must_use]
    pub fn effective_bindings(&self, profile: &Profile) -> Vec<&ManifestBinding> {
        let mut by_key: std::collections::BTreeMap<&str, &ManifestBinding> =
            std::collections::BTreeMap::new();
        // First insert defaults...
        for b in &self.bindings {
            if b.profile.is_default() {
                by_key.insert(b.key.as_str(), b);
            }
        }
        // ...then override with profile-specific bindings.
        for b in &self.bindings {
            if b.profile == *profile {
                by_key.insert(b.key.as_str(), b);
            }
        }
        by_key.into_values().collect()
    }
}

/// A single variable binding declared by a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestBinding {
    /// Environment-variable name the project will see (e.g. `DATABASE_URL`).
    pub key: String,

    /// Source of the value.
    pub source: BindingSource,

    /// Profile this binding applies to.
    pub profile: Profile,
}

/// Where a binding's value comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BindingSource {
    /// Reference to a variable in the central registry.
    Registry {
        /// Identifier of the referenced [`crate::model::Var`].
        var_id: VarId,
    },
    /// Literal value embedded in the manifest (use only for non-sensitive values).
    Inline {
        /// The literal value.
        value: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(key: &str, profile: Profile, value: &str) -> ManifestBinding {
        ManifestBinding {
            key: key.to_owned(),
            source: BindingSource::Inline {
                value: value.to_owned(),
            },
            profile,
        }
    }

    #[test]
    fn effective_default_profile_keeps_defaults() {
        let snap = ManifestSnapshot {
            project_id: ProjectId::new_v4(),
            name: "x".into(),
            bindings: vec![
                binding("FOO", Profile::default_profile(), "1"),
                binding("BAR", Profile::default_profile(), "2"),
            ],
        };
        let effective = snap.effective_bindings(&Profile::default_profile());
        assert_eq!(effective.len(), 2);
    }

    #[test]
    fn named_profile_overrides_default_on_key() {
        let snap = ManifestSnapshot {
            project_id: ProjectId::new_v4(),
            name: "x".into(),
            bindings: vec![
                binding("FOO", Profile::default_profile(), "default"),
                binding("FOO", Profile::named("dev"), "dev"),
                binding("BAR", Profile::default_profile(), "common"),
            ],
        };
        let dev = snap.effective_bindings(&Profile::named("dev"));
        // Two effective keys: FOO (overridden) and BAR (default).
        assert_eq!(dev.len(), 2);
        let foo = dev.iter().find(|b| b.key == "FOO").expect("FOO present");
        match &foo.source {
            BindingSource::Inline { value } => assert_eq!(value, "dev"),
            BindingSource::Registry { .. } => panic!("unexpected source"),
        }
    }
}
