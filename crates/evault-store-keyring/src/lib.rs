//! OS-keyring-backed implementation of `evault-core`'s
//! [`SecretStore`](evault_core::traits::SecretStore) trait.
//!
//! [`OsKeyringSecretStore`] delegates to the platform's native credential
//! store via the `keyring` crate:
//!
//! - **Windows**: Credential Manager (DPAPI).
//! - **macOS**: Keychain.
//! - **Linux/BSD**: D-Bus Secret Service (`gnome-keyring`, `KWallet`, …).
//!
//! Each variable's value is stored under the canonical service identifier
//! `"evault"` keyed by the variable's UUID. The mapping is one credential
//! per variable so platform tooling can audit individual secrets.
//!
//! # Error semantics
//!
//! Per the [`evault_core::traits::SecretStore`] contract:
//! - `get` returns `Ok(None)` when no credential exists for the supplied
//!   id. Platform "item not found" codes are the only path to this
//!   `None`; every other backend error propagates as
//!   [`evault_core::error::SecretError::Backend`] or
//!   [`evault_core::error::SecretError::Unavailable`].
//! - `delete` is idempotent: deleting an absent item is `Ok(())`.
//! - `put` always either writes or returns an error — it never silently
//!   fails.
//!
//! # Examples
//!
//! ```ignore
//! use evault_core::traits::SecretStore;
//! use evault_core::crypto::{ExposeSecret, SecretString};
//! use evault_core::model::VarId;
//! use evault_store_keyring::OsKeyringSecretStore;
//!
//! let store = OsKeyringSecretStore::new().expect("init keyring");
//! let id = VarId::new_v4();
//! store.put(id, SecretString::from(String::from("hunter2"))).unwrap();
//! let got = store.get(id).unwrap().expect("present");
//! assert_eq!(got.expose_secret(), "hunter2");
//! store.delete(id).unwrap();
//! ```
#![forbid(unsafe_code)]

mod errors;
mod store;

pub use store::OsKeyringSecretStore;
