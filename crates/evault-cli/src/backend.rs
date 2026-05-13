//! Backend wiring — adapts a concrete [`RegistryService`] to the
//! TUI's [`VarProvider`] + [`VarMutator`] traits.
//!
//! Phase 1 wires the in-memory backend so the CLI is runnable end-to-end
//! without touching disk or the OS keyring. The shape is identical to
//! what a real `SQLCipher` + keyring backend will need — only the trait
//! params change.

use evault_core::error::CoreError;
use evault_core::model::{VarFilter, VarId};
use evault_core::service::RegistryService;
use evault_core::traits::{SystemClock, UuidV4IdGenerator};
use evault_store_memory::{MemoryAuditSink, MemoryMetadataStore, MemorySecretStore};
use evault_tui::{ProviderError, VarMutator, VarProvider, VarSummary};

/// Convenience alias for the concrete in-memory registry shape.
type MemoryRegistry = RegistryService<
    MemoryMetadataStore,
    MemorySecretStore,
    MemoryAuditSink,
    SystemClock,
    UuidV4IdGenerator,
>;

/// Adapter that owns a [`RegistryService`] backed by the in-memory
/// stores and exposes the TUI's read/write trait surface.
///
/// Every CLI invocation gets a fresh `InMemoryBackend`; nothing is
/// persisted across runs. Real persistence (`SQLCipher` + keyring) is
/// a follow-up phase.
pub struct InMemoryBackend {
    registry: MemoryRegistry,
}

impl InMemoryBackend {
    /// Construct a backend with empty stores.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: RegistryService::with_defaults(
                MemoryMetadataStore::new(),
                MemorySecretStore::new(),
                MemoryAuditSink::new(),
            ),
        }
    }

    /// Borrow the underlying registry for future subcommand wiring.
    /// Currently only the TUI path uses this struct via its trait
    /// impls; the accessor is in place for phase-2 subcommands.
    #[allow(dead_code)]
    #[must_use]
    pub const fn registry(&self) -> &MemoryRegistry {
        &self.registry
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VarProvider for InMemoryBackend {
    fn list(&self) -> Result<Vec<VarSummary>, ProviderError> {
        let vars = self
            .registry
            .list_vars(&VarFilter::new())
            .map_err(|e| core_to_provider(&e))?;

        let mut summaries = Vec::with_capacity(vars.len());
        for var in vars {
            // `links_for_var` failures are surfaced verbatim — the
            // TUI will toast them. We could fall back to `0` and
            // hide the issue, but silent-failure-hunter would
            // (rightly) flag that.
            let links = self
                .registry
                .links_for_var(var.id())
                .map_err(|e| core_to_provider(&e))?;
            summaries.push(VarSummary {
                id: var.id(),
                name: var.name().to_owned(),
                group: var.group().clone(),
                kind: var.kind(),
                value_len: var.length(),
                linked_projects: links.len(),
                updated_at: var.updated_at(),
            });
        }
        // Stable ordering: alphabetical by name. Without this the
        // dashboard would jitter across refreshes.
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(summaries)
    }
}

impl VarMutator for InMemoryBackend {
    fn delete(&self, id: VarId) -> Result<(), ProviderError> {
        self.registry
            .delete_var(id)
            .map_err(|e| core_to_provider(&e))
    }
}

/// Map a [`CoreError`] into a [`ProviderError`] for the TUI.
///
/// The TUI only renders the message string, so we keep the mapping
/// trivial: every backend failure becomes [`ProviderError::Backend`]
/// with the verbatim error. Phase 2 can split availability vs.
/// integrity if surfacing different colors / actions becomes useful.
fn core_to_provider(err: &CoreError) -> ProviderError {
    ProviderError::Backend(err.to_string())
}
