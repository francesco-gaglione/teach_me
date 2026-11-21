//! End-to-end tests of the FSRS review (`docs/contracts/reviews.md`) over the
//! real router, SQLite and scheduler, with a clock the test moves day by day.
//! Nothing here calls a model.

mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use api::app::{Limits, build_router};
use api::state::{AppOptions, AppState};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::ScriptedLlm;
use domain::{
    CardTarget, Clock, CompletionSave, GlossaryTermId, IdGenerator, LearningRecordId,
    LearningRepository, LessonId, NewCard, NewRecord, RecordKind, ReviewCardId, TermUpsert,
    TopicId,
};
use http_body_util::BodyExt;
use infrastructure::SqliteLearningRepository;
use infrastructure::sqlite::{self, Pool};
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = common::TOKEN;
const T0: OffsetDateTime = datetime!(2026-11-09 15:00:00.000 UTC);

struct TestClock(Mutex<OffsetDateTime>);
impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        *self.0.lock().unwrap()
    }
}

struct SeqIds(AtomicU64);
impl IdGenerator for SeqIds {
    fn new_id(&self) -> Uuid {
        Uuid::from_u128(
            0x0193_3b4e_0000_7000_8000_0000_0000_0000
                + u128::from(self.0.fetch_add(1, Ordering::SeqCst)),
        )
    }
}

struct Res {
    status: StatusCode,
    body: Value,
}

struct App {
    router: Router,
    pool: Pool,
    clock: Arc<TestClock>,
    topic: TopicId,
}

fn uid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

impl App {
    async fn new() -> Self {
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        let clock = Arc::new(TestClock(Mutex::new(T0)));
        let state = AppState::new(
            clock.clone(),
            Arc::new(SeqIds(AtomicU64::new(0))),
            common::auth(),
            &pool,
            ScriptedLlm::with(vec![]),
            AppOptions::default(),
        );
        common::sign_in_owner(&state, &pool).await;
        let mut app = Self {
            router: build_router(state, Limits::default()),
            pool,
            clock,
            topic: TopicId::new(uid(0)),
        };
        let f = app
            .post("/api/v1/folders", json!({ "name": "Studio" }))
            .await;
        let folder = f.body["id"].as_str().unwrap().to_owned();
        let t = app
            .post(
                &format!("/api/v1/folders/{folder}/topics"),
                json!({ "title": "Fotografia" }),
            )
            .await;
        app.topic = TopicId::new(Uuid::parse_str(t.body["id"].as_str().unwrap()).unwrap());
        app
    }

    fn set_clock(&self, at: OffsetDateTime) {
        *self.clock.0.lock().unwrap() = at;
    }

