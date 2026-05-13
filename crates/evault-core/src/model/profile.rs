//! [`Profile`] for per-project variants (dev / staging / prod / etc.).

use serde::{Deserialize, Serialize};

use crate::error::ManifestError;

/// A named variant of a project's variable bindings.
///
/// Projects often need different values for the same variable across
/// environments — for instance, `DATABASE_URL` differs between development
/// and production. A `Profile` lets each project distinguish those variants
/// without creating duplicate registry entries.
///
/// The canonical default profile is named `"default"`. Use
/// [`Profile::default_profile`] to obtain it.
///
/// # Examples
/// ```
/// use evault_core::model::Profile;
///
/// assert!(Profile::default_profile().is_default());
/// assert_eq!(Profile::try_named("dev").expect("valid").as_str(), "dev");
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Profile(String);

impl Profile {
    /// The canonical default profile name.
    pub const DEFAULT_NAME: &'static str = "default";

    /// Construct the canonical default profile.
    #[must_use]
    pub fn default_profile() -> Self {
        Self(Self::DEFAULT_NAME.to_owned())
    }

    /// Construct a named profile from already-validated input.
    ///
    /// If `name == "default"`, returns [`Self::default_profile`] to keep a
    /// single canonical representation. Otherwise the string is stored
    /// verbatim. **No validation**: use [`Self::try_named`] for user input.
    pub fn named(name: impl Into<String>) -> Self {
        let name = name.into();
        if name == Self::DEFAULT_NAME {
            return Self::default_profile();
        }
        Self(name)
    }

    /// Construct a named profile from possibly-untrusted input.
    ///
    /// Accepted names:
    /// - 1 to 32 characters
    /// - ASCII alphanumerics, hyphen, or underscore
    /// - not entirely composed of digits
    ///
    /// `"default"` is normalized to the canonical default profile.
    ///
    /// # Errors
    /// Returns [`ManifestError::Invalid`] if `name` violates any of the
    /// rules above.
    pub fn try_named(name: impl Into<String>) -> Result<Self, ManifestError> {
        let name = name.into();
        if name.is_empty() {
            return Err(ManifestError::Invalid("profile name is empty".into()));
        }
        if name.len() > 32 {
            return Err(ManifestError::Invalid(
                "profile name longer than 32 characters".into(),
            ));
        }
        for (offset, &b) in name.as_bytes().iter().enumerate() {
            if !(b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
                return Err(ManifestError::Invalid(format!(
                    "profile name contains an invalid character at byte offset {offset}"
                )));
            }
        }
        if name.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ManifestError::Invalid(
                "profile name must contain at least one non-digit character".into(),
            ));
        }
        Ok(Self::named(name))
    }

    /// Returns the profile name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` if this is the canonical default profile.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.0 == Self::DEFAULT_NAME
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self::default_profile()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_named_default() {
        let p = Profile::default_profile();
        assert!(p.is_default());
        assert_eq!(p.as_str(), "default");
    }

    #[test]
    fn named_profile_is_not_default() {
        let p = Profile::named("dev");
        assert!(!p.is_default());
        assert_eq!(p.as_str(), "dev");
    }

    #[test]
    fn named_default_normalizes_to_canonical() {
        let p = Profile::named("default");
        assert!(p.is_default());
        assert_eq!(p, Profile::default_profile());
    }

    #[test]
    fn try_named_accepts_valid_names() {
        for name in ["dev", "staging", "prod-2", "feat_branch_123"] {
            assert!(Profile::try_named(name).is_ok(), "expected {name} ok");
        }
    }

    #[test]
    fn try_named_rejects_invalid_names() {
        let bad = [
            "",
            " ",
            "with space",
            "with\nnewline",
            "[evil]",
            "123",
            &"a".repeat(33),
        ];
        for name in bad {
            assert!(
                Profile::try_named(name).is_err(),
                "expected {name:?} rejected"
            );
        }
    }

    #[test]
    fn try_named_default_normalizes() {
        let p = Profile::try_named("default").expect("valid");
        assert!(p.is_default());
    }

    #[test]
    fn profile_serializes_transparently() {
        let p = Profile::named("staging");
        let s = serde_json::to_string(&p).expect("serde");
        assert_eq!(s, "\"staging\"");
    }
}
