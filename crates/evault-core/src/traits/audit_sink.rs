//! [`AuditSink`] — append-only log of state-changing operations.

use crate::error::MetadataError;
use crate::model::AuditEntry;

/// Append-only storage for [`AuditEntry`] records.
///
/// Audit entries are intentionally low-volume (one per user action) so simple
/// SQL or file-based backends are sufficient. Implementations must preserve
/// insertion order and provide newest-first iteration in [`AuditSink::list`].
pub trait AuditSink: Send + Sync {
    /// Append a new entry. Entries are never overwritten or removed.
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failure.
    fn append(&self, entry: &AuditEntry) -> Result<(), MetadataError>;

    /// Return up to `limit` newest entries, most recent first.
    ///
    /// `limit == 0` returns an empty vector.
    ///
    /// # Errors
    /// Returns [`MetadataError::Backend`] on storage failure.
    fn list(&self, limit: usize) -> Result<Vec<AuditEntry>, MetadataError>;
}
