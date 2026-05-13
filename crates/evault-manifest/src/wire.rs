//! On-disk wire format for `evault.toml`.
//!
//! The shapes here mirror the TOML structure 1:1 and convert to / from
//! [`ManifestSnapshot`] under [`ManifestFile::into_snapshot`] /
//! [`ManifestFile::from_snapshot`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use evault_core::error::ManifestError;
use evault_core::model::{
    BindingSource, ManifestBinding, ManifestSnapshot, Profile, ProjectId, VarId,
};

/// Top-level TOML structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestFile {
    project_id: String,
    name: String,
    #[serde(default)]
    vars: BTreeMap<String, BindingDef>,
    #[serde(default)]
    profiles: BTreeMap<String, BTreeMap<String, BindingDef>>,
}

/// One binding entry: either an inline literal or a registry reference.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BindingDef {
    Inline(String),
    Registry(RegistryRef),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegistryRef {
    #[serde(rename = "ref")]
    var_id: String,
}

impl ManifestFile {
    /// Convert the file shape into the in-memory snapshot.
    ///
    /// # Errors
    /// Returns [`ManifestError::Invalid`] on:
    /// - Malformed UUIDs in `project_id` or any `ref`.
    /// - Profile names that fail [`Profile::try_named`].
    /// - Duplicate keys within a profile (TOML parsing rejects these
    ///   before we see them, but we still defend against shape drift).
    pub fn into_snapshot(self) -> Result<ManifestSnapshot, ManifestError> {
        let project_uuid = Uuid::parse_str(&self.project_id)
            .map_err(|_| ManifestError::Invalid("project_id is not a valid UUID".into()))?;

        let mut bindings = Vec::new();
        for (key, def) in self.vars {
            bindings.push(binding_from_def(key, def, Profile::default_profile())?);
        }
        for (profile_name, vars) in self.profiles {
            let profile = Profile::try_named(profile_name)?;
            for (key, def) in vars {
                bindings.push(binding_from_def(key, def, profile.clone())?);
            }
        }

        Ok(ManifestSnapshot {
            project_id: ProjectId::from_uuid(project_uuid),
            name: self.name,
            bindings,
        })
    }

    /// Convert a snapshot to the file shape, ready for TOML serialization.
    pub fn from_snapshot(snap: &ManifestSnapshot) -> Self {
        let mut vars: BTreeMap<String, BindingDef> = BTreeMap::new();
        let mut profiles: BTreeMap<String, BTreeMap<String, BindingDef>> = BTreeMap::new();

        for binding in &snap.bindings {
            let def = def_from_binding(&binding.source);
            if binding.profile.is_default() {
                vars.insert(binding.key.clone(), def);
            } else {
                profiles
                    .entry(binding.profile.as_str().to_owned())
                    .or_default()
                    .insert(binding.key.clone(), def);
            }
        }

        Self {
            project_id: snap.project_id.as_uuid().to_string(),
            name: snap.name.clone(),
            vars,
            profiles,
        }
    }
}

fn binding_from_def(
    key: String,
    def: BindingDef,
    profile: Profile,
) -> Result<ManifestBinding, ManifestError> {
    let source = match def {
        BindingDef::Inline(value) => BindingSource::Inline { value },
        BindingDef::Registry(RegistryRef { var_id }) => {
            let uuid = Uuid::parse_str(&var_id).map_err(|_| {
                ManifestError::Invalid(format!("binding {key:?} ref is not a valid UUID"))
            })?;
            BindingSource::Registry {
                var_id: VarId::from_uuid(uuid),
            }
        }
    };
    Ok(ManifestBinding {
        key,
        source,
        profile,
    })
}

fn def_from_binding(source: &BindingSource) -> BindingDef {
    match source {
        BindingSource::Inline { value } => BindingDef::Inline(value.clone()),
        BindingSource::Registry { var_id } => BindingDef::Registry(RegistryRef {
            var_id: var_id.as_uuid().to_string(),
        }),
    }
}
