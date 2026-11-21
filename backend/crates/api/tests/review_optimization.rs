//! Personal weights (ADR 0014 points 5, 11, 12) on SQLite with the real
//! migrations, the real scheduler and the real optimizer. Nothing here calls
//! a model.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use application::{OptimizeReviewParameters, ReviewService, UserRun, Vote};
use domain::{
    Clock, IdGenerator, Rating, ReviewCardId, ReviewRepository, ReviewScheduler as _, UserId,
};
use infrastructure::sqlite::{self, Pool};
use infrastructure::{FsrsOptimizer, FsrsScheduler, SqliteReviewRepository};
use sqlx::Row;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const T0: OffsetDateTime = datetime!(2026-03-02 08:00:00.000 UTC);
const TOPIC: &str = "00000000-0000-0000-0000-0000000000a1";
const LESSON: &str = "00000000-0000-0000-0000-0000000000b1";
const CARDS: u128 = 40;
const VOTES_PER_CARD: u32 = 11;

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

struct Env {
    pool: Pool,
    clock: Arc<TestClock>,
    repo: Arc<SqliteReviewRepository>,
    service: ReviewService,
    optimization: OptimizeReviewParameters,
}

impl Env {
    async fn new() -> Self {
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        for q in [
            "INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) VALUES ('00000000-0000-0000-0000-00000000000f', '00000000-0000-7000-8000-000000000001', 'F', 'f', 0, '2026-03-01T09:00:00.000Z', '2026-03-01T09:00:00.000Z')",
            "INSERT INTO topics (id, user_id, folder_id, title, title_key, created_at, updated_at) VALUES ('00000000-0000-0000-0000-0000000000a1', '00000000-0000-7000-8000-000000000001', '00000000-0000-0000-0000-00000000000f', 'Rust', 'rust', '2026-03-01T09:00:00.000Z', '2026-03-01T09:00:00.000Z')",
            "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, catalog_version, mission_version, document, plan, generation_job_id, created_at) VALUES ('00000000-0000-0000-0000-0000000000b1', '00000000-0000-0000-0000-0000000000a1', 1, 'L', 'o', 5, 1, 1, '{}', '{}', 'job1', '2026-03-01T09:00:00.000Z')",
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
            optimization: OptimizeReviewParameters::new(
                repo.clone(),
                repo.clone(),
                scheduler,
                Arc::new(FsrsOptimizer::new()),
                clock.clone(),
            ),
            pool,
            clock,
            repo,
        }
    }

    async fn card(&self, n: u128) {
        let term = Uuid::from_u128(0x100 + n).to_string();
        let created = "2026-03-01T09:00:00.000Z";
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
             created_at, due, model) VALUES (?, ?, ?, ?, 'f', 'b', ?, ?, 'fsrs7')",
        )
        .bind(cid(n).to_string())
        .bind(TOPIC)
        .bind(LESSON)
        .bind(&term)
        .bind(created)
        .bind(api::health::format_instant(T0))
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

    /// 40 cards of 11 votes: 400 useful ones, plus a vote that is undone.
    async fn history(&self) -> OffsetDateTime {
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut last = T0;
        for card in 1..=CARDS {
            self.card(card).await;
            let mut at = T0 + Duration::hours(i64::try_from(card).unwrap() * 5);
            for k in 0..VOTES_PER_CARD {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let rating = match (seed >> 33) % 10 {
                    0 => Rating::Again,
                    1 | 2 => Rating::Hard,
                    3..=7 => Rating::Good,
                    _ => Rating::Easy,
                };
                self.vote(card, rating, at, k).await;
                last = last.max(at);
                at += match k {
                    0 => Duration::minutes(1),
                    1 => Duration::minutes(10),
                    _ => {
                        Duration::days(1 + i64::from(k) * i64::try_from((seed >> 40) % 9).unwrap())
                    }
                };
            }
        }
        // One more vote on card 1, undone at once: not a useful vote.
        let at = last + Duration::days(1);
        self.vote(1, Rating::Good, at, VOTES_PER_CARD).await;
        self.service.undo(UserId::OWNER, cid(1)).await.unwrap();
        at
    }

    async fn count(&self, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(&self.pool).await.unwrap()
    }
}

