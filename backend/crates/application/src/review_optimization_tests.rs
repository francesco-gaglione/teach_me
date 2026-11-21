//! The personal-weights optimization over in-memory fakes. The real trainer is
//! exercised in `infrastructure`, the real tables in `api/tests`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use domain::{
    CardSchedule, CardVotes, DueCursor, DueStats, MemoryValues, OptimizationAttempt,
    OptimizationOutcome, OptimizerError, ParameterOptimizer, Rating, RatingIntervals,
    RepositoryError, ReviewCardId, ReviewEvent, ReviewItem, ReviewLog, ReviewLogId,
    ReviewOptimizationRepository, ReviewRepository, ReviewSample, ReviewScheduler, SchedulerError,
    SchedulerParameters, UserId, UserWeights, VoteRecord,
};
use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::fakes::TestClock;
use crate::review_optimization::{
    MIN_TIME_AFTER_FAILURE, MIN_USEFUL_VOTES, OptimizeReviewParameters, UserRun,
};

const T0: OffsetDateTime = datetime!(2026-06-01 09:00 UTC);
/// When the weights a test gives a user were adopted.
const OLD_STAMP: OffsetDateTime = datetime!(2026-03-01 09:00 UTC);

type SharedWeights = Arc<Mutex<HashMap<UserId, UserWeights>>>;

fn user(n: u128) -> UserId {
    UserId::new(Uuid::from_u128(n))
}

fn card(n: u128) -> ReviewCardId {
    ReviewCardId::new(Uuid::from_u128(n))
}

fn params(model: &str, first: f64) -> SchedulerParameters {
    SchedulerParameters {
        model: model.into(),
        weights: vec![first],
    }
}

/// `n` votes of one card, a minute apart from `start`, all stored at `stored`.
fn votes(start: OffsetDateTime, n: i64, stored: OffsetDateTime) -> Vec<VoteRecord> {
    (0..n)
        .map(|k| VoteRecord {
            event: ReviewEvent {
                rating: Rating::Good,
                at: start + Duration::minutes(k),
            },
            recorded_at: stored,
        })
        .collect()
}

/// Cards that make exactly `useful` useful votes: 11 votes each (10 useful),
/// then one card for the remainder. Card 1 is the newest in time, so that the
/// order of the repository is not the order of the votes.
fn history(useful: i64, stored: OffsetDateTime) -> Vec<CardVotes> {
    let mut cards = Vec::new();
    let mut left = useful;
    let mut n = 1_u128;
    while left > 0 {
        let take = left.min(10);
        let day = 100 - i64::try_from(n).unwrap();
        cards.push(CardVotes {
            card: card(n),
            votes: votes(T0 - Duration::days(day), take + 1, stored),
        });
        left -= take;
        n += 1;
    }
    cards
}

struct FakeScheduler;

impl ReviewScheduler for FakeScheduler {
    fn default_parameters(&self) -> SchedulerParameters {
        params("default", 0.0)
    }
    fn check_parameters(&self, p: &SchedulerParameters) -> Result<(), SchedulerError> {
        if p.model == "fake" || p.model == "default" {
            Ok(())
        } else {
            Err(SchedulerError::InvalidParameters)
        }
    }
    fn schedule(
        &self,
        _: &CardSchedule,
        _: Rating,
        _: OffsetDateTime,
        _: &SchedulerParameters,
    ) -> Result<CardSchedule, SchedulerError> {
        Err(SchedulerError::InvalidState)
    }
    fn preview(
        &self,
        _: &CardSchedule,
        _: OffsetDateTime,
        _: &SchedulerParameters,
    ) -> Result<RatingIntervals, SchedulerError> {
        Err(SchedulerError::InvalidState)
    }
    /// Stability: the first weight plus the number of votes.
    fn replay(
        &self,
        p: &SchedulerParameters,
        history: &[ReviewEvent],
    ) -> Result<Option<MemoryValues>, SchedulerError> {
        #[allow(clippy::cast_precision_loss)]
        let n = history.len() as f64;
        Ok((!history.is_empty()).then(|| MemoryValues {
            stability: p.weights[0] + n,
            difficulty: 5.0,
            stability_fast: 1.0,
        }))
    }
}

/// Only the personal weights matter to the use case. The map is shared with
/// the store, which writes the weights an attempt adopts.
#[derive(Default)]
struct FakeWeights(SharedWeights);

