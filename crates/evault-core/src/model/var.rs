//! The [`Var`] entity, the central record of the registry.

use std::fmt;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::MetadataError;

/// Stable identifier of a [`Var`].
///
/// IDs are version-4 UUIDs. The wrapper exists so that callers cannot
/// accidentally confuse a [`VarId`] with a [`crate::model::ProjectId`].
///
/// # Examples
/// ```
/// use evault_core::model::VarId;
///
/// let a = VarId::new_v4();
/// let b = VarId::new_v4();
/// assert_ne!(a, b);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VarId(Uuid);

impl VarId {
    /// Generate a fresh identifier using [`Uuid::new_v4`].
    #[must_use]
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing [`Uuid`] without generating a new one.
    ///
    /// Use this when reconstructing a record from a backend that already has
    /// the id (e.g. a database row).
    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Borrow the inner [`Uuid`].
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for VarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Logical grouping of a variable.
///
/// Groups are a coarse classification displayed prominently in the dashboard.
/// Finer-grained categorisation is achieved through [`Var`] tags.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Group {
    /// Conventional user-scope (personal projects, one-off secrets).
    User,
    /// Conventional system-scope (machine-wide credentials, infrastructure).
    System,
    /// Conventional project-scope (used by a single project).
    Project,
    /// Custom group name supplied by the user.
    Custom(String),
}

impl Group {
    /// Returns the canonical short name used for display and serialization.
    ///
    /// # Examples
    /// ```
    /// use evault_core::model::Group;
    /// assert_eq!(Group::User.as_str(), "user");
    /// assert_eq!(Group::Custom("aws".into()).as_str(), "aws");
    /// ```
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::System => "system",
            Self::Project => "project",
            Self::Custom(s) => s.as_str(),
        }
    }
}

/// How a variable stores its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VarKind {
    /// The value lives in the secret store (OS keyring or encrypted fallback)
    /// and is never written to the metadata store.
    Secret,
    /// The value lives in the metadata store next to the [`Var`] record.
    /// Suitable for non-sensitive values like `NODE_ENV=production`.
    Plain,
}

/// A managed environment variable.
///
/// `Var` carries only metadata. The actual value is stored separately:
///
/// - For [`VarKind::Secret`], the value lives in the
///   [`SecretStore`](crate::traits::SecretStore).
/// - For [`VarKind::Plain`], the value lives in the
///   [`MetadataStore`](crate::traits::MetadataStore) and is fetched on demand
///   so we do not keep it in memory across operations.
///
/// # Examples
/// ```
/// use evault_core::model::{Var, Group, VarKind};
///
/// let v = Var::new("DATABASE_URL", Group::User, VarKind::Secret);
/// assert_eq!(v.name(), "DATABASE_URL");
/// assert_eq!(v.kind(), VarKind::Secret);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Var {
    id: VarId,
    name: String,
    group: Group,
    kind: VarKind,
    tags: Vec<String>,
    length: usize,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

impl Var {
    /// Create a new [`Var`] from already-validated input.
    ///
    /// This constructor does **not** call [`Self::validate_name`]; it is
    /// intended for tests and code paths that have already validated the
    /// name upstream. **Never** call this directly with user-supplied input:
    /// use [`Self::try_new`] instead.
    pub fn new(name: impl Into<String>, group: Group, kind: VarKind) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            id: VarId::new_v4(),
            name: name.into(),
            group,
            kind,
            tags: Vec::new(),
            length: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new [`Var`] from possibly-untrusted input, validating the
    /// name through [`Self::validate_name`].
    ///
    /// This is the constructor that CLI/TUI surfaces and any other path
    /// that accepts user input should use.
    ///
    /// # Errors
    /// Returns [`MetadataError::Invalid`] if `name` does not satisfy
    /// [`Self::validate_name`].
    ///
    /// # Examples
    /// ```
    /// use evault_core::model::{Var, Group, VarKind};
    ///
    /// assert!(Var::try_new("DATABASE_URL", Group::User, VarKind::Plain).is_ok());
    /// assert!(Var::try_new("1BAD", Group::User, VarKind::Plain).is_err());
    /// ```
    pub fn try_new(
        name: impl Into<String>,
        group: Group,
        kind: VarKind,
    ) -> Result<Self, MetadataError> {
        let name = name.into();
        Self::validate_name(&name)?;
        Ok(Self::new(name, group, kind))
    }

