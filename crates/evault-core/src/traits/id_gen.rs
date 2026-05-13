//! [`IdGenerator`] — abstraction over UUID/identifier creation.
//!
//! Like [`Clock`](super::Clock), this exists so tests can swap in a
//! deterministic generator and inspect generated ids without consulting
//! globals.

use uuid::Uuid;

/// Source of fresh UUIDs.
pub trait IdGenerator: Send + Sync {
    /// Produce a fresh identifier.
    fn next(&self) -> Uuid;
}

/// Production [`IdGenerator`] that returns version-4 UUIDs.
#[derive(Debug, Default, Clone, Copy)]
pub struct UuidV4IdGenerator;

impl IdGenerator for UuidV4IdGenerator {
    fn next(&self) -> Uuid {
        Uuid::new_v4()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_v4_generator_returns_unique_ids() {
        let g = UuidV4IdGenerator;
        let a = g.next();
        let b = g.next();
        assert_ne!(a, b);
    }
}
