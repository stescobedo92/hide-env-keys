//! Secret value types.
//!
//! These are thin newtypes around [`secrecy::SecretBox`] tuned to the two
//! shapes we actually store: UTF-8 string values (`SecretString`) and fixed
//! byte buffers (`SecretBytes<N>`). Both wipe their contents on drop and
//! redact in `Debug` output.

use secrecy::SecretBox;

/// A heap-allocated UTF-8 secret string.
///
/// This is the type alias defined by [`secrecy`]; we re-export it under
/// [`crate::crypto::SecretString`] for ergonomics.
///
/// # Examples
/// ```
/// use evault_core::crypto::{ExposeSecret, SecretString};
///
/// let s = SecretString::from(String::from("hunter2"));
/// // `Debug` is redacted:
/// assert!(!format!("{s:?}").contains("hunter2"));
/// // Explicit access via `ExposeSecret`:
/// assert_eq!(s.expose_secret(), "hunter2");
/// ```
pub type SecretString = secrecy::SecretString;

/// A fixed-size byte buffer that wipes on drop.
///
/// Used for master keys and other binary secrets. The const parameter `N`
/// fixes the size so that callers do not have to validate it at runtime.
///
/// # Examples
/// ```
/// use evault_core::crypto::{ExposeSecret, SecretBytes};
///
/// let buf: SecretBytes<32> = secrecy::SecretBox::new(Box::new([0xAB_u8; 32]));
/// assert_eq!(buf.expose_secret()[0], 0xAB);
/// ```
pub type SecretBytes<const N: usize> = SecretBox<[u8; N]>;
