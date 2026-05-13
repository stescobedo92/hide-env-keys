//! [`OsKeyringSecretStore`] — wraps `keyring_core::Entry` to satisfy the
//! [`SecretStore`](evault_core::traits::SecretStore) trait contract.

use std::sync::OnceLock;

use keyring_core::Entry;

use evault_core::crypto::{ExposeSecret, SecretString};
use evault_core::error::SecretError;
use evault_core::model::VarId;
use evault_core::traits::SecretStore;

use crate::errors::{is_not_found, map};

/// Canonical service identifier under which `evault` stores all secrets in
/// the OS native keyring.
pub const DEFAULT_SERVICE: &str = "evault";

/// `SecretStore` impl backed by the platform's native credential store.
///
/// Each variable's value is stored as a single credential keyed by the
/// variable's UUID. See the crate-level docs for the per-platform mapping.
///
/// # Examples
/// ```ignore
/// use evault_store_keyring::OsKeyringSecretStore;
///
/// let store = OsKeyringSecretStore::new().expect("init keyring");
/// # let _ = store;
/// ```
pub struct OsKeyringSecretStore {
    service: String,
}

impl std::fmt::Debug for OsKeyringSecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the service identifier or any credential text. The
        // service is recoverable from the constructor; including it here
        // would let it bleed into logs.
        f.debug_struct("OsKeyringSecretStore")
            .finish_non_exhaustive()
    }
}

impl OsKeyringSecretStore {
    /// Construct a store using the canonical `"evault"` service identifier.
    ///
    /// Initializes the platform-native backend on first call; subsequent
    /// calls re-use the same backend.
    ///
    /// # Errors
    /// Returns [`SecretError::Unavailable`] if the platform offers no usable
    /// secret store (e.g. headless Linux without D-Bus). Returns
    /// [`SecretError::Backend`] for other initialisation failures.
    pub fn new() -> Result<Self, SecretError> {
        Self::with_service(DEFAULT_SERVICE)
    }

    /// Construct a store using a caller-supplied service identifier.
    /// Useful for tests that need to isolate from any production data.
    ///
    /// # Errors
    /// Returns [`SecretError::Backend`] with label `"empty_service"` if
    /// the service identifier is empty (platform behaviour for the empty
    /// case is inconsistent — Windows DPAPI accepts it, Linux Secret
    /// Service may reject it, macOS Keychain may silently accept). The
    /// remaining error cases are the same as [`Self::new`].
    pub fn with_service(service: impl Into<String>) -> Result<Self, SecretError> {
        let service = service.into();
        if service.is_empty() {
            return Err(SecretError::Backend("empty_service".into()));
        }
        ensure_native_backend()?;
        Ok(Self { service })
    }

    fn entry(&self, id: VarId) -> Result<Entry, SecretError> {
        Entry::new(&self.service, &id.as_uuid().to_string()).map_err(map)
    }
}

/// Initialize the OS-native credential store on first call; cache the
/// result so subsequent calls are a single atomic load.
///
/// The `keyring` 4.x API requires a one-time global selector. We pass
/// `true` so Linux uses Secret Service (`gnome-keyring` / `KWallet`) rather
/// than kernel keyutils — the latter does not persist across reboots,
/// which would silently drop every secret on next boot.
fn ensure_native_backend() -> Result<(), SecretError> {
    static INIT: OnceLock<Result<(), SecretError>> = OnceLock::new();
    let result = INIT.get_or_init(|| keyring::use_native_store(true).map_err(map));
    match result {
        Ok(()) => Ok(()),
        Err(SecretError::Unavailable) => Err(SecretError::Unavailable),
        Err(SecretError::Backend(s)) => Err(SecretError::Backend(s.clone())),
        Err(_) => Err(SecretError::Backend("keyring init".into())),
    }
}

impl SecretStore for OsKeyringSecretStore {
    fn put(&self, id: VarId, value: SecretString) -> Result<(), SecretError> {
        let entry = self.entry(id)?;
        entry.set_password(value.expose_secret()).map_err(map)
    }

    fn get(&self, id: VarId) -> Result<Option<SecretString>, SecretError> {
        let entry = self.entry(id)?;
        match entry.get_password() {
            Ok(s) => Ok(Some(SecretString::from(s))),
            Err(e) if is_not_found(&e) => Ok(None),
            Err(e) => Err(map(e)),
        }
    }

    fn delete(&self, id: VarId) -> Result<(), SecretError> {
        let entry = self.entry(id)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(e) if is_not_found(&e) => Ok(()),
            Err(e) => Err(map(e)),
        }
    }
}