#[async_trait]
impl ReviewRepository for FakeWeights {
    async fn get(&self, _: UserId, _: ReviewCardId) -> Result<Option<ReviewItem>, RepositoryError> {
        unreachable!()
    }
    async fn list_due(
        &self,
        _: UserId,
        _: OffsetDateTime,
        _: Option<domain::TopicId>,
        _: Option<DueCursor>,
        _: u32,
    ) -> Result<Vec<ReviewItem>, RepositoryError> {
        unreachable!()
    }
    async fn due_stats(
        &self,
        _: UserId,
        _: OffsetDateTime,
        _: Option<domain::TopicId>,
    ) -> Result<DueStats, RepositoryError> {
        unreachable!()
    }
    async fn due_topics(
        &self,
        _: UserId,
        _: OffsetDateTime,
    ) -> Result<Vec<domain::DueTopic>, RepositoryError> {
        unreachable!()
    }
    async fn topic_exists(&self, _: UserId, _: domain::TopicId) -> Result<bool, RepositoryError> {
        unreachable!()
    }
    async fn apply_review(
        &self,
        _: u32,
        _: &CardSchedule,
        _: &ReviewLog,
        _: Option<OffsetDateTime>,
    ) -> Result<(), RepositoryError> {
        unreachable!()
    }
    async fn card_history(&self, _: ReviewCardId) -> Result<Vec<ReviewEvent>, RepositoryError> {
        unreachable!()
    }
    async fn last_log(&self, _: ReviewCardId) -> Result<Option<ReviewLog>, RepositoryError> {
        unreachable!()
    }
    async fn undo_review(&self, _: ReviewLogId, _: OffsetDateTime) -> Result<(), RepositoryError> {
        unreachable!()
    }
    async fn suspend(
        &self,
        _: UserId,
        _: ReviewCardId,
        _: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        unreachable!()
    }
    async fn unsuspend(&self, _: UserId, _: ReviewCardId) -> Result<(), RepositoryError> {
        unreachable!()
    }
    async fn list_suspended(
        &self,
        _: UserId,
        _: Option<domain::SuspendedCursor>,
        _: u32,
    ) -> Result<Vec<ReviewItem>, RepositoryError> {
        unreachable!()
    }
    async fn suspended_count(&self, _: UserId) -> Result<u32, RepositoryError> {
        unreachable!()
    }
    async fn parameters(&self, u: UserId) -> Result<Option<UserWeights>, RepositoryError> {
        Ok(self.0.lock().unwrap().get(&u).cloned())
    }
}

/// A card as the rebuild sees it.
struct Stored {
    review_count: u32,
    history: Vec<ReviewEvent>,
    memory: Option<MemoryValues>,
    /// The weights its memory was computed with: `None` for the defaults.
    stamp: Option<OffsetDateTime>,
}

#[derive(Default)]
struct Mem {
    votes: HashMap<UserId, Vec<CardVotes>>,
    attempts: Vec<(UserId, OptimizationAttempt)>,
    weights: SharedWeights,
    cards: HashMap<ReviewCardId, Stored>,
    /// Reading the user's votes fails.
    broken: HashSet<UserId>,
    /// A vote arrives right after the snapshot of these cards is read.
    voted_meanwhile: HashSet<ReviewCardId>,
    /// Reading the snapshot of these cards fails.
    unreadable: HashSet<ReviewCardId>,
}

#[derive(Default)]
struct FakeStore(Mutex<Mem>);

