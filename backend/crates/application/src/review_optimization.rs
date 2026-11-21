//! Personal scheduling weights (ADR 0014 points 5, 6, 11 and 12): once a day
//! it looks for users with enough votes, trains candidate weights on the older
//! 80% of their votes and adopts them only if they predict the newer 20%
//! better than the weights in use (those validated weights are the ones
//! adopted: the votes kept out are not trained on until the next attempt).
//!
//! Every card carries the stamp of the weights its memory was computed with.
//! The same pass, for every user with saved weights whether or not they can be
//! optimized, rebuilds the cards whose stamp is not the weights in use: that
//! finishes a rebuild that was interrupted or failed for some cards.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use domain::{
    CardVotes, Clock, OptimizationAttempt, OptimizationOutcome, OptimizerError, ParameterOptimizer,
    ReviewCardId, ReviewOptimizationRepository, ReviewRepository, ReviewSample, ReviewScheduler,
    SchedulerParameters, UserId,
};
use time::{Duration, OffsetDateTime};

use crate::error::AppError;
use crate::reviews::weights_in_use;
use crate::time_util::now_millis;

/// Useful votes (preceded by another vote of the same card) a user needs
/// before the first attempt.
pub const MIN_USEFUL_VOTES: usize = 300;
/// Least time between two attempts of a user after one that was adopted or
/// rejected.
pub const MIN_TIME_BETWEEN_ATTEMPTS: Duration = Duration::days(30);
/// Least time after an attempt that failed with an error: sooner, since it
/// told nothing about the votes.
pub const MIN_TIME_AFTER_FAILURE: Duration = Duration::days(7);
/// The newest 1/`TEST_SHARE` of the useful votes (rounded down) is kept out of
/// the training and used to compare the weights: 1/5 is 20%.
pub const TEST_SHARE: usize = 5;
/// How long after the start the first pass runs: not in the way of the start.
pub const FIRST_RUN_DELAY: StdDuration = StdDuration::from_secs(120);
/// Time between two passes.
pub const RUN_INTERVAL: StdDuration = StdDuration::from_hours(24);

/// When the background task runs. The production values are the constants above.
#[derive(Debug, Clone, Copy)]
pub struct OptimizationSchedule {
    pub first_run_delay: StdDuration,
    pub interval: StdDuration,
}

impl Default for OptimizationSchedule {
    fn default() -> Self {
        Self {
            first_run_delay: FIRST_RUN_DELAY,
            interval: RUN_INTERVAL,
        }
    }
}

/// What a pass did for one user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserRun {
    /// Too few votes, too soon, or nothing new since the last attempt.
    NotEligible,
    /// An attempt was made and recorded.
    Attempted(OptimizationOutcome),
    /// The optimizer was stopped (shutdown): nothing was recorded, and the
    /// next pass tries again.
    Interrupted,
}

/// A useful vote: the card's history up to it, and when it was stored.
struct Sample {
    sample: ReviewSample,
    recorded_at: OffsetDateTime,
}

/// Every vote that follows another vote of the same card, with the history up
/// to it, ordered by the time the vote was given. The order inside a card is
/// kept (stable sort), and cards come in the repository's order, so the same
/// data always gives the same list.
fn useful_samples(cards: &[CardVotes]) -> Vec<Sample> {
    let mut samples: Vec<Sample> = cards
        .iter()
        .flat_map(|card| {
            (1..card.votes.len()).map(|k| Sample {
                sample: ReviewSample {
                    card: card.card,
                    votes: card.votes[..=k].iter().map(|v| v.event).collect(),
                },
                recorded_at: card.votes[k].recorded_at,
            })
        })
        .collect();
    samples.sort_by_key(|s| s.sample.votes.last().map(|e| e.at));
    samples
}

/// What rebuilding a user's cards did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RebuildReport {
    pub rebuilt: u32,
    /// Cards that got a vote meanwhile, or have none.
    pub skipped: u32,
    pub failed: u32,
}

