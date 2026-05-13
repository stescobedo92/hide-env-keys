//! Integration tests for `SqlCipherMetadataStore` using a tmpdir database.
//!
//! These exercise the full CRUD surface of every trait method against a
//! real file-backed connection so we catch SQL string errors, transaction
//! bugs, and migration regressions that the in-memory tests miss.

// Test-helper functions outside `#[test]` (`fresh_store`, `make_var`)
// use `.expect(...)` for clarity. Production code is bound by the
// workspace `expect_used` lint; this allow is scoped to the test crate.
#![allow(clippy::expect_used)]

use std::path::PathBuf;

use evault_core::crypto::MasterKey;
use evault_core::error::MetadataError;
use evault_core::model::{
    AuditAction, AuditEntry, Group, Profile, Project, ProjectVar, Var, VarFilter, VarId, VarKind,
};
use evault_core::traits::{AuditSink, MetadataStore};
use evault_store_sqlcipher::SqlCipherMetadataStore;
use tempfile::TempDir;

/// Create a fresh store at a tmpdir path. The [`TempDir`] handle must
/// outlive the store; both are returned.
fn fresh_store() -> (TempDir, SqlCipherMetadataStore, MasterKey) {
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("evault.sqlite");
    let key = MasterKey::generate().expect("OS RNG");
    let store = SqlCipherMetadataStore::open(&path, &key).expect("open");
    (dir, store, key)
}

fn make_var(name: &str, kind: VarKind) -> Var {
    Var::try_new(name, Group::Project, kind).expect("valid var")
}

#[test]
fn open_then_reopen_preserves_data_with_same_key() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("evault.sqlite");
    let key = MasterKey::generate().unwrap();

    let store = SqlCipherMetadataStore::open(&path, &key).unwrap();
    let var = make_var("DATABASE_URL", VarKind::Plain);
    let id = var.id();
    store.upsert_var(&var).unwrap();
    drop(store);

    let store2 = SqlCipherMetadataStore::open(&path, &key).unwrap();
    let fetched = store2.get_var(id).unwrap().expect("present");
    assert_eq!(fetched.name(), "DATABASE_URL");
}

#[test]
fn reopen_with_different_key_is_rejected() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("evault.sqlite");
    let key_a = MasterKey::generate().unwrap();
    let key_b = MasterKey::generate().unwrap();

    let store = SqlCipherMetadataStore::open(&path, &key_a).unwrap();
    store
        .upsert_var(&make_var("DATABASE_URL", VarKind::Plain))
        .unwrap();
    drop(store);

    let result = SqlCipherMetadataStore::open(&path, &key_b);
    assert!(
        result.is_err(),
        "expected open with wrong key to fail, got Ok"
    );
}

#[test]
fn duplicate_var_name_returns_typed_error() {
    let (_dir, store, _key) = fresh_store();
    let a = make_var("FOO", VarKind::Plain);
    let b = make_var("FOO", VarKind::Plain);
    store.upsert_var(&a).unwrap();
    match store.upsert_var(&b) {
        Err(MetadataError::DuplicateName(name)) => assert_eq!(name, "FOO"),
        other => panic!("expected DuplicateName, got {other:?}"),
    }
}

#[test]
fn set_plain_value_rejects_secret_kind() {
    let (_dir, store, _key) = fresh_store();
    let secret = make_var("API_KEY", VarKind::Secret);
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
    // Plain-value tier is empty.
    assert!(store.get_plain_value(id).unwrap().is_none());
}

#[test]
fn delete_var_cascades_to_plain_values_and_links() {
    let (_dir, store, _key) = fresh_store();
    let var = make_var("FOO", VarKind::Plain);
    let var_id = var.id();
    store.upsert_var(&var).unwrap();
    store.set_plain_value(var_id, "v").unwrap();

    let project = Project::new("proj", PathBuf::from("."));
    store.upsert_project(&project).unwrap();
    store
        .upsert_link(&ProjectVar::new(project.id(), var_id))
        .unwrap();

    store.delete_var(var_id).unwrap();
    assert!(store.get_var(var_id).unwrap().is_none());
    assert!(store.get_plain_value(var_id).unwrap().is_none());
    assert!(store.list_links_for_var(var_id).unwrap().is_empty());
}

