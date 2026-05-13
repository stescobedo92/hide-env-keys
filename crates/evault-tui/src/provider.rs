//! Adapter trait between the TUI and any data source.

use evault_core::model::{Group, VarId, VarKind};
use thiserror::Error;
use time::OffsetDateTime;

/// One row of dashboard data.
///
/// Deliberately flat and **value-free**: the dashboard never holds a
/// secret value in memory, only its metadata. Implementations of
/// [`VarProvider`] derive this struct from whatever backing store
/// they wrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarSummary {
    /// Stable identifier of the variable.
    pub id: VarId,
    /// Display name (e.g. `DATABASE_URL`).
    pub name: String,
    /// Logical group the variable belongs to.
    pub group: Group,
    /// Whether the value lives in the keyring (secret) or alongside
    /// metadata (plain).
    pub kind: VarKind,
    /// Length of the underlying value, in characters (not bytes).
    /// Surfaced in the dashboard as a privacy-preserving size hint.
    pub value_len: usize,
    /// How many projects currently link to this variable.
    pub linked_projects: usize,
    /// Last time the variable's metadata changed.
    pub updated_at: OffsetDateTime,
}

/// Errors produced by a [`VarProvider`].
///
/// Intentionally a small enum — the TUI only renders the message to
/// the user and does not branch on the cause. Concrete error types
/// from the wrapped backend should be stringified into one of these
/// variants by the [`VarProvider`] implementation.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum ProviderError {
    /// The provider could not be reached or its data is temporarily
    /// unavailable (e.g. locked metadata store, dropped connection).
    #[error("data unavailable: {0}")]
    DataUnavailable(String),

    /// The underlying backend reported an error.
    #[error("backend error: {0}")]
    Backend(String),
}

/// Read-side data source for the dashboard.
///
/// Implementations adapt the registry / manifest / store layer into
/// the flat [`VarSummary`] row representation the TUI consumes. The
/// trait is intentionally narrow so adapters can be added incrementally
/// (an in-memory test stub today, a `RegistryService` wrapper tomorrow,
/// a remote facade later) without churning view code.
///
/// Implementations must be safe to share across threads.
pub trait VarProvider: Send + Sync {
    /// Return the full list of variables. The dashboard re-runs this
    /// on explicit refresh, so the implementation should make each
    /// call deterministic with respect to the underlying state at
    /// call time.
    ///
    /// # Errors
    /// Returns [`ProviderError`] if the backend is unreachable or
    /// returns an error. The TUI surfaces the message as a toast.
    fn list(&self) -> Result<Vec<VarSummary>, ProviderError>;
}

/// Write-side counterpart to [`VarProvider`].
///
/// The TUI invokes [`VarMutator`] methods on user-confirmed actions
/// (delete in phase 2b2; create / update / link in phase 2c). The
/// trait is split from `VarProvider` so adapters that only support
/// reading (e.g. a remote read-only view) need not implement writes,
/// though [`crate::run_tui`] requires both for now.
///
/// Implementations must be safe to share across threads.
pub trait VarMutator: Send + Sync {
    /// Delete the variable with the given id.
    ///
    /// Implementations should make the call idempotent if at all
    /// possible: re-deleting an already-absent variable should not
    /// fail. The TUI refreshes its row buffer after every successful
    /// delete, so a non-idempotent backend will surface spurious
    /// errors when the user clicks delete twice on a stale view.
    ///
    /// # Errors
    /// Returns [`ProviderError`] if the backend refused or could not
    /// perform the delete. The TUI surfaces the message as a sticky
    /// error toast and the user can retry from the dashboard.
    fn delete(&self, id: VarId) -> Result<(), ProviderError>;
}
