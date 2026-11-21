//! The startup migration of the review cards to FSRS-7 (ADR 0014 point 9) on
//! SQLite with the real migrations, the real scheduler and the real review
//! service. Nothing here calls a model.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use application::{AppError, ConflictReason, MigrateReviewModel, ReviewService, Vote};
use domain::{Clock, IdGenerator, Rating, ReviewCardId, UserId};
use infrastructure::sqlite::{self, Pool};
use infrastructure::{FsrsScheduler, SqliteReviewRepository};
use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const T0: OffsetDateTime = datetime!(2026-11-09 15:00:00.000 UTC);
const TOPIC: &str = "00000000-0000-0000-0000-0000000000a1";
const LESSON: &str = "00000000-0000-0000-0000-0000000000b1";

struct TestClock(Mutex<OffsetDateTime>);
impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        *self.0.lock().unwrap()
    }
}

struct SeqIds(AtomicU64);
impl IdGenerator for SeqIds {
    fn new_id(&self) -> Uuid {
        Uuid::from_u128(0x7000_0000 + u128::from(self.0.fetch_add(1, Ordering::SeqCst)))
    }
}

fn cid(n: u128) -> ReviewCardId {
    ReviewCardId::new(Uuid::from_u128(n))
}

fn iso(at: OffsetDateTime) -> String {
    api::health::format_instant(at)
}

struct Env {
    pool: Pool,
    clock: Arc<TestClock>,
    service: ReviewService,
    migration: MigrateReviewModel,
}

impl Env {
    async fn new() -> Self {
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        for q in [
            "INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) VALUES ('00000000-0000-0000-0000-00000000000f', '00000000-0000-7000-8000-000000000001', 'F', 'f', 0, '2026-11-09T09:00:00.000Z', '2026-11-09T09:00:00.000Z')",
            "INSERT INTO topics (id, user_id, folder_id, title, title_key, created_at, updated_at) VALUES ('00000000-0000-0000-0000-0000000000a1', '00000000-0000-7000-8000-000000000001', '00000000-0000-0000-0000-00000000000f', 'Rust', 'rust', '2026-11-09T09:00:00.000Z', '2026-11-09T09:00:00.000Z')",
            "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, catalog_version, mission_version, document, plan, generation_job_id, created_at) VALUES ('00000000-0000-0000-0000-0000000000b1', '00000000-0000-0000-0000-0000000000a1', 1, 'L', 'o', 5, 1, 1, '{}', '{}', 'job1', '2026-11-09T09:00:00.000Z')",
        ] {
            sqlx::query(q).execute(&pool).await.unwrap();
        }
        let clock = Arc::new(TestClock(Mutex::new(T0)));
        let repo = Arc::new(SqliteReviewRepository::new(pool.clone()));
        let scheduler = Arc::new(FsrsScheduler::new());
        Self {
            service: ReviewService::new(
                repo.clone(),
                scheduler.clone(),
                clock.clone(),
                Arc::new(SeqIds(AtomicU64::new(0))),
            ),
            migration: MigrateReviewModel::new(repo, scheduler),
            pool,
            clock,
        }
    }

