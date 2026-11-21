//! Review use cases over an in-memory repository and a scripted scheduler.
//! The real FSRS adapter is exercised in `infrastructure` and end to end in
//! `api/tests/reviews.rs`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use domain::{
    CardOrigin, CardSchedule, CardStatus, DueCursor, DueStats, DueTopic, LearningRecordId,
    LessonId, MemoryBasis, MemoryValues, Page, PageRequest, Rating, RatingIntervals,
    RepositoryError, ReviewCard, ReviewCardId, ReviewEvent, ReviewItem, ReviewLog, ReviewLogId,
    ReviewRepository, ReviewScheduler, ReviewState, SchedulerError, SchedulerParameters,
    SuspendedCursor, Suspension, SuspensionReason, TopicId, UserWeights,
};
use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::error::{AppError, ConflictReason};
use crate::fakes::{SeqIds, TestClock};
use crate::reviews::{ReviewService, Vote};

const T0: OffsetDateTime = datetime!(2026-11-09 09:00 UTC);

/// Intervals that depend on the vote and the number of votes so far, so that
/// a wrong state restored by an undo shows up.
struct Scripted;

impl Scripted {
    fn interval(count: u32, rating: Rating) -> Duration {
        match rating {
            Rating::Again => Duration::minutes(10),
            Rating::Hard => Duration::days(1),
            Rating::Good => Duration::days(3 * i64::from(count + 1)),
            Rating::Easy => Duration::days(30),
        }
    }
}

impl ReviewScheduler for Scripted {
    fn default_parameters(&self) -> SchedulerParameters {
        SchedulerParameters {
            model: "scripted".into(),
            weights: Vec::new(),
        }
    }

    fn check_parameters(&self, parameters: &SchedulerParameters) -> Result<(), SchedulerError> {
        if parameters.model == "scripted" {
            Ok(())
        } else {
            Err(SchedulerError::InvalidParameters)
        }
    }

    fn schedule(
        &self,
        s: &CardSchedule,
        rating: Rating,
        now: OffsetDateTime,
        parameters: &SchedulerParameters,
    ) -> Result<CardSchedule, SchedulerError> {
        Ok(CardSchedule {
            state: if rating == Rating::Again {
                ReviewState::Relearning
            } else {
                ReviewState::Review
            },
            // The first weight shows in the result: which weights were used.
            stability: 1.0
                + f64::from(s.review_count)
                + parameters.weights.first().copied().unwrap_or(0.0),
            difficulty: 5.0,
            stability_fast: 1.0,
            learning_step: 0,
            due: now + Self::interval(s.review_count, rating),
            last_reviewed_at: Some(now),
            review_count: s.review_count + 1,
            lapses: s.lapses,
        })
    }

    fn preview(
        &self,
        s: &CardSchedule,
        _now: OffsetDateTime,
        _parameters: &SchedulerParameters,
    ) -> Result<RatingIntervals, SchedulerError> {
        Ok(RatingIntervals {
            again: Self::interval(s.review_count, Rating::Again),
            hard: Self::interval(s.review_count, Rating::Hard),
            good: Self::interval(s.review_count, Rating::Good),
            easy: Self::interval(s.review_count, Rating::Easy),
        })
    }

    /// Stability shows the weights and the number of votes replayed, 100 above
    /// anything `schedule` gives, so a replayed card is recognised.
    fn replay(
        &self,
        parameters: &SchedulerParameters,
        history: &[ReviewEvent],
    ) -> Result<Option<MemoryValues>, SchedulerError> {
        #[allow(clippy::cast_precision_loss)]
        let n = history.len() as f64;
        Ok((!history.is_empty()).then(|| MemoryValues {
            stability: 100.0 + n + parameters.weights.first().copied().unwrap_or(0.0),
            difficulty: 6.0,
            stability_fast: 2.0,
        }))
    }
}

#[derive(Default)]
struct Mem {
    items: Vec<ReviewItem>,
    logs: Vec<ReviewLog>,
    /// The user's personal weights.
    weights: Option<UserWeights>,
    /// The votes of each card, as `card_history` returns them.
    history: Vec<(ReviewCardId, ReviewEvent)>,
    /// Reading the history of these cards fails.
    unreadable: Vec<ReviewCardId>,
    /// The next `apply_review` loses a race against a suspension: the card
    /// gets suspended and the write answers `Conflict`.
    suspend_during_vote: bool,
}

#[derive(Default)]
struct FakeReviews(Mutex<Mem>);

