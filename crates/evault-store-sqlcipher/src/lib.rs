//! SQLCipher-encrypted backend for `evault-core`'s metadata + audit traits.
//!
//! [`SqlCipherMetadataStore`] implements both
//! [`evault_core::traits::MetadataStore`] and
//! [`evault_core::traits::AuditSink`] on top of a single
//! [`rusqlite::Connection`] backed by `SQLCipher`. The encryption key is
//! provided at open time by the caller (typically read from the OS keyring
//! via [`evault_core::crypto::MasterKey`]). The on-disk file is opaque to
//! plain `sqlite3` and unusable without the key.
//!
//! # Atomicity
//!
//! Every mutating method runs inside a single `Connection::transaction`.
//! This matches the contractual atomicity that
//! `evault_core::traits::MetadataStore::delete_var` and `delete_project`
//! now mandate at the trait level: a delete cascades to plain values,
//! links, and the side tables in one observable step.
//!
//! # On-disk schema
//!
//! Schema versioning is internal. The current version is tracked through
//! `SQLite`'s `user_version` pragma; opening a database with a newer schema
//! is rejected to avoid silently mis-interpreting columns.
//!
//! # Examples
//!
//! ```ignore
//! use std::path::PathBuf;
//! use evault_core::crypto::MasterKey;
//! use evault_store_sqlcipher::SqlCipherMetadataStore;
//!
//! let key = MasterKey::generate().expect("OS RNG");
//! let store = SqlCipherMetadataStore::open(
//!     &PathBuf::from("/tmp/evault.sqlite"),
//!     &key,
//! ).expect("open");
//! # let _ = store;
//! ```
#![forbid(unsafe_code)]
// Every public method takes the connection mutex, performs its SQL work,
// and returns. The pedantic lint asks for explicit `drop(guard)` before
// each `Ok(())`, but the row-into-Vec collection is part of the operation
// itself — the lock cannot be released earlier without breaking the
// `Statement` borrow. Wrapping every method in scoped blocks adds noise
// without lowering real contention.
#![allow(clippy::significant_drop_tightening)]

mod encoding;
mod metadata;
mod migrations;

pub use metadata::{SqlCipherMetadataStore, SqlCipherOpenError};

pub use migrations::SCHEMA_VERSION;