    /// A card on the previous model (the column's default), due at `T0`.
    async fn card(&self, n: u128) {
        let term = Uuid::from_u128(0x100 + n).to_string();
        let created = "2026-11-09T09:00:00.000Z";
        sqlx::query(
            "INSERT INTO glossary_terms (id, topic_id, key, term, term_key, definition, avoid, \
             lesson_id, created_at, updated_at) VALUES (?, ?, ?, 'x', 'x', 'd', '[]', ?, ?, ?)",
        )
        .bind(&term)
        .bind(TOPIC)
        .bind(&term)
        .bind(LESSON)
        .bind(created)
        .bind(created)
        .execute(&self.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO review_cards (id, topic_id, lesson_id, term_id, front, back, \
             created_at, due) VALUES (?, ?, ?, ?, 'f', 'b', ?, ?)",
        )
        .bind(cid(n).to_string())
        .bind(TOPIC)
        .bind(LESSON)
        .bind(&term)
        .bind(created)
        .bind(iso(T0))
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn vote(&self, card: u128, rating: Rating, at: OffsetDateTime, count: u32) {
        *self.clock.0.lock().unwrap() = at;
        self.service
            .rate(
                UserId::OWNER,
                cid(card),
                Vote {
                    rating,
                    reviewed_at: Some(at),
                    review_count: count,
                },
            )
            .await
            .unwrap();
    }

    async fn row(&self, card: u128) -> Row {
        sqlx::query_as(
            "SELECT model, stability, difficulty, stability_fast, learning_step, state, due, \
             last_reviewed_at, review_count, lapses FROM review_cards WHERE id = ?",
        )
        .bind(cid(card).to_string())
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }
}

impl Env {
    /// Card `to` gets the votes and schedule of card `from`, stored the way the
    /// previous model stored them (snapshots without the new fields), plus a
    /// vote that was undone, and memory values of the previous model.
    async fn copy_as_legacy(
        &self,
        from: u128,
        to: u128,
        reference: &Row,
        last_vote: OffsetDateTime,
    ) {
        let at = last_vote;
        self.card(to).await;
        let logs: Vec<LogRow> = sqlx::query_as(
            "SELECT rating, reviewed_at, state_before, state_after, created_at, id \
         FROM review_logs WHERE card_id = ? ORDER BY rowid",
        )
        .bind(cid(from).to_string())
        .fetch_all(&self.pool)
        .await
        .unwrap();
        assert_eq!(logs.len(), 6);
        let legacy = |raw: &str| {
            let mut v: serde_json::Value = serde_json::from_str(raw).unwrap();
            for k in ["model", "stability_fast", "learning_step"] {
                v.as_object_mut().unwrap().remove(k);
            }
            v.to_string()
        };
        for (i, (rating, reviewed, before, after, created, _)) in logs.iter().enumerate() {
            for (suffix, undone) in [(0_u128, false), (1, true)] {
                // The undone vote sits in the middle of the real ones and must not count.
                if undone && i != 3 {
                    continue;
                }
                let (rating, reviewed) = if undone {
                    ("again", iso(at - Duration::days(40)))
                } else {
                    (rating.as_str(), reviewed.clone())
                };
                sqlx::query(
                    "INSERT INTO review_logs (id, card_id, rating, reviewed_at, state_before, \
                 state_after, created_at, undone_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(Uuid::from_u128(0x9000 + 0x1000 * to + 10 * i as u128 + suffix).to_string())
                .bind(cid(to).to_string())
                .bind(rating)
                .bind(reviewed)
                .bind(legacy(before))
                .bind(legacy(after))
                .bind(created)
                .bind(undone.then(|| iso(T0)))
                .execute(&self.pool)
                .await
                .unwrap();
            }
        }
        sqlx::query(
            "UPDATE review_cards SET state = ?, due = ?, last_reviewed_at = ?, review_count = ?, \
         lapses = ?, stability = 99, difficulty = 9, stability_fast = 0, learning_step = 1, \
         model = 'fsrs5' WHERE id = ?",
        )
        .bind(&reference.state)
        .bind(&reference.due)
        .bind(&reference.last_reviewed_at)
        .bind(reference.review_count)
        .bind(reference.lapses)
        .bind(cid(to).to_string())
        .execute(&self.pool)
        .await
        .unwrap();
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
struct Row {
    model: String,
    stability: f64,
    difficulty: f64,
    stability_fast: f64,
    learning_step: i64,
    state: String,
    due: String,
    last_reviewed_at: Option<String>,
    review_count: i64,
    lapses: i64,
}

type LogRow = (String, String, String, String, String, String);

#[tokio::test]
async fn a_card_on_the_previous_model_gets_the_memory_it_would_have_had_on_the_new_one() {
    let env = Env::new().await;
    // Card 1 is voted on FSRS-7 from the start: the reference.
    env.card(1).await;
    let plan = [
        (Rating::Good, Duration::ZERO),
        (Rating::Good, Duration::days(5)),
        (Rating::Hard, Duration::days(20)),
        (Rating::Again, Duration::days(30)),
        (Rating::Good, Duration::minutes(10)),
        (Rating::Easy, Duration::hours(26)),
    ];
    let mut at = T0;
    for (count, (rating, wait)) in plan.into_iter().enumerate() {
        at += wait;
        env.vote(1, rating, at, u32::try_from(count).unwrap()).await;
    }
    let reference = env.row(1).await;
    assert_eq!(reference.model, "fsrs7");

    env.copy_as_legacy(1, 2, &reference, at).await;
    let before = env.row(2).await;

    let report = env.migration.run().await.unwrap();
    assert_eq!((report.migrated, report.failed.len()), (1, 0));

    let after = env.row(2).await;
    // (a) The memory is the one of voting on FSRS-7 from the start.
    assert_eq!(after.model, "fsrs7");
    assert_eq!(after.stability, reference.stability);
    assert_eq!(after.difficulty, reference.difficulty);
    assert_eq!(after.stability_fast, reference.stability_fast);
    // (b) The schedule and the counts stay; the short-step index starts again.
    assert_eq!(
        (&after.state, &after.due, &after.last_reviewed_at),
        (&before.state, &before.due, &before.last_reviewed_at)
    );
    assert_eq!(
        (after.review_count, after.lapses, after.learning_step),
        (before.review_count, before.lapses, 0)
    );
    // The card that was on FSRS-7 already was not touched.
    assert_eq!(env.row(1).await, reference);

    // (c) A second run finds nothing to do and changes nothing.
    let again = env.migration.run().await.unwrap();
    assert_eq!((again.migrated, again.failed.len()), (0, 0));
    assert_eq!(env.row(2).await, after);

    // (f) The last vote of the migrated card cannot be undone (its snapshot is
    // of the previous model), where the one of the reference card can.
    *env.clock.0.lock().unwrap() = at + Duration::minutes(1);
    let refused = env.service.undo(UserId::OWNER, cid(2)).await;
    assert!(
        matches!(
            refused,
            Err(AppError::Conflict(ConflictReason::UndoExpired))
        ),
        "{refused:?}"
    );
    assert_eq!(env.row(2).await, after);
    env.service.undo(UserId::OWNER, cid(1)).await.unwrap();
}

#[tokio::test]
async fn a_card_without_votes_only_changes_its_model() {
    let env = Env::new().await;
    env.card(1).await;
    let before = env.row(1).await;
    assert_eq!(before.model, "fsrs5");

    let report = env.migration.run().await.unwrap();
    assert_eq!((report.migrated, report.failed.len()), (1, 0));
    let after = env.row(1).await;
    assert_eq!(after.model, "fsrs7");
    assert_eq!(
        Row {
            model: before.model.clone(),
            ..after.clone()
        },
        before
    );
    assert_eq!((after.stability, after.stability_fast), (0.0, 0.0));

    // A first vote on the migrated card works from a new card, and the run
    // that follows leaves it alone.
    env.vote(1, Rating::Good, T0 + Duration::days(1), 0).await;
    let voted = env.row(1).await;
    assert_eq!(env.migration.run().await.unwrap().migrated, 0);
    assert_eq!(env.row(1).await, voted);
}

#[tokio::test]
async fn a_vote_on_a_card_not_yet_migrated_gives_what_migrating_and_then_voting_gives() {
    let env = Env::new().await;
    env.card(1).await;
    let plan = [
        (Rating::Good, Duration::ZERO),
        (Rating::Good, Duration::days(5)),
        (Rating::Hard, Duration::days(20)),
        (Rating::Again, Duration::days(30)),
        (Rating::Good, Duration::minutes(10)),
        (Rating::Easy, Duration::hours(26)),
    ];
    let mut at = T0;
    for (count, (rating, wait)) in plan.into_iter().enumerate() {
        at += wait;
        env.vote(1, rating, at, u32::try_from(count).unwrap()).await;
    }
    let reference = env.row(1).await;
    // Two identical cards on the previous model, both with the votes of the
    // reference and previous-model memory values.
    env.copy_as_legacy(1, 2, &reference, at).await;
    env.copy_as_legacy(1, 3, &reference, at).await;
    assert_eq!(
        (env.row(2).await.model, env.row(3).await.model),
        ("fsrs5".into(), "fsrs5".into())
    );

    let next = at + Duration::days(3);
    let count = u32::try_from(reference.review_count).unwrap();
    // Card 3 is voted as it is: the vote replays its votes first.
    env.vote(3, Rating::Good, next, count).await;
    // Card 2 is migrated first, then voted.
    assert_eq!(env.migration.run().await.unwrap().migrated, 1);
    env.vote(2, Rating::Good, next, count).await;

    let (migrated_then_voted, voted_unmigrated) = (env.row(2).await, env.row(3).await);
    assert_eq!(voted_unmigrated.model, "fsrs7");
    // The same memory, short-step index, schedule and counts, whatever the
    // order: no card ever holds a mix of the two models.
    assert_eq!(voted_unmigrated, migrated_then_voted);
    // And the memory is the one of voting on the current model all along.
    env.vote(1, Rating::Good, next, count).await;
    let (stability, difficulty, fast) = {
        let r = env.row(1).await;
        (r.stability, r.difficulty, r.stability_fast)
    };
    assert_eq!(
        (
            voted_unmigrated.stability,
            voted_unmigrated.difficulty,
            voted_unmigrated.stability_fast
        ),
        (stability, difficulty, fast)
    );
}