    /// Reconstruct a [`Var`] from already-stored fields **without re-validating**.
    ///
    /// This bypasses [`Self::validate_name`]. Reach for it only from a code path
    /// where the data has demonstrably been validated upstream — e.g. tests or
    /// in-process serialization. Backends that rehydrate from external storage
    /// (`SQLite`, files, …) must call [`Self::try_from_parts`] instead so that a
    /// corrupted or tampered row cannot inject malformed names into the rest of
    /// the system.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn from_trusted_parts(
        id: VarId,
        name: String,
        group: Group,
        kind: VarKind,
        tags: Vec<String>,
        length: usize,
        created_at: OffsetDateTime,
        updated_at: OffsetDateTime,
    ) -> Self {
        Self {
            id,
            name,
            group,
            kind,
            tags,
            length,
            created_at,
            updated_at,
        }
    }

    /// Reconstruct a [`Var`] from already-stored fields, re-validating the
    /// name through [`Self::validate_name`].
    ///
    /// This is the entry point that storage backends (`SQLCipher`,
    /// in-memory, …) should use when rehydrating rows: it ensures a tampered
    /// or corrupted database cannot smuggle in a malformed variable name.
    ///
    /// # Errors
    /// Returns [`MetadataError::Invalid`] if `name` does not satisfy
    /// [`Self::validate_name`].
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_parts(
        id: VarId,
        name: String,
        group: Group,
        kind: VarKind,
        tags: Vec<String>,
        length: usize,
        created_at: OffsetDateTime,
        updated_at: OffsetDateTime,
    ) -> Result<Self, MetadataError> {
        Self::validate_name(&name)?;
        Ok(Self::from_trusted_parts(
            id, name, group, kind, tags, length, created_at, updated_at,
        ))
    }

    /// Validate that `candidate` is acceptable as a variable name.
    ///
    /// Accepted names follow the conventional environment-variable shape:
    /// - non-empty
    /// - 64 characters or fewer
    /// - first character is an ASCII letter or underscore
    /// - subsequent characters are ASCII alphanumerics or underscores
    ///
    /// # Errors
    /// Returns [`MetadataError::Invalid`] if any rule is violated.
    ///
    /// # Examples
    /// ```
    /// use evault_core::model::Var;
    /// assert!(Var::validate_name("DATABASE_URL").is_ok());
    /// assert!(Var::validate_name("").is_err());
    /// assert!(Var::validate_name("1BAD").is_err());
    /// ```
    pub fn validate_name(candidate: &str) -> Result<&str, MetadataError> {
        if candidate.is_empty() {
            return Err(MetadataError::Invalid("name is empty".into()));
        }
        if candidate.len() > 64 {
            return Err(MetadataError::Invalid(
                "name is longer than 64 characters".into(),
            ));
        }
        let bytes = candidate.as_bytes();
        // `is_empty` was already checked, so `bytes[0]` is safe to read; the
        // structural alternative would re-check via `bytes.first()`.
        let first = bytes[0];
        if !(first.is_ascii_alphabetic() || first == b'_') {
            return Err(MetadataError::Invalid(
                "name must start with an ASCII letter or underscore".into(),
            ));
        }
        for (offset, &b) in bytes.iter().enumerate().skip(1) {
            if !(b.is_ascii_alphanumeric() || b == b'_') {
                // Deliberately do NOT echo the offending byte: in CLI/TUI
                // surfaces a user might paste a secret value into the name
                // field, and a single byte from the input is enough to narrow
                // entropy in logs. The byte offset is enough for debuggability.
                return Err(MetadataError::Invalid(format!(
                    "name contains an invalid character at byte offset {offset}"
                )));
            }
        }
        Ok(candidate)
    }

    /// Returns the variable's stable identifier.
    #[must_use]
    pub const fn id(&self) -> VarId {
        self.id
    }

    /// Returns the variable's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the variable's group.
    #[must_use]
    pub const fn group(&self) -> &Group {
        &self.group
    }

    /// Returns the variable's storage kind.
    #[must_use]
    pub const fn kind(&self) -> VarKind {
        self.kind
    }

    /// Returns the tag list.
    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    /// Replaces the tag list.
    ///
    /// Tags are not deduplicated nor sorted; callers should apply their own
    /// normalization where it matters.
    pub fn set_tags(&mut self, tags: Vec<String>) {
        self.tags = tags;
        self.touch();
    }

    /// Returns the length of the value (without revealing it).
    ///
    /// The length is captured at write-time by the registry and is intended
    /// for display only.
    #[must_use]
    pub const fn length(&self) -> usize {
        self.length
    }

    /// Sets the recorded value length and bumps `updated_at`.
    pub fn set_length(&mut self, length: usize) {
        self.length = length;
        self.touch();
    }

    /// Returns when the variable was created (UTC).
    #[must_use]
    pub const fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    /// Returns when the variable was last modified (UTC).
    #[must_use]
    pub const fn updated_at(&self) -> OffsetDateTime {
        self.updated_at
    }

    /// Marks the record as modified by bumping `updated_at` to "now".
    fn touch(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
    }
}

