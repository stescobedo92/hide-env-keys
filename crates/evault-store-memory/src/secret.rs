//! [`MemorySecretStore`] — in-memory implementation of
//! [`evault_core::traits::SecretStore`].

use std::collections::HashMap;
use std::sync::RwLock;

use evault_core::crypto::SecretString;
use evault_core::error::SecretError;
use evault_core::model::VarId;
use evault_core::traits::SecretStore;

/// In-memory store for secret values keyed by [`VarId`].
///
/// Secrets are wrapped in [`SecretString`] inside the map so that they wipe
/// on drop, but the backing buffer is still heap memory in the same process
/// — this is **only** suitable for tests, never for production. For
/// production, use `evault-store-keyring`.
///
/// # Examples
/// ```
/// use evault_core::crypto::{ExposeSecret, SecretString};
/// use evault_core::model::VarId;
/// use evault_core::traits::SecretStore;
/// use evault_store_memory::MemorySecretStore;
///
/// let store = MemorySecretStore::new();
/// let id = VarId::new_v4();
/// store.put(id, SecretString::from(String::from("hunter2"))).unwrap();
/// let got = store.get(id).unwrap().expect("present");
/// assert_eq!(got.expose_secret(), "hunter2");
/// ```
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    inner: RwLock<HashMap<VarId, SecretString>>,
}

impl MemorySecretStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemorySecretStore {
    fn put(&self, id: VarId, value: SecretString) -> Result<(), SecretError> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| SecretError::Backend("in-memory secret store lock poisoned".into()))?;
        guard.insert(id, value);
        drop(guard);
        Ok(())
    }

    fn get(&self, id: VarId) -> Result<Option<SecretString>, SecretError> {
        let guard = self
            .inner
            .read()
            .map_err(|_| SecretError::Backend("in-memory secret store lock poisoned".into()))?;
        // Clone the SecretString so the caller owns its copy; the map retains
        // its own. Both wipe on drop independently.
        let cloned = guard.get(&id).cloned();
        drop(guard);
        Ok(cloned)
    }

    fn delete(&self, id: VarId) -> Result<(), SecretError> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| SecretError::Backend("in-memory secret store lock poisoned".into()))?;
        guard.remove(&id);
        drop(guard);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evault_core::crypto::ExposeSecret;

    #[test]
    fn put_then_get_roundtrips() {
        let store = MemorySecretStore::new();
        let id = VarId::new_v4();
        store
            .put(id, SecretString::from(String::from("s3cret")))
            .unwrap();
        let got = store.get(id).unwrap().expect("present");
        assert_eq!(got.expose_secret(), "s3cret");
    }

    #[test]
    fn get_missing_returns_none() {
        let store = MemorySecretStore::new();
        let missing = VarId::new_v4();
        assert!(store.get(missing).unwrap().is_none());
    }

    #[test]
    fn delete_removes_secret() {
        let store = MemorySecretStore::new();
        let id = VarId::new_v4();
        store
            .put(id, SecretString::from(String::from("s")))
            .unwrap();
        store.delete(id).unwrap();
        assert!(store.get(id).unwrap().is_none());
    }

    #[test]
    fn put_overwrites_previous_value() {
        let store = MemorySecretStore::new();
        let id = VarId::new_v4();
        store
            .put(id, SecretString::from(String::from("old")))
            .unwrap();
        store
            .put(id, SecretString::from(String::from("new")))
            .unwrap();
        let got = store.get(id).unwrap().expect("present");
        assert_eq!(got.expose_secret(), "new");
    }
}