// One scenario on one database; stored values are compared as written.
#[allow(clippy::too_many_lines, clippy::float_cmp)]
#[tokio::test]
async fn an_optimization_on_real_tables_records_the_attempt_and_changes_only_the_memory() {
    let e = Env::new().await;
    let end = e.history().await;
    e.clock
        .0
        .lock()
        .unwrap()
        .clone_from(&(end + Duration::days(1)));

    let cards_before = sqlx::query(
        "SELECT id, due, state, learning_step, last_reviewed_at, review_count, lapses, stability \
         FROM review_cards ORDER BY id",
    )
    .fetch_all(&e.pool)
    .await
    .unwrap();

    let results = e.optimization.run_once().await.unwrap();
    assert_eq!(results.len(), 1);
    let UserRun::Attempted(outcome) = *results[0].1.as_ref().unwrap() else {
        panic!("the user has 400 useful votes");
    };

    let row = sqlx::query(
        "SELECT useful_votes, train_votes, test_votes, current_log_loss, candidate_log_loss, \
         outcome, error FROM review_optimization_attempts",
    )
    .fetch_one(&e.pool)
    .await
    .unwrap();
    // 40 cards x 10 votes; the undone vote is not counted.
    assert_eq!(row.get::<i64, _>("useful_votes"), 400);
    assert_eq!(row.get::<i64, _>("train_votes"), 320);
    assert_eq!(row.get::<i64, _>("test_votes"), 80);
    assert!(row.get::<f64, _>("current_log_loss").is_finite());
    assert!(row.get::<f64, _>("candidate_log_loss").is_finite());
    assert_eq!(row.get::<String, _>("outcome"), outcome.as_str());
    assert_eq!(row.get::<Option<String>, _>("error"), None);

    let stored = e.repo.parameters(UserId::OWNER).await.unwrap();
    let cards_after = sqlx::query(
        "SELECT id, due, state, learning_step, last_reviewed_at, review_count, lapses, stability \
         FROM review_cards ORDER BY id",
    )
    .fetch_all(&e.pool)
    .await
    .unwrap();
    for (a, b) in cards_before.iter().zip(&cards_after) {
        for column in ["due", "state", "last_reviewed_at"] {
            assert_eq!(
                a.get::<Option<String>, _>(column),
                b.get::<Option<String>, _>(column)
            );
        }
        for column in ["learning_step", "review_count", "lapses"] {
            assert_eq!(a.get::<i64, _>(column), b.get::<i64, _>(column));
        }
    }
    if outcome == domain::OptimizationOutcome::Adopted {
        let weights = stored.unwrap();
        assert_eq!(weights.parameters.model, "fsrs7");
        assert_eq!(weights.parameters.weights.len(), 34);
        // Every card carries the stamp of the weights it was rebuilt with, so
        // a rebuild that stops halfway is finished by the next pass.
        let stamps: Vec<Option<String>> =
            sqlx::query_scalar("SELECT DISTINCT weights_adopted_at FROM review_cards")
                .fetch_all(&e.pool)
                .await
                .unwrap();
        assert_eq!(
            stamps,
            [Some(api::health::format_instant(weights.adopted_at))]
        );
        let changed = cards_before
            .iter()
            .zip(&cards_after)
            .any(|(a, b)| a.get::<f64, _>("stability") != b.get::<f64, _>("stability"));
        assert!(changed, "the memory was rebuilt with the new weights");
    } else {
        assert_eq!(stored, None);
        assert_eq!(
            e.count("SELECT COUNT(*) FROM review_cards WHERE weights_adopted_at IS NOT NULL")
                .await,
            0
        );
        assert_eq!(
            cards_before
                .iter()
                .zip(&cards_after)
                .filter(|(a, b)| a.get::<f64, _>("stability") != b.get::<f64, _>("stability"))
                .count(),
            0
        );
    }

    // The service reads the user's weights: with a row in place a vote and a
    // preview still work, and a row it cannot use falls back without failing.
    sqlx::query(
        "INSERT INTO user_review_parameters (user_id, model, weights, adopted_at) \
         VALUES (?, 'fsrs7', '[1.0, 2.0]', '2026-04-01T00:00:00.000Z') \
         ON CONFLICT (user_id) DO UPDATE SET weights = excluded.weights",
    )
    .bind(UserId::OWNER.to_string())
    .execute(&e.pool)
    .await
    .unwrap();
    let due = e
        .service
        .due(
            UserId::OWNER,
            None,
            domain::PageRequest {
                after: None,
                limit: 5,
            },
        )
        .await
        .unwrap();
    assert!(due.page.items.iter().all(|i| i.intervals_preview.is_some()));
    sqlx::query("UPDATE user_review_parameters SET weights = 'not json'")
        .execute(&e.pool)
        .await
        .unwrap();
    assert_eq!(e.repo.parameters(UserId::OWNER).await.unwrap(), None);

    // A second pass right away does nothing: 30 days have not passed.
    let again = e.optimization.run_once().await.unwrap();
    assert_eq!(*again[0].1.as_ref().unwrap(), UserRun::NotEligible);
    assert_eq!(
        e.count("SELECT COUNT(*) FROM review_optimization_attempts")
            .await,
        1
    );

    // Whatever the attempt came to: weights saved for the user (here by hand,
    // so that this does not depend on the training) bring every card to them
    // at the next pass, though the user is not eligible for an attempt.
    let mut saved = FsrsScheduler::new().default_parameters().weights;
    saved[0] *= 2.0;
    sqlx::query(
        "UPDATE user_review_parameters SET weights = ?, adopted_at = '2026-12-01T00:00:00.000Z'",
    )
    .bind(serde_json::to_string(&saved).unwrap())
    .execute(&e.pool)
    .await
    .unwrap();
    let rebuild = e.optimization.run_once().await.unwrap();
    assert_eq!(*rebuild[0].1.as_ref().unwrap(), UserRun::NotEligible);
    let stamps: Vec<Option<String>> =
        sqlx::query_scalar("SELECT DISTINCT weights_adopted_at FROM review_cards")
            .fetch_all(&e.pool)
            .await
            .unwrap();
    assert_eq!(stamps, [Some("2026-12-01T00:00:00.000Z".to_owned())]);

    // Deleting the user takes the weights and the attempts with them.
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(UserId::OWNER.to_string())
        .execute(&e.pool)
        .await
        .unwrap();
    assert_eq!(
        e.count("SELECT COUNT(*) FROM review_optimization_attempts")
            .await,
        0
    );
    assert_eq!(
        e.count("SELECT COUNT(*) FROM user_review_parameters").await,
        0
    );
}
