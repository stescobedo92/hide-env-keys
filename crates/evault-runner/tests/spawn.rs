//! Integration tests for `StdProcessRunner`.
//!
//! We use `cargo --version` as the canonical "always exits 0" command
//! because cargo is guaranteed to be present in any environment that
//! built this workspace, and `--version` doesn't care about cwd or env.

#![allow(clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use evault_core::error::RunnerError;
use evault_core::traits::ProcessRunner;
use evault_runner::StdProcessRunner;

fn cwd() -> std::path::PathBuf {
    std::env::current_dir().expect("cwd")
}

#[test]
fn run_cargo_version_succeeds() {
    let runner = StdProcessRunner::new();
    let outcome = runner
        .run("cargo", &["--version".into()], &cwd(), &BTreeMap::new())
        .expect("spawn cargo --version");
    assert!(
        outcome.is_success(),
        "expected exit code 0, got {outcome:?}"
    );
}

#[test]
fn run_unknown_program_returns_spawn_error() {
    let runner = StdProcessRunner::new();
    let result = runner.run(
        "this-program-does-not-exist-evault-test",
        &[],
        &cwd(),
        &BTreeMap::new(),
    );
    match result {
        Err(RunnerError::Spawn(msg)) => assert!(
            msg.contains("not found") || msg.contains("NotFound"),
            "expected NotFound-flavoured message: {msg}"
        ),
        other => panic!("expected Spawn error, got {other:?}"),
    }
}

#[test]
fn run_with_invalid_env_key_returns_invalid_error() {
    let runner = StdProcessRunner::new();
    let mut env = BTreeMap::new();
    env.insert("with space".to_owned(), "v".to_owned());
    let result = runner.run("cargo", &["--version".into()], &cwd(), &env);
    assert!(matches!(result, Err(RunnerError::Invalid(_))));
}

#[test]
fn run_with_nul_byte_in_value_returns_invalid_error() {
    let runner = StdProcessRunner::new();
    let mut env = BTreeMap::new();
    env.insert("TOKEN".to_owned(), "abc\0xyz".to_owned());
    let result = runner.run("cargo", &["--version".into()], &cwd(), &env);
    match result {
        Err(RunnerError::Invalid(msg)) => {
            assert!(msg.contains("TOKEN"));
            assert!(!msg.contains("xyz"), "value text leaked: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn run_with_valid_overlay_succeeds() {
    let runner = StdProcessRunner::new();
    let mut env = BTreeMap::new();
    env.insert("EVAULT_RUNNER_TEST_VAR".to_owned(), "hello".to_owned());
    // EVAULT_RUNNER_TEST_VAR matches the EVAULT_ prefix, but the strip
    // applies to PARENT env only — overlay always wins. Confirm the
    // spawn still succeeds; deeper "child can see it" verification would
    // require capturing child stdout (out of scope for v1).
    let outcome = runner
        .run("cargo", &["--version".into()], &cwd(), &env)
        .expect("spawn");
    assert!(outcome.is_success());
}

#[test]
fn cwd_is_honoured_for_spawn() {
    // `cargo --version` works in any cwd; we just confirm the runner
    // doesn't panic or fail when handed an unusual but valid cwd.
    let tmp = tempfile::TempDir::new().expect("tmpdir");
    let runner = StdProcessRunner::new();
    let outcome = runner
        .run("cargo", &["--version".into()], tmp.path(), &BTreeMap::new())
        .expect("spawn in tmp cwd");
    assert!(outcome.is_success());
}
