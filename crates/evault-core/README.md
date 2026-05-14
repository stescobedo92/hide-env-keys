# evault-core

[![crates.io](https://img.shields.io/crates/v/evault-core.svg)](https://crates.io/crates/evault-core)
[![docs.rs](https://docs.rs/evault-core/badge.svg)](https://docs.rs/evault-core)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)

> Core domain model, trait contracts, and services for the [evault](https://github.com/stescobedo92/hide-env-keys) workspace.

This crate is intentionally **backend-agnostic**. It defines the types and traits that every concrete storage / manifest / runner implementation in the workspace must satisfy. Business logic (services like `RegistryService`) is generic over those traits, so tests can wire in-memory stubs and production code can wire SQLCipher + the OS keyring without any service-layer change.

## What's inside

- **Domain model** — `Var`, `Project`, `Profile`, `AuditEntry`, `Group`, `VarKind`, `ManifestSnapshot`, …
- **Trait contracts** — `MetadataStore`, `SecretStore`, `AuditSink`, `ManifestIo`, `Materializer`, `CodeScanner`, `ProcessRunner`, `Clock`, `IdGenerator`.
- **Services** — `RegistryService<M, S, A, C, I>` generic over the storage/clock/id traits.
- **Crypto helpers** — `SecretString` (zeroized on drop), `MasterKey` (32-byte CSPRNG-generated).

## Install

```toml
[dependencies]
evault-core = "0.1"
```

## Example: a complete registry with in-memory backends

```rust
use evault_core::model::{Group, VarKind};
use evault_core::service::RegistryService;
use evault_core::traits::{SystemClock, UuidV4IdGenerator};
use evault_store_memory::{MemoryAuditSink, MemoryMetadataStore, MemorySecretStore};
use secrecy::{ExposeSecret, SecretString};

let registry = RegistryService::with_defaults(
    MemoryMetadataStore::new(),
    MemorySecretStore::new(),
    MemoryAuditSink::new(),
);

// Create a secret variable.
let id = registry
    .create_var(
        "DATABASE_URL",
        Group::User,
        VarKind::Secret,
        SecretString::new("postgres://localhost".to_owned().into()),
    )
    .expect("create");

// Read it back. Values are wrapped in `SecretString` so the buffer
// is zeroized when the binding goes out of scope.
let value = registry.get_value(id).expect("get").expect("present");
assert_eq!(value.expose_secret(), "postgres://localhost");
```

## Example: validate a variable name

```rust
use evault_core::model::Var;

assert!(Var::validate_name("DATABASE_URL").is_ok());
assert!(Var::validate_name("with space").is_err());
assert!(Var::validate_name("1BAD").is_err()); // must start with letter or `_`
```

## Part of the evault workspace

This crate is the foundation of [evault](https://github.com/stescobedo92/hide-env-keys), a secure cross-platform TUI + CLI for managing environment variables. See the workspace README for the full picture, including the SQLCipher metadata store, OS keyring secret store, TOML manifest format, and the `evault` CLI.

## License

[MIT](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)