pub struct OptimizeReviewParameters {
    reviews: Arc<dyn ReviewRepository>,
    optimization: Arc<dyn ReviewOptimizationRepository>,
    scheduler: Arc<dyn ReviewScheduler>,
    optimizer: Arc<dyn ParameterOptimizer>,
    clock: Arc<dyn Clock>,
    schedule: OptimizationSchedule,
}

impl OptimizeReviewParameters {
    #[must_use]
    pub fn new(
        reviews: Arc<dyn ReviewRepository>,
        optimization: Arc<dyn ReviewOptimizationRepository>,
        scheduler: Arc<dyn ReviewScheduler>,
        optimizer: Arc<dyn ParameterOptimizer>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            reviews,
            optimization,
            scheduler,
            optimizer,
            clock,
            schedule: OptimizationSchedule::default(),
        }
    }

    #[must_use]
    pub const fn with_schedule(mut self, schedule: OptimizationSchedule) -> Self {
        self.schedule = schedule;
        self
    }

    /// The background task: a first pass shortly after the start, then one
    /// every interval, until it is aborted at shutdown.
    pub async fn run(self: Arc<Self>) {
        tokio::time::sleep(self.schedule.first_run_delay).await;
        loop {
            if let Err(error) = self.run_once().await {
                tracing::error!(%error, "could not list the users to optimize review weights for");
            }
            tokio::time::sleep(self.schedule.interval).await;
        }
    }

    /// One pass over every user, one at a time. A user who fails does not stop
    /// the others: the error is logged and returned next to their id.
    pub async fn run_once(&self) -> Result<Vec<(UserId, Result<UserRun, AppError>)>, AppError> {
        let users = self.optimization.users_with_review_data().await?;
        let mut results = Vec::with_capacity(users.len());
        for user in users {
            let result = self.process_user(user).await;
            if let Err(error) = &result {
                tracing::error!(%user, %error, "review weights optimization failed for the user");
            }
            results.push((user, result));
        }
        Ok(results)
    }

    /// Everything a pass does for one user: the attempt at new weights, if
    /// they are due for one, and then the rebuild of the cards whose memory is
    /// not from the weights in use. The rebuild runs whatever the attempt
    /// came to, even if the user is not eligible or the attempt failed.
    async fn process_user(&self, user: UserId) -> Result<UserRun, AppError> {
        let run = self.optimize_user(user).await;
        match self.rebuild_out_of_date(user).await {
            Ok(Some(report)) if report.rebuilt + report.skipped + report.failed > 0 => {
                tracing::info!(
                    %user,
                    rebuilt = report.rebuilt,
                    skipped = report.skipped,
                    failed = report.failed,
                    "rebuilt the memory of the review cards with the weights in use"
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::error!(%user, %error, "could not rebuild the review cards of the user");
                // The attempt's own result is the one reported.
                if run.is_ok() {
                    return Err(error);
                }
            }
        }
        run
    }

    async fn optimize_user(&self, user: UserId) -> Result<UserRun, AppError> {
        let cards = self.optimization.card_votes(user).await?;
        let samples = useful_samples(&cards);
        if samples.len() < MIN_USEFUL_VOTES {
            return Ok(UserRun::NotEligible);
        }
        let now = now_millis(&*self.clock);
        if let Some(last) = self.optimization.last_attempt(user).await? {
            let wait = if last.outcome == OptimizationOutcome::Failed {
                MIN_TIME_AFTER_FAILURE
            } else {
                MIN_TIME_BETWEEN_ATTEMPTS
            };
            let new_votes = samples.iter().any(|s| s.recorded_at > last.at);
            if now - last.at < wait || !new_votes {
                return Ok(UserRun::NotEligible);
            }
        }
        let samples: Vec<ReviewSample> = samples.into_iter().map(|s| s.sample).collect();
        let current = weights_in_use(&*self.reviews, &*self.scheduler, user)
            .await?
            .parameters;
        let test_votes = samples.len() / TEST_SHARE;
        let train_votes = samples.len() - test_votes;
        let mut attempt = OptimizationAttempt {
            at: now,
            useful_votes: count(samples.len()),
            train_votes: count(train_votes),
            test_votes: count(test_votes),
            current_log_loss: None,
            candidate_log_loss: None,
            outcome: OptimizationOutcome::Failed,
            error: None,
        };
        let adopted = match self
            .compare(&mut attempt, &current, &samples, train_votes)
            .await
        {
            Ok(adopted) => adopted,
            Err(OptimizerError::Interrupted) => return Ok(UserRun::Interrupted),
            Err(error) => {
                attempt.outcome = OptimizationOutcome::Failed;
                attempt.error = Some(error.to_string());
                None
            }
        };
        // The weights are stored with the attempt, before any card is touched.
        self.optimization
            .record_attempt(user, &attempt, adopted.as_ref())
            .await?;
        log_attempt(user, &attempt);
        // The cards of the adopted weights are rebuilt by the caller, like any
        // other card that is out of date.
        Ok(UserRun::Attempted(attempt.outcome))
    }

    /// Trains on the older votes and compares on the newer ones. The
    /// candidate, if strictly better, is what gets adopted: it is the very
    /// weights that were measured. Fills the attempt as it goes, so a failure
    /// keeps what was measured.
    async fn compare(
        &self,
        attempt: &mut OptimizationAttempt,
        current: &SchedulerParameters,
        samples: &[ReviewSample],
        train_votes: usize,
    ) -> Result<Option<SchedulerParameters>, OptimizerError> {
        let (train, test) = samples.split_at(train_votes);
        let candidate = self.optimizer.train(train).await?;
        let current_loss = self.optimizer.evaluate(current, test).await?;
        attempt.current_log_loss = Some(current_loss);
        let candidate_loss = self.optimizer.evaluate(&candidate, test).await?;
        attempt.candidate_log_loss = Some(candidate_loss);
        if candidate_loss >= current_loss {
            attempt.outcome = OptimizationOutcome::Rejected;
            return Ok(None);
        }
        attempt.outcome = OptimizationOutcome::Adopted;
        Ok(Some(candidate))
    }

    /// Replays every card of the user that is not on the weights in use and
    /// stores the memory with their stamp. Due dates, states and counts stay.
    /// A card that got a vote since it was read is left for that vote, which
    /// has already used the weights in use. `None` when the user has no saved
    /// weights (their cards are on the defaults, which the migration set).
    async fn rebuild_out_of_date(&self, user: UserId) -> Result<Option<RebuildReport>, AppError> {
        let Some(weights) = self.reviews.parameters(user).await? else {
            return Ok(None);
        };
        if let Err(error) = self.scheduler.check_parameters(&weights.parameters) {
            // The weights in use are the defaults (see `weights_in_use`):
            // nothing to rebuild with.
            tracing::warn!(%user, %error, "the stored review weights are not usable");
            return Ok(None);
        }
        let mut report = RebuildReport::default();
        for card in self
            .optimization
            .cards_out_of_date(user, weights.adopted_at)
            .await?
        {
            match self.rebuild_card(card, &weights).await {
                Ok(true) => report.rebuilt += 1,
                Ok(false) => report.skipped += 1,
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(%user, %card, %error, "could not rebuild the review card");
                }
            }
        }
        Ok(Some(report))
    }

    async fn rebuild_card(
        &self,
        card: ReviewCardId,
        weights: &domain::UserWeights,
    ) -> Result<bool, AppError> {
        let (review_count, history) = self.optimization.card_snapshot(card).await?;
        let memory = self.scheduler.replay(&weights.parameters, &history)?;
        Ok(self
            .optimization
            .rebuild_memory(card, review_count, memory, weights.adopted_at)
            .await?)
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn log_attempt(user: UserId, a: &OptimizationAttempt) {
    if a.outcome == OptimizationOutcome::Failed {
        tracing::error!(
            %user,
            useful_votes = a.useful_votes,
            error = a.error.as_deref().unwrap_or(""),
            "review weights optimization failed"
        );
    } else {
        tracing::info!(
            %user,
            outcome = %a.outcome,
            useful_votes = a.useful_votes,
            train_votes = a.train_votes,
            test_votes = a.test_votes,
            current_log_loss = a.current_log_loss,
            candidate_log_loss = a.candidate_log_loss,
            "review weights optimization"
        );
    }
}
