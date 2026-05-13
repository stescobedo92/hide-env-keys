//! In-memory implementations of `evault-core`'s storage traits.
//!
//! Every backend in this crate is process-local, lock-protected, and **not
//! intended for production use**: secrets live in cleartext in heap memory
//! and metadata is lost when the process ends. The purpose of these
//! implementations is twofold:
//!
//! 1. Provide fast, deterministic fixtures for unit and integration tests of
//!    every other crate in the workspace. Tests that exercise the trait
//!    contracts can construct the in-memory variants in a single line and
//!    avoid the cost (and security surface) of `SQLCipher` and the OS keyring.
//! 2. Document the trait contracts by reference implementation: each method
//!    enforces the documented invariants, providing a working oracle when
//!    writing production backends.
//!
//! # Available backends
//!
//! - [`MemoryMetadataStore`] implements [`evault_core::traits::MetadataStore`].
//! - [`MemorySecretStore`] implements [`evault_core::traits::SecretStore`].
//! - [`MemoryAuditSink`] implements [`evault_core::traits::AuditSink`].
//! - [`MemoryManifestIo`] implements [`evault_core::traits::ManifestIo`].
//!
//! # Example
//!
//! ```
//! use evault_core::model::{Group, Var, VarKind};
//! use evault_core::traits::MetadataStore;
//! use evault_store_memory::MemoryMetadataStore;
//!
//! let store = MemoryMetadataStore::new();
//! let var = Var::try_new("DATABASE_URL", Group::User, VarKind::Plain).unwrap();
//! let id = var.id();
//! store.upsert_var(&var).unwrap();
//! assert_eq!(store.get_var(id).unwrap().unwrap().name(), "DATABASE_URL");
//! ```
#![forbid(unsafe_code)]
// This crate is explicitly documented as test-only: every backend lives in a
// single-process `RwLock` and is expected to be used by tests that hold no
// concurrent readers/writers. The pedantic `significant_drop_tightening` lint
// asks us to bracket every guard with an explicit `drop` even when the work
// fits trivially in the method body. For a fixture crate the extra noise
// hides the actual logic without lowering real contention.
#![allow(clippy::significant_drop_tightening)]

mod audit;
mod manifest;
mod metadata;
mod secret;

pub use audit::MemoryAuditSink;
pub use manifest::MemoryManifestIo;
pub use metadata::MemoryMetadataStore;
pub use secret::MemorySecretStore;
