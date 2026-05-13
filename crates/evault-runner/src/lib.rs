//! Child-process runner that satisfies the
//! [`ProcessRunner`](evault_core::traits::ProcessRunner) trait by
//! delegating to [`std::process::Command`].
//!
//! [`StdProcessRunner`] is the production implementation used by the
//! `evault run -- <cmd>` flow. The child inherits the parent's stdin,
//! stdout, and stderr so the user sees `npm start` (or whatever they
//! invoked) directly in their terminal.
//!
//! # Security discipline
//!
//! - **`EVAULT_*` strip**: every parent env var whose name starts with
//!   `EVAULT_` is removed before applying the overlay. Internal evault
//!   runtime configuration must not leak into the child.
//! - **Key validation**: each overlay key is validated through
//!   [`Var::validate_name`](evault_core::model::Var::validate_name).
//!   Keys that fail validation produce
//!   [`RunnerError::Invalid`](evault_core::error::RunnerError::Invalid).
//! - **NUL-byte rejection**: any overlay value containing a NUL byte is
//!   rejected up-front. A NUL would prematurely terminate the OS
//!   environment block on Windows and is invalid on POSIX
//!   `execve(2)`-style APIs.
//! - **No env-block flattening**: we use
//!   [`Command::env`](std::process::Command::env), not
//!   `env_clear()` + a manual rebuild, so `PATH` and other inherited
//!   variables remain available to the child.
//!
//! # Examples
//!
//! ```ignore
//! use std::collections::BTreeMap;
//! use evault_core::traits::ProcessRunner;
//! use evault_runner::StdProcessRunner;
//!
//! let runner = StdProcessRunner::new();
//! let outcome = runner
//!     .run("cargo", &["--version".into()], &std::env::current_dir().unwrap(), &BTreeMap::new())
//!     .unwrap();
//! assert!(outcome.is_success());
//! ```
#![forbid(unsafe_code)]

mod runner;

pub use runner::{StdProcessRunner, EVAULT_ENV_PREFIX};
