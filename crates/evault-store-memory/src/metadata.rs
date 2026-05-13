//! [`MemoryMetadataStore`] — in-memory implementation of
//! [`evault_core::traits::MetadataStore`].

use std::collections::HashMap;
use std::sync::RwLock;

use evault_core::error::MetadataError;
use evault_core::model::{Profile, Project, ProjectId, ProjectVar, Var, VarFilter, VarId, VarKind};
use evault_core::traits::MetadataStore;

/// In-memory store for variables, projects, plain values, and project ↔ var
/// linkage.
///
/// All operations take an immutable `&self`; interior mutability is provided
/// by a single [`RwLock`]. Suitable for fast tests and short-lived processes;
/// **not** suitable for persistence.
///
/// # Examples
/// ```
/// use evault_store_memory::MemoryMetadataStore;
/// use evault_core::traits::MetadataStore;
///
/// let store = MemoryMetadataStore::new();
/// assert!(store.list_projects().unwrap().is_empty());
/// ```
#[derive(Debug, Default)]
pub struct MemoryMetadataStore {
    inner: RwLock<State>,
}

#[derive(Debug, Default)]
struct State {
    vars: HashMap<VarId, Var>,
    plain_values: HashMap<VarId, String>,
    projects: HashMap<ProjectId, Project>,
    links: Vec<ProjectVar>,
}

impl MemoryMetadataStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, State>, MetadataError> {
        self.inner
            .read()
            .map_err(|_| MetadataError::Backend("in-memory store lock poisoned".into()))
    }

    fn write(&self) -> Result<std::sync::RwLockWriteGuard<'_, State>, MetadataError> {
        self.inner
            .write()
            .map_err(|_| MetadataError::Backend("in-memory store lock poisoned".into()))
    }
}

impl MetadataStore for MemoryMetadataStore {
    fn upsert_var(&self, var: &Var) -> Result<(), MetadataError> {
        let mut state = self.write()?;
        // Enforce name uniqueness across the registry. Allow the same name to
        // re-occur on the *same* id (idempotent upsert).
        for existing in state.vars.values() {
            if existing.name() == var.name() && existing.id() != var.id() {
                return Err(MetadataError::DuplicateName(var.name().to_owned()));
            }
        }
        state.vars.insert(var.id(), var.clone());
        Ok(())
    }

    fn get_var(&self, id: VarId) -> Result<Option<Var>, MetadataError> {
        let state = self.read()?;
        Ok(state.vars.get(&id).cloned())
    }

    fn find_var_by_name(&self, name: &str) -> Result<Option<Var>, MetadataError> {
        let state = self.read()?;
        Ok(state.vars.values().find(|v| v.name() == name).cloned())
    }

    fn list_vars(&self, filter: &VarFilter) -> Result<Vec<Var>, MetadataError> {
        let state = self.read()?;
        let mut result: Vec<Var> = state
            .vars
            .values()
            .filter(|v| filter.matches(v))
            .cloned()
            .collect();
        // Deterministic order by name to keep tests stable across HashMap
        // iteration nondeterminism.
        result.sort_by(|a, b| a.name().cmp(b.name()));
        Ok(result)
    }

    fn delete_var(&self, id: VarId) -> Result<(), MetadataError> {
        let mut state = self.write()?;
        state.vars.remove(&id);
        state.plain_values.remove(&id);
        state.links.retain(|l| l.var_id != id);
        Ok(())
    }

    fn get_plain_value(&self, id: VarId) -> Result<Option<String>, MetadataError> {
        let state = self.read()?;
        Ok(state.plain_values.get(&id).cloned())
    }

    fn set_plain_value(&self, id: VarId, value: &str) -> Result<(), MetadataError> {
        let mut state = self.write()?;
        let Some(var) = state.vars.get(&id) else {
            return Err(MetadataError::VarNotFound(id));
        };
        let actual = var.kind();
        if actual != VarKind::Plain {
            // Defense against silent secret leakage into the plain-value tier:
            // refuse loudly rather than accept the mis-typed write.
            return Err(MetadataError::KindMismatch {
                id,
                expected: VarKind::Plain,
                actual,
            });
        }
        state.plain_values.insert(id, value.to_owned());
        Ok(())
    }

    fn upsert_project(&self, project: &Project) -> Result<(), MetadataError> {
        let mut state = self.write()?;
        state.projects.insert(project.id(), project.clone());
        Ok(())
    }