#[async_trait]
impl ReviewOptimizationRepository for FakeStore {
    async fn users_with_review_data(&self) -> Result<Vec<UserId>, RepositoryError> {
        let m = self.0.lock().unwrap();
        let mut users: Vec<_> = m.votes.keys().copied().collect();
        users.extend(m.weights.lock().unwrap().keys().copied());
        users.sort_by_key(ToString::to_string);
        users.dedup();
        Ok(users)
    }
    async fn card_votes(&self, u: UserId) -> Result<Vec<CardVotes>, RepositoryError> {
        let m = self.0.lock().unwrap();
        if m.broken.contains(&u) {
            return Err(RepositoryError::Storage("disk".into()));
        }
        Ok(m.votes.get(&u).cloned().unwrap_or_default())
    }
    async fn last_attempt(
        &self,
        u: UserId,
    ) -> Result<Option<OptimizationAttempt>, RepositoryError> {
        let m = self.0.lock().unwrap();
        Ok(m.attempts
            .iter()
            .rev()
            .find(|(who, _)| *who == u)
            .map(|(_, a)| a.clone()))
    }
    async fn record_attempt(
        &self,
        u: UserId,
        attempt: &OptimizationAttempt,
        adopted: Option<&SchedulerParameters>,
    ) -> Result<(), RepositoryError> {
        let mut m = self.0.lock().unwrap();
        m.attempts.push((u, attempt.clone()));
        if let Some(p) = adopted {
            m.weights.lock().unwrap().insert(
                u,
                UserWeights {
                    parameters: p.clone(),
                    adopted_at: attempt.at,
                },
            );
        }
        Ok(())
    }
    async fn cards_out_of_date(
        &self,
        u: UserId,
        weights: OffsetDateTime,
    ) -> Result<Vec<ReviewCardId>, RepositoryError> {
        let m = self.0.lock().unwrap();
        let mut ids: Vec<ReviewCardId> = m
            .votes
            .get(&u)
            .into_iter()
            .flatten()
            .map(|c| c.card)
            .filter(|c| m.cards[c].stamp != Some(weights))
            .collect();
        ids.sort_by_key(ToString::to_string);
        Ok(ids)
    }
    async fn card_snapshot(
        &self,
        c: ReviewCardId,
    ) -> Result<(u32, Vec<ReviewEvent>), RepositoryError> {
        let mut m = self.0.lock().unwrap();
        if m.unreadable.contains(&c) {
            return Err(RepositoryError::Storage("disk".into()));
        }
        let meanwhile = m.voted_meanwhile.contains(&c);
        let s = m.cards.get_mut(&c).ok_or(RepositoryError::NotFound)?;
        let snapshot = (s.review_count, s.history.clone());
        if meanwhile {
            s.review_count += 1;
        }
        Ok(snapshot)
    }
    async fn rebuild_memory(
        &self,
        c: ReviewCardId,
        expected: u32,
        memory: Option<MemoryValues>,
        weights: OffsetDateTime,
    ) -> Result<bool, RepositoryError> {
        let mut m = self.0.lock().unwrap();
        let s = m.cards.get_mut(&c).ok_or(RepositoryError::NotFound)?;
        if s.review_count != expected {
            return Ok(false);
        }
        if memory.is_some() {
            s.memory = memory;
        }
        s.stamp = Some(weights);
        Ok(true)
    }
}

/// Trains `fake` weights whose first value is the number of samples; scores
/// `fake` weights as `candidate_loss` and any other as `current_loss`.
struct FakeOptimizer {
    current_loss: f64,
    candidate_loss: f64,
    fail_train_on: Mutex<Option<usize>>,
    /// Training reports that it was stopped.
    interrupted: Mutex<bool>,
    trains: Mutex<Vec<Vec<OffsetDateTime>>>,
    evaluations: Mutex<Vec<(SchedulerParameters, Vec<OffsetDateTime>)>>,
}

impl FakeOptimizer {
    fn new(current_loss: f64, candidate_loss: f64) -> Self {
        Self {
            current_loss,
            candidate_loss,
            fail_train_on: Mutex::new(None),
            interrupted: Mutex::new(false),
            trains: Mutex::default(),
            evaluations: Mutex::default(),
        }
    }
}

fn times(samples: &[ReviewSample]) -> Vec<OffsetDateTime> {
    samples.iter().map(|s| s.votes.last().unwrap().at).collect()
}

#[async_trait]
impl ParameterOptimizer for FakeOptimizer {
    async fn train(&self, samples: &[ReviewSample]) -> Result<SchedulerParameters, OptimizerError> {
        self.trains.lock().unwrap().push(times(samples));
        if *self.interrupted.lock().unwrap() {
            return Err(OptimizerError::Interrupted);
        }
        if *self.fail_train_on.lock().unwrap() == Some(self.trains.lock().unwrap().len()) {
            return Err(OptimizerError::Failed("no convergence".into()));
        }
        #[allow(clippy::cast_precision_loss)]
        Ok(params("fake", samples.len() as f64))
    }
    async fn evaluate(
        &self,
        p: &SchedulerParameters,
        samples: &[ReviewSample],
    ) -> Result<f64, OptimizerError> {
        self.evaluations
            .lock()
            .unwrap()
            .push((p.clone(), times(samples)));
        Ok(if p.model == "fake" {
            self.candidate_loss
        } else {
            self.current_loss
        })
    }
}

