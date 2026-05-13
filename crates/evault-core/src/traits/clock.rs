//! [`Clock`] — abstraction over "current time".
//!
//! Inject a fake clock in tests to assert that timestamps are recorded
//! deterministically without depending on `OffsetDateTime::now_utc()`.

use time::OffsetDateTime;

/// Source of the current wall-clock time, in UTC.
pub trait Clock: Send + Sync {
    /// Return the current UTC instant.
    fn now(&self) -> OffsetDateTime;
}

/// Production [`Clock`] that delegates to [`OffsetDateTime::now_utc`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_recent_time() {
        let before = OffsetDateTime::now_utc();
        let t = SystemClock.now();
        let after = OffsetDateTime::now_utc();
        assert!(t >= before);
        assert!(t <= after);
    }
}