#[async_trait]
impl ReviewRepository for FakeReviews {
    async fn get(
        &self,
        _user: domain::UserId,
        id: ReviewCardId,
    ) -> Result<Option<ReviewItem>, RepositoryError> {
        let m = self.0.lock().unwrap();
        Ok(m.items.iter().find(|i| i.card.id == id).cloned())
    }

    async fn list_due(
        &self,
        _user: domain::UserId,
        now: OffsetDateTime,
        topic: Option<TopicId>,
        after: Option<DueCursor>,
        limit: u32,
    ) -> Result<Vec<ReviewItem>, RepositoryError> {
        let m = self.0.lock().unwrap();
        let mut v: Vec<_> = m
            .items
            .iter()
            .filter(|i| i.card.status == CardStatus::Active && i.card.schedule.due <= now)
            .filter(|i| topic.is_none_or(|t| i.card.topic_id == t))
            .filter(|i| after.is_none_or(|c| (i.card.schedule.due, i.card.id) > (c.due, c.id)))
            .cloned()
            .collect();
        v.sort_by_key(|i| (i.card.schedule.due, i.card.id));
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn due_stats(
        &self,
        _user: domain::UserId,
        now: OffsetDateTime,
        topic: Option<TopicId>,
    ) -> Result<DueStats, RepositoryError> {
        let m = self.0.lock().unwrap();
        let active = m
            .items
            .iter()
            .filter(|i| i.card.status == CardStatus::Active)
            .filter(|i| topic.is_none_or(|t| i.card.topic_id == t));
        Ok(DueStats {
            due_count: u32::try_from(
                active
                    .clone()
                    .filter(|i| i.card.schedule.due <= now)
                    .count(),
            )
            .unwrap(),
            next_due_at: active
                .filter(|i| i.card.schedule.due > now)
                .map(|i| i.card.schedule.due)
                .min(),
        })
    }

    async fn due_topics(
        &self,
        _user: domain::UserId,
        now: OffsetDateTime,
    ) -> Result<Vec<DueTopic>, RepositoryError> {
        let m = self.0.lock().unwrap();
        let mut out: Vec<DueTopic> = Vec::new();
        for i in m
            .items
            .iter()
            .filter(|i| i.card.status == CardStatus::Active && i.card.schedule.due <= now)
        {
            match out.iter_mut().find(|t| t.topic_id == i.card.topic_id) {
                Some(t) => t.due_count += 1,
                None => out.push(DueTopic {
                    topic_id: i.card.topic_id,
                    topic_title: i.topic_title.clone(),
                    due_count: 1,
                }),
            }
        }
        out.sort_by(|a, b| {
            (b.due_count, &a.topic_title, a.topic_id).cmp(&(
                a.due_count,
                &b.topic_title,
                b.topic_id,
            ))
        });
        Ok(out)
    }

    async fn topic_exists(
        &self,
        _user: domain::UserId,
        topic: TopicId,
    ) -> Result<bool, RepositoryError> {
        let m = self.0.lock().unwrap();
        Ok(m.items.iter().any(|i| i.card.topic_id == topic))
    }

    async fn apply_review(
        &self,
        expected: u32,
        after: &CardSchedule,
        log: &ReviewLog,
        weights: Option<OffsetDateTime>,
    ) -> Result<(), RepositoryError> {
        let mut m = self.0.lock().unwrap();
        let mut suspend_during_vote = m.suspend_during_vote;
        m.suspend_during_vote = false;
        let item = m
            .items
            .iter_mut()
            .find(|i| i.card.id == log.card_id)
            .ok_or(RepositoryError::NotFound)?;
        if std::mem::take(&mut suspend_during_vote) && item.card.status == CardStatus::Active {
            item.card.status = CardStatus::Suspended;
            item.card.suspension = Some(Suspension {
                reason: SuspensionReason::User,
                at: log.created_at,
            });
        }
        if item.card.status != CardStatus::Active || item.card.schedule.review_count != expected {
            return Err(RepositoryError::Conflict);
        }
        item.card.schedule = *after;
        item.basis = MemoryBasis::current(weights);
        m.logs.push(log.clone());
        Ok(())
    }

    async fn card_history(&self, card: ReviewCardId) -> Result<Vec<ReviewEvent>, RepositoryError> {
        let m = self.0.lock().unwrap();
        if m.unreadable.contains(&card) {
            return Err(RepositoryError::Storage("disk".into()));
        }
        Ok(m.history
            .iter()
            .filter(|(c, _)| *c == card)
            .map(|(_, e)| *e)
            .collect())
    }

    async fn last_log(&self, card_id: ReviewCardId) -> Result<Option<ReviewLog>, RepositoryError> {
        let m = self.0.lock().unwrap();
        Ok(m.logs.iter().rev().find(|l| l.card_id == card_id).cloned())
    }

    async fn undo_review(
        &self,
        log_id: ReviewLogId,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        let mut m = self.0.lock().unwrap();
        let log = m
            .logs
            .iter_mut()
            .find(|l| l.id == log_id)
            .ok_or(RepositoryError::NotFound)?;
        if log.undone_at.is_some() {
            return Err(RepositoryError::Conflict);
        }
        log.undone_at = Some(at);
        let (card_id, before) = (log.card_id, log.before);
        if let Some(i) = m.items.iter_mut().find(|i| i.card.id == card_id) {
            i.card.schedule = before;
        }
        Ok(())
    }

    async fn suspend(
        &self,
        _user: domain::UserId,
        id: ReviewCardId,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        let mut m = self.0.lock().unwrap();
        let item = m
            .items
            .iter_mut()
            .find(|i| i.card.id == id)
            .ok_or(RepositoryError::NotFound)?;
        if item.card.status == CardStatus::Active {
            item.card.status = CardStatus::Suspended;
            item.card.suspension = Some(Suspension {
                reason: SuspensionReason::User,
                at,
            });
        }
        Ok(())
    }

    async fn unsuspend(
        &self,
        _user: domain::UserId,
        id: ReviewCardId,
    ) -> Result<(), RepositoryError> {
        let mut m = self.0.lock().unwrap();
        let item = m
            .items
            .iter_mut()
            .find(|i| i.card.id == id)
            .ok_or(RepositoryError::NotFound)?;
        if matches!(
            item.card.suspension,
            Some(Suspension {
                reason: SuspensionReason::Superseded,
                ..
            })
        ) {
            return Err(RepositoryError::Conflict);
        }
        item.card.status = CardStatus::Active;
        item.card.suspension = None;
        Ok(())
    }

    async fn list_suspended(
        &self,
        _user: domain::UserId,
        after: Option<SuspendedCursor>,
        limit: u32,
    ) -> Result<Vec<ReviewItem>, RepositoryError> {
        let m = self.0.lock().unwrap();
        // Newest first, then by id: the key sorts descending on the instant.
        let key = |i: &ReviewItem| {
            (
                std::cmp::Reverse(i.card.suspension.map(|s| s.at)),
                i.card.id,
            )
        };
        let mut v: Vec<_> = m
            .items
            .iter()
            .filter(|i| matches!(i.card.suspension, Some(s) if s.reason == SuspensionReason::User))
            .filter(|i| {
                after.is_none_or(|c| key(i) > (std::cmp::Reverse(Some(c.suspended_at)), c.id))
            })
            .cloned()
            .collect();
        v.sort_by_key(key);
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn suspended_count(&self, _user: domain::UserId) -> Result<u32, RepositoryError> {
        let m = self.0.lock().unwrap();
        Ok(u32::try_from(
            m.items
                .iter()
                .filter(
                    |i| matches!(i.card.suspension, Some(s) if s.reason == SuspensionReason::User),
                )
                .count(),
        )
        .unwrap())
    }

    async fn parameters(
        &self,
        _user: domain::UserId,
    ) -> Result<Option<UserWeights>, RepositoryError> {
        Ok(self.0.lock().unwrap().weights.clone())
    }
}

struct Env {
    service: ReviewService,
    repo: Arc<FakeReviews>,
    clock: Arc<TestClock>,
}

fn card_id(n: u128) -> ReviewCardId {
    ReviewCardId::new(Uuid::from_u128(n))
}

fn item(n: u128, created: OffsetDateTime) -> ReviewItem {
    ReviewItem {
        card: ReviewCard {
            id: card_id(n),
            topic_id: TopicId::new(Uuid::from_u128(1)),
            lesson_id: LessonId::new(Uuid::from_u128(2)),
            origin: CardOrigin::Record(LearningRecordId::new(Uuid::from_u128(3))),
            front: "f".into(),
            back: "b".into(),
            status: CardStatus::Active,
            suspension: None,
            schedule: CardSchedule::initial(created),
            created_at: created,
            to_consolidate: false,
        },
        basis: MemoryBasis::current(None),
        topic_title: "Rust".into(),
        lesson_title: "Result".into(),
        lesson_number: 2,
    }
}

fn env(items: Vec<ReviewItem>) -> Env {
    let repo = Arc::new(FakeReviews(Mutex::new(Mem {
        items,
        logs: vec![],
        weights: None,
        history: vec![],
        unreadable: vec![],
        suspend_during_vote: false,
    })));
    let clock = Arc::new(TestClock::new(T0));
    Env {
        service: ReviewService::new(
            repo.clone(),
            Arc::new(Scripted),
            clock.clone(),
            Arc::new(SeqIds::default()),
        ),
        repo,
        clock,
    }
}

fn vote(rating: Rating, count: u32) -> Vote {
    Vote {
        rating,
        reviewed_at: None,
        review_count: count,
    }
}

fn conflict(r: Result<impl std::fmt::Debug, AppError>) -> ConflictReason {
    match r {
        Err(AppError::Conflict(reason)) => reason,
        other => panic!("expected a conflict, got {other:?}"),
    }
}

#[tokio::test]
async fn a_new_card_is_not_due_until_a_day_after_its_lesson() {
    let e = env(vec![item(1, T0)]);
    let req = || PageRequest {
        after: None,
        limit: 50,
    };
    let due = e
        .service
        .due(domain::UserId::OWNER, None, req())
        .await
        .unwrap();
    assert_eq!((due.due_count, due.estimated_minutes), (0, 0));
    assert_eq!(due.next_due_at, Some(T0 + Duration::hours(24)));

    e.clock
        .set(T0 + Duration::hours(23) + Duration::minutes(59));
    assert_eq!(
        e.service
            .due(domain::UserId::OWNER, None, req())
            .await
            .unwrap()
            .due_count,
        0
    );

    e.clock.set(T0 + Duration::hours(24));
    let due = e
        .service
        .due(domain::UserId::OWNER, None, req())
        .await
        .unwrap();
    assert_eq!((due.due_count, due.estimated_minutes), (1, 1));
    assert_eq!(due.next_due_at, None);
    assert!(due.page.items[0].intervals_preview.is_some());
}

#[tokio::test]
async fn the_due_list_is_ordered_paged_and_counts_every_card() {
    let items = (1..=5).map(|n| item(n, T0)).collect();
    let e = env(items);
    e.clock.set(T0 + Duration::days(2));
    let first = e
        .service
        .due(
            domain::UserId::OWNER,
            None,
            PageRequest {
                after: None,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.page.items.len(), 2);
    assert_eq!(first.due_count, 5);
    assert_eq!(first.estimated_minutes, 2);
    let next = first.page.next.expect("more pages");
    let second = e
        .service
        .due(
            domain::UserId::OWNER,
            None,
            PageRequest {
                after: Some(next),
                limit: 2,
            },
        )
        .await
        .unwrap();
    let ids: Vec<_> = first
        .page
        .items
        .iter()
        .chain(&second.page.items)
        .map(|v| v.item.card.id)
        .collect();
    assert_eq!(ids, (1..=4).map(card_id).collect::<Vec<_>>());
}

#[tokio::test]
async fn a_vote_moves_due_by_exactly_the_previewed_interval() {
    let e = env(vec![item(1, T0)]);
    e.clock.set(T0 + Duration::days(1));
    let now = T0 + Duration::days(1);
    let before = e
        .repo
        .get(domain::UserId::OWNER, card_id(1))
        .await
        .unwrap()
        .unwrap();
    let preview = Scripted
        .preview(&before.card.schedule, now, &Scripted.default_parameters())
        .unwrap();

    let view = e
        .service
        .rate(domain::UserId::OWNER, card_id(1), vote(Rating::Good, 0))
        .await
        .unwrap();
    assert_eq!(view.item.card.schedule.due, now + preview.good);
    assert_eq!(view.item.card.schedule.review_count, 1);
    assert_eq!(view.item.card.schedule.last_reviewed_at, Some(now));
    // The preview of the updated card is for its next vote.
    assert_eq!(view.intervals_preview.unwrap().good, Duration::days(6));
    assert_eq!(
        e.service
            .due(
                domain::UserId::OWNER,
                None,
                PageRequest {
                    after: None,
                    limit: 5
                }
            )
            .await
            .unwrap()
            .due_count,
        0
    );
}

#[tokio::test]
async fn again_brings_the_card_back_in_minutes() {
    let e = env(vec![item(1, T0)]);
    e.clock.set(T0 + Duration::days(1));
    let view = e
        .service
        .rate(domain::UserId::OWNER, card_id(1), vote(Rating::Again, 0))
        .await
        .unwrap();
    assert_eq!(view.item.card.schedule.state, ReviewState::Relearning);
    e.clock.set(T0 + Duration::days(1) + Duration::minutes(10));
    let due = e
        .service
        .due(
            domain::UserId::OWNER,
            None,
            PageRequest {
                after: None,
                limit: 5,
            },
        )
        .await
        .unwrap();
    assert_eq!(due.due_count, 1);
}

#[tokio::test]
async fn a_suspension_that_wins_the_race_is_reported_as_card_suspended() {
    let e = env(vec![item(1, T0)]);
    e.clock.set(T0 + Duration::days(1));
    e.repo.0.lock().unwrap().suspend_during_vote = true;
    // The card was active when read, and suspended when the write failed: the
    // reason is the suspension, not a stale vote.
    assert_eq!(
        conflict(
            e.service
                .rate(domain::UserId::OWNER, card_id(1), vote(Rating::Good, 0))
                .await
        ),
        ConflictReason::CardSuspended
    );
    assert_eq!(e.repo.0.lock().unwrap().logs.len(), 0);
}

#[tokio::test]
async fn a_stale_or_suspended_card_is_a_conflict_and_stores_nothing() {
    let e = env(vec![item(1, T0), item(2, T0)]);
    e.clock.set(T0 + Duration::days(1));
    e.service
        .rate(domain::UserId::OWNER, card_id(1), vote(Rating::Good, 0))
        .await
        .unwrap();
    // The same vote sent again (offline queue replay).
    assert_eq!(
        conflict(
            e.service
                .rate(domain::UserId::OWNER, card_id(1), vote(Rating::Good, 0))
                .await
        ),
        ConflictReason::StaleReview
    );
    assert_eq!(e.repo.0.lock().unwrap().logs.len(), 1);

    e.service
        .suspend(domain::UserId::OWNER, card_id(2))
        .await
        .unwrap();
    assert_eq!(
        conflict(
            e.service
                .rate(domain::UserId::OWNER, card_id(2), vote(Rating::Good, 0))
                .await
        ),
        ConflictReason::CardSuspended
    );
    assert!(matches!(
        e.service
            .rate(domain::UserId::OWNER, card_id(9), vote(Rating::Good, 0))
            .await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn reviewed_at_is_kept_between_the_last_vote_and_now() {
    let e = env(vec![item(1, T0)]);
    let day1 = T0 + Duration::days(1);
    e.clock.set(day1 + Duration::minutes(5));
    // Offline vote, 3 minutes ago: kept.
    let v = Vote {
        reviewed_at: Some(day1 + Duration::minutes(2)),
        ..vote(Rating::Good, 0)
    };
    let view = e
        .service
        .rate(domain::UserId::OWNER, card_id(1), v)
        .await
        .unwrap();
    assert_eq!(
        view.item.card.schedule.last_reviewed_at,
        Some(day1 + Duration::minutes(2))
    );

    // A clock running ahead: capped at now.
    e.clock.set(day1 + Duration::days(4));
    let v = Vote {
        reviewed_at: Some(day1 + Duration::days(40)),
        ..vote(Rating::Good, 1)
    };
    let view = e
        .service
        .rate(domain::UserId::OWNER, card_id(1), v)
        .await
        .unwrap();
    assert_eq!(
        view.item.card.schedule.last_reviewed_at,
        Some(day1 + Duration::days(4))
    );

    // A time before the previous vote: raised to it.
    e.clock.set(day1 + Duration::days(20));
    let v = Vote {
        reviewed_at: Some(T0),
        ..vote(Rating::Good, 2)
    };
    let view = e
        .service
        .rate(domain::UserId::OWNER, card_id(1), v)
        .await
        .unwrap();
    assert_eq!(
        view.item.card.schedule.last_reviewed_at,
        Some(day1 + Duration::days(4))
    );
}

#[tokio::test]
async fn undo_restores_the_card_once_and_only_within_ten_minutes() {
    let e = env(vec![item(1, T0)]);
    let day1 = T0 + Duration::days(1);
    e.clock.set(day1);
    let original = e
        .repo
        .get(domain::UserId::OWNER, card_id(1))
        .await
        .unwrap()
        .unwrap()
        .card
        .schedule;
    assert_eq!(
        conflict(e.service.undo(domain::UserId::OWNER, card_id(1)).await),
        ConflictReason::NothingToUndo
    );

    e.service
        .rate(domain::UserId::OWNER, card_id(1), vote(Rating::Easy, 0))
        .await
        .unwrap();
    e.clock.set(day1 + Duration::minutes(9));
    let view = e
        .service
        .undo(domain::UserId::OWNER, card_id(1))
        .await
        .unwrap();
    assert_eq!(view.item.card.schedule, original);
    assert_eq!(
        e.repo
            .get(domain::UserId::OWNER, card_id(1))
            .await
            .unwrap()
            .unwrap()
            .card
            .schedule,
        original
    );
    // A second undo does not reach back to an earlier vote.
    assert_eq!(
        conflict(e.service.undo(domain::UserId::OWNER, card_id(1)).await),
        ConflictReason::NothingToUndo
    );
    // The vote can be given again: the version is back to 0.
    e.service
        .rate(domain::UserId::OWNER, card_id(1), vote(Rating::Good, 0))
        .await
        .unwrap();

    e.clock
        .set(day1 + Duration::minutes(9) + Duration::minutes(11));
    assert_eq!(
        conflict(e.service.undo(domain::UserId::OWNER, card_id(1)).await),
        ConflictReason::UndoExpired
    );
}

#[tokio::test]
async fn suspending_is_idempotent_and_removes_the_card_from_the_due_list() {
    let e = env(vec![item(1, T0)]);
    e.clock.set(T0 + Duration::days(2));
    let a = e
        .service
        .suspend(domain::UserId::OWNER, card_id(1))
        .await
        .unwrap();
    let b = e
        .service
        .suspend(domain::UserId::OWNER, card_id(1))
        .await
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.item.card.status, CardStatus::Suspended);
    assert_eq!(a.intervals_preview, None);
    let due = e
        .service
        .due(
            domain::UserId::OWNER,
            None,
            PageRequest {
                after: None,
                limit: 5,
            },
        )
        .await
        .unwrap();
    assert_eq!((due.due_count, due.next_due_at), (0, None));
    assert_eq!(
        due.page,
        Page {
            items: vec![],
            next: None
        }
    );
    assert!(matches!(
        e.service.suspend(domain::UserId::OWNER, card_id(9)).await,
        Err(AppError::NotFound)
    ));
}

fn superseded(mut it: ReviewItem, at: OffsetDateTime) -> ReviewItem {
    it.card.status = CardStatus::Suspended;
    it.card.suspension = Some(Suspension {
        reason: SuspensionReason::Superseded,
        at,
    });
    it
}

#[tokio::test]
async fn suspending_by_hand_records_reason_and_first_instant_only() {
    let e = env(vec![item(1, T0)]);
    let user = domain::UserId::OWNER;
    e.clock.set(T0 + Duration::hours(1));
    let first = e.service.suspend(user, card_id(1)).await.unwrap();
    e.clock.set(T0 + Duration::hours(2));
    let again = e.service.suspend(user, card_id(1)).await.unwrap();
    let expected = Some(Suspension {
        reason: SuspensionReason::User,
        at: T0 + Duration::hours(1),
    });
    assert_eq!(first.item.card.suspension, expected);
    assert_eq!(again.item.card.suspension, expected);
}

#[tokio::test]
async fn unsuspending_restores_the_card_and_is_idempotent() {
    let e = env(vec![item(1, T0)]);
    let user = domain::UserId::OWNER;
    e.service.suspend(user, card_id(1)).await.unwrap();
    let back = e.service.unsuspend(user, card_id(1)).await.unwrap();
    assert_eq!(back.item.card.status, CardStatus::Active);
    assert_eq!(back.item.card.suspension, None);
    assert!(back.intervals_preview.is_some());
    // Schedule untouched.
    assert_eq!(back.item.card.schedule, CardSchedule::initial(T0));
    let twice = e.service.unsuspend(user, card_id(1)).await.unwrap();
    assert_eq!(twice, back);
    assert!(matches!(
        e.service.unsuspend(user, card_id(9)).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn a_superseded_card_is_not_reactivated() {
    let e = env(vec![superseded(item(1, T0), T0)]);
    assert_eq!(
        conflict(e.service.unsuspend(domain::UserId::OWNER, card_id(1)).await),
        ConflictReason::CardSuperseded
    );
}

#[tokio::test]
async fn the_suspended_list_has_only_hand_made_ones_newest_first() {
    let e = env(vec![
        item(1, T0),
        item(2, T0),
        item(3, T0),
        item(4, T0),
        superseded(item(5, T0), T0),
    ]);
    let user = domain::UserId::OWNER;
    for (n, hours) in [(1, 1), (2, 3), (3, 3)] {
        e.clock.set(T0 + Duration::hours(hours));
        e.service.suspend(user, card_id(n)).await.unwrap();
    }
    let first = e
        .service
        .suspended(
            user,
            PageRequest {
                after: None,
                limit: 2,
            },
        )
        .await
        .unwrap();
    // 2 and 3 share the instant: by id; 1 is older.
    let ids: Vec<_> = first.page.items.iter().map(|v| v.item.card.id).collect();
    assert_eq!(ids, [card_id(2), card_id(3)]);
    assert_eq!(first.suspended_count, 3);
    assert!(
        first
            .page
            .items
            .iter()
            .all(|v| v.intervals_preview.is_none())
    );
    let second = e
        .service
        .suspended(
            user,
            PageRequest {
                after: first.page.next,
                limit: 2,
            },
        )
        .await
        .unwrap();
    let ids: Vec<_> = second.page.items.iter().map(|v| v.item.card.id).collect();
    assert_eq!(ids, [card_id(1)]);
    assert_eq!(second.page.next, None);
}

const ADOPTED: OffsetDateTime = datetime!(2026-11-01 04:00 UTC);

fn personal(model: &str, first: f64) -> UserWeights {
    UserWeights {
        parameters: SchedulerParameters {
            model: model.into(),
            weights: vec![first],
        },
        adopted_at: ADOPTED,
    }
}

#[tokio::test]
async fn a_vote_uses_the_weights_of_the_user_or_the_defaults_without_any() {
    let e = env(vec![item(1, T0), item(2, T0)]);
    let user = domain::UserId::OWNER;
    let plain = e
        .service
        .rate(user, card_id(1), vote(Rating::Good, 0))
        .await;
    assert_eq!(plain.unwrap().item.card.schedule.stability, 1.0);

    e.repo.0.lock().unwrap().weights = Some(personal("scripted", 100.0));
    let own = e
        .service
        .rate(user, card_id(2), vote(Rating::Good, 0))
        .await;
    assert_eq!(own.unwrap().item.card.schedule.stability, 101.0);
}

#[tokio::test]
async fn weights_the_scheduler_cannot_use_fall_back_to_the_defaults_without_failing() {
    let e = env(vec![item(1, T0)]);
    let user = domain::UserId::OWNER;
    e.repo.0.lock().unwrap().weights = Some(personal("fsrs5", 100.0));
    let voted = e
        .service
        .rate(user, card_id(1), vote(Rating::Good, 0))
        .await;
    assert_eq!(voted.unwrap().item.card.schedule.stability, 1.0);
    // The list and the preview do not fail either.
    e.clock.set(T0 + Duration::days(2));
    let due = e
        .service
        .due(
            user,
            None,
            PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await;
    assert!(due.is_ok());
}

/// A card that was voted twice on `model`, with the memory `stability` and the
/// stamp `weights`; due now, `learning_step` 1.
fn voted_card(n: u128, basis: MemoryBasis, stability: f64) -> (ReviewItem, Vec<ReviewEvent>) {
    let mut it = item(n, T0 - Duration::days(10));
    it.basis = basis;
    it.card.schedule = CardSchedule {
        state: ReviewState::Review,
        stability,
        difficulty: 3.0,
        stability_fast: 0.0,
        learning_step: 1,
        due: T0 - Duration::hours(1),
        last_reviewed_at: Some(T0 - Duration::days(2)),
        review_count: 2,
        lapses: 0,
    };
    let history = vec![
        ReviewEvent {
            rating: Rating::Good,
            at: T0 - Duration::days(5),
        },
        ReviewEvent {
            rating: Rating::Good,
            at: T0 - Duration::days(2),
        },
    ];
    (it, history)
}

fn with_history(e: &Env, card: u128, history: Vec<ReviewEvent>) {
    let mut m = e.repo.0.lock().unwrap();
    m.history
        .extend(history.into_iter().map(|h| (card_id(card), h)));
}

#[tokio::test]
async fn a_vote_on_a_card_of_the_previous_model_starts_from_its_replayed_memory() {
    let previous = MemoryBasis {
        current_model: false,
        weights: None,
    };
    let (it, history) = voted_card(1, previous, 7.5);
    let e = env(vec![it]);
    with_history(&e, 1, history);
    e.repo.0.lock().unwrap().weights = Some(personal("scripted", 10.0));
    let user = domain::UserId::OWNER;

    let voted = e
        .service
        .rate(user, card_id(1), vote(Rating::Good, 2))
        .await
        .unwrap();
    // `Scripted::schedule` ignores the memory, so what shows the start is the
    // log: the state before the vote is the replayed one (110 + 2 votes), with
    // the short-step index of the previous model reset, as the migration does.
    let log = e.repo.0.lock().unwrap().logs[0].clone();
    assert_eq!(log.before.stability, 112.0);
    assert_eq!(
        (log.before.stability_fast, log.before.learning_step),
        (2.0, 0)
    );
    // Everything else of the card is as it was.
    assert_eq!(log.before.due, T0 - Duration::hours(1));
    assert_eq!(log.before.review_count, 2);
    // The vote used the personal weights and stamped the card with them.
    assert_eq!(voted.item.card.schedule.stability, 3.0 + 10.0);
    assert_eq!(
        e.repo.0.lock().unwrap().items[0].basis,
        MemoryBasis::current(Some(ADOPTED))
    );
}

#[tokio::test]
async fn a_card_computed_with_other_weights_is_replayed_before_the_preview_and_the_vote() {
    // On the current model, but stamped with the defaults while the user has
    // weights: the rebuild has not reached it yet.
    let (it, history) = voted_card(1, MemoryBasis::current(None), 7.5);
    let (current, history2) = voted_card(2, MemoryBasis::current(Some(ADOPTED)), 8.5);
    let e = env(vec![it, current]);
    with_history(&e, 1, history);
    with_history(&e, 2, history2);
    e.repo.0.lock().unwrap().weights = Some(personal("scripted", 10.0));
    let user = domain::UserId::OWNER;

    let due = e
        .service
        .due(
            user,
            None,
            PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    let stability = |n: u128| {
        due.page
            .items
            .iter()
            .find(|v| v.item.card.id == card_id(n))
            .map(|v| v.item.card.schedule.stability)
    };
    // Card 1 shows the replayed memory, card 2 (already on these weights) its own.
    assert_eq!((stability(1), stability(2)), (Some(112.0), Some(8.5)));
    // The replay keeps the short-step index of a card on the current model.
    assert_eq!(due.page.items[0].item.card.schedule.learning_step, 1);

    e.service
        .rate(user, card_id(1), vote(Rating::Good, 2))
        .await
        .unwrap();
    assert_eq!(e.repo.0.lock().unwrap().logs[0].before.stability, 112.0);
    e.service
        .rate(user, card_id(2), vote(Rating::Good, 2))
        .await
        .unwrap();
    assert_eq!(e.repo.0.lock().unwrap().logs[1].before.stability, 8.5);
}

#[tokio::test]
async fn a_new_card_needs_no_replay() {
    // The history is not even read for a card never voted.
    let mut it = item(1, T0 - Duration::days(2));
    it.basis = MemoryBasis {
        current_model: false,
        weights: None,
    };
    let e = env(vec![it]);
    e.repo.0.lock().unwrap().weights = Some(personal("scripted", 10.0));
    let voted = e
        .service
        .rate(domain::UserId::OWNER, card_id(1), vote(Rating::Good, 0))
        .await
        .unwrap();
    assert_eq!(voted.item.card.schedule.stability, 11.0);
    assert_eq!(e.repo.0.lock().unwrap().logs[0].before.stability, 0.0);
}

#[tokio::test]
async fn a_card_whose_replay_fails_still_appears_in_the_due_list() {
    // `Scripted::replay` never fails, so the history read does: a card on the
    // previous model has to be replayed, and its history is unreadable.
    let previous = MemoryBasis {
        current_model: false,
        weights: None,
    };
    let (broken, _) = voted_card(1, previous, 7.5);
    let (fine, history) = voted_card(2, MemoryBasis::current(None), 8.5);
    let e = env(vec![broken, fine]);
    with_history(&e, 2, history);
    e.repo.0.lock().unwrap().unreadable.push(card_id(1));
    let due = e
        .service
        .due(
            domain::UserId::OWNER,
            None,
            PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(due.page.items.len(), 2);
    // The stored state, as it was, with its preview.
    assert_eq!(due.page.items[0].item.card.schedule.stability, 7.5);
    assert!(due.page.items[0].intervals_preview.is_some());
}

#[tokio::test]
async fn due_can_be_limited_to_a_topic_and_lists_every_topic_either_way() {
    let other = TopicId::new(Uuid::from_u128(9));
    let mut a = item(1, T0);
    let mut b = item(2, T0);
    let c = item(3, T0);
    a.topic_title = "Zeta".into();
    b.card.topic_id = other;
    b.topic_title = "Alfa".into();
    let e = env(vec![a, b, c]);
    e.clock.set(T0 + Duration::days(2));
    let req = || PageRequest {
        after: None,
        limit: 50,
    };
    let user = domain::UserId::OWNER;

    let all = e.service.due(user, None, req()).await.unwrap();
    assert_eq!(all.due_count, 3);
    let only = e.service.due(user, Some(other), req()).await.unwrap();
    assert_eq!((only.due_count, only.page.items.len()), (1, 1));
    // The topics ignore the filter.
    assert_eq!(only.topics, all.topics);
    assert_eq!(all.topics.len(), 2);
    assert_eq!(all.topics[0].due_count, 2);
    assert_eq!(all.topics[1].topic_title, "Alfa");

    let missing = TopicId::new(Uuid::from_u128(77));
    assert!(matches!(
        e.service.due(user, Some(missing), req()).await,
        Err(crate::error::AppError::NotFound)
    ));
}
