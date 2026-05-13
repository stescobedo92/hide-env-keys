//! [`MemoryAuditSink`] — in-memory implementation of
//! [`evault_core::traits::AuditSink`].

use std::sync::RwLock;

use evault_core::error::MetadataError;
use evault_core::model::AuditEntry;
use evault_core::traits::AuditSink;

/// Append-only in-memory audit log.
///
/// Insertion order is preserved internally; [`list`](AuditSink::list) returns
/// entries newest-first as required by the trait contract.
#[derive(Debug, Default)]
pub struct MemoryAuditSink {
    inner: RwLock<Vec<AuditEntry>>,
}

impl MemoryAuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl AuditSink for MemoryAuditSink {
    fn append(&self, entry: &AuditEntry) -> Result<(), MetadataError> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| MetadataError::Backend("in-memory audit lock poisoned".into()))?;
        guard.push(entry.clone());
        drop(guard);
        Ok(())
    }

    fn list(&self, limit: usize) -> Result<Vec<AuditEntry>, MetadataError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let guard = self
            .inner
            .read()
            .map_err(|_| MetadataError::Backend("in-memory audit lock poisoned".into()))?;
        let count = guard.len().min(limit);
        // Newest first: iterate from the end.
        let collected: Vec<AuditEntry> = guard.iter().rev().take(count).cloned().collect();
        drop(guard);
        Ok(collected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evault_core::model::{AuditAction, VarId};

    #[test]
    fn append_then_list_returns_newest_first() {
        let sink = MemoryAuditSink::new();
        let v = VarId::new_v4();
        sink.append(&AuditEntry::for_var(v, AuditAction::Created))
            .unwrap();
        sink.append(&AuditEntry::for_var(v, AuditAction::Updated))
            .unwrap();
        let listed = sink.list(10).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].action(), &AuditAction::Updated);
        assert_eq!(listed[1].action(), &AuditAction::Created);
    }

    #[test]
    fn list_with_limit_zero_returns_empty() {
        let sink = MemoryAuditSink::new();
        sink.append(&AuditEntry::for_var(VarId::new_v4(), AuditAction::Created))
            .unwrap();
        assert!(sink.list(0).unwrap().is_empty());
    }

    #[test]
    fn list_limits_to_n_entries() {
        let sink = MemoryAuditSink::new();
        for _ in 0..5 {
            sink.append(&AuditEntry::for_var(VarId::new_v4(), AuditAction::Updated))
                .unwrap();
        }
        let listed = sink.list(3).unwrap();
        assert_eq!(listed.len(), 3);
    }

    #[test]
    fn list_limits_return_newest_first_combined_with_count() {
        let sink = MemoryAuditSink::new();
        // Five distinguishable appends; the limit slice must come from the
        // tail of the insertion order and be reversed.
        let actions = [
            AuditAction::Created,
            AuditAction::Updated,
            AuditAction::Linked,
            AuditAction::Materialized,
            AuditAction::Deleted,
        ];
        for action in &actions {
            sink.append(&AuditEntry::for_var(VarId::new_v4(), action.clone()))
                .unwrap();
        }
        let listed = sink.list(3).unwrap();
        // Newest first: the three most-recent are the last three appended,
        // in reverse insertion order.
        let got: Vec<&AuditAction> = listed.iter().map(AuditEntry::action).collect();
        assert_eq!(
            got,
            vec![
                &AuditAction::Deleted,
                &AuditAction::Materialized,
                &AuditAction::Linked,
            ]
        );
    }
}
