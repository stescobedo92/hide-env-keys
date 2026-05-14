# evault-store-keyring

[![crates.io](https://img.shields.io/crates/v/evault-store-keyring.svg)](https://crates.io/crates/evault-store-keyring)
[![docs.rs](https://docs.rs/evault-store-keyring/badge.svg)](https://docs.rs/evault-store-keyring)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/stescobedo/hide-env-keys/blob/master/LICENSE)

> OS-keyring–backed implementation of [`evault-core`](https://crates.io/crates/evault-core)'s `SecretStore` trait — wraps `keyring-core` to satisfy the trait contract on Windows, macOS, and Linux.

`OsKeyringSecretStore` stores each variable's secret value in the platform's native credential store, keyed by the variable's UUID:

- **Windows** — Credential Manager (DPAPI-protected).
- **macOS** — Keychain.
- **Linux** — Secret Service (GNOME Keyring / KWallet) via D-Bus.

Errors from the underlying `keyring-core` crate are reduced to a fixed set of category labels: the service name, the account (a UUID — not secret, but not useful to attackers), and platform-specific paths or D-Bus identifiers are never carried into the error string. This is intentional: error messages are surfaced to the user and must not leak structural details that could narrow entropy.

## Install

```toml
[dependencies]
evault-core = "0.1"
evault-store-keyring = "0.1"
```

## Example: round-trip a secret

```rust,no_run
use evault_core::crypto::{ExposeSecret, SecretString};
use evault_core::model::VarId;
use evault_core::traits::SecretStore;
use evault_store_keyring::OsKeyringSecretStore;

// Pick a unique service identifier so multiple applications can
// coexist on the same machine without colliding.
let store = OsKeyringSecretStore::with_service("my-app")
    .expect("keyring backend unavailable (headless Linux without D-Bus?)");

let id = VarId::new_v4();
store
    .put(id, SecretString::from(String::from("hunter2")))
    .expect("put");

let value = store.get(id).expect("get").expect("present");
assert_eq!(value.expose_secret(), "hunter2");

store.delete(id).expect("delete");
```

## Headless environments

On Linux servers without D-Bus / Secret Service (e.g. SSH-only hosts, plain Docker containers), `OsKeyringSecretStore::with_service` returns `SecretError::Unavailable`. Callers should detect this and either:

- Fail fast with a clear message, **or**
- Fall back to a different `SecretStore` implementation (e.g. `MemorySecretStore` for ephemeral use, or a file-encrypted variant).

The integration tests in this crate demonstrate the "skip if unavailable" pattern.

## Part of the evault workspace

This is the production secret-store implementation used by [evault](https://github.com/stescobedo/hide-env-keys). See the workspace README for how it fits with the SQLCipher metadata store and the TUI.

## License

[MIT](https://github.com/stescobedo/hide-env-keys/blob/master/LICENSE)
