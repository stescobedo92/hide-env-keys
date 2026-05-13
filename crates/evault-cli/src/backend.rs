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

    /// Construct a backend pre-populated with a handful of demo
    /// variables so the TUI has something to interact with on a
    /// fresh launch.
    ///
    /// Intended for `--demo` runs and CI smoke tests; production code
    /// uses [`Self::new`]. The seed touches every kind / group / link
    /// permutation the dashboard renders so manual testers see the
    /// full palette without needing to script any setup.
    ///
    /// # Panics
    /// Panics if any of the seed `create_var` calls fails — the
    /// inputs are static and known-valid; a panic here means the
    /// in-memory store impl regressed.
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn with_demo_data() -> Self {
        use evault_core::model::{Group, VarKind};
        use secrecy::SecretString;

        let backend = Self::new();
        let seed: &[(&str, Group, VarKind, &str)] = &[
            (
                "DATABASE_URL",
                Group::User,
                VarKind::Secret,
                "postgres://localhost:5432/dev",
            ),
            ("NODE_ENV", Group::User, VarKind::Plain, "development"),
            ("API_KEY", Group::User, VarKind::Secret, "sk_live_abcdef"),
            ("LOG_LEVEL", Group::User, VarKind::Plain, "info"),
            (
                "AWS_ACCESS_KEY_ID",
                Group::System,
                VarKind::Secret,
                "AKIAEXAMPLE",
            ),
            (
                "AWS_SECRET_ACCESS_KEY",
                Group::System,
                VarKind::Secret,
                "wJalrXUtnFEMIxxK7MDENG",
            ),
            ("PORT", Group::Project, VarKind::Plain, "3000"),
            (
                "REDIS_URL",
                Group::User,
                VarKind::Secret,
                "redis://localhost:6379",
            ),
            (
                "FEATURE_FLAGS",
                Group::Project,
                VarKind::Plain,
                "new_ui=on,beta_login=off",
            ),
            (
                "SENTRY_DSN",
                Group::User,
                VarKind::Secret,
                "https://abc@sentry.io/123",
            ),
        ];
        for (name, group, kind, value) in seed {
            backend
                .registry
                .create_var(
                    name,
                    group.clone(),
                    *kind,
                    SecretString::new((*value).to_owned().into()),
                )
                .expect("demo seed must succeed against an empty in-memory backend");
        }
        backend
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
            // `links_for_var` failures wrap the offending var name so
            // a future N+1 failure points at the actual row instead
            // of a generic "backend error". The wrapping format is
            // `links for NAME: <chain>` — the chain itself already
            // carries `: source: source...` via `core_to_provider`.
            let links = self.registry.links_for_var(var.id()).map_err(|e| {
                let chain = format_core_chain(&e);
                ProviderError::Backend(format!("links for {}: {chain}", var.name()))
            })?;
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
/// Walks the `source()` chain and joins each layer's `Display` with
/// `: ` so the inner cause (storage error, FS error kind, …) reaches
/// the toast. Without the chain walk the TUI would only see the
/// outer variant name and the user could not act on it.
///
/// Phase 2 can split availability vs. integrity if surfacing
/// different colors / actions becomes useful.
fn core_to_provider(err: &CoreError) -> ProviderError {
    ProviderError::Backend(format_core_chain(err))
}

/// Render an error and its full `source()` chain into
/// `outer: middle: leaf` form. Lives here rather than in `error.rs`
/// so the backend adapter does not need to depend on that module.
fn format_core_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut msg = err.to_string();
    let mut source = err.source();
    while let Some(inner) = source {
        msg.push_str(": ");
        msg.push_str(&inner.to_string());
        source = inner.source();
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use evault_core::model::{Group, VarKind};
    use secrecy::SecretString;

    #[test]
    fn empty_backend_lists_no_vars() {
        let backend = InMemoryBackend::new();
        let rows = backend.list().expect("list must not fail on empty");
        assert!(rows.is_empty());
    }

    #[test]
    fn list_returns_summary_for_created_var_sorted_by_name() {
        let backend = InMemoryBackend::new();
        // Create directly via the underlying registry (subcommands are
        // phase 2; this test exercises the adapter, not the CLI surface).
        backend
            .registry()
            .create_var(
                "BETA",
                Group::User,
                VarKind::Plain,
                SecretString::new("v2".to_owned().into()),
            )
            .expect("create BETA");
        backend
            .registry()
            .create_var(
                "ALPHA",
                Group::User,
                VarKind::Plain,
                SecretString::new("v1".to_owned().into()),
            )
            .expect("create ALPHA");

        let rows = backend.list().expect("list must succeed");
        assert_eq!(rows.len(), 2);
        // Sort policy: alphabetical by name (asserted explicitly so a
        // future refactor that drops the sort fails loudly).
        assert_eq!(rows[0].name, "ALPHA");
        assert_eq!(rows[1].name, "BETA");
        // Length is the size of the stored value, not the metadata.
        assert_eq!(rows[0].value_len, 2);
    }

    #[test]
    fn delete_removes_the_var_from_subsequent_listings() {
        let backend = InMemoryBackend::new();
        let id = backend
            .registry()
            .create_var(
                "DOOMED",
                Group::User,
                VarKind::Plain,
                SecretString::new("x".to_owned().into()),
            )
            .expect("create");
        VarMutator::delete(&backend, id).expect("delete");
        assert!(backend.list().expect("list").is_empty());
    }

    /// Synthetic error with a chain — used to exercise the chain
    /// walker without depending on whichever `CoreError` variants
    /// happen to be chained today.
    #[derive(Debug)]
    struct Outer(Inner);
    #[derive(Debug)]
    struct Inner;
    impl std::fmt::Display for Outer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("outer")
        }
    }
    impl std::fmt::Display for Inner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("inner")
        }
    }
    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }
    impl std::error::Error for Inner {}

    #[test]
    fn format_core_chain_walks_all_levels() {
        assert_eq!(format_core_chain(&Outer(Inner)), "outer: inner");
    }
}