#[test]
fn list_vars_is_filtered_and_sorted_by_name() {
    let (_dir, store, _key) = fresh_store();
    for name in ["FOO", "BAR", "BAZ"] {
        store.upsert_var(&make_var(name, VarKind::Plain)).unwrap();
    }
    let listed = store.list_vars(&VarFilter::new()).unwrap();
    let names: Vec<&str> = listed.iter().map(Var::name).collect();
    assert_eq!(names, vec!["BAR", "BAZ", "FOO"]);

    let filtered = store
        .list_vars(&VarFilter {
            name_contains: Some("ba".into()),
            ..VarFilter::new()
        })
        .unwrap();
    let names: Vec<&str> = filtered.iter().map(Var::name).collect();
    assert_eq!(names, vec!["BAR", "BAZ"]);
}

#[test]
fn upsert_link_replaces_existing_triple() {
    let (_dir, store, _key) = fresh_store();
    let project = Project::new("p", PathBuf::from("."));
    store.upsert_project(&project).unwrap();
    let var = make_var("FOO", VarKind::Plain);
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
fn delete_link_reports_whether_a_row_was_removed() {
    let (_dir, store, _key) = fresh_store();
    let project = Project::new("p", PathBuf::from("."));
    store.upsert_project(&project).unwrap();

    // Absent triple: Ok(false).
    let removed = store
        .delete_link(project.id(), VarId::new_v4(), &Profile::default_profile())
        .unwrap();
    assert!(!removed);

    let var = make_var("FOO", VarKind::Plain);
    store.upsert_var(&var).unwrap();
    store
        .upsert_link(&ProjectVar::new(project.id(), var.id()))
        .unwrap();

    // Present triple: Ok(true).
    let removed = store
        .delete_link(project.id(), var.id(), &Profile::default_profile())
        .unwrap();
    assert!(removed);
}

#[test]
fn upsert_link_rejects_missing_project_or_var() {
    let (_dir, store, _key) = fresh_store();
    let link = ProjectVar::new(evault_core::model::ProjectId::new_v4(), VarId::new_v4());
    assert!(matches!(
        store.upsert_link(&link),
        Err(MetadataError::ProjectNotFound(_))
    ));
}

#[test]
fn delete_project_cascades_links() {
    let (_dir, store, _key) = fresh_store();
    let project = Project::new("p", PathBuf::from("."));
    store.upsert_project(&project).unwrap();
    let var = make_var("FOO", VarKind::Plain);
    store.upsert_var(&var).unwrap();
    store
        .upsert_link(&ProjectVar::new(project.id(), var.id()))
        .unwrap();

    store.delete_project(project.id()).unwrap();
    assert!(store.list_links_for_var(var.id()).unwrap().is_empty());
}

#[test]
fn audit_append_then_list_returns_newest_first() {
    let (_dir, store, _key) = fresh_store();
    let v = VarId::new_v4();
    store
        .append(&AuditEntry::for_var(v, AuditAction::Created))
        .unwrap();
    // Ensure the second entry has a strictly later timestamp so ORDER BY
    // resolves deterministically.
    std::thread::sleep(std::time::Duration::from_millis(2));
    store
        .append(&AuditEntry::for_var(v, AuditAction::Updated))
        .unwrap();
    let listed = store.list(10).unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].action(), &AuditAction::Updated);
    assert_eq!(listed[1].action(), &AuditAction::Created);
}

#[test]
fn audit_list_limit_zero_returns_empty() {
    let (_dir, store, _key) = fresh_store();
    store
        .append(&AuditEntry::for_var(VarId::new_v4(), AuditAction::Created))
        .unwrap();
    assert!(store.list(0).unwrap().is_empty());
}

#[test]
fn upsert_var_idempotent_for_same_id() {
    let (_dir, store, _key) = fresh_store();
    let var = make_var("FOO", VarKind::Plain);
    store.upsert_var(&var).unwrap();
    store.upsert_var(&var).unwrap();
    assert_eq!(store.list_vars(&VarFilter::new()).unwrap().len(), 1);
}

#[test]
fn set_plain_value_on_missing_var_returns_var_not_found() {
    let (_dir, store, _key) = fresh_store();
    let missing = VarId::new_v4();
    match store.set_plain_value(missing, "x") {
        Err(MetadataError::VarNotFound(id)) => assert_eq!(id, missing),
        other => panic!("expected VarNotFound, got {other:?}"),
    }
}
