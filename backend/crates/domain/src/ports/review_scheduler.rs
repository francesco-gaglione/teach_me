use time::OffsetDateTime;

use crate::learning::{
    CardSchedule, MemoryValues, Rating, RatingIntervals, ReviewEvent, SchedulerParameters,
};

/// The scheduler cannot answer for the inputs it was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SchedulerError {
    /// The weights are not usable: wrong model name or wrong count.
    #[error("the scheduler parameters are not valid")]
    InvalidParameters,
    /// The card's memory values or dates produce no finite schedule.
    #[error("the card state cannot be scheduled")]
    InvalidState,
}

/// The spaced-repetition algorithm. Pure: the same inputs give the same
/// outputs, with no randomness (no fuzz), so that the intervals shown before
/// a vote are exactly the ones applied by it.
pub trait ReviewScheduler: Send + Sync {
    /// The weights a user has until some are trained for them.
    fn default_parameters(&self) -> SchedulerParameters;

    /// Whether `parameters` can be scheduled with: right model, right count,
    /// finite values. Lets a caller fall back to the defaults once, instead of
    /// meeting the error on every card.
    fn check_parameters(&self, parameters: &SchedulerParameters) -> Result<(), SchedulerError>;

    /// The schedule after `rating` given at `now`: new state, memory values and
    /// `due`, with `last_reviewed_at` set to `now` and `review_count` + 1.
    fn schedule(
        &self,
        state: &CardSchedule,
        rating: Rating,
        now: OffsetDateTime,
        parameters: &SchedulerParameters,
    ) -> Result<CardSchedule, SchedulerError>;

    /// What each of the four votes would do if given at `now`.
    fn preview(
        &self,
        state: &CardSchedule,
        now: OffsetDateTime,
        parameters: &SchedulerParameters,
    ) -> Result<RatingIntervals, SchedulerError>;

    /// The memory a card ends up with after `history`, a list of votes in
    /// order (the first at elapsed time 0, each next one at the time since
    /// the previous). Equal to what voting them one by one from a new card
    /// gives. `None` for an empty history: a card never voted has no memory.
    /// Reused to rebuild every card of a user after a change of weights.
    fn replay(
        &self,
        parameters: &SchedulerParameters,
        history: &[ReviewEvent],
    ) -> Result<Option<MemoryValues>, SchedulerError>;
}