    fn advance(&self, by: Duration) {
        let now = self.clock.now();
        self.set_clock(now + by);
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Res {
        // The clock jumps by months here, past the sliding session lifetime;
        // the sessions are not what these tests are about.
        sqlx::query("UPDATE sessions SET expires_at = '2099-01-01T00:00:00.000Z'")
            .execute(&self.pool)
            .await
            .unwrap();
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
        let body = match body {
            Some(v) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let res = self
            .router
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let (parts, body) = res.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        Res {
            status: parts.status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }

    async fn get(&self, path: &str) -> Res {
        self.call(Method::GET, path, None).await
    }

    async fn post(&self, path: &str, body: Value) -> Res {
        self.call(Method::POST, path, Some(body)).await
    }

    /// What the completion of lesson `n` stores: one record card (`rec{n}`)
    /// and one term card (`term{n}`), created at the current instant.
    async fn complete_lesson(&self, n: u128, supersedes: Vec<u32>) {
        let lesson = LessonId::new(uid(0x1000 + n));
        let at = self.clock.now();
        sqlx::query(
            "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, \
             catalog_version, mission_version, document, plan, generation_job_id, created_at) \
             VALUES (?, ?, ?, ?, 'o', 5, 1, 1, '{}', '{}', ?, '2026-11-09T09:00:00.000Z')",
        )
        .bind(lesson.to_string())
        .bind(self.topic.to_string())
        .bind(i64::try_from(n).unwrap())
        .bind(format!("Lezione {n}"))
        .bind(uid(0x2000 + n).to_string())
        .execute(&self.pool)
        .await
        .unwrap();
        SqliteLearningRepository::new(self.pool.clone())
            .save_completion(&CompletionSave {
                topic_id: self.topic,
                lesson_id: lesson,
                at,
                records: vec![NewRecord {
                    id: LearningRecordId::new(uid(0x3000 + n)),
                    kind: RecordKind::Demonstrated,
                    title: format!("Record {n}"),
                    body: "b".into(),
                    evidence: "e".into(),
                    supersedes,
                }],
                terms: vec![TermUpsert {
                    id: GlossaryTermId::new(uid(0x4000 + n)),
                    key: format!("term{n}"),
                    term: format!("Termine {n}"),
                    definition: "d".into(),
                    avoid: vec![],
                }],
                cards: vec![
                    NewCard {
                        id: ReviewCardId::new(uid(0x5000 + n)),
                        target: CardTarget::Record(0),
                        front: format!("rec{n}"),
                        back: "b".into(),
                        unpassed_block: None,
                    },
                    NewCard {
                        id: ReviewCardId::new(uid(0x6000 + n)),
                        target: CardTarget::Term(0),
                        front: format!("term{n}"),
                        back: "b".into(),
                        unpassed_block: None,
                    },
                ],
            })
            .await
            .unwrap();
    }

    async fn due(&self) -> Value {
        let r = self.get("/api/v1/reviews/due").await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        r.body
    }

    /// The card as `GET /reviews/due` shows it, after moving the clock to its
    /// due date if it is not due yet.
    async fn card_when_due(&self, card: u128) -> Value {
        let find = |d: Value| {
            d["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == uid(card).to_string())
                .cloned()
        };
        if let Some(c) = find(self.due().await) {
            return c;
        }
        // Not due yet: it is the soonest one, and the list says when.
        let next = self.due().await["next_due_at"].as_str().unwrap().to_owned();
        self.set_clock(OffsetDateTime::parse(&next, &Rfc3339).unwrap());
        find(self.due().await).expect("due at its due date")
    }

    async fn vote(&self, card: u128, rating: &str, count: u32) -> Res {
        self.post(
            &format!("/api/v1/reviews/{}", uid(card)),
            json!({ "rating": rating, "review_count": count, "reviewed_at": iso(self.clock.now()) }),
        )
        .await
    }
}

fn iso(at: OffsetDateTime) -> String {
    api::health::format_instant(at)
}

fn secs(d: &Value, field: &str) -> i64 {
    d[field].as_i64().unwrap()
}

fn due_of(card: &Value) -> OffsetDateTime {
    OffsetDateTime::parse(card["due"].as_str().unwrap(), &Rfc3339).unwrap()
}

const REC1: u128 = 0x5001;
const TERM1: u128 = 0x6001;

#[tokio::test]
async fn a_card_from_a_lesson_is_due_after_a_day_and_a_vote_moves_it_by_the_previewed_interval() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;

    // Right after the lesson: nothing to review, the next card is tomorrow's.
    let d = app.due().await;
    assert_eq!(d["items"], json!([]));
    assert_eq!(d["due_count"], 0);
    assert_eq!(d["estimated_minutes"], 0);
    assert_eq!(d["next_due_at"], iso(T0 + Duration::hours(24)));
    assert_eq!(d["next_cursor"], Value::Null);

    app.advance(Duration::hours(24) - Duration::seconds(1));
    assert_eq!(app.due().await["due_count"], 0);

    app.advance(Duration::seconds(1));
    let d = app.due().await;
    assert_eq!(d["due_count"], 2);
    assert_eq!(d["estimated_minutes"], 1);
    assert_eq!(d["next_due_at"], Value::Null);
    let card = &d["items"][0];
    assert_eq!(card["id"], uid(REC1).to_string());
    assert_eq!(card["topic_title"], "Fotografia");
    assert_eq!(card["lesson_title"], "Lezione 1");
    assert_eq!(card["lesson_number"], 1);
    assert_eq!(card["kind"], "record");
    assert_eq!(d["items"][1]["kind"], "term");
    assert_eq!(card["status"], "active");
    assert_eq!(card["state"], "new");
    assert_eq!(card["review_count"], 0);
    assert_eq!(card["last_reviewed_at"], Value::Null);
    // New card (FSRS-7, default weights): a minute for "again", then the
    // model's own fractional days (0.5975, 4.7777, 53.87).
    let p = &card["intervals_preview"];
    let in_days = |rating| Duration::seconds(secs(p, rating)).as_seconds_f64() / 86_400.0;
    assert_eq!(secs(p, "again"), 60);
    for (rating, days) in [("hard", 0.5975), ("good", 4.7777), ("easy", 53.8694)] {
        assert!((in_days(rating) - days).abs() < 1e-3, "{rating}");
    }

    // "good": due exactly as announced, a number of days away that is not whole.
    let announced = secs(p, "good");
    let now = app.clock.now();
    let r = app.vote(REC1, "good", 0).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    // The API shows whole seconds; the due date itself is exact to the millisecond.
    assert_eq!((due_of(&r.body) - now).whole_seconds(), announced);
    assert_ne!(announced % 86_400, 0);
    assert_eq!(r.body["state"], "review");
    assert_eq!(r.body["review_count"], 1);
    assert_eq!(r.body["last_reviewed_at"], iso(now));
    assert_eq!(app.due().await["due_count"], 1);

    // Ten minutes later it is still not due; at its due date it is, and
    // "again" now means a step of at most a day.
    app.advance(Duration::minutes(10));
    assert_eq!(app.due().await["due_count"], 1);
    let card = app.card_when_due(REC1).await;
    assert!(secs(&card["intervals_preview"], "good") >= 86_400);
    let again = secs(&card["intervals_preview"], "again");
    assert!((600..=86_400).contains(&again), "{again} s");
}

#[tokio::test]
async fn every_vote_applies_the_interval_its_preview_announced() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.advance(Duration::hours(24));

    // Walk one card through learning, review and a lapse; at every step the
    // four previews are what the four possible votes would do (checked on
    // copies of the card that are voted once each, then the real path goes on).
    let path = ["good", "good", "good", "again", "good", "easy"];
    for (count, rating) in path.into_iter().enumerate() {
        let count = u32::try_from(count).unwrap();
        let card = app.card_when_due(REC1).await;
        assert_eq!(card["review_count"], count);
        let now = app.clock.now();
        let announced = secs(&card["intervals_preview"], rating);
        let r = app.vote(REC1, rating, count).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        assert_eq!(
            (due_of(&r.body) - now).whole_seconds(),
            announced,
            "{rating}"
        );
    }
}

#[tokio::test]
async fn again_brings_a_reviewed_card_back_after_the_relearning_step() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.advance(Duration::hours(24));
    // Easy: straight to "review" with a long interval.
    let r = app.vote(REC1, "easy", 0).await;
    assert_eq!(r.body["state"], "review");
    let card = app.card_when_due(REC1).await;
    // max(10 min step, min(model, 1 day)): between ten minutes and a day, and
    // exactly what the preview announced.
    let announced = secs(&card["intervals_preview"], "again");
    assert!((600..=86_400).contains(&announced), "{announced} s");
    let r = app.vote(REC1, "again", 1).await;
    assert_eq!(r.body["state"], "relearning");
    assert_eq!(
        (due_of(&r.body) - app.clock.now()).whole_seconds(),
        announced
    );
}

#[tokio::test]
async fn conflicts_validation_and_unknown_cards() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.advance(Duration::hours(24));