struct Env {
    uc: OptimizeReviewParameters,
    store: Arc<FakeStore>,
    own: Arc<FakeWeights>,
    optimizer: Arc<FakeOptimizer>,
    clock: Arc<TestClock>,
}

fn env(current_loss: f64, candidate_loss: f64) -> Env {
    let store = Arc::new(FakeStore::default());
    let own = Arc::new(FakeWeights(store.0.lock().unwrap().weights.clone()));
    let optimizer = Arc::new(FakeOptimizer::new(current_loss, candidate_loss));
    let clock = Arc::new(TestClock::new(T0));
    Env {
        uc: OptimizeReviewParameters::new(
            own.clone(),
            store.clone(),
            Arc::new(FakeScheduler),
            optimizer.clone(),
            clock.clone(),
        ),
        store,
        own,
        optimizer,
        clock,
    }
}

impl Env {
    /// Gives `u` a history with `useful` useful votes and the rebuilt state of
    /// its cards.
    fn give(&self, u: UserId, useful: i64) {
        let cards = history(useful, T0 - Duration::days(1));
        let mut m = self.store.0.lock().unwrap();
        for c in &cards {
            m.cards.insert(
                c.card,
                Stored {
                    review_count: u32::try_from(c.votes.len()).unwrap(),
                    history: c.votes.iter().map(|v| v.event).collect(),
                    memory: None,
                    stamp: None,
                },
            );
        }
        m.votes.insert(u, cards);
    }

    /// Saves `parameters` as the weights of `u`, adopted at `OLD_STAMP`.
    fn set_weights(&self, u: UserId, parameters: SchedulerParameters) {
        self.own.0.lock().unwrap().insert(
            u,
            UserWeights {
                parameters,
                adopted_at: OLD_STAMP,
            },
        );
    }

    fn weights(&self, u: UserId) -> Option<SchedulerParameters> {
        self.own
            .0
            .lock()
            .unwrap()
            .get(&u)
            .map(|w| w.parameters.clone())
    }

    async fn run(&self, u: UserId) -> Result<UserRun, crate::error::AppError> {
        let mut all = self.uc.run_once().await.unwrap();
        let at = all.iter().position(|(who, _)| *who == u).unwrap();
        all.swap_remove(at).1
    }

    fn attempts(&self, u: UserId) -> Vec<OptimizationAttempt> {
        let m = self.store.0.lock().unwrap();
        m.attempts
            .iter()
            .filter(|(who, _)| *who == u)
            .map(|(_, a)| a.clone())
            .collect()
    }
}

fn threshold() -> i64 {
    i64::try_from(MIN_USEFUL_VOTES).unwrap()
}

#[tokio::test]
async fn a_user_needs_300_useful_votes_and_the_first_vote_of_a_card_is_not_one() {
    let e = env(1.0, 0.5);
    e.give(user(1), threshold() - 1);
    assert_eq!(e.run(user(1)).await.unwrap(), UserRun::NotEligible);
    assert_eq!(e.attempts(user(1)).len(), 0);

    // 300 votes spread over 300 cards: none is preceded by another.
    let single: Vec<CardVotes> = (1..=300)
        .map(|n| CardVotes {
            card: card(1000 + n),
            votes: votes(T0 - Duration::days(1), 1, T0 - Duration::days(1)),
        })
        .collect();
    e.store.0.lock().unwrap().votes.insert(user(2), single);
    assert_eq!(e.run(user(2)).await.unwrap(), UserRun::NotEligible);

    e.give(user(3), threshold());
    assert!(matches!(
        e.run(user(3)).await.unwrap(),
        UserRun::Attempted(_)
    ));
}

#[tokio::test]
async fn the_newest_fifth_is_kept_out_of_the_training_and_judges_both_weights() {
    let e = env(1.0, 0.5);
    e.give(user(1), 300);
    e.run(user(1)).await.unwrap();

    let trains = e.optimizer.trains.lock().unwrap();
    let evals = e.optimizer.evaluations.lock().unwrap();
    // One training, on the older votes: the weights judged are the ones kept.
    assert_eq!(trains.len(), 1);
    let first = &trains[0];
    assert_eq!(first.len(), 240);
    assert_eq!(evals.len(), 2);
    let test = &evals[0].1;
    assert_eq!(test.len(), 60);
    assert_eq!(
        evals[0].1, evals[1].1,
        "both weights judged on the same votes"
    );
    // Chronological, and the test votes are the newest.
    assert!(first.windows(2).all(|w| w[0] <= w[1]));
    assert!(first.last().unwrap() <= test.first().unwrap());
    assert!(test.windows(2).all(|w| w[0] <= w[1]));
    // The weights in use are the defaults, then the candidate.
    assert_eq!(evals[0].0.model, "default");
    assert_eq!(evals[1].0, params("fake", 240.0));
}

