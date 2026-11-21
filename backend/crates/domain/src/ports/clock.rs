use time::OffsetDateTime;

/// Source of the current time, injected so tests are deterministic.
pub trait Clock: Send + Sync {
    /// Current instant, always in UTC.
    fn now(&self) -> OffsetDateTime;
}
