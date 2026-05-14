---
title: Architecture
description: How the evault workspace is organised — pure-logic core, swappable backends, thin CLI and TUI on top.
---

`evault` is a Rust workspace of 10 crates. Business logic depends only on **traits**; concrete implementations are isolated so backends can be swapped without churning service code.

## Layered view

```
┌─────────────────────────────────────────────────────────────┐
│                       evault-cli                            │   binary
│              (clap, wire-up, exit codes)                    │
├─────────────────────────────────────────────────────────────┤
│                       evault-tui                            │   UI
│              (ratatui, app state, modals)                   │
├─────────────────────────────────────────────────────────────┤
│                      evault-core                            │   logic
│       (domain model, traits, RegistryService<...>)          │
├──────────────┬──────────────┬─────────────────┬─────────────┤
│ store-       │ store-       │ manifest        │ runner      │
│ sqlcipher    │ keyring      │ materializer    │ scanner     │
│ (impl meta)  │ (impl secret)│ (impl IO + gen) │ (impl exec) │
└──────────────┴──────────────┴─────────────────┴─────────────┘
                            ▲
                   store-memory (tests / fixtures)
```

## The 10 crates

| Crate | Purpose |
|---|---|
| [`evault-core`](https://crates.io/crates/evault-core) | Domain model, traits, generic services. **Zero IO; pure logic.** |
| [`evault-store-sqlcipher`](https://crates.io/crates/evault-store-sqlcipher) | SQLite-backed `MetadataStore`. Optional SQLCipher encryption at rest. |
| [`evault-store-keyring`](https://crates.io/crates/evault-store-keyring) | OS-keyring–backed `SecretStore`. DPAPI / Keychain / Secret Service. |
| [`evault-store-memory`](https://crates.io/crates/evault-store-memory) | In-memory implementations of every trait. Tests and reference impls. |
| [`evault-manifest`](https://crates.io/crates/evault-manifest) | TOML parser / serializer for `evault.toml`. |
| [`evault-materializer`](https://crates.io/crates/evault-materializer) | `.env` generator with atomic writes + `.gitignore` management. |
| [`evault-runner`](https://crates.io/crates/evault-runner) | Child-process exec with injected env overlay. |
| [`evault-scanner-regex`](https://crates.io/crates/evault-scanner-regex) | Source-code scanner (JS/TS, Python, Rust, Go, Shell). |
| [`evault-tui`](https://crates.io/crates/evault-tui) | ratatui-based TUI. Decoupled from any storage layer. |
| [`evault-cli`](https://crates.io/crates/evault-cli) | The `evault` binary. Clap wiring + subcommand dispatch. |

## Trait-driven design

Every IO concern is a trait in `evault-core::traits`:

```rust
pub trait MetadataStore: Send + Sync {
    fn upsert_var(&self, var: &Var) -> Result<(), MetadataError>;
    fn get_var(&self, id: VarId) -> Result<Option<Var>, MetadataError>;
    fn list_vars(&self, filter: &VarFilter) -> Result<Vec<Var>, MetadataError>;
    fn delete_var(&self, id: VarId) -> Result<(), MetadataError>;
    // … project + project_var APIs
}

pub trait SecretStore: Send + Sync {
    fn put(&self, id: VarId, value: SecretString) -> Result<(), SecretError>;
    fn get(&self, id: VarId) -> Result<Option<SecretString>, SecretError>;
    fn delete(&self, id: VarId) -> Result<(), SecretError>;
}
```

`RegistryService<M, S, A, C, I>` in `evault-core::service::registry` is generic over the metadata store, secret store, audit sink, clock, and id generator. The same service is reused by tests (wired against in-memory implementations) and production (wired against SQLCipher + the OS keyring).

This pays three dividends:

1. **Tests don't need the keyring or SQLCipher.** They run in microseconds against `MemoryMetadataStore` / `MemorySecretStore`.
2. **Service-layer changes are isolated from backend changes.** Renaming a method on `MetadataStore` recompiles every implementor; refactoring `RegistryService` doesn't touch any backend.
3. **Headless fallback is cheap.** When the OS keyring is unavailable (Linux without D-Bus), the runtime can swap in a file-encrypted secret store with no service-layer change. (Not implemented in v0.1.0, but the door is open.)

## TUI / backend decoupling

`evault-tui` exports a `VarProvider + VarMutator` pair of traits and the binary's wire-up code implements them on top of the concrete backend. The TUI knows nothing about SQLCipher, the keyring, or the manifest layer — it just calls `list()`, `get_value()`, `create()`, `update_value()`, `delete()`, `link_to_project()`, and `run_in_project()`.

That means anyone with a different backing store (a remote vault, a cloud-secrets service, an organisational password manager) can implement those traits and reuse the entire TUI verbatim.

## What lives at the binary boundary

`evault-cli` is intentionally thin. It:

1. Parses CLI flags via `clap`.
2. Picks one of three concrete backends — `SqlCipherBackend`, `InMemoryBackend` with demo seed data, or `InMemoryBackend` with no seed.
3. Dispatches the chosen subcommand against the backend through the `BackendOps` trait (CLI-private superset of `VarProvider + VarMutator`).
4. Formats `CliError`s into exit codes + stderr messages with recovery hints.

Anything more interesting than wiring lives in the layer below the CLI.

## Static dispatch, not `dyn Trait`

Generic types — `RegistryService<M, S, A, C, I>` — are monomorphised at the call site, so the binary uses static dispatch end-to-end. There is no `dyn Backend` indirection. The reason is twofold:

1. Every method on every trait is called through bounds (`B: VarProvider + VarMutator + BackendOps`); a `dyn` object would have to merge those bounds into a trait object, which Rust does not support directly.
2. Static dispatch elides one indirection per call. The binary is small enough that monomorphisation cost is negligible.
