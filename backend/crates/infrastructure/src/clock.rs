use domain::Clock;
use time::OffsetDateTime;

/// `Clock` backed by the operating system.
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
    fn returns_utc() {
        assert!(SystemClock.now().offset().is_utc());
    }
}
