//! Spaced-repetition state of a review card (`docs/contracts/reviews.md`).
//!
//! The algorithm (FSRS) lives behind the `ReviewScheduler` port; this module
//! holds only the data it reads and writes, so no type of the scheduling
//! crate reaches the rest of the system.

use std::fmt;
use std::str::FromStr;

use time::{Duration, OffsetDateTime};

use super::UnknownLearningValue;
use super::attempt::Rating;
use super::record::{CardOrigin, ReviewCard};
use crate::ids::{ReviewCardId, ReviewLogId, TopicId};

/// A new card is due this long after its creation: reviewing right after the
/// lesson that made it would test short-term memory, not learning.
pub const FIRST_REVIEW_DELAY: Duration = Duration::hours(24);

/// How long after a vote the learner may still undo it.
pub const UNDO_WINDOW: Duration = Duration::minutes(10);

/// Seconds the estimate assumes for one card.
pub const SECONDS_PER_CARD: u32 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewState {
    New,
    Learning,
    Review,
    Relearning,
}

impl ReviewState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Learning => "learning",
            Self::Review => "review",
            Self::Relearning => "relearning",
        }
    }
}

impl FromStr for ReviewState {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "new" => Ok(Self::New),
            "learning" => Ok(Self::Learning),
            "review" => Ok(Self::Review),
            "relearning" => Ok(Self::Relearning),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

/// Everything the scheduler needs to know about a card, and what it decides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardSchedule {
    pub state: ReviewState,
    /// FSRS memory stability, in days. 0 for a card never reviewed.
    pub stability: f64,
    /// FSRS difficulty, 1 to 10. 0 for a card never reviewed.
    pub difficulty: f64,
    /// The fast trace of the two-trace memory model (FSRS-7), in days. 0 for a
    /// card never reviewed, and for a card still on the previous model.
    pub stability_fast: f64,
    /// Index of the next short step of a `new`, `learning` or `relearning`
    /// card; 0 in every other state.
    pub learning_step: u32,
    pub due: OffsetDateTime,
    pub last_reviewed_at: Option<OffsetDateTime>,
    /// Votes applied, undone ones excluded. Also the card's version for the
    /// idempotent vote.
    pub review_count: u32,
    pub lapses: u32,
}

impl CardSchedule {
    /// The schedule of a card created at `created_at`.
    #[must_use]
    pub fn initial(created_at: OffsetDateTime) -> Self {
        Self {
            state: ReviewState::New,
            stability: 0.0,
            difficulty: 0.0,
            stability_fast: 0.0,
            learning_step: 0,
            due: created_at + FIRST_REVIEW_DELAY,
            last_reviewed_at: None,
            review_count: 0,
            lapses: 0,
        }
    }
}

/// The weights the scheduler runs on. Opaque to the domain and the use cases:
/// the scheduler adapter builds the defaults and reads the contents.
#[derive(Debug, Clone, PartialEq)]
pub struct SchedulerParameters {
    /// Name of the memory model the weights are for (e.g. `fsrs7`).
    pub model: String,
    pub weights: Vec<f64>,
}

/// The memory of a card as the scheduling model keeps it, with nothing else
/// of the schedule: what replaying a history of votes produces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoryValues {
    pub stability: f64,
    pub difficulty: f64,
    pub stability_fast: f64,
}

/// One vote of a card's history, as a replay reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewEvent {
    pub rating: Rating,
    /// When the vote was given (`ReviewLog::reviewed_at`).
    pub at: OffsetDateTime,
}

/// Time from a vote to the next due date, for each of the four votes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RatingIntervals {
    pub again: Duration,
    pub hard: Duration,
    pub good: Duration,
    pub easy: Duration,
}

impl RatingIntervals {
    #[must_use]
    pub const fn get(&self, rating: Rating) -> Duration {
        match rating {
            Rating::Again => self.again,
            Rating::Hard => self.hard,
            Rating::Good => self.good,
            Rating::Easy => self.easy,
        }
    }
}

/// Where a card came from, as the API names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    Record,
    Term,
}

impl CardKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Term => "term",
        }
    }
}

impl fmt::Display for CardKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the memory values of a card were computed with. A card is scheduled
/// from its memory only when this equals what the user's weights call for;
/// otherwise its votes are replayed first (ADR 0014 points 9 and 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryBasis {
    /// The values come from the current memory model (not the previous one).
    pub current_model: bool,
    /// When the user's weights used for them were adopted; `None` for the
    /// model's default weights.
    pub weights: Option<OffsetDateTime>,
}

impl MemoryBasis {
    /// The basis of a card computed by the current model with the weights
    /// adopted at `weights`.
    #[must_use]
    pub const fn current(weights: Option<OffsetDateTime>) -> Self {
        Self {
            current_model: true,
            weights,
        }
    }
}

/// The weights saved for a user, with the instant they were adopted: the
/// stamp that every card computed with them carries.
#[derive(Debug, Clone, PartialEq)]
pub struct UserWeights {
    pub parameters: SchedulerParameters,
    pub adopted_at: OffsetDateTime,
}

