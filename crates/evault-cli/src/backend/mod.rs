//! Backend wiring — adapts a concrete [`RegistryService`] (from
//! `evault-core`) to the TUI's [`VarProvider`] / [`VarMutator`] traits
//! and to the per-subcommand operations the CLI surface uses.
//!
//! Most of [`BackendOps`] is dead code today; phase B / D will wire
//! the ls / add / rm / link / gen / run / scan / import / export
//! subcommands to it. The trait is shaped to absorb those without
//! a churn pass.

#![allow(dead_code, clippy::redundant_pub_crate, clippy::doc_markdown)]
//!
//! Two concrete shapes ship today:
//!
//! - [`in_memory::InMemoryBackend`] — ephemeral, used for tests, the
//!   `--demo` flag, and the `--ephemeral` flag.
//! - [`sqlcipher::SqlCipherBackend`] — production backend: metadata
//!   in an on-disk SQLite database (encrypted with SQLCipher when the
//!   `sqlcipher` feature is enabled at build time), values in the OS
//!   keyring (DPAPI / Keychain / Secret Service), master key
//!   bootstrapped from / into the OS keyring on first run.
//!
//! Backends are dispatched statically: `main.rs` picks one and the
//! generic subcommand functions are monomorphised for that concrete
//! type. There is no `dyn Backend` because every method needs trait
//! bounds the TUI / subcommands rely on (`VarProvider + VarMutator +
//! BackendOps`).

pub mod in_memory;
pub mod ops;
pub mod sqlcipher;

pub use in_memory::InMemoryBackend;
pub use ops::BackendOps;
pub use sqlcipher::{SqlCipherBackend, SqlCipherOpenError};

use evault_core::error::CoreError;
use evault_tui::ProviderError;

/// Map a [`CoreError`] into a [`ProviderError`] for the TUI / CLI.
///
/// Walks the `source()` chain so the inner cause (storage error,
/// filesystem error kind, …) reaches the user instead of just the
/// outer variant name.
pub(crate) fn core_to_provider(err: &CoreError) -> ProviderError {
    ProviderError::Backend(format_core_chain(err))
}

/// Walk an error's `source()` chain into `outer: middle: leaf` form.
/// Shared between the in-memory and SQLCipher adapters.
pub(crate) fn format_core_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut msg = err.to_string();
    let mut source = err.source();
    while let Some(inner) = source {
        msg.push_str(": ");
        msg.push_str(&inner.to_string());
        source = inner.source();
    }
    msg
}