/// Filter applied when listing variables from a
/// [`MetadataStore`](crate::traits::MetadataStore).
///
/// Fields are additive: every supplied filter must match. A filter with all
/// fields `None` matches every variable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VarFilter {
    /// Match only variables in this group (canonical name).
    pub group: Option<Group>,
    /// Match only variables whose name contains this substring (case-insensitive).
    pub name_contains: Option<String>,
    /// Match only variables with this storage kind.
    pub kind: Option<VarKind>,
    /// Match only variables that contain all of these tags.
    pub tags_all: Vec<String>,
}

impl VarFilter {
    /// Construct an empty filter (matches everything).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if the supplied [`Var`] satisfies every active criterion.
    #[must_use]
    pub fn matches(&self, var: &Var) -> bool {
        if let Some(group) = &self.group {
            if var.group() != group {
                return false;
            }
        }
        if let Some(needle) = &self.name_contains {
            let hay = var.name().to_ascii_lowercase();
            let needle = needle.to_ascii_lowercase();
            if !hay.contains(&needle) {
                return false;
            }
        }
        if let Some(kind) = self.kind {
            if var.kind() != kind {
                return false;
            }
        }
        for tag in &self.tags_all {
            if !var.tags().iter().any(|t| t == tag) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_var_has_fresh_id_and_equal_timestamps() {
        let v = Var::new("FOO", Group::User, VarKind::Plain);
        assert_eq!(v.name(), "FOO");
        assert_eq!(v.created_at(), v.updated_at());
    }

    #[test]
    fn touch_bumps_updated_at() {
        let mut v = Var::new("FOO", Group::User, VarKind::Plain);
        let original = v.updated_at();
        std::thread::sleep(std::time::Duration::from_millis(2));
        v.set_length(5);
        assert!(v.updated_at() > original);
    }

    #[test]
    fn validate_name_accepts_typical_names() {
        for name in ["FOO", "_FOO", "DB_URL_2", "x"] {
            assert!(Var::validate_name(name).is_ok(), "expected {name} valid");
        }
    }

    #[test]
    fn validate_name_rejects_invalid_inputs() {
        let bad = ["", "1FOO", "FOO-BAR", "FOO BAR", &"A".repeat(65)];
        for name in bad {
            assert!(
                Var::validate_name(name).is_err(),
                "expected {name:?} invalid"
            );
        }
    }

    #[test]
    fn try_from_parts_validates_name() {
        let now = time::OffsetDateTime::now_utc();
        let bad = Var::try_from_parts(
            VarId::new_v4(),
            "1BAD".to_owned(),
            Group::User,
            VarKind::Plain,
            Vec::new(),
            0,
            now,
            now,
        );
        assert!(bad.is_err(), "try_from_parts should reject invalid names");
    }

    #[test]
    fn try_from_parts_round_trips_a_valid_var() {
        let now = time::OffsetDateTime::now_utc();
        let id = VarId::new_v4();
        let var = Var::try_from_parts(
            id,
            "DATABASE_URL".to_owned(),
            Group::Project,
            VarKind::Secret,
            vec!["db".into()],
            12,
            now,
            now,
        )
        .expect("valid var");
        assert_eq!(var.id(), id);
        assert_eq!(var.name(), "DATABASE_URL");
        assert_eq!(var.tags(), &["db".to_owned()]);
        assert_eq!(var.length(), 12);
    }

    #[test]
    fn var_filter_default_matches_everything() {
        let f = VarFilter::new();
        let v = Var::new("FOO", Group::Project, VarKind::Secret);
        assert!(f.matches(&v));
    }

    #[test]
    fn var_filter_combines_criteria() {
        let v = {
            let mut v = Var::new("DATABASE_URL", Group::Project, VarKind::Secret);
            v.set_tags(vec!["db".into(), "prod".into()]);
            v
        };

        let f = VarFilter {
            group: Some(Group::Project),
            name_contains: Some("data".into()),
            kind: Some(VarKind::Secret),
            tags_all: vec!["db".into()],
        };
        assert!(f.matches(&v));

        let f = VarFilter {
            kind: Some(VarKind::Plain),
            ..VarFilter::default()
        };
        assert!(!f.matches(&v));
    }

    #[test]
    fn group_as_str_uses_canonical_names() {
        assert_eq!(Group::User.as_str(), "user");
        assert_eq!(Group::System.as_str(), "system");
        assert_eq!(Group::Project.as_str(), "project");
        assert_eq!(Group::Custom("aws".into()).as_str(), "aws");
    }

    #[test]
    fn var_id_display_matches_uuid() {
        let id = VarId::new_v4();
        assert_eq!(id.to_string(), id.as_uuid().to_string());
    }
}
