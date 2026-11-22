//! Caps the SSE streams a user holds open at once
//! (`TEACH_ME_MAX_SSE_PER_USER`, `api-conventions.md` section 12).
//!
//! A stream is only dropped by the client going away, the server closing it or
//! the session ending, so a slot is tied to the stream's lifetime with a drop
//! guard: whichever way the stream ends, the slot comes back.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use domain::UserId;

#[derive(Clone)]
pub struct StreamLimiter {
    max_per_user: usize,
    open: Arc<Mutex<HashMap<UserId, usize>>>,
}

/// One open stream; frees its slot when dropped.
#[must_use = "the slot is held only while the guard lives"]
pub struct StreamSlot {
    user: UserId,
    open: Arc<Mutex<HashMap<UserId, usize>>>,
}

impl StreamLimiter {
    #[must_use]
    pub fn new(max_per_user: usize) -> Self {
        Self {
            max_per_user,
            open: Arc::default(),
        }
    }

    /// A slot for a new stream of `user`, or `None` when they are at the cap.
    pub fn acquire(&self, user: UserId) -> Option<StreamSlot> {
        // A poisoned lock only means another thread panicked while counting;
        // the counts are still usable.
        let mut open = self
            .open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = open.entry(user).or_insert(0);
        if *count >= self.max_per_user {
            return None;
        }
        *count += 1;
        Some(StreamSlot {
            user,
            open: self.open.clone(),
        })
    }
}

impl Drop for StreamSlot {
    fn drop(&mut self) {
        let mut open = self
            .open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(count) = open.get_mut(&self.user) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                open.remove(&self.user);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cap_is_per_user_and_a_dropped_slot_is_reusable() {
        let limiter = StreamLimiter::new(2);
        let other = UserId::new(uuid::Uuid::from_u128(9));
        let a = limiter.acquire(UserId::OWNER).unwrap();
        let _b = limiter.acquire(UserId::OWNER).unwrap();
        assert!(limiter.acquire(UserId::OWNER).is_none());
        assert!(limiter.acquire(other).is_some());
        drop(a);
        assert!(limiter.acquire(UserId::OWNER).is_some());
    }
}
