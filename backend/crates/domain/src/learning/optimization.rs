//! Data of the personal-weights optimization (ADR 0014 points 5, 6, 11).
//! The use case lives in `application`; the training itself behind the
//! `ParameterOptimizer` port.

use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;

use super::UnknownLearningValue;
use super::review::ReviewEvent;
use crate::ids::ReviewCardId;

/// A vote that was not undone, with the two instants it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoteRecord {
    /// The vote and when the learner gave it (the app's clock).
    pub event: ReviewEvent,
    /// When the backend stored it: unlike `event.at` it cannot be backdated,
    /// so "new votes since" is measured on it.
    pub recorded_at: OffsetDateTime,
}

/// The votes of one card that were not undone, in the order they were given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardVotes {
    pub card: ReviewCardId,
    pub votes: Vec<VoteRecord>,
}

/// One vote to predict, with everything the card's history had before it:
/// `votes` is the history up to and including the vote, so it has at least two
/// events and the last one is the target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSample {
    /// The card the votes belong to: samples of one card are prefixes of the
    /// same history, which the optimizer can exploit.
    pub card: ReviewCardId,
    pub votes: Vec<ReviewEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationOutcome {
    /// The trained weights predicted better and became the user's.
    Adopted,
    /// The weights in use predicted as well or better; they stay.
    Rejected,
    /// The attempt broke before a decision.
    Failed,
}

impl OptimizationOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Adopted => "adopted",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for OptimizationOutcome {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "adopted" => Ok(Self::Adopted),
            "rejected" => Ok(Self::Rejected),
            "failed" => Ok(Self::Failed),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

impl fmt::Display for OptimizationOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One try at personal weights, whatever came of it.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizationAttempt {
    pub at: OffsetDateTime,
    /// Votes preceded by another vote of the same card.
    pub useful_votes: u32,
    pub train_votes: u32,
    pub test_votes: u32,
    /// Log loss of the weights in use on the test votes.
    pub current_log_loss: Option<f64>,
    /// Log loss of the candidate weights on the test votes.
    pub candidate_log_loss: Option<f64>,
    pub outcome: OptimizationOutcome,
    pub error: Option<String>,
}
