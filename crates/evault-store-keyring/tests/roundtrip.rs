//! Integration tests against the real OS keyring.
//!
//! These tests touch the platform's native credential store, so they each
//! use a unique service identifier to avoid colliding with production data
//! or with parallel test runs, and always clean up the credentials they
//! create.
//!
//! On systems without a usable backend (notably headless Linux without a
//! D-Bus / Secret Service daemon) the tests skip themselves with a
//! diagnostic rather than failing — the trait contract for
//! [`SecretError::Unavailable`] is what we test elsewhere; here we only
//! exercise the happy path on a backend that is actually present.

// Tests use expect/panic/eprintln freely; production code is bound by the
// workspace lints. The clippy.toml `allow-*-in-tests` flags handle inline
// `#[test]` functions, but integration-test crates need an explicit allow
// at the crate root for the helper functions outside `#[test]`.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    clippy::map_unwrap_or
)]

use evault_core::crypto::{ExposeSecret, SecretString};
use evault_core::error::SecretError;
use evault_core::model::VarId;
use evault_core::traits::SecretStore;
use evault_store_keyring::OsKeyringSecretStore;

/// Build a unique service identifier so parallel tests don't collide and
/// so dev runs never pollute the production `"evault"` namespace.
fn unique_service(test_name: &str) -> String {
    format!("evault-test-{test_name}-{}", uuid_like())
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format!("{}-{}", d.as_secs(), d.subsec_nanos()))
        .unwrap_or_else(|_| "0-0".into())
}

/// Try to construct a store AND verify it can actually write.
///
/// Returns `None` when the platform has no usable backend so the
/// test skips gracefully. This catches two distinct failure modes:
///
/// 1. `with_service` fails outright (no D-Bus / Secret Service).
///    The classic headless-Linux case.
/// 2. `with_service` succeeds but `put` fails with `Unavailable`.
///    Seen on CI runners where `gnome-keyring-daemon` starts but the
///    secrets component is in a bad state (locked keyring, session
///    not properly initialised, etc.). The first case skips on init;
///    this second case is what the probe-write below catches.
fn try_store(test_name: &str) -> Option<OsKeyringSecretStore> {
    let store = match OsKeyringSecretStore::with_service(unique_service(test_name)) {
        Ok(s) => s,
        Err(SecretError::Unavailable) => {
            eprintln!(
                "skipping {test_name}: no usable keyring backend at init (likely headless Linux)"
            );
            return None;
        }
        Err(e) => panic!("unexpected init failure: {e:?}"),
    };
    // Probe a write+delete cycle so we don't fail later inside the
    // test body with a misleading `expect("put")` panic.
    let probe = VarId::new_v4();
    match store.put(probe, SecretString::from(String::from("probe"))) {
        Ok(()) => {
            // Best-effort cleanup; the probe id will never be reused.
            let _ = store.delete(probe);
            Some(store)
        }
        Err(SecretError::Unavailable) => {
            eprintln!(
                "skipping {test_name}: keyring write probe returned Unavailable \
                 (daemon present but rejecting puts — common on headless CI)"
            );
            None
        }
        Err(e) => panic!("unexpected probe-write failure: {e:?}"),
    }
}

#[test]
fn put_then_get_returns_the_secret() {
    let Some(store) = try_store("put_then_get") else {
        return;
    };
    let id = VarId::new_v4();
    store
        .put(id, SecretString::from(String::from("s3cret")))
        .expect("put");
    let got = store.get(id).expect("get").expect("present");
    assert_eq!(got.expose_secret(), "s3cret");
    // Clean up so we don't leak credentials.
    store.delete(id).expect("delete");
}

#[test]
fn get_absent_id_returns_none() {
    let Some(store) = try_store("get_absent") else {
        return;
    };
    let id = VarId::new_v4();
    assert!(store.get(id).expect("get").is_none());
}

#[test]
fn delete_absent_id_is_idempotent_ok() {
    let Some(store) = try_store("delete_absent") else {
        return;
    };
    let id = VarId::new_v4();
    store.delete(id).expect("delete should be no-op");
    // Second delete also fine.
    store.delete(id).expect("delete twice");
}

#[test]
fn put_overwrites_existing_value() {
    let Some(store) = try_store("put_overwrite") else {
        return;
    };
    let id = VarId::new_v4();
    store
        .put(id, SecretString::from(String::from("first")))
        .expect("put 1");
    store
        .put(id, SecretString::from(String::from("second")))
        .expect("put 2");
    let got = store.get(id).expect("get").expect("present");
    assert_eq!(got.expose_secret(), "second");
    store.delete(id).expect("cleanup");
}

#[test]
fn delete_then_get_returns_none() {
    let Some(store) = try_store("delete_then_get") else {
        return;
    };
    let id = VarId::new_v4();
    store
        .put(id, SecretString::from(String::from("v")))
        .expect("put");
    store.delete(id).expect("delete");
    assert!(store.get(id).expect("get").is_none());
}

#[test]
fn with_service_rejects_empty_string() {
    match OsKeyringSecretStore::with_service("") {
        Err(SecretError::Backend(label)) => assert_eq!(label, "empty_service"),
        other => panic!("expected Backend(\"empty_service\"), got {other:?}"),
    }
}

#[test]
fn distinct_ids_isolate_secrets() {
    let Some(store) = try_store("distinct_ids") else {
        return;
    };
    let a = VarId::new_v4();
    let b = VarId::new_v4();
    store
        .put(a, SecretString::from(String::from("alpha")))
        .expect("put a");
    store
        .put(b, SecretString::from(String::from("beta")))
        .expect("put b");
    assert_eq!(
        store
            .get(a)
            .expect("get a")
            .expect("present")
            .expose_secret(),
        "alpha"
    );
    assert_eq!(
        store
            .get(b)
            .expect("get b")
            .expect("present")
            .expose_secret(),
        "beta"
    );
    store.delete(a).expect("cleanup a");
    store.delete(b).expect("cleanup b");
}
