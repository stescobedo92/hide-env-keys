#![allow(clippy::doc_markdown)]
//! Persistent backend: SQLite metadata (encrypted with SQLCipher
//! when the `sqlcipher` feature is enabled on `evault-store-sqlcipher`)
//! + OS keyring secret store.
//!
//! # Master key bootstrap
//!
//! The metadata store is opened with a 32-byte master key. On first
//! run the key is generated with [`MasterKey::generate`] and stored
//! in the OS keyring under the service `evault` / username
//! `master-key` as a hex string. On subsequent runs the key is read
//! back from the keyring.
//!
//! If the OS keyring is not available (Linux headless without
//! DBus / Secret Service), [`SqlCipherBackend::open_or_init`]
//! returns a descriptive [`SqlCipherOpenError`]. The CLI's top-level
//! dispatch surfaces it to the user.
//!
//! # Storage layout
//!
//! - Metadata DB: `<data_dir>/evault/db.sqlite`
//!   - Linux: `~/.local/share/evault/db.sqlite`
//!   - macOS: `~/Library/Application Support/evault/db.sqlite`
//!   - Windows: `%APPDATA%\evault\db.sqlite`
//! - Secret values: OS keyring under service `evault`, user `<VarId>`

use std::path::{Path, PathBuf};

use evault_core::crypto::{MasterKey, MASTER_KEY_LEN};
use evault_core::model::{
    AuditEntry, Group, Profile, Project, ProjectId, ProjectVar, Var, VarFilter, VarId, VarKind,
};
use evault_core::service::RegistryService;
use evault_core::traits::{SystemClock, UuidV4IdGenerator};
use evault_store_keyring::OsKeyringSecretStore;
use evault_store_sqlcipher::SqlCipherMetadataStore;
use evault_tui::AuditProvider;
use evault_tui::{ProviderError, VarDraft, VarMutator, VarProvider, VarSummary};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;

use super::{core_to_provider, format_core_chain, BackendOps};

/// Service identifier used in both the metadata-DB-master-key
/// keyring entry and (via `OsKeyringSecretStore::with_service`)
/// every per-variable secret value.
pub const KEYRING_SERVICE: &str = "evault";

/// Username under which the master key (hex-encoded) is stored in
/// the OS keyring. Picked to be distinct from any plausible
/// [`VarId`] string (which is a UUID v4).
pub const MASTER_KEY_KEYRING_USER: &str = "master-key";

/// Concrete persistent registry shape.
pub type SqlCipherRegistry = RegistryService<
    SqlCipherMetadataStore,
    OsKeyringSecretStore,
    SqlCipherMetadataStore,
    SystemClock,
    UuidV4IdGenerator,
>;

/// Failures while opening the persistent backend.
///
/// Distinct from [`ProviderError`] because these are top-level
/// (no TUI is running yet); the CLI's `main` formats them with a
/// chain walk before exit.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum SqlCipherOpenError {
    /// Could not resolve a data directory for the current platform.
    #[error("could not determine a data directory for the current platform")]
    NoDataDir,
    /// The data directory could not be created.
    #[error("create data dir '{path}': {source}")]
    CreateDataDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Bootstrapping the master key via the OS keyring failed.
    /// Common cause on Linux: no DBus / Secret Service available.
    #[error("OS keyring bootstrap: {0}")]
    Keyring(String),
    /// Opening the SQLite database failed (locked, corrupt, wrong
    /// master key, …).
    #[error("open database '{path}': {source}")]
    OpenDb {
        path: PathBuf,
        #[source]
        source: evault_store_sqlcipher::SqlCipherOpenError,
    },
    /// Initialising the per-variable secret store failed.
    #[error("init secret store: {0}")]
    SecretStore(String),
}

/// Persistent backend.
pub struct SqlCipherBackend {
    registry: SqlCipherRegistry,
    /// Filesystem location of the metadata DB. Kept for the eventual
    /// `evault status` subcommand and the `db_path` accessor below.
    #[allow(dead_code)]
    db_path: PathBuf,
}

