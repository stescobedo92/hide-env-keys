//! Integration tests for `FileManifestIo`.
//!
//! Each test uses `tempfile::TempDir` so concurrent runs don't clash and
//! no leaked files can survive a panic.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use evault_core::error::ManifestError;
use evault_core::model::{
    BindingSource, ManifestBinding, ManifestSnapshot, Profile, ProjectId, VarId,
};
use evault_core::traits::ManifestIo;
use evault_manifest::FileManifestIo;
use tempfile::TempDir;

fn sample_snapshot() -> ManifestSnapshot {
    let var_id = VarId::new_v4();
    ManifestSnapshot {
        project_id: ProjectId::new_v4(),
        name: "demo".into(),
        bindings: vec![
            ManifestBinding {
                key: "NODE_ENV".into(),
                source: BindingSource::Inline {
                    value: "production".into(),
                },
                profile: Profile::default_profile(),
            },
            ManifestBinding {
                key: "DATABASE_URL".into(),
                source: BindingSource::Registry { var_id },
                profile: Profile::default_profile(),
            },
            ManifestBinding {
                key: "NODE_ENV".into(),
                source: BindingSource::Inline {
                    value: "development".into(),
                },
                profile: Profile::named("dev"),
            },
        ],
    }
}

#[test]
fn save_then_load_roundtrips_a_snapshot() {
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("evault.toml");
    let io = FileManifestIo::new();

    let snap = sample_snapshot();
    io.save(&path, &snap).expect("save");
    let loaded = io.load(&path).expect("load");

    assert_eq!(loaded.project_id, snap.project_id);
    assert_eq!(loaded.name, snap.name);
    // BTreeMap-ordered serialization means we can compare as sets.
    let mut loaded_keys: Vec<_> = loaded
        .bindings
        .iter()
        .map(|b| (b.key.clone(), b.profile.as_str().to_owned()))
        .collect();
    let mut original_keys: Vec<_> = snap
        .bindings
        .iter()
        .map(|b| (b.key.clone(), b.profile.as_str().to_owned()))
        .collect();
    loaded_keys.sort();
    original_keys.sort();
    assert_eq!(loaded_keys, original_keys);
}

#[test]
fn load_missing_file_returns_not_found() {
    let io = FileManifestIo::new();
    let path = PathBuf::from("/nonexistent/evault-test-missing.toml");
    match io.load(&path) {
        Err(ManifestError::NotFound(p)) => assert_eq!(p, path),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn load_malformed_toml_returns_parse_error() {
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "this is not valid = toml = at all =\n").expect("write");
    let io = FileManifestIo::new();
    match io.load(&path) {
        Err(ManifestError::Parse(_)) => {}
        other => panic!("expected Parse, got {other:?}"),
    }
}

#[test]
fn load_invalid_project_uuid_returns_invalid_error() {
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("bad-uuid.toml");
    std::fs::write(&path, "project_id = \"not-a-uuid\"\nname = \"x\"\n[vars]\n").expect("write");
    let io = FileManifestIo::new();
    match io.load(&path) {
        Err(ManifestError::Invalid(msg)) => assert!(msg.contains("project_id")),
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn load_invalid_ref_uuid_returns_invalid_error() {
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("bad-ref.toml");
    std::fs::write(
        &path,
        "project_id = \"550e8400-e29b-41d4-a716-446655440000\"\n\
         name = \"demo\"\n\
         [vars]\n\
         FOO = { ref = \"not-a-uuid\" }\n",
    )
    .expect("write");
    let io = FileManifestIo::new();
    match io.load(&path) {
        Err(ManifestError::Invalid(msg)) => assert!(msg.contains("FOO")),
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn load_invalid_profile_name_returns_invalid_error() {
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("bad-profile.toml");
    std::fs::write(
        &path,
        "project_id = \"550e8400-e29b-41d4-a716-446655440000\"\n\
         name = \"demo\"\n\
         [vars]\n\
         FOO = \"v\"\n\
         [profiles.\"with space\"]\n\
         FOO = \"w\"\n",
    )
    .expect("write");
    let io = FileManifestIo::new();
    match io.load(&path) {
        Err(ManifestError::Invalid(_)) => {}
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn save_writes_a_default_section_for_default_profile_bindings() {
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("evault.toml");
    let io = FileManifestIo::new();

    let snap = ManifestSnapshot {
        project_id: ProjectId::new_v4(),
        name: "demo".into(),
        bindings: vec![ManifestBinding {
            key: "FOO".into(),
            source: BindingSource::Inline {
                value: "bar".into(),
            },
            profile: Profile::default_profile(),
        }],
    };
    io.save(&path, &snap).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("[vars]"));
    assert!(text.contains("FOO"));
    // Default-profile bindings don't land under [profiles.default].
    assert!(!text.contains("[profiles.default]"));
}

#[test]
fn save_writes_a_profile_section_for_named_profile_bindings() {
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("evault.toml");
    let io = FileManifestIo::new();

    let snap = ManifestSnapshot {
        project_id: ProjectId::new_v4(),
        name: "demo".into(),
        bindings: vec![ManifestBinding {
            key: "FOO".into(),
            source: BindingSource::Inline {
                value: "bar".into(),
            },
            profile: Profile::named("dev"),
        }],
    };
    io.save(&path, &snap).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("[profiles.dev]"));
}

#[test]
fn save_is_atomic_no_partial_file_visible_on_success() {
    // We can't easily induce a mid-write crash from a test, but we can
    // confirm the operation reaches the destination and that no
    // tempfile is left behind.
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("evault.toml");
    let io = FileManifestIo::new();
    io.save(&path, &sample_snapshot()).expect("save");

    // After a successful save, only the target file should exist.
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .expect("read_dir")
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "tempfile should have been renamed: {entries:?}"
    );
}
