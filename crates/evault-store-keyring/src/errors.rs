//! Mapping from `keyring_core::Error` to evault's typed
//! [`evault_core::error::SecretError`].
//!
//! Two responsibilities live here:
//! 1. Decide whether an error is the canonical "item does not exist"
//!    signal (which `get`/`delete` translate to `Ok(None)`/`Ok(())`) or a
//!    real backend failure.
//! 2. Reduce the error to a category label that never carries the service
//!    name, the account (which is the variable's UUID — not secret, but
//!    not useful either), or any platform-specific path or D-Bus name.

use evault_core::error::SecretError;

/// Returns `true` for the canonical "no credential exists for this key"
/// signal. The keyring crate uses [`keyring_core::Error::NoEntry`] for
/// this on every supported platform.
pub const fn is_not_found(e: &keyring_core::Error) -> bool {
    matches!(e, keyring_core::Error::NoEntry)
}

/// Map any non-not-found error to a typed [`SecretError`].
pub fn map(e: keyring_core::Error) -> SecretError {
    use keyring_core::Error;
    match e {
        Error::NoStorageAccess(_) => SecretError::Unavailable,
        // `NoEntry` should be filtered upstream by `is_not_found` before
        // reaching this map; if it still arrives here treat it as a
        // backend bug.
        Error::NoEntry => SecretError::Backend("no_entry_unfiltered".into()),
        other => SecretError::Backend(label(&other).into()),
    }
}

/// Stable category label for a `keyring_core::Error`. Never embeds raw
/// platform strings; the `keyring_core::Error` `Display` impl can include
/// service names or D-Bus error text on some platforms.
///
/// Every variant in `keyring-core 1.0` is enumerated so production
/// telemetry can distinguish e.g. "blob corrupted" from "init didn't
/// take effect" from "platform refused the operation". The `_` arm
/// remains for future additions under the enum's `#[non_exhaustive]`.
const fn label(e: &keyring_core::Error) -> &'static str {
    use keyring_core::Error;
    match e {
        Error::NoEntry => "no_entry",
        Error::NoStorageAccess(_) => "no_storage_access",
        Error::Ambiguous(_) => "ambiguous",
        Error::Invalid(_, _) => "invalid",
        Error::PlatformFailure(_) => "platform_failure",
        Error::BadEncoding(_) => "bad_encoding",
        Error::TooLong(_, _) => "too_long",
        Error::BadDataFormat(_, _) => "bad_data_format",
        Error::BadStoreFormat(_) => "bad_store_format",
        Error::NoDefaultStore => "no_default_store",
        Error::NotSupportedByStore(_) => "not_supported_by_store",
        // NOTE: `keyring_core::Error` is `#[non_exhaustive]`. Re-audit
        // this match on every `keyring-core` bump. In particular, a
        // future "access denied", "locked", or "user cancelled" variant
        // should route to `SecretError::Unavailable` in `map()` above,
        // not fall through here.
        _ => "other",
    }
}
