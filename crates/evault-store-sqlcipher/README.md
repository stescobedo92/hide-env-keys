# evault-store-sqlcipher

[![crates.io](https://img.shields.io/crates/v/evault-store-sqlcipher.svg)](https://crates.io/crates/evault-store-sqlcipher)
[![docs.rs](https://docs.rs/evault-store-sqlcipher/badge.svg)](https://docs.rs/evault-store-sqlcipher)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/stescobedo/hide-env-keys/blob/master/LICENSE)

> SQLite-backed implementation of [`evault-core`](https://crates.io/crates/evault-core)'s `MetadataStore` trait, with optional [SQLCipher](https://www.zetetic.net/sqlcipher/) encryption.

`SqlCipherMetadataStore` persists variable metadata, projects, profile bindings, and audit log entries to a single SQLite file. Variable **values** never live here — secrets go to [`evault-store-keyring`](https://crates.io/crates/evault-store-keyring), plain values to a separate table inside this DB. With the `sqlcipher` feature enabled, the entire database file is encrypted at rest with a 256-bit master key.

## Install

```toml
[dependencies]
evault-core = "0.1"
evault-store-sqlcipher = "0.1"
```

To enable encryption (requires Strawberry Perl on Windows; `libssl-dev` + `pkg-config` on Linux/macOS):

```toml
evault-store-sqlcipher = { version = "0.1", features = ["sqlcipher"] }
```

## Example: open an encrypted store and round-trip a variable

```rust,no_run
use evault_core::crypto::MasterKey;
use evault_core::model::{Group, Var, VarKind};
use evault_core::traits::MetadataStore;
use evault_store_sqlcipher::SqlCipherMetadataStore;

let key = MasterKey::generate().expect("CSPRNG");
let store = SqlCipherMetadataStore::open("data.sqlite", &key).expect("open");

let var = Var::try_new("API_KEY", Group::User, VarKind::Secret).expect("validate");
store.upsert_var(&var).expect("upsert");

assert_eq!(
    store.get_var(var.id()).unwrap().unwrap().name(),
    "API_KEY",
);
```

## Schema

The database is versioned via SQLite's `user_version` pragma. Migrations run inside a single transaction on open; a failure mid-way leaves the file at the previous version. The current schema covers:

| Table | Purpose |
|---|---|
| `vars` | Variable metadata (name, group, kind, length, timestamps). Values are NOT stored here. |
| `plain_values` | Non-secret values, FK to `vars.id`. |
| `projects` | Per-project records. |
| `project_vars` | Bindings from projects to variables (with profile + optional alias). |
| `audit_log` | Append-only audit trail. |

## Part of the evault workspace

This is the production metadata store for [evault](https://github.com/stescobedo/hide-env-keys). The companion `evault-store-keyring` crate handles secret values.

## License

[MIT](https://github.com/stescobedo/hide-env-keys/blob/master/LICENSE)