    fn get_project(&self, id: ProjectId) -> Result<Option<Project>, MetadataError> {
        let state = self.read()?;
        Ok(state.projects.get(&id).cloned())
    }

    fn list_projects(&self) -> Result<Vec<Project>, MetadataError> {
        let state = self.read()?;
        let mut result: Vec<Project> = state.projects.values().cloned().collect();
        result.sort_by(|a, b| a.name().cmp(b.name()));
        Ok(result)
    }

    fn delete_project(&self, id: ProjectId) -> Result<(), MetadataError> {
        let mut state = self.write()?;
        state.projects.remove(&id);
        state.links.retain(|l| l.project_id != id);
        Ok(())
    }

    fn upsert_link(&self, link: &ProjectVar) -> Result<(), MetadataError> {
        let mut state = self.write()?;
        if !state.projects.contains_key(&link.project_id) {
            return Err(MetadataError::ProjectNotFound(link.project_id));
        }
        if !state.vars.contains_key(&link.var_id) {
            return Err(MetadataError::VarNotFound(link.var_id));
        }
        // Replace any existing link for this (project, var, profile) triple.
        state.links.retain(|l| {
            !(l.project_id == link.project_id
                && l.var_id == link.var_id
                && l.profile == link.profile)
        });
        state.links.push(link.clone());
        Ok(())
    }

    fn delete_link(
        &self,
        project_id: ProjectId,
        var_id: VarId,
        profile: &Profile,
    ) -> Result<bool, MetadataError> {
        let mut state = self.write()?;
        let before = state.links.len();
        state.links.retain(|l| {
            !(l.project_id == project_id && l.var_id == var_id && &l.profile == profile)
        });
        Ok(state.links.len() != before)
    }

    fn list_links_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectVar>, MetadataError> {
        let state = self.read()?;
        Ok(state
            .links
            .iter()
            .filter(|l| l.project_id == project_id)
            .cloned()
            .collect())
    }

