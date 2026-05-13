//! [`SecretStore`] — opaque storage of secret variable values.
//!
//! Production implementations delegate to the operating system's keyring
//! (`evault-store-keyring`). In headless environments without `DBus` / Secret
//! Service, a fallback backed by an `age`-encrypted file may be used.

use crate::crypto::SecretString;
use crate::error::SecretError;
use crate::model::VarId;

/// Backend that holds **secret values** keyed by [`VarId`].
///
/// Values must never be returned through other channels (e.g. `Debug`) nor
/// retained longer than the immediate caller needs. Implementations are
/// expected to use the host OS's native secret storage; see the architecture
/// notes in the workspace README.
pub trait SecretStore: Send + Sync {
    /// Store `value` for `id`, replacing any previous value.
    ///
    /// # Errors
    /// Returns [`SecretError::Backend`] if the keyring rejected the write or
    /// [`SecretError::Unavailable`] if the host platform offers no usable
    /// secret storage and no fallback was configured.
    fn put(&self, id: VarId, value: SecretString) -> Result<(), SecretError>;

    /// Retrieve the secret value for `id`, or `Ok(None)` if absent.
    ///
    /// # Errors
    /// Returns [`SecretError::Backend`] on backend failure.
    fn get(&self, id: VarId) -> Result<Option<SecretString>, SecretError>;

    /// Delete the secret value for `id`. No-op if absent.
    ///
    /// # Errors
    /// Returns [`SecretError::Backend`] on backend failure.
    fn delete(&self, id: VarId) -> Result<(), SecretError>;
}
