# evault-tui

[![crates.io](https://img.shields.io/crates/v/evault-tui.svg)](https://crates.io/crates/evault-tui)
[![docs.rs](https://docs.rs/evault-tui/badge.svg)](https://docs.rs/evault-tui)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)

> Terminal user interface for [evault](https://github.com/stescobedo92/hide-env-keys), built on [ratatui](https://crates.io/crates/ratatui). Plug any backend that implements `VarProvider + VarMutator` and get a full dashboard with CRUD, link-to-project, view-value, fuzzy filter, and run-in-project flows.

The crate is intentionally decoupled from any storage layer: the TUI reads rows from a caller-supplied [`VarProvider`] and writes through [`VarMutator`]. The same TUI can drive a SQLCipher-backed registry, an in-memory test stub, or a remote facade.

## Features

- **Dashboard table** with sortable columns: name, group, kind, value length, linked projects, last update.
- **CRUD** — `n` new variable, `e` edit value, `d` delete (with confirm modal).
- **Link to project** — `l` opens a modal capturing project path + profile + materialize toggle.
- **View value** — `v` reveals the decrypted secret in a centered modal.
- **Run in project** — `R` launches a child process with the project's env injected, no `.env` on disk.
- **Fuzzy filter** — `Ctrl+F` opens a centered fuzzy-input powered by `nucleo-matcher`.
- **Error modals** — failed actions surface in a centered modal with a plain-English hint explaining the failure and how to recover.
- **Themable** via the `Theme` struct.

## Install

```toml
[dependencies]
evault-tui = "0.1"
evault-core = "0.1"
secrecy = "0.10"
```

## Example: launch the TUI against an in-memory backend

```rust,no_run
use std::path::PathBuf;
use evault_core::model::VarId;
use evault_tui::{AuditProvider, run_tui, ProviderError, VarDraft, VarMutator, VarProvider, VarSummary};
use secrecy::SecretString;

struct EmptyBackend;

impl VarProvider for EmptyBackend {
    fn list(&self) -> Result<Vec<VarSummary>, ProviderError> { Ok(Vec::new()) }
    fn get_value(&self, _id: VarId) -> Result<Option<SecretString>, ProviderError> {
        Ok(None)
    }
}

impl VarMutator for EmptyBackend {
    fn delete(&self, _id: VarId) -> Result<(), ProviderError> { Ok(()) }
    fn create(&self, _draft: VarDraft) -> Result<VarId, ProviderError> {
        Ok(VarId::new_v4())
    }
    fn update_value(&self, _id: VarId, _value: SecretString) -> Result<(), ProviderError> {
        Ok(())
    }
    fn record_copy(&self, _id: VarId) -> Result<(), ProviderError> { Ok(()) }
    fn link_to_project(
        &self,
        _var_id: VarId,
        _var_name: String,
        _project_path: PathBuf,
        _profile: String,
        _materialize: bool,
    ) -> Result<(), ProviderError> { Ok(()) }
    fn run_in_project(
        &self,
        _project_path: PathBuf,
        _profile: String,
        _program: String,
        _args: Vec<String>,
    ) -> Result<Option<i32>, ProviderError> { Ok(Some(0)) }
}

impl AuditProvider for EmptyBackend {
    fn recent_audit(
        &self,
        _limit: usize,
    ) -> Result<Vec<evault_core::model::AuditEntry>, ProviderError> {
        Ok(Vec::new())
    }
}

run_tui(EmptyBackend).unwrap();
```

## Terminal lifecycle

`run_tui` owns the terminal lifecycle: it enters raw mode + the alternate screen, installs a panic hook that restores them on unwind, and guarantees the restore happens whether the loop returns `Ok`, returns `Err`, or panics. For the run-in-project flow specifically, the TUI restores the terminal **before** spawning the child (so the child inherits a normal terminal with stdio passthrough) and re-enters raw mode after the child exits.

## Part of the evault workspace

Used by the [`evault` CLI](https://crates.io/crates/evault-cli) as the default action when invoked without a subcommand. See the workspace README for the full feature set and keymap.

## License

[MIT](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)