    assert_eq!(app.vote(REC1, "good", 0).await.status, StatusCode::OK);
    // The same vote sent again by the offline queue: the app's version is old.
    let r = app.vote(REC1, "good", 0).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.body["error"]["code"], "already_reviewed");
    let r = app.vote(REC1, "good", 7).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.body["error"]["code"], "already_reviewed");

    let s = app
        .post(
            &format!("/api/v1/reviews/{}/suspend", uid(TERM1)),
            json!({}),
        )
        .await;
    assert_eq!(s.status, StatusCode::OK);
    assert_eq!(s.body["status"], "suspended");
    assert_eq!(s.body["intervals_preview"], Value::Null);
    let again = app
        .post(
            &format!("/api/v1/reviews/{}/suspend", uid(TERM1)),
            json!({}),
        )
        .await;
    assert_eq!((again.status, &again.body), (StatusCode::OK, &s.body));
    // Suspended wins over a stale version.
    for count in [0, 5] {
        let r = app.vote(TERM1, "good", count).await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert_eq!(r.body["error"]["code"], "card_suspended");
    }
    // Suspended cards are never due.
    let d = app.due().await;
    assert!(
        d["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != uid(TERM1).to_string())
    );

    let bad = |body: Value| {
        let app = &app;
        async move {
            app.post(&format!("/api/v1/reviews/{}", uid(REC1)), body)
                .await
        }
    };
    let r = bad(json!({ "rating": "ok", "review_count": 1 })).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.body["error"]["details"][0]["field"], "rating");
    let r = bad(json!({ "rating": "good" })).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.body["error"]["details"][0]["field"], "review_count");
    let r = bad(json!({ "rating": "good", "review_count": 1, "reviewed_at": "ieri" })).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);

    assert_eq!(
        app.vote(0x9999, "good", 0).await.status,
        StatusCode::NOT_FOUND
    );
    for path in ["", "/undo", "/suspend", "/unsuspend"] {
        let r = app
            .post(
                &format!("/api/v1/reviews/not-a-uuid{path}"),
                json!({ "rating": "good", "review_count": 0 }),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(r.body["error"]["code"], "invalid_id");
    }
    assert_eq!(
        app.post(&format!("/api/v1/reviews/{}/undo", uid(0x9999)), json!({}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.get("/api/v1/reviews/due?limit=0").await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.get("/api/v1/reviews/due?cursor=@@").await.status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn reviewed_at_from_the_offline_queue_is_kept_within_bounds() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.advance(Duration::hours(24));
    let now = app.clock.now();
    let at = |d: Duration| iso(now + d);
    // A new card's intervals do not depend on when it is voted.
    let good = secs(&app.card_when_due(REC1).await["intervals_preview"], "good");

    // Voted two minutes ago on the phone, sent now.
    let r = app
        .post(
            &format!("/api/v1/reviews/{}", uid(REC1)),
            json!({ "rating": "good", "review_count": 0, "reviewed_at": at(-Duration::minutes(2)) }),
        )
        .await;
    assert_eq!(r.body["last_reviewed_at"], at(-Duration::minutes(2)));
    assert_eq!(
        (due_of(&r.body) - (now - Duration::minutes(2))).whole_seconds(),
        good
    );

    // A phone clock one hour ahead: capped at the server's now.
    let r = app
        .post(
            &format!("/api/v1/reviews/{}", uid(REC1)),
            json!({ "rating": "good", "review_count": 1, "reviewed_at": at(Duration::hours(1)) }),
        )
        .await;
    assert_eq!(r.body["last_reviewed_at"], at(Duration::ZERO));

    // A time before the previous vote is raised to it.
    app.advance(Duration::hours(1));
    let r = app
        .post(
            &format!("/api/v1/reviews/{}", uid(REC1)),
            json!({ "rating": "good", "review_count": 2, "reviewed_at": at(-Duration::days(3)) }),
        )
        .await;
    assert_eq!(r.body["last_reviewed_at"], at(Duration::ZERO));
}

#[tokio::test]
async fn undo_restores_the_card_once_and_only_for_ten_minutes() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.advance(Duration::hours(24));
    let undo = |card: u128| {
        let app = &app;
        async move {
            app.post(&format!("/api/v1/reviews/{}/undo", uid(card)), json!({}))
                .await
        }
    };
    let original = app.due().await["items"][0].clone();

    // Nothing voted yet.
    let r = undo(REC1).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.body["error"]["code"], "nothing_to_undo");

    app.vote(REC1, "easy", 0).await;
    app.advance(Duration::minutes(9));
    let r = undo(REC1).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["state"], "new");
    assert_eq!(r.body["review_count"], 0);
    assert_eq!(r.body["last_reviewed_at"], Value::Null);
    assert_eq!(r.body["due"], original["due"]);
    // Back among the due cards, same as before the vote.
    let back = app.due().await["items"][0].clone();
    assert_eq!(back["id"], original["id"]);
    assert_eq!(back["due"], original["due"]);

    // A second undo does not reach for any earlier vote.
    let r = undo(REC1).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.body["error"]["code"], "nothing_to_undo");

    // The vote can be given again, and after 11 minutes it is final.
    assert_eq!(app.vote(REC1, "good", 0).await.status, StatusCode::OK);
    app.advance(Duration::minutes(11));
    let r = undo(REC1).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.body["error"]["code"], "undo_expired");
    let log: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM review_logs WHERE undone_at IS NULL")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(log, 1);
}