impl SqlCipherBackend {
    /// Open the persistent backend, bootstrapping the master key from
    /// (or into) the OS keyring on first run.
    ///
    /// # Errors
    /// Returns [`SqlCipherOpenError`] on any bootstrap step that
    /// fails — keyring unavailable, database locked, master key not
    /// 32 bytes, etc.
    pub fn open_or_init() -> Result<Self, SqlCipherOpenError> {
        let db_path = default_db_path()?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| {
                SqlCipherOpenError::CreateDataDir {
                    path: parent.to_path_buf(),
                    source,
                }
            })?;
        }
        // Create the secret store FIRST so its OnceLock-guarded call
        // to `keyring::use_native_store(true)` runs before any
        // `keyring_core::Entry::new` below. The store is reused as
        // the [`SecretStore`] for variable values once the registry
        // is wired up.
        let secrets = OsKeyringSecretStore::with_service(KEYRING_SERVICE)
            .map_err(|e| SqlCipherOpenError::SecretStore(e.to_string()))?;
        let master = load_or_create_master_key()?;
        let metadata = SqlCipherMetadataStore::open(&db_path, &master).map_err(|source| {
            SqlCipherOpenError::OpenDb {
                path: db_path.clone(),
                source,
            }
        })?;
        let audit = metadata.clone();
        Ok(Self {
            registry: RegistryService::with_defaults(metadata, secrets, audit),
            db_path,
        })
    }

    /// Filesystem location of the metadata database. Useful for the
    /// CLI to display in `evault status`-style hints once that
    /// subcommand lands.
    #[allow(dead_code)]
    #[must_use]
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

/// Compute the canonical location of the metadata database.
fn default_db_path() -> Result<PathBuf, SqlCipherOpenError> {
    let base = dirs::data_dir().ok_or(SqlCipherOpenError::NoDataDir)?;
    Ok(base.join("evault").join("db.sqlite"))
}

/// Read the master key from the OS keyring, generating + storing a
/// fresh one on first run.
///
/// Caller MUST have already triggered the keyring native-store init
/// (via `OsKeyringSecretStore::with_service`) so that
/// `keyring_core::Entry` operations have a backend.
fn load_or_create_master_key() -> Result<MasterKey, SqlCipherOpenError> {
    let entry = keyring_core::Entry::new(KEYRING_SERVICE, MASTER_KEY_KEYRING_USER)
        .map_err(|e| SqlCipherOpenError::Keyring(format!("entry init: {e}")))?;
    match entry.get_password() {
        Ok(hex) => decode_hex_key(&hex).map_err(SqlCipherOpenError::Keyring),
        Err(keyring_core::Error::NoEntry) => {
            let key = MasterKey::generate()
                .map_err(|e| SqlCipherOpenError::Keyring(format!("generate: {e}")))?;
            let hex = key.to_hex_secret();
            entry
                .set_password(hex.expose_secret())
                .map_err(|e| SqlCipherOpenError::Keyring(format!("store: {e}")))?;
            Ok(key)
        }
        Err(other) => Err(SqlCipherOpenError::Keyring(format!("read: {other}"))),
    }
}

fn decode_hex_key(hex: &str) -> Result<MasterKey, String> {
    if hex.len() != MASTER_KEY_LEN * 2 {
        return Err(format!(
            "stored master key has wrong length: expected {} hex chars, got {}",
            MASTER_KEY_LEN * 2,
            hex.len()
        ));
    }
    let mut bytes = [0_u8; MASTER_KEY_LEN];
    for (i, b) in bytes.iter_mut().enumerate() {
        let pair = hex
            .get(2 * i..2 * i + 2)
            .ok_or_else(|| format!("master key hex truncated at byte {i}"))?;
        *b = u8::from_str_radix(pair, 16).map_err(|e| format!("invalid hex at byte {i}: {e}"))?;
    }
    Ok(MasterKey::from_bytes(bytes))
}

impl VarProvider for SqlCipherBackend {
    fn list(&self) -> Result<Vec<VarSummary>, ProviderError> {
        let vars = self
            .registry
            .list_vars(&VarFilter::new())
            .map_err(|e| core_to_provider(&e))?;
        let mut summaries = Vec::with_capacity(vars.len());
        for var in vars {
            summaries.push(self.summarise(&var)?);
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(summaries)
    }

    fn get_value(&self, id: VarId) -> Result<Option<SecretString>, ProviderError> {
        BackendOps::get_value(self, id)
    }
}

impl AuditProvider for SqlCipherBackend {
    fn recent_audit(&self, limit: usize) -> Result<Vec<AuditEntry>, ProviderError> {
        BackendOps::recent_audit(self, limit)
    }
}

impl VarMutator for SqlCipherBackend {
    fn delete(&self, id: VarId) -> Result<(), ProviderError> {
        self.registry
            .delete_var(id)
            .map_err(|e| core_to_provider(&e))
    }