    fn list_links_for_var(&self, var_id: VarId) -> Result<Vec<ProjectVar>, MetadataError> {
        let state = self.read()?;
        Ok(state
            .links
            .iter()
            .filter(|l| l.var_id == var_id)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evault_core::model::{Group, Project, VarKind};
    use std::path::PathBuf;

    fn make_var(name: &str) -> Var {
        Var::try_new(name, Group::User, VarKind::Plain).expect("valid var")
    }

    #[test]
    fn upsert_then_get_var_roundtrips() {
        let store = MemoryMetadataStore::new();
        let var = make_var("FOO");
        store.upsert_var(&var).unwrap();
        let fetched = store.get_var(var.id()).unwrap().expect("present");
        assert_eq!(fetched.name(), "FOO");
    }

    #[test]
    fn upsert_var_rejects_duplicate_name_for_different_id() {
        let store = MemoryMetadataStore::new();
        store.upsert_var(&make_var("FOO")).unwrap();
        let other = make_var("FOO");
        match store.upsert_var(&other) {
            Err(MetadataError::DuplicateName(name)) => assert_eq!(name, "FOO"),
            other => panic!("expected DuplicateName, got {other:?}"),
        }
    }

    #[test]
    fn upsert_var_is_idempotent_for_same_id() {
        let store = MemoryMetadataStore::new();
        let var = make_var("FOO");
        store.upsert_var(&var).unwrap();
        store.upsert_var(&var).unwrap();
        let listed = store.list_vars(&VarFilter::new()).unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[test]
    fn delete_var_removes_metadata_plain_value_and_links() {
        let store = MemoryMetadataStore::new();
        let var = make_var("FOO");
        store.upsert_var(&var).unwrap();
        let project = Project::new("proj", PathBuf::from("."));
        store.upsert_project(&project).unwrap();
        store.set_plain_value(var.id(), "1").unwrap();
        store
            .upsert_link(&ProjectVar::new(project.id(), var.id()))
            .unwrap();

        store.delete_var(var.id()).unwrap();

        assert!(store.get_var(var.id()).unwrap().is_none());
        assert!(store.get_plain_value(var.id()).unwrap().is_none());
        assert!(store
            .list_links_for_project(project.id())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn list_vars_is_filtered_and_sorted_by_name() {
        let store = MemoryMetadataStore::new();
        store.upsert_var(&make_var("BAR")).unwrap();
        store.upsert_var(&make_var("FOO")).unwrap();
        store.upsert_var(&make_var("BAZ")).unwrap();
        let listed = store.list_vars(&VarFilter::new()).unwrap();
        let names: Vec<&str> = listed.iter().map(Var::name).collect();
        assert_eq!(names, vec!["BAR", "BAZ", "FOO"]);
    }

    #[test]
    fn set_plain_value_requires_existing_var() {
        let store = MemoryMetadataStore::new();
        let missing = VarId::new_v4();
        match store.set_plain_value(missing, "x") {
            Err(MetadataError::VarNotFound(id)) => assert_eq!(id, missing),
            other => panic!("expected VarNotFound, got {other:?}"),
        }
    }

    #[test]
    fn set_plain_value_rejects_secret_kind() {
        let store = MemoryMetadataStore::new();
        let secret = Var::try_new("API_KEY", Group::User, VarKind::Secret).expect("valid");
        let id = secret.id();
        store.upsert_var(&secret).unwrap();
        match store.set_plain_value(id, "leak") {
            Err(MetadataError::KindMismatch {
                id: bad_id,
                expected,
                actual,
            }) => {
                assert_eq!(bad_id, id);
                assert_eq!(expected, VarKind::Plain);
                assert_eq!(actual, VarKind::Secret);
            }
            other => panic!("expected KindMismatch, got {other:?}"),
        }
        // And nothing was actually written to the plain-value side table.
        assert!(store.get_plain_value(id).unwrap().is_none());
    }

    #[test]
    fn delete_link_returns_false_on_absent_triple() {
        let store = MemoryMetadataStore::new();
        let project = Project::new("p", PathBuf::from("."));
        store.upsert_project(&project).unwrap();
        // Absent link: returns Ok(false), no error.
        let removed = store
            .delete_link(project.id(), VarId::new_v4(), &Profile::default_profile())
            .unwrap();
        assert!(!removed);
        assert!(store
            .list_links_for_project(project.id())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn delete_link_returns_true_when_a_row_is_removed() {
        let store = MemoryMetadataStore::new();
        let project = Project::new("p", PathBuf::from("."));
        store.upsert_project(&project).unwrap();
        let var = make_var("FOO");
        store.upsert_var(&var).unwrap();
        store
            .upsert_link(&ProjectVar::new(project.id(), var.id()))
            .unwrap();
        let removed = store
            .delete_link(project.id(), var.id(), &Profile::default_profile())
            .unwrap();
        assert!(removed);
    }

    #[test]
    fn find_var_by_name_returns_none_when_absent() {
        let store = MemoryMetadataStore::new();
        assert!(store.find_var_by_name("MISSING").unwrap().is_none());
    }

    #[test]
    fn upsert_link_rejects_missing_project_or_var() {
        let store = MemoryMetadataStore::new();
        let pid = ProjectId::new_v4();
        let vid = VarId::new_v4();
        match store.upsert_link(&ProjectVar::new(pid, vid)) {
            Err(MetadataError::ProjectNotFound(id)) => assert_eq!(id, pid),
            other => panic!("expected ProjectNotFound, got {other:?}"),
        }
        let project = Project::new("p", PathBuf::from("."));
        store.upsert_project(&project).unwrap();
        match store.upsert_link(&ProjectVar::new(project.id(), vid)) {
            Err(MetadataError::VarNotFound(id)) => assert_eq!(id, vid),
            other => panic!("expected VarNotFound, got {other:?}"),
        }
    }

    #[test]
    fn upsert_link_replaces_existing_triple() {
        let store = MemoryMetadataStore::new();
        let project = Project::new("p", PathBuf::from("."));
        store.upsert_project(&project).unwrap();
        let var = make_var("FOO");
        store.upsert_var(&var).unwrap();

        let mut link = ProjectVar::new(project.id(), var.id());
        link.alias = Some("FOO_LOCAL".into());
        store.upsert_link(&link).unwrap();
        let mut link2 = ProjectVar::new(project.id(), var.id());
        link2.alias = Some("FOO_OTHER".into());
        store.upsert_link(&link2).unwrap();

        let listed = store.list_links_for_project(project.id()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].alias.as_deref(), Some("FOO_OTHER"));
    }

    #[test]
    fn delete_project_cascades_links() {
        let store = MemoryMetadataStore::new();
        let project = Project::new("p", PathBuf::from("."));
        store.upsert_project(&project).unwrap();
        let var = make_var("FOO");
        store.upsert_var(&var).unwrap();
        store
            .upsert_link(&ProjectVar::new(project.id(), var.id()))
            .unwrap();

        store.delete_project(project.id()).unwrap();
        assert!(store.list_links_for_var(var.id()).unwrap().is_empty());
    }
}
