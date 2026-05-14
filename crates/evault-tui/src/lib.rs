//! Terminal user interface for evault.
//!
//! This crate ships the interactive front-end. It is intentionally
//! decoupled from the storage layer: the dashboard reads rows from a
//! caller-supplied [`VarProvider`], so any backend
//! ([`evault_core::service::RegistryService`], an in-memory test stub,
//! or a remote facade) can be wired in without the TUI knowing.
//!
//! # Architecture
//!
//! Three layers compose the UI:
//!
//! 1. [`Action`] — high-level intent derived from a key event.
//!    Translating keys to actions in one place keeps view code free of
//!    crossterm internals and gives a single auditable mapping table.
//! 2. [`AppState`] — observable, deterministic state machine. Pure
//!    functions of `(state, action) -> state'`; no I/O. Drives all
//!    rendering.
//! 3. [`run_tui`] — terminal lifecycle (raw mode, alternate screen,
//!    panic-safe restore) plus the event loop that pumps keys into the
//!    state and renders the result.
//!
//! # Examples
//!
//! Drive the state machine programmatically (no terminal needed):
//!
//! ```
//! use evault_core::model::VarId;
//! use evault_tui::{Action, AppState, ProviderError, VarProvider, VarSummary};
//! use secrecy::SecretString;
//!
//! struct Empty;
//! impl VarProvider for Empty {
//!     fn list(&self) -> Result<Vec<VarSummary>, ProviderError> { Ok(Vec::new()) }
//!     fn get_value(&self, _id: VarId) -> Result<Option<SecretString>, ProviderError> {
//!         unimplemented!()
//!     }
//! }
//!
//! let mut app = AppState::new();
//! app.refresh(&Empty).unwrap();
//! app.apply(Action::ToggleHelp);
//! assert!(app.help_visible());
//! app.apply(Action::Dismiss);
//! assert!(!app.help_visible());
//! ```
//!
//! [`run_tui`] additionally requires its backend to implement
//! [`VarMutator`] for the delete flow:
//!
//! ```no_run
//! use std::path::PathBuf;
//! use evault_core::model::VarId;
//! use evault_tui::{run_tui, ProviderError, VarDraft, VarMutator, VarProvider, VarSummary};
//! use secrecy::SecretString;
//!
//! struct EmptyBackend;
//! impl VarProvider for EmptyBackend {
//!     fn list(&self) -> Result<Vec<VarSummary>, ProviderError> { Ok(Vec::new()) }
//!     fn get_value(&self, _id: VarId) -> Result<Option<SecretString>, ProviderError> {
//!         Ok(None)
//!     }
//! }
//! impl VarMutator for EmptyBackend {
//!     fn delete(&self, _id: VarId) -> Result<(), ProviderError> { Ok(()) }
//!     fn create(&self, _draft: VarDraft) -> Result<VarId, ProviderError> {
//!         Ok(VarId::new_v4())
//!     }
//!     fn update_value(&self, _id: VarId, _value: SecretString) -> Result<(), ProviderError> {
//!         Ok(())
//!     }
//!     fn link_to_project(
//!         &self,
//!         _var_id: VarId,
//!         _var_name: String,
//!         _project_path: PathBuf,
//!         _profile: String,
//!         _materialize: bool,
//!     ) -> Result<(), ProviderError> { Ok(()) }
//!     fn run_in_project(
//!         &self,
//!         _project_path: PathBuf,
//!         _profile: String,
//!         _program: String,
//!         _args: Vec<String>,
//!     ) -> Result<Option<i32>, ProviderError> { Ok(Some(0)) }
//! }
//! run_tui(EmptyBackend).unwrap();
//! ```
#![forbid(unsafe_code)]

mod app;
mod components;
mod error;
mod event;
mod filter;
mod provider;
mod runtime;
mod theme;
mod views;

pub use app::{AppState, DispatchOutcome, View};
pub use error::TuiError;
pub use event::Action;
pub use provider::{ProviderError, VarDraft, VarMutator, VarProvider, VarSummary};
pub use runtime::run_tui;
pub use theme::Theme;