#[tokio::test]
async fn a_superseded_record_takes_its_card_out_of_the_review() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.advance(Duration::days(2));
    assert_eq!(app.due().await["due_count"], 2);

    // Lesson 2 deepens record #1: its card is suspended, the term's stays.
    app.complete_lesson(2, vec![1]).await;
    let d = app.due().await;
    assert_eq!(d["due_count"], 1);
    assert_eq!(d["items"][0]["id"], uid(TERM1).to_string());
    // The new cards of lesson 2 wait their day.
    assert_eq!(d["next_due_at"], iso(app.clock.now() + Duration::hours(24)));
}

#[tokio::test]
async fn the_due_list_pages_by_due_date() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.advance(Duration::hours(1));
    app.complete_lesson(2, vec![]).await;
    app.advance(Duration::days(3));

    let first = app.get("/api/v1/reviews/due?limit=3").await.body;
    assert_eq!(first["due_count"], 4);
    assert_eq!(first["estimated_minutes"], 1);
    assert_eq!(first["items"].as_array().unwrap().len(), 3);
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let second = app
        .get(&format!("/api/v1/reviews/due?limit=3&cursor={cursor}"))
        .await
        .body;
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["next_cursor"], Value::Null);
    let ids: Vec<_> = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["items"].as_array().unwrap())
        .map(|c| c["lesson_number"].as_u64().unwrap())
        .collect();
    assert_eq!(ids, [1, 1, 2, 2]);
}