    fn create(&self, draft: VarDraft) -> Result<VarId, ProviderError> {
        BackendOps::create_var(self, &draft.name, draft.group, draft.kind, draft.value)
    }

    fn update_value(&self, id: VarId, value: SecretString) -> Result<(), ProviderError> {
        BackendOps::update_value(self, id, value)
    }

    fn link_to_project(
        &self,
        var_id: VarId,
        var_name: String,
        project_path: PathBuf,
        profile: String,
        materialize: bool,
    ) -> Result<(), ProviderError> {
        super::link_helper::link_to_project(
            self,
            var_id,
            var_name,
            project_path,
            profile,
            materialize,
        )
    }

    fn run_in_project(
        &self,
        project_path: PathBuf,
        profile: String,
        program: String,
        args: Vec<String>,
    ) -> Result<Option<i32>, ProviderError> {
        super::run_helper::run_in_project(self, project_path, profile, program, args)
    }
}

impl BackendOps for SqlCipherBackend {
    fn find_var_by_name(&self, name: &str) -> Result<Option<Var>, ProviderError> {
        self.registry
            .find_var_by_name(name)
            .map_err(|e| core_to_provider(&e))
    }

    fn create_var(
        &self,
        name: &str,
        group: Group,
        kind: VarKind,
        value: SecretString,
    ) -> Result<VarId, ProviderError> {
        self.registry
            .create_var(name, group, kind, value)
            .map_err(|e| core_to_provider(&e))
    }

    fn update_value(&self, id: VarId, value: SecretString) -> Result<(), ProviderError> {
        self.registry
            .update_value(id, value)
            .map_err(|e| core_to_provider(&e))
    }

    fn get_value(&self, id: VarId) -> Result<Option<SecretString>, ProviderError> {
        self.registry
            .get_value(id)
            .map_err(|e| core_to_provider(&e))
    }

    fn list_projects(&self) -> Result<Vec<Project>, ProviderError> {
        self.registry
            .list_projects()
            .map_err(|e| core_to_provider(&e))
    }

    fn find_project_by_path(&self, path: &Path) -> Result<Option<Project>, ProviderError> {
        let projects = self.list_projects()?;
        Ok(projects.into_iter().find(|p| p.path() == path))
    }

    fn create_project(&self, name: &str, path: PathBuf) -> Result<ProjectId, ProviderError> {
        self.registry
            .create_project(name, path)
            .map_err(|e| core_to_provider(&e))
    }

    fn link_var(
        &self,
        project_id: ProjectId,
        var_id: VarId,
        profile: Profile,
        alias: Option<String>,
    ) -> Result<(), ProviderError> {
        self.registry
            .link_var(project_id, var_id, profile, alias)
            .map_err(|e| core_to_provider(&e))
    }

    fn links_for_project(&self, project_id: ProjectId) -> Result<Vec<ProjectVar>, ProviderError> {
        self.registry
            .links_for_project(project_id)
            .map_err(|e| core_to_provider(&e))
    }

    fn recent_audit(&self, limit: usize) -> Result<Vec<AuditEntry>, ProviderError> {
        self.registry
            .recent_audit(limit)
            .map_err(|e| core_to_provider(&e))
    }

    fn summarise(&self, var: &Var) -> Result<VarSummary, ProviderError> {
        let links = self.registry.links_for_var(var.id()).map_err(|e| {
            let chain = format_core_chain(&e);
            ProviderError::Backend(format!("links for {}: {chain}", var.name()))
        })?;
        Ok(VarSummary {
            id: var.id(),
            name: var.name().to_owned(),
            group: var.group().clone(),
            kind: var.kind(),
            value_len: var.length(),
            linked_projects: links.len(),
            updated_at: var.updated_at(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_hex_key_roundtrip() {
        let original = MasterKey::from_bytes([0xAB; MASTER_KEY_LEN]);
        let hex = original.to_hex_secret();
        let decoded = decode_hex_key(hex.expose_secret()).expect("decode");
        assert!(original.ct_eq(&decoded));
    }

    #[test]
    fn decode_hex_key_rejects_wrong_length() {
        assert!(decode_hex_key("deadbeef").is_err());
    }

    #[test]
    fn decode_hex_key_rejects_invalid_hex() {
        let bad = "zz".repeat(MASTER_KEY_LEN);
        assert!(decode_hex_key(&bad).is_err());
    }
}