/// A card with the names the review screen shows next to it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewItem {
    pub card: ReviewCard,
    pub basis: MemoryBasis,
    pub topic_title: String,
    pub lesson_title: String,
    pub lesson_number: u32,
}

impl ReviewItem {
    #[must_use]
    pub const fn kind(&self) -> CardKind {
        match self.card.origin {
            CardOrigin::Record(_) => CardKind::Record,
            CardOrigin::Term(_) => CardKind::Term,
        }
    }
}

/// Whether a card is "to consolidate" (`reviews.md` §1.3): born from an
/// exercise the learner never passed and not yet promoted. `ratings` are the
/// card's votes that were not undone, in the order they were applied.
/// Promotion is two consecutive `good`/`easy`; later worse votes do not undo
/// it, while undoing one of the two votes removes it from `ratings`.
#[must_use]
pub fn is_to_consolidate(born_unpassed: bool, ratings: &[Rating]) -> bool {
    let passing = |r: &Rating| matches!(r, Rating::Good | Rating::Easy);
    born_unpassed && !ratings.windows(2).any(|w| w.iter().all(passing))
}

/// One applied vote, with the states needed to undo it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewLog {
    pub id: ReviewLogId,
    pub card_id: ReviewCardId,
    pub rating: Rating,
    pub reviewed_at: OffsetDateTime,
    pub before: CardSchedule,
    pub after: CardSchedule,
    /// When the backend applied the vote (the undo window counts from here).
    pub created_at: OffsetDateTime,
    pub undone_at: Option<OffsetDateTime>,
}

/// Keyset position of the due list: (`due`, `id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueCursor {
    pub due: OffsetDateTime,
    pub id: ReviewCardId,
}

/// Keyset position of the list of suspended cards: (`suspended_at` descending, `id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuspendedCursor {
    pub suspended_at: OffsetDateTime,
    pub id: ReviewCardId,
}

/// A topic with the number of its cards due now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueTopic {
    pub topic_id: TopicId,
    pub topic_title: String,
    pub due_count: u32,
}

/// Totals of the cards that can be reviewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueStats {
    /// Active cards due now.
    pub due_count: u32,
    /// The first due date after now among active cards.
    pub next_due_at: Option<OffsetDateTime>,
}

/// Minutes the review of `due_count` cards takes, rounded up.
#[must_use]
pub const fn estimated_minutes(due_count: u32) -> u32 {
    (due_count * SECONDS_PER_CARD).div_ceil(60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn consolidation_ends_after_two_consecutive_passing_votes() {
        use Rating::{Again, Easy, Good, Hard};
        assert!(is_to_consolidate(true, &[]));
        assert!(is_to_consolidate(true, &[Good]));
        assert!(!is_to_consolidate(true, &[Good, Easy]));
        // A failing vote breaks the series.
        assert!(is_to_consolidate(true, &[Good, Again, Good]));
        assert!(is_to_consolidate(true, &[Easy, Hard, Easy]));
        assert!(!is_to_consolidate(true, &[Good, Again, Good, Good]));
    }

    #[test]
    fn promotion_stays_after_a_worse_vote() {
        use Rating::{Again, Good};
        assert!(!is_to_consolidate(true, &[Good, Good, Again]));
    }

    #[test]
    fn undoing_a_vote_brings_the_card_back() {
        use Rating::Good;
        // The undone vote is no longer in the list.
        assert!(!is_to_consolidate(true, &[Good, Good]));
        assert!(is_to_consolidate(true, &[Good]));
    }

    #[test]
    fn a_card_not_born_unpassed_is_never_to_consolidate() {
        assert!(!is_to_consolidate(false, &[]));
        assert!(!is_to_consolidate(false, &[Rating::Again]));
    }

    #[test]
    fn a_new_card_is_due_a_day_after_creation() {
        let at = datetime!(2026-11-09 10:00 UTC);
        let s = CardSchedule::initial(at);
        assert_eq!(s.state, ReviewState::New);
        assert_eq!(s.due, datetime!(2026-11-10 10:00 UTC));
        assert_eq!((s.review_count, s.last_reviewed_at), (0, None));
    }

    #[test]
    fn estimate_rounds_up_to_whole_minutes() {
        assert_eq!(estimated_minutes(0), 0);
        assert_eq!(estimated_minutes(1), 1);
        assert_eq!(estimated_minutes(4), 1);
        assert_eq!(estimated_minutes(5), 2);
        assert_eq!(estimated_minutes(12), 3);
    }

    #[test]
    fn state_names_round_trip() {
        for s in [
            ReviewState::New,
            ReviewState::Learning,
            ReviewState::Review,
            ReviewState::Relearning,
        ] {
            assert_eq!(s.as_str().parse::<ReviewState>().unwrap(), s);
        }
        assert!("x".parse::<ReviewState>().is_err());
    }
}
