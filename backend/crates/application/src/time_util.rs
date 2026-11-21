use domain::Clock;
use time::OffsetDateTime;

/// Current time truncated to milliseconds: the precision of the API and of the
/// stored value, so what a use case returns equals what is read back later.
pub(crate) fn now_millis(clock: &dyn Clock) -> OffsetDateTime {
    let now = clock.now();
    let nanos = now.nanosecond() / 1_000_000 * 1_000_000;
    // Invariant: `nanos` is a multiple of 1e6 below 1e9, always valid.
    now.replace_nanosecond(nanos).unwrap_or(now)
}