#[tokio::test]
async fn candidates_are_adopted_only_if_strictly_better() {
    for (current, candidate, outcome) in [
        (1.0, 0.9, OptimizationOutcome::Adopted),
        (1.0, 1.0, OptimizationOutcome::Rejected),
        (1.0, 1.1, OptimizationOutcome::Rejected),
    ] {
        let e = env(current, candidate);
        e.give(user(1), 300);
        assert_eq!(e.run(user(1)).await.unwrap(), UserRun::Attempted(outcome));
        let [attempt] = e.attempts(user(1)).try_into().unwrap();
        assert_eq!(attempt.outcome, outcome);
        assert_eq!(attempt.at, T0);
        assert_eq!(
            (
                attempt.useful_votes,
                attempt.train_votes,
                attempt.test_votes
            ),
            (300, 240, 60)
        );
        assert_eq!(attempt.current_log_loss, Some(current));
        assert_eq!(attempt.candidate_log_loss, Some(candidate));
        assert_eq!(attempt.error, None);
        let stored = e.weights(user(1));
        if outcome == OptimizationOutcome::Adopted {
            // The candidate trained on the older 80%, as it was judged.
            assert_eq!(stored, Some(params("fake", 240.0)));
        } else {
            assert_eq!(stored, None);
        }
        assert_eq!(e.optimizer.trains.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn the_weights_in_use_are_the_ones_judged() {
    let e = env(1.0, 0.5);
    e.give(user(1), 300);
    e.set_weights(user(1), params("fake", 7.0));
    e.run(user(1)).await.unwrap();
    // The user's own weights (the fake scores `fake` as the candidate: what
    // counts is that they were the ones evaluated first).
    assert_eq!(
        e.optimizer.evaluations.lock().unwrap()[0].0,
        params("fake", 7.0)
    );

    // Weights the scheduler refuses: the defaults are judged, with a warning.
    let e = env(1.0, 0.5);
    e.give(user(1), 300);
    e.set_weights(user(1), params("fsrs5", 7.0));
    e.run(user(1)).await.unwrap();
    assert_eq!(
        e.optimizer.evaluations.lock().unwrap()[0].0.model,
        "default"
    );
}

#[tokio::test]
async fn a_failed_attempt_is_recorded_and_keeps_the_weights() {
    // Training breaks.
    let e = env(1.0, 0.5);
    e.give(user(1), 300);
    *e.optimizer.fail_train_on.lock().unwrap() = Some(1);
    assert_eq!(
        e.run(user(1)).await.unwrap(),
        UserRun::Attempted(OptimizationOutcome::Failed)
    );
    let [attempt] = e.attempts(user(1)).try_into().unwrap();
    assert_eq!(attempt.outcome, OptimizationOutcome::Failed);
    assert!(attempt.error.unwrap().contains("no convergence"));
    assert_eq!(
        (attempt.current_log_loss, attempt.candidate_log_loss),
        (None, None)
    );
    assert_eq!(e.weights(user(1)), None);
}

#[tokio::test]
async fn a_stopped_optimizer_records_nothing_so_the_attempt_is_made_again() {
    let e = env(1.0, 0.5);
    e.give(user(1), 300);
    *e.optimizer.interrupted.lock().unwrap() = true;
    assert_eq!(e.run(user(1)).await.unwrap(), UserRun::Interrupted);
    assert_eq!(e.attempts(user(1)).len(), 0);
    assert_eq!(e.weights(user(1)), None);

    // Not stopped any more (the next start): nothing waits, it just goes.
    *e.optimizer.interrupted.lock().unwrap() = false;
    assert_eq!(
        e.run(user(1)).await.unwrap(),
        UserRun::Attempted(OptimizationOutcome::Adopted)
    );
}

/// The votes of `history(300, ..)` plus one new vote stored at `stored`.
fn with_a_new_vote(stored: OffsetDateTime) -> Vec<CardVotes> {
    let mut cards = history(300, T0 - Duration::days(1));
    cards[0].votes.push(VoteRecord {
        event: ReviewEvent {
            rating: Rating::Good,
            at: T0 + Duration::days(2),
        },
        recorded_at: stored,
    });
    cards
}

#[tokio::test]
async fn an_attempt_that_failed_is_made_again_after_7_days_not_30() {
    let e = env(1.0, 0.5);
    e.give(user(1), 300);
    *e.optimizer.fail_train_on.lock().unwrap() = Some(1);
    assert_eq!(
        e.run(user(1)).await.unwrap(),
        UserRun::Attempted(OptimizationOutcome::Failed)
    );
    e.store
        .0
        .lock()
        .unwrap()
        .votes
        .insert(user(1), with_a_new_vote(T0 + Duration::days(1)));

    e.clock
        .set(T0 + MIN_TIME_AFTER_FAILURE - Duration::minutes(1));
    assert_eq!(e.run(user(1)).await.unwrap(), UserRun::NotEligible);
    e.clock.set(T0 + MIN_TIME_AFTER_FAILURE);
    assert_eq!(
        e.run(user(1)).await.unwrap(),
        UserRun::Attempted(OptimizationOutcome::Adopted)
    );
    assert_eq!(e.attempts(user(1)).len(), 2);

    // A rejected attempt keeps the 30 days.
    let e = env(1.0, 1.0);
    e.give(user(1), 300);
    e.run(user(1)).await.unwrap();
    e.store
        .0
        .lock()
        .unwrap()
        .votes
        .insert(user(1), with_a_new_vote(T0 + Duration::days(1)));
    e.clock.set(T0 + MIN_TIME_AFTER_FAILURE);
    assert_eq!(e.run(user(1)).await.unwrap(), UserRun::NotEligible);
}

#[tokio::test]
async fn a_user_who_fails_does_not_stop_the_others() {
    let e = env(1.0, 0.5);
    e.give(user(1), 300);
    e.give(user(2), 300);
    e.give(user(3), 300);
    e.store.0.lock().unwrap().broken.insert(user(1));
    let all = e.uc.run_once().await.unwrap();
    assert_eq!(all.len(), 3);
    for (who, result) in &all {
        if *who == user(1) {
            assert!(result.is_err());
        } else {
            assert_eq!(
                *result.as_ref().unwrap(),
                UserRun::Attempted(OptimizationOutcome::Adopted)
            );
        }
    }
}

#[tokio::test]
async fn the_next_attempt_waits_30_days_and_new_votes() {
    let e = env(1.0, 1.0); // rejected: the wait applies whatever the outcome
    e.give(user(1), 300);
    e.run(user(1)).await.unwrap();

    // Long enough, but no vote since the attempt.
    e.clock.set(T0 + Duration::days(31));
    assert_eq!(e.run(user(1)).await.unwrap(), UserRun::NotEligible);

    // New votes, but too soon (a minute short of 30 days).
    let fresh = |stored| {
        let mut cards = history(300, T0 - Duration::days(1));
        cards[0].votes.push(VoteRecord {
            event: ReviewEvent {
                rating: Rating::Good,
                at: T0 + Duration::days(2),
            },
            recorded_at: stored,
        });
        cards
    };
    e.clock.set(T0 + Duration::days(30) - Duration::minutes(1));
    e.store
        .0
        .lock()
        .unwrap()
        .votes
        .insert(user(1), fresh(T0 + Duration::days(2)));
    assert_eq!(e.run(user(1)).await.unwrap(), UserRun::NotEligible);

    // 30 days and a vote stored after the attempt.
    e.clock.set(T0 + Duration::days(30));
    assert!(matches!(
        e.run(user(1)).await.unwrap(),
        UserRun::Attempted(_)
    ));
    assert_eq!(e.attempts(user(1)).len(), 2);

    // A backdated vote (`reviewed_at` before the attempt) stored after it
    // counts as new: the check is on the time it was stored.
    e.clock.set(T0 + Duration::days(61));
    let mut cards = fresh(T0 + Duration::days(2));
    cards[1].votes.push(VoteRecord {
        event: ReviewEvent {
            rating: Rating::Good,
            at: T0 - Duration::days(1),
        },
        recorded_at: T0 + Duration::days(40),
    });
    e.store.0.lock().unwrap().votes.insert(user(1), cards);
    assert!(matches!(
        e.run(user(1)).await.unwrap(),
        UserRun::Attempted(_)
    ));
}

#[tokio::test]
async fn adopting_rebuilds_the_memory_of_every_card_with_a_guard_on_the_vote_count() {
    let e = env(1.0, 0.5);
    e.give(user(1), 300);
    // A card without votes, and one that gets a vote while being rebuilt.
    e.store.0.lock().unwrap().cards.insert(
        card(900),
        Stored {
            review_count: 0,
            history: vec![],
            memory: None,
            stamp: None,
        },
    );
    e.store
        .0
        .lock()
        .unwrap()
        .votes
        .get_mut(&user(1))
        .unwrap()
        .push(CardVotes {
            card: card(900),
            votes: vec![],
        });
    e.store.0.lock().unwrap().voted_meanwhile.insert(card(2));

    assert_eq!(
        e.run(user(1)).await.unwrap(),
        UserRun::Attempted(OptimizationOutcome::Adopted)
    );
    let adopted_at = e.own.0.lock().unwrap()[&user(1)].adopted_at;
    {
        let m = e.store.0.lock().unwrap();
        // The new weights were stored, then used: first weight 240 + 11 votes.
        assert_eq!(e.weights(user(1)), Some(params("fake", 240.0)));
        assert_eq!(m.cards[&card(1)].memory.unwrap().stability, 251.0);
        assert_eq!(m.cards[&card(3)].memory.unwrap().stability, 251.0);
        assert_eq!(m.cards[&card(1)].stamp, Some(adopted_at));
        // Voted meanwhile: left alone, still out of date. No votes: no memory
        // to write, but stamped, so it is not listed again.
        assert_eq!(m.cards[&card(2)].memory, None);
        assert_eq!(m.cards[&card(2)].stamp, None);
        assert_eq!(m.cards[&card(900)].memory, None);
        assert_eq!(m.cards[&card(900)].stamp, Some(adopted_at));
    }

    // The next pass picks up the card that was skipped (its vote is in the
    // history by now), whatever the attempt decides.
    e.store.0.lock().unwrap().voted_meanwhile.clear();
    assert_eq!(e.run(user(1)).await.unwrap(), UserRun::NotEligible);
    let m = e.store.0.lock().unwrap();
    assert_eq!(m.cards[&card(2)].stamp, Some(adopted_at));
    assert_eq!(m.cards[&card(2)].memory.unwrap().stability, 251.0);
}

#[tokio::test]
async fn a_rebuild_that_was_cut_short_is_finished_by_the_next_pass_even_for_a_user_who_is_not_eligible()
 {
    // 20 useful votes: far from 300, so no attempt is ever made for them.
    let e = env(1.0, 0.5);
    e.give(user(1), 20);
    e.set_weights(user(1), params("fake", 7.0));
    {
        let mut m = e.store.0.lock().unwrap();
        // Card 2 is already on the weights; card 1 cannot be read this time.
        m.cards.get_mut(&card(2)).unwrap().stamp = Some(OLD_STAMP);
        m.unreadable.insert(card(1));
    }
    assert_eq!(e.run(user(1)).await.unwrap(), UserRun::NotEligible);
    {
        let m = e.store.0.lock().unwrap();
        assert_eq!(m.cards[&card(1)].stamp, None);
        assert_eq!(m.cards[&card(1)].memory, None);
    }

    // The failure is gone: the next pass rebuilds it, with the weights stored.
    e.store.0.lock().unwrap().unreadable.clear();
    assert_eq!(e.run(user(1)).await.unwrap(), UserRun::NotEligible);
    let m = e.store.0.lock().unwrap();
    assert_eq!(m.cards[&card(1)].stamp, Some(OLD_STAMP));
    assert_eq!(m.cards[&card(1)].memory.unwrap().stability, 7.0 + 11.0);
    // Nothing was trained or recorded for a user with too few votes.
    assert!(e.optimizer.trains.lock().unwrap().is_empty());
    assert_eq!(m.attempts.len(), 0);
}

#[tokio::test]
async fn a_rejected_attempt_touches_no_card() {
    let e = env(1.0, 2.0);
    e.give(user(1), 300);
    e.run(user(1)).await.unwrap();
    let m = e.store.0.lock().unwrap();
    assert!(m.cards.values().all(|c| c.memory.is_none()));
    // No weights saved: the cards are on the defaults and stay as they are.
    assert!(m.cards.values().all(|c| c.stamp.is_none()));
}