#[tokio::test]
async fn the_due_list_can_be_limited_to_a_topic() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.advance(Duration::days(3));
    let topic = app.topic.to_string();

    let all = app.get("/api/v1/reviews/due").await.body;
    let filtered = app
        .get(&format!("/api/v1/reviews/due?topic_id={topic}"))
        .await
        .body;
    assert_eq!(filtered["due_count"], 2);
    assert_eq!(
        all["topics"],
        json!([{ "topic_id": topic, "topic_title": "Fotografia", "due_count": 2 }])
    );
    // `topics` ignores the filter.
    assert_eq!(filtered["topics"], all["topics"]);

    // A topic nobody owns, a malformed one.
    let r = app
        .get(&format!("/api/v1/reviews/due?topic_id={}", uid(0xdead)))
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = app.get("/api/v1/reviews/due?topic_id=nope").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.body["error"]["code"], "bad_request");

    // A cursor is only good with the filter it came with.
    let page = app
        .get(&format!("/api/v1/reviews/due?limit=1&topic_id={topic}"))
        .await
        .body;
    let cursor = page["next_cursor"].as_str().unwrap().to_owned();
    let ok = app
        .get(&format!(
            "/api/v1/reviews/due?limit=1&topic_id={topic}&cursor={cursor}"
        ))
        .await;
    assert_eq!(ok.status, StatusCode::OK);
    let r = app
        .get(&format!("/api/v1/reviews/due?limit=1&cursor={cursor}"))
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let unfiltered = app.get("/api/v1/reviews/due?limit=1").await.body;
    let plain = unfiltered["next_cursor"].as_str().unwrap();
    let r = app
        .get(&format!(
            "/api/v1/reviews/due?topic_id={topic}&cursor={plain}"
        ))
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // Nothing due: empty topics.
    let early = App::new().await;
    assert_eq!(
        early.get("/api/v1/reviews/due").await.body["topics"],
        json!([])
    );
}

