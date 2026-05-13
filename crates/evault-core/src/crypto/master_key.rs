//! [`MasterKey`] — the 256-bit symmetric key used to unlock the encrypted
//! metadata store.
//!
//! The key is generated with the OS CSPRNG ([`rand::rngs::SysRng`]) and
//! stored in the OS keyring under a well-known service/account pair. It is
//! never written to disk in plaintext.

use std::fmt;

use rand::rngs::SysRng;
use rand::TryRng;
use secrecy::{ExposeSecret, SecretBox};
use zeroize::Zeroize;

use crate::error::SecretError;

/// Length of [`MasterKey`] material in bytes (256 bits).
pub const MASTER_KEY_LEN: usize = 32;

/// A 256-bit symmetric key that wipes its contents on drop.
///
/// Used as the `SQLCipher` key for the metadata store. The constructor only
/// exposes generated keys and rehydration from existing bytes; the bytes
/// themselves never leak through `Display`, `Debug`, or `Serialize` impls.
///
/// # Examples
/// ```
/// use evault_core::crypto::{ExposeSecret, MasterKey, MASTER_KEY_LEN};
/// let k = MasterKey::generate().expect("OS RNG should be available");
/// assert_eq!(k.bytes().expose_secret().len(), MASTER_KEY_LEN);
/// ```
pub struct MasterKey {
    inner: SecretBox<[u8; MASTER_KEY_LEN]>,
}

// `Debug` is written by hand so that future fields cannot accidentally leak
// through a derived implementation. The inner secret box also redacts itself,
// but defense in depth: only the type name is printed.
impl fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MasterKey").finish_non_exhaustive()
    }
}

impl MasterKey {
    /// Generate a new key using the operating system's secure RNG.
    ///
    /// # Errors
    /// Returns [`SecretError::Backend`] if the OS RNG is unavailable, which
    /// is essentially fatal — every supported platform exposes one and
    /// failures here generally indicate a sandbox restriction or a hardware
    /// fault.
    pub fn generate() -> Result<Self, SecretError> {
        let mut bytes = [0_u8; MASTER_KEY_LEN];
        SysRng
            .try_fill_bytes(&mut bytes)
            .map_err(|e| SecretError::Backend(format!("OS RNG: {e}")))?;
        Ok(Self::from_bytes(bytes))
    }

    /// Wrap pre-existing bytes (e.g. fetched from the OS keyring).
    ///
    /// Prefer [`Self::generate`] for new keys.
    #[must_use]
    pub fn from_bytes(bytes: [u8; MASTER_KEY_LEN]) -> Self {
        Self {
            inner: SecretBox::new(Box::new(bytes)),
        }
    }

    /// Borrow the key bytes through a [`SecretBox`].
    ///
    /// Use [`ExposeSecret::expose_secret`] (re-exported as
    /// [`crate::crypto::ExposeSecret`]) to obtain the raw byte slice when
    /// passing it to the encryption layer.
    #[must_use]
    pub const fn bytes(&self) -> &SecretBox<[u8; MASTER_KEY_LEN]> {
        &self.inner
    }

    /// Encode the key as lowercase hex.
    ///
    /// `SQLCipher` accepts hex keys in the form `PRAGMA key = "x'<hex>'"`.
    /// The returned [`SecretString`](crate::crypto::SecretString) is itself
    /// redacted in `Debug` output and zeroized on drop.
    ///
    /// The staging buffer used to build the hex representation is explicitly
    /// zeroized before this function returns so that the plaintext hex form
    /// of the key cannot linger in heap memory.
    ///
    /// # Panics
    /// This function never panics in practice: it constructs the hex
    /// representation from a fixed ASCII alphabet, so the
    /// [`std::str::from_utf8`] check on the staging buffer always succeeds.
    /// The `expect` call exists only to encode the invariant structurally;
    /// clippy's `expect_used` lint is allowed locally.
    #[must_use]
    pub fn to_hex_secret(&self) -> crate::crypto::SecretString {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let bytes = self.inner.expose_secret();
        let mut buf: Vec<u8> = vec![0_u8; MASTER_KEY_LEN * 2];
        for (i, b) in bytes.iter().enumerate() {
            buf[2 * i] = HEX[(b >> 4) as usize];
            buf[2 * i + 1] = HEX[(b & 0x0F) as usize];
        }
        // `buf` contains only bytes from `HEX`, which are all valid ASCII and
        // therefore valid UTF-8. Build a fresh `Box<str>` (a separate heap
        // allocation) and then wipe `buf` before it goes out of scope.
        let boxed: Box<str> = {
            #[allow(clippy::expect_used)]
            let hex_str: &str = std::str::from_utf8(&buf).expect("hex characters are always ASCII");
            Box::<str>::from(hex_str)
        };
        buf.zeroize();
        crate::crypto::SecretString::from(boxed)
    }

