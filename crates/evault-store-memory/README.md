# evault-store-memory

[![crates.io](https://img.shields.io/crates/v/evault-store-memory.svg)](https://crates.io/crates/evault-store-memory)
[![docs.rs](https://docs.rs/evault-store-memory/badge.svg)](https://docs.rs/evault-store-memory)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)

> In-memory implementations of [`evault-core`](https://crates.io/crates/evault-core)'s storage traits — fixtures for tests, reference implementations for the trait contracts.

Every backend in this crate is process-local, lock-protected, and **not intended for production use**: secrets live in cleartext in heap memory and everything is lost when the process ends. The crate exists for two reasons:

1. Fast deterministic fixtures for unit + integration tests across the workspace, without paying the cost of SQLCipher or the OS keyring.
2. Reference implementations: each method enforces the documented trait invariants and serves as an oracle when writing production backends.

## Available backends

| Type | Implements |
|---|---|
| `MemoryMetadataStore` | `evault_core::traits::MetadataStore` |
| `MemorySecretStore` | `evault_core::traits::SecretStore` |
| `MemoryAuditSink` | `evault_core::traits::AuditSink` |
| `MemoryManifestIo` | `evault_core::traits::ManifestIo` |

## Install

```toml
[dev-dependencies]
evault-core = "0.1"
evault-store-memory = "0.1"
```

(It's also available as a regular `[dependencies]` entry — the crate just isn't recommended for production code paths.)

## Example: test a service against in-memory backends

```rust
use evault_core::model::{Group, Var, VarKind};
use evault_core::traits::MetadataStore;
use evault_store_memory::MemoryMetadataStore;

let store = MemoryMetadataStore::new();
let var = Var::try_new("DATABASE_URL", Group::User, VarKind::Plain).unwrap();
let id = var.id();
store.upsert_var(&var).unwrap();

assert_eq!(
    store.get_var(id).unwrap().unwrap().name(),
    "DATABASE_URL"
);
```

## Example: wire all three stores into a `RegistryService`

```rust
use evault_core::service::RegistryService;
use evault_store_memory::{MemoryAuditSink, MemoryMetadataStore, MemorySecretStore};

let registry = RegistryService::with_defaults(
    MemoryMetadataStore::new(),
    MemorySecretStore::new(),
    MemoryAuditSink::new(),
);
// Now exercise the registry exactly as production code would.
```

## Part of the evault workspace

This crate is part of [evault](https://github.com/stescobedo92/hide-env-keys). Production deployments use [`evault-store-sqlcipher`](https://crates.io/crates/evault-store-sqlcipher) for metadata and [`evault-store-keyring`](https://crates.io/crates/evault-store-keyring) for secrets.

## License

[MIT](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)