#[tokio::test]
async fn a_topic_of_another_user_is_not_found_and_not_counted() {
    let mut app = App::new().await;
    let mine = app.topic;
    // A second user with a folder and a topic of its own, with due cards.
    let other = TopicId::new(uid(0xb3));
    for sql in [
        "INSERT INTO users (id, username, display_name, created_at) \
         VALUES ('00000000-0000-7000-8000-0000000000b2', 'other', 'Other', '2026-01-01T00:00:00.000Z')",
        "INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) \
         VALUES ('00000000-0000-0000-0000-0000000000b1', '00000000-0000-7000-8000-0000000000b2', \
         'Suo', 'suo', 0, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
    ] {
        sqlx::query(sql).execute(&app.pool).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO topics (id, user_id, folder_id, title, title_key, state, created_at, updated_at) \
         VALUES (?, '00000000-0000-7000-8000-0000000000b2', '00000000-0000-0000-0000-0000000000b1', \
         'Segreto', 'segreto', 'ready', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
    )
    .bind(other.to_string())
    .execute(&app.pool)
    .await
    .unwrap();
    app.topic = other;
    app.complete_lesson(1, vec![]).await;
    app.topic = mine;
    app.complete_lesson(2, vec![]).await;
    app.advance(Duration::days(3));

    let r = app
        .get(&format!("/api/v1/reviews/due?topic_id={other}"))
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let all = app.get("/api/v1/reviews/due").await.body;
    assert_eq!(all["due_count"], 2);
    assert_eq!(
        all["topics"],
        json!([{ "topic_id": mine.to_string(), "topic_title": "Fotografia", "due_count": 2 }])
    );
}

fn card_ids(list: &Value) -> Vec<String> {
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn suspending_by_hand_is_reversible_and_leaves_the_schedule_alone() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.advance(Duration::hours(24));
    let suspend = format!("/api/v1/reviews/{}/suspend", uid(TERM1));
    let unsuspend = format!("/api/v1/reviews/{}/unsuspend", uid(TERM1));
    let active = app.card_when_due(TERM1).await;
    assert_eq!(
        (&active["suspension"], &active["suspended_at"]),
        (&Value::Null, &Value::Null)
    );

    let at = app.clock.now();
    let s = app.post(&suspend, json!({})).await;
    assert_eq!(s.body["suspension"], "user");
    assert_eq!(s.body["suspended_at"], iso(at));
    // Idempotent: reason and instant are those of the first call.
    app.advance(Duration::hours(5));
    let again = app.post(&suspend, json!({})).await;
    assert_eq!(again.body, s.body);

    let back = app.post(&unsuspend, json!({})).await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.body);
    assert_eq!(back.body["status"], "active");
    assert_eq!(back.body["suspension"], Value::Null);
    assert_eq!(back.body["suspended_at"], Value::Null);
    for field in ["state", "due", "review_count", "last_reviewed_at"] {
        assert_eq!(back.body[field], active[field], "{field}");
    }
    assert!(back.body["intervals_preview"].is_object());
    // Already active: nothing changes.
    let twice = app.post(&unsuspend, json!({})).await;
    assert_eq!((twice.status, &twice.body), (StatusCode::OK, &back.body));
    // Overdue and back in the queue at once.
    assert!(card_ids(&app.due().await).contains(&uid(TERM1).to_string()));
    assert_eq!(
        app.post(
            &format!("/api/v1/reviews/{}/unsuspend", uid(0x9999)),
            json!({})
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_superseded_card_cannot_be_reactivated_even_if_the_learner_had_suspended_it() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.advance(Duration::hours(1));
    let rec = format!("/api/v1/reviews/{}", uid(REC1));
    let hand = app.post(&format!("{rec}/suspend"), json!({})).await;
    assert_eq!(hand.body["suspension"], "user");

    app.advance(Duration::hours(1));
    let superseded_at = app.clock.now();
    app.complete_lesson(2, vec![1]).await;
    let list = app.get("/api/v1/reviews/suspended").await.body;
    assert_eq!(list["items"], json!([]), "superseded cards are not listed");
    assert_eq!(list["suspended_count"], 0);

    let r = app.post(&format!("{rec}/unsuspend"), json!({})).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.body["error"]["code"], "card_superseded");
    // A suspend does not turn it into a hand-made one.
    let s = app.post(&format!("{rec}/suspend"), json!({})).await;
    assert_eq!(s.body["suspension"], "superseded");
    assert_eq!(s.body["suspended_at"], iso(superseded_at));
}

#[tokio::test]
async fn the_suspended_list_is_newest_first_paged_and_counted() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    app.complete_lesson(2, vec![]).await;
    let empty = app.get("/api/v1/reviews/suspended").await;
    assert_eq!(empty.status, StatusCode::OK);
    assert_eq!(
        empty.body,
        json!({ "items": [], "next_cursor": null, "suspended_count": 0 })
    );

    // 0x5001 first, then the other three at the same instant.
    let order = [0x5001, 0x5002, 0x6001, 0x6002];
    for (i, card) in order.into_iter().enumerate() {
        if i == 1 {
            app.advance(Duration::minutes(10));
        }
        app.post(&format!("/api/v1/reviews/{}/suspend", uid(card)), json!({}))
            .await;
    }
    let first = app.get("/api/v1/reviews/suspended?limit=2").await.body;
    assert_eq!(first["suspended_count"], 4);
    assert_eq!(
        card_ids(&first),
        [uid(0x5002), uid(0x6001)].map(|u| u.to_string())
    );
    assert!(
        first["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["intervals_preview"].is_null() && c["suspension"] == "user")
    );
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let second = app
        .get(&format!(
            "/api/v1/reviews/suspended?limit=2&cursor={cursor}"
        ))
        .await
        .body;
    assert_eq!(
        card_ids(&second),
        [uid(0x6002), uid(0x5001)].map(|u| u.to_string())
    );
    assert_eq!(second["next_cursor"], Value::Null);
    assert_eq!(second["suspended_count"], 4);

    // Reactivating takes a card off the list.
    app.post(
        &format!("/api/v1/reviews/{}/unsuspend", uid(0x5002)),
        json!({}),
    )
    .await;
    let after = app.get("/api/v1/reviews/suspended?limit=1").await.body;
    assert_eq!(after["suspended_count"], 3);
    assert_eq!(after["items"].as_array().unwrap().len(), 1);

    for bad in ["?limit=0", "?cursor=@@"] {
        let r = app.get(&format!("/api/v1/reviews/suspended{bad}")).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    // A cursor of the due list is not one of this list.
    let due_cursor = {
        app.post(
            &format!("/api/v1/reviews/{}/unsuspend", uid(0x6001)),
            json!({}),
        )
        .await;
        app.advance(Duration::days(3));
        app.get("/api/v1/reviews/due?limit=1").await.body["next_cursor"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(
        app.get(&format!("/api/v1/reviews/suspended?cursor={due_cursor}"))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn a_card_born_unpassed_is_to_consolidate_until_two_good_votes_and_again_after_an_undo() {
    let app = App::new().await;
    app.complete_lesson(1, vec![]).await;
    sqlx::query("UPDATE review_cards SET unpassed_block = 'q1' WHERE id = ?")
        .bind(uid(REC1).to_string())
        .execute(&app.pool)
        .await
        .unwrap();
    app.advance(Duration::hours(24));
    let flags = |list: &Value| -> Vec<bool> {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["to_consolidate"].as_bool().unwrap())
            .collect()
    };
    // The record card is the flagged one; the term card never is.
    assert_eq!(flags(&app.due().await), [true, false]);

    let r = app.vote(REC1, "good", 0).await;
    assert_eq!(r.body["to_consolidate"], true);
    let r = app.vote(REC1, "good", 1).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["to_consolidate"], false);
    // A worse vote afterwards does not bring it back.
    let r = app.vote(REC1, "again", 2).await;
    assert_eq!(r.body["to_consolidate"], false);

    // Undoing the "again" keeps it normal; the vote before it can no longer
    // be undone (one step only), so use a fresh card for the return trip.
    let undo = |card: u128| {
        let app = &app;
        async move {
            app.post(&format!("/api/v1/reviews/{}/undo", uid(card)), json!({}))
                .await
        }
    };
    assert_eq!(undo(REC1).await.body["to_consolidate"], false);

    app.complete_lesson(2, vec![]).await;
    let card2: u128 = 0x5002;
    sqlx::query("UPDATE review_cards SET unpassed_block = 'q1' WHERE id = ?")
        .bind(uid(card2).to_string())
        .execute(&app.pool)
        .await
        .unwrap();
    assert_eq!(
        app.vote(card2, "easy", 0).await.body["to_consolidate"],
        true
    );
    let r = app.vote(card2, "good", 1).await;
    assert_eq!(r.body["to_consolidate"], false);
    let r = undo(card2).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["to_consolidate"], true);

    // Suspension keeps the flag, in the card and in the suspended list.
    let s = app
        .post(
            &format!("/api/v1/reviews/{}/suspend", uid(card2)),
            json!({}),
        )
        .await;
    assert_eq!(s.status, StatusCode::OK, "{}", s.body);
    assert_eq!(s.body["to_consolidate"], true);
    let list = app.get("/api/v1/reviews/suspended").await;
    assert_eq!(flags(&list.body), [true]);
    let u = app
        .post(
            &format!("/api/v1/reviews/{}/unsuspend", uid(card2)),
            json!({}),
        )
        .await;
    assert_eq!(u.body["to_consolidate"], true);
}