    /// Compare two master keys in constant time.
    ///
    /// `[u8; N]` uses bytewise `memcmp` which is allowed by the compiler to
    /// short-circuit on the first mismatching byte — a textbook timing side
    /// channel. This helper folds an OR-accumulator over every byte so that
    /// the running time depends only on the key length, not on the data.
    ///
    /// Callers that need to compare key material **must** use this method
    /// instead of `==` on the bytes returned by `bytes().expose_secret()`.
    ///
    /// # Examples
    /// ```
    /// use evault_core::crypto::{MasterKey, MASTER_KEY_LEN};
    ///
    /// let a = MasterKey::from_bytes([0xAB; MASTER_KEY_LEN]);
    /// let b = MasterKey::from_bytes([0xAB; MASTER_KEY_LEN]);
    /// let c = MasterKey::from_bytes([0xCD; MASTER_KEY_LEN]);
    /// assert!(a.ct_eq(&b));
    /// assert!(!a.ct_eq(&c));
    /// ```
    #[must_use]
    pub fn ct_eq(&self, other: &Self) -> bool {
        let a = self.inner.expose_secret();
        let b = other.inner.expose_secret();
        // OR-accumulate every byte difference. `iter().zip()` avoids any
        // bounds-check panic edge; no `if`, no `break` — LLVM cannot legally
        // short-circuit. `black_box` prevents future optimiser passes from
        // defeating the loop.
        let mut diff: u8 = 0;
        for (x, y) in a.iter().zip(b.iter()) {
            diff |= x ^ y;
        }
        std::hint::black_box(diff) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_correct_length() {
        let k = MasterKey::generate().expect("OS RNG");
        assert_eq!(k.bytes().expose_secret().len(), MASTER_KEY_LEN);
    }

    #[test]
    fn two_generated_keys_are_distinct() {
        let a = MasterKey::generate().expect("OS RNG");
        let b = MasterKey::generate().expect("OS RNG");
        assert_ne!(a.bytes().expose_secret(), b.bytes().expose_secret());
    }

    #[test]
    fn from_bytes_roundtrips() {
        let bytes = [0xA5_u8; MASTER_KEY_LEN];
        let k = MasterKey::from_bytes(bytes);
        assert_eq!(k.bytes().expose_secret(), &bytes);
    }

    #[test]
    fn to_hex_secret_has_double_length() {
        let bytes = [0xAB_u8; MASTER_KEY_LEN];
        let k = MasterKey::from_bytes(bytes);
        let hex = k.to_hex_secret();
        assert_eq!(hex.expose_secret().len(), MASTER_KEY_LEN * 2);
        assert_eq!(hex.expose_secret(), &"ab".repeat(MASTER_KEY_LEN));
    }

    #[test]
    fn to_hex_secret_uses_lowercase_alphabet() {
        let bytes = [0xDE_u8; MASTER_KEY_LEN];
        let hex = MasterKey::from_bytes(bytes).to_hex_secret();
        assert!(hex
            .expose_secret()
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn debug_redacts_key_bytes() {
        let k = MasterKey::from_bytes([0xCD_u8; MASTER_KEY_LEN]);
        let dbg = format!("{k:?}");
        assert!(!dbg.contains("cd"));
        assert!(!dbg.contains("CD"));
        // Should print only the struct name and a `..` marker thanks to
        // `finish_non_exhaustive`.
        assert!(dbg.contains("MasterKey"));
        assert!(dbg.contains(".."));
    }

    #[test]
    fn ct_eq_is_true_for_equal_keys() {
        let a = MasterKey::from_bytes([0x12; MASTER_KEY_LEN]);
        let b = MasterKey::from_bytes([0x12; MASTER_KEY_LEN]);
        assert!(a.ct_eq(&b));
    }

    #[test]
    fn ct_eq_is_false_for_keys_that_differ_only_in_last_byte() {
        let a = MasterKey::from_bytes([0x12; MASTER_KEY_LEN]);
        let mut diff = [0x12_u8; MASTER_KEY_LEN];
        diff[MASTER_KEY_LEN - 1] = 0x13;
        let b = MasterKey::from_bytes(diff);
        assert!(!a.ct_eq(&b));
    }

    #[test]
    fn ct_eq_is_false_for_keys_that_differ_only_in_first_byte() {
        let a = MasterKey::from_bytes([0x12; MASTER_KEY_LEN]);
        let mut diff = [0x12_u8; MASTER_KEY_LEN];
        diff[0] = 0x13;
        let b = MasterKey::from_bytes(diff);
        assert!(!a.ct_eq(&b));
    }
}
