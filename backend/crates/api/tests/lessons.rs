#![allow(clippy::too_many_lines, clippy::needless_pass_by_value)] // one scenario over the whole flow
//! End-to-end tests of lessons, attempts, completion, glossary and records
//! (`docs/contracts/lessons.md`) over the real router and SQLite, in process,
//! with a scripted model. Nothing here calls a real model.

mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::app::{Limits, build_router};
use api::state::{AppOptions, AppState};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::{ScriptedLlm, Step};
use domain::{Clock, IdGenerator, JobKind};
use http_body_util::BodyExt;
use infrastructure::sqlite::{self, Pool};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = common::TOKEN;
const RESOURCE: u128 = 0x51;
const RESOURCE_2: u128 = 0x52;
const FIXTURE: &str =
    include_str!("../../../../docs/contracts/fixtures/lesson-photo-exposure.json");

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
    llm: Arc<ScriptedLlm>,
}

fn say(text: &str) -> Vec<Step> {
    vec![Step::Delta(Box::leak(text.to_owned().into_boxed_str()))]
}

impl App {
    async fn new(scripts: Vec<Vec<Step>>) -> Self {
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        let llm = ScriptedLlm::with(scripts);
        let mut options = AppOptions::default();
        options.chat.flush_interval = Duration::ZERO;
        let state = AppState::new(
            Arc::new(TestClock(Mutex::new(
                datetime!(2026-11-09 15:00:00.000 UTC),
            ))),
            Arc::new(SeqIds(AtomicU64::new(0))),
            common::auth(),
            &pool,
            llm.clone(),
            options,
        );
        common::sign_in_owner(&state, &pool).await;
        let runner = state.job_runner(vec![
            (JobKind::LessonGeneration, state.generation_handler()),
            (JobKind::LessonCompletion, state.completion_handler()),
        ]);
        tokio::spawn(Arc::new(runner).run());
        Self {
            router: build_router(state, Limits::default()),
            pool,
            llm,
        }
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Res {
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

    async fn topic(&self, folder_name: &str) -> String {
        self.topic_in(folder_name, None).await
    }

    async fn topic_in(&self, folder_name: &str, language: Option<&str>) -> String {
        let f = self
            .post("/api/v1/folders", json!({ "name": folder_name }))
            .await;
        let folder = f.body["id"].as_str().unwrap().to_owned();
        let t = self
            .post(
                &format!("/api/v1/folders/{folder}/topics"),
                json!({ "title": "Fotografia", "language": language }),
            )
            .await;
        assert_eq!(t.status, StatusCode::CREATED, "{}", t.body);
        t.body["id"].as_str().unwrap().to_owned()
    }

    /// The state a topic has after the mission and the research: `ready`,
    /// with a confirmed mission and two knowledge sources.
    async fn make_ready(&self, topic: &str) {
        let at = "2026-11-09T14:00:00.000Z";
        sqlx::query("UPDATE topics SET state = 'ready' WHERE id = ?")
            .bind(topic)
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO missions (topic_id, version, status, why, success_criteria, \
             mission_constraints, out_of_scope, created_at, confirmed_at) \
             VALUES (?, 1, 'confirmed', 'Fotografare bambini', '[\"Scatto nitido\"]', '[]', '[]', ?, ?)",
        )
        .bind(topic)
        .bind(at)
        .bind(at)
        .execute(&self.pool)
        .await
        .unwrap();
        for (position, id) in [RESOURCE, RESOURCE_2].into_iter().enumerate() {
            sqlx::query(
                "INSERT INTO resources (id, topic_id, position, kind, source_type, title, url, \
                 language, annotation, use_for, created_at) \
                 VALUES (?, ?, ?, 'knowledge', 'documentation', ?, ?, 'en', 'Copre gli stop.', 'capire gli stop', ?)",
            )
            .bind(Uuid::from_u128(id).to_string())
            .bind(topic)
            .bind(i64::try_from(position).unwrap())
            .bind(format!("Fonte {id}"))
            .bind(format!("https://example.org/{id}"))
            .bind(at)
            .execute(&self.pool)
            .await
            .unwrap();
        }
    }

    async fn wait_job(&self, topic: &str, kind: &str, status: &str) -> Value {
        for _ in 0..600 {
            let jobs = self.get(&format!("/api/v1/topics/{topic}/jobs")).await.body;
            if let Some(job) = jobs["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|j| j["kind"] == kind)
                && job["status"] == status
            {
                return job.clone();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("the {kind} job never reached {status}");
    }
}

fn document() -> String {
    let mut doc: Value = serde_json::from_str(FIXTURE).unwrap();
    for (i, id) in [RESOURCE, RESOURCE_2].into_iter().enumerate() {
        doc["sources"][i]["resource_id"] = json!(Uuid::from_u128(id).to_string());
    }
    doc.to_string()
}

fn plan() -> String {
    json!({
        "title": "Gli stop", "objective": "Saper compensare uno stop.", "why_now": "La mission lo chiede.",
        "estimated_minutes": 8, "prerequisites": [],
        "concepts": [{"name": "Stop", "key_point": "Uno stop raddoppia la luce.", "example": "Un caso con valori.", "steps": ["primo passaggio", "secondo passaggio"], "pitfall": ""}, {"name": "Compensazione", "key_point": "Uno stop si ridà.", "example": "Un caso con valori.", "steps": ["primo passaggio", "secondo passaggio"], "pitfall": ""}, {"name": "Tempo", "key_point": "Il tempo dimezza la luce.", "example": "Un caso con valori.", "steps": ["primo passaggio", "secondo passaggio"], "pitfall": ""}, {"name": "Diaframma", "key_point": "Il diaframma regola la luce.", "example": "Un caso con valori.", "steps": ["primo passaggio", "secondo passaggio"], "pitfall": ""}],
        "terms": {"introduce": [], "reuse": []},
        "primary_source_id": Uuid::from_u128(RESOURCE).to_string(),
        "other_source_ids": [],
        "exercises": [
            {"type": "quiz_single", "purpose": "Riconoscere", "verifies_objective": false},
            {"type": "open_answer", "purpose": "Spiegare", "verifies_objective": true}
        ],
        "lesson_links": [],
        "document_sections": []
    })
    .to_string()
}

fn attempt(block: &str, answer: Value, outcome: Option<&str>) -> Value {
    let mut a = json!({"block_id": block, "attempt_number": 1, "answer": answer, "answered_at": "2026-11-09T15:08:40.000Z"});
    if let Some(o) = outcome {
        a["outcome"] = json!(o);
    }
    a
}

#[tokio::test]
async fn a_lesson_from_generation_to_the_learning_record() {
    let grade = json!({"verdict": "partial", "feedback": "Hai colto l'idea; manca il prezzo.", "misconception": null}).to_string();
    let recorder = json!({
        "records": [{"kind": "demonstrated", "title": "Compensare uno stop", "body": "Hai scelto la compensazione senza rumore.",
            "evidence": "Hai risposto bene.", "evidence_blocks": ["q1"], "supersedes": []}],
        "terms": [{"key": "stop", "term": "stop", "definition": "Unità di luce.", "avoid": [], "evidence_blocks": ["q1"]}],
        "cards": [{"front": "Da f/8 a f/5.6?", "back": "Raddoppia la luce.", "record_index": 0, "term_key": null, "unpassed_block": null}]
    })
    .to_string();
    let app = App::new(vec![
        say(&plan()),
        say(&document()),
        say(&grade),
        say(&recorder),
        say("Certo, ti spiego."),
    ])
    .await;
    let topic = app.topic("F").await;

    // Not ready yet.
    let r = app
        .post(&format!("/api/v1/topics/{topic}/lessons"), json!({}))
        .await;
    assert_eq!(
        (r.status, r.body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict"))
    );
    assert_eq!(
        app.post("/api/v1/topics/nope/lessons", json!({}))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    let r = app
        .post(
            &format!("/api/v1/topics/{}/lessons", Uuid::from_u128(9)),
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    app.make_ready(&topic).await;
    let r = app
        .post(&format!("/api/v1/topics/{topic}/lessons"), json!({}))
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);
    assert_eq!(r.body["kind"], "lesson_generation");
    assert_eq!(r.body["lesson_id"], Value::Null);
    let job = app.wait_job(&topic, "lesson_generation", "succeeded").await;
    assert_eq!(job["result"]["lesson_number"], 1);
    let lesson_id = job["result"]["lesson_id"].as_str().unwrap().to_owned();

    let list = app.get(&format!("/api/v1/topics/{topic}/lessons")).await;
    assert_eq!(list.body["items"].as_array().unwrap().len(), 1);
    assert_eq!(list.body["items"][0]["status"], "not_started");
    assert_eq!(list.body["items"][0]["mission_version"], 1);

    let view = app.get(&format!("/api/v1/lessons/{lesson_id}")).await;
    assert_eq!(view.status, StatusCode::OK);
    assert!(
        !view.body["document"].to_string().contains("rubric"),
        "the rubric never leaves the backend"
    );
    assert!(view.body["document"]["blocks"].as_array().unwrap().len() > 3);
    assert_eq!(view.body["attempts"], json!([]));
    assert_eq!(view.body["language"], "it", "default topic language");
    assert!(view.body["quality_warnings"].is_array());
    assert!(view.body["lesson"]["quality_warnings"].is_array());
    assert!(list.body["items"][0]["quality_warnings"].is_array());
    assert_eq!(view.body["completion"], Value::Null);
    assert_eq!(
        app.get("/api/v1/lessons/nope").await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.get(&format!("/api/v1/lessons/{}", Uuid::from_u128(9)))
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // Attempts: a closed one, a flashcard, and a graded open answer.
    let path = format!("/api/v1/lessons/{lesson_id}/attempts");
    let r = app
        .post(
            &path,
            json!({"attempts": [
                attempt("q1", json!({"option": "c"}), Some("correct")),
                attempt("fc1", json!({"rating": "good"}), None),
            ]}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["attempts"][0]["block_type"], "quiz_single");
    assert_eq!(r.body["attempts"][1]["outcome"], "correct");
    assert_eq!(r.body["attempts"][1]["grade"], Value::Null);
    let first_id = r.body["attempts"][0]["id"].clone();

    let r = app
        .post(
            &path,
            json!({"attempts": [
                attempt("q1", json!({"option": "a"}), Some("incorrect")),
                attempt("oa1", json!({"text": "Accorcio il tempo e apro il diaframma"}), None),
            ]}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["attempts"][0]["id"], first_id, "the first one wins");
    assert_eq!(r.body["attempts"][0]["outcome"], "correct");
    assert_eq!(r.body["attempts"][1]["outcome"], "partial");
    assert_eq!(r.body["attempts"][1]["grade"]["verdict"], "partial");

    // Validation errors name the field.
    let bad = app
        .post(
            &path,
            json!({"attempts": [attempt("zzz", json!({"option": "c"}), Some("correct"))]}),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad.body["error"]["code"], "validation_failed");
    assert_eq!(
        bad.body["error"]["details"],
        json!([{"field": "attempts[0].block_id", "issue": "not_found"}])
    );
    assert_eq!(
        app.post(&path, json!({"attempts": []})).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let missing = app
        .post(
            &format!("/api/v1/lessons/{}/attempts", Uuid::from_u128(9)),
            json!({"attempts": []}),
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    let view = app.get(&format!("/api/v1/lessons/{lesson_id}")).await;
    assert_eq!(view.body["lesson"]["status"], "in_progress");
    assert_eq!(view.body["attempts"].as_array().unwrap().len(), 3);

    // Close it.
    let done = app
        .post(&format!("/api/v1/lessons/{lesson_id}/complete"), json!({}))
        .await;
    assert_eq!(done.status, StatusCode::ACCEPTED, "{}", done.body);
    assert_eq!(done.body["kind"], "lesson_completion");
    assert_eq!(done.body["lesson_id"], lesson_id.as_str());
    let job = app.wait_job(&topic, "lesson_completion", "succeeded").await;
    assert_eq!(
        job["result"],
        json!({"lesson_id": lesson_id, "record_count": 1, "term_count": 1, "review_card_count": 1})
    );
    assert_eq!(
        app.post(&format!("/api/v1/lessons/{lesson_id}/complete"), json!({}))
            .await
            .status,
        StatusCode::CONFLICT
    );
    let late = app
        .post(
            &path,
            json!({"attempts": [attempt("fc1", json!({"rating": "easy"}), None)]}),
        )
        .await;
    assert_eq!(late.status, StatusCode::CONFLICT);

    let view = app.get(&format!("/api/v1/lessons/{lesson_id}")).await;
    assert_eq!(view.body["lesson"]["status"], "completed");
    let completion = &view.body["completion"];
    assert_eq!(completion["job"]["status"], "succeeded");
    assert_eq!(completion["records"][0]["number"], 1);
    assert_eq!(completion["records"][0]["kind"], "demonstrated");
    assert_eq!(completion["terms"][0]["key"], "stop");
    assert_eq!(
        completion["terms"][0]["first_lesson_id"],
        lesson_id.as_str()
    );
    assert_eq!(completion["review_card_count"], 1);

    let glossary = app.get(&format!("/api/v1/topics/{topic}/glossary")).await;
    assert_eq!(glossary.body["items"][0]["term"], "stop");
    assert_eq!(glossary.body["items"][0]["lesson_id"], lesson_id.as_str());
    assert_eq!(
        glossary.body["items"][0]["first_lesson_id"],
        lesson_id.as_str()
    );
    let records = app
        .get(&format!("/api/v1/topics/{topic}/learning-records"))
        .await;
    assert_eq!(records.body["items"].as_array().unwrap().len(), 1);
    assert_eq!(records.body["items"][0]["status"], "active");
    assert_eq!(records.body["items"][0]["superseded_by"], Value::Null);
    let all = app
        .get(&format!(
            "/api/v1/topics/{topic}/learning-records?status=all"
        ))
        .await;
    assert_eq!(all.status, StatusCode::OK);
    assert_eq!(
        app.get(&format!(
            "/api/v1/topics/{topic}/learning-records?status=nope"
        ))
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.get(&format!("/api/v1/topics/{topic}/glossary?cursor=zz"))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    // The chat opened from the lesson tells the Tutor which one.
    let other_topic = app.topic("G").await;
    let r = app
        .post(
            &format!("/api/v1/topics/{other_topic}/messages"),
            json!({"text": "ciao", "lesson_id": lesson_id}),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        r.body["error"]["details"],
        json!([{"field": "lesson_id", "issue": "not_found"}])
    );
    let r = app
        .post(
            &format!("/api/v1/topics/{topic}/messages"),
            json!({"text": "Non capisco", "lesson_id": lesson_id}),
        )
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);
    assert!(
        r.body["user_message"].get("lesson_id").is_none(),
        "the field is not part of the message"
    );
    for _ in 0..400 {
        if app.llm.requests.lock().unwrap().len() >= 5 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let requests = app.llm.requests.lock().unwrap();
    assert!(requests[4].system[1].text.contains("Lezione 1:"));
}

async fn put_empty(app: &App, path: &str) -> Res {
    app.call(Method::PUT, path, Some(json!({"highlights": []})))
        .await
}

#[tokio::test]
async fn highlights_are_saved_whole_served_in_order_and_go_with_the_topic() {
    let app = App::new(vec![say(&plan()), say(&document())]).await;
    let topic = app.topic("F").await;
    app.make_ready(&topic).await;
    app.post(&format!("/api/v1/topics/{topic}/lessons"), json!({}))
        .await;
    let job = app.wait_job(&topic, "lesson_generation", "succeeded").await;
    let lesson = job["result"]["lesson_id"].as_str().unwrap().to_owned();
    let path = format!("/api/v1/lessons/{lesson}/highlights");

    // UTF-16 length of the text of the first paragraph (`intro`) of the fixture.
    let doc: Value = serde_json::from_str(&document()).unwrap();
    let text_of = |id: &str| {
        doc["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == id)
            .unwrap()["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let intro = u32::try_from(text_of("intro").encode_utf16().count()).unwrap();
    let hl = |block: &str, start: u32, end: u32, color: &str| json!({"block_id": block, "start": start, "end": end, "color": color});

    // Out of order and touching; the answer is in document order.
    let r = app
        .call(
            Method::PUT,
            &path,
            Some(json!({"highlights": [
                hl("s1_p1", 0, 5, "pink"),
                hl("intro", 3, intro, "green"),
                hl("intro", 0, 3, "yellow"),
            ]})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let expected = json!([
        hl("intro", 0, 3, "yellow"),
        hl("intro", 3, intro, "green"),
        hl("s1_p1", 0, 5, "pink"),
    ]);
    assert_eq!(r.body["highlights"], expected);
    let view = app.get(&format!("/api/v1/lessons/{lesson}")).await;
    assert_eq!(view.body["highlights"], expected);

    // The same body again changes nothing.
    let body = json!({"highlights": [hl("intro", 0, 3, "yellow")]});
    let a = app.call(Method::PUT, &path, Some(body.clone())).await;
    let b = app.call(Method::PUT, &path, Some(body)).await;
    assert_eq!(
        (a.body.clone(), b.body),
        (a.body, json!({"highlights": [hl("intro", 0, 3, "yellow")]}))
    );

    // One past the end, in UTF-16 units: rejected, and the stored set stays.
    let r = app
        .call(
            Method::PUT,
            &path,
            Some(json!({"highlights": [hl("intro", 0, intro + 1, "yellow")]})),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        r.body["error"]["details"],
        json!([{"field": "highlights[0].end", "issue": "out_of_range"}])
    );
    let r = app
        .call(
            Method::PUT,
            &path,
            Some(json!({"highlights": [hl("q1", 0, 1, "yellow")]})),
        )
        .await;
    assert_eq!(
        r.body["error"]["details"],
        json!([{"field": "highlights[0].block_id", "issue": "invalid"}])
    );
    assert_eq!(
        app.get(&format!("/api/v1/lessons/{lesson}")).await.body["highlights"],
        json!([hl("intro", 0, 3, "yellow")])
    );

    // A completed lesson takes highlights and stays completed.
    sqlx::query("UPDATE lessons SET status = 'completed', completed_at = ? WHERE id = ?")
        .bind("2026-11-09T15:30:00.000Z")
        .bind(&lesson)
        .execute(&app.pool)
        .await
        .unwrap();
    let r = app
        .call(
            Method::PUT,
            &path,
            Some(json!({"highlights": [hl("intro", 1, 2, "blue")]})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let view = app.get(&format!("/api/v1/lessons/{lesson}")).await;
    assert_eq!(view.body["lesson"]["status"], "completed");
    assert_eq!(view.body["highlights"], json!([hl("intro", 1, 2, "blue")]));

    // Ids and missing lessons.
    assert_eq!(
        put_empty(&app, "/api/v1/lessons/nope/highlights")
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        put_empty(
            &app,
            &format!("/api/v1/lessons/{}/highlights", Uuid::from_u128(9))
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // An empty set clears; deleting the topic takes the rest away.
    let r = put_empty(&app, &path).await;
    assert_eq!(r.body, json!({"highlights": []}));
    app.call(
        Method::PUT,
        &path,
        Some(json!({"highlights": [hl("intro", 0, 1, "pink")]})),
    )
    .await;
    let gone = app
        .call(Method::DELETE, &format!("/api/v1/topics/{topic}"), None)
        .await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT, "{}", gone.body);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM highlights")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn highlights_reject_bad_requests_with_the_exact_errors() {
    let app = App::new(vec![say(&plan()), say(&document())]).await;
    let topic = app.topic("F").await;
    app.make_ready(&topic).await;
    app.post(&format!("/api/v1/topics/{topic}/lessons"), json!({}))
        .await;
    let job = app.wait_job(&topic, "lesson_generation", "succeeded").await;
    let lesson = job["result"]["lesson_id"].as_str().unwrap().to_owned();
    let path = format!("/api/v1/lessons/{lesson}/highlights");
    let hl = |block: &str, start: u32, end: u32| json!({"block_id": block, "start": start, "end": end, "color": "yellow"});
    let details = |r: &Res| r.body["error"]["details"].clone();

    // No array.
    let r = app.call(Method::PUT, &path, Some(json!({}))).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        details(&r),
        json!([{"field": "highlights", "issue": "invalid"}])
    );

    // 501 entries.
    let many: Vec<Value> = (0..501).map(|_| hl("intro", 0, 1)).collect();
    let r = app
        .call(Method::PUT, &path, Some(json!({"highlights": many})))
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        details(&r),
        json!([{"field": "highlights", "issue": "too_many"}])
    );

    // Overlap, reported on the second.
    let r = app
        .call(
            Method::PUT,
            &path,
            Some(json!({"highlights": [hl("intro", 0, 5), hl("intro", 4, 8)]})),
        )
        .await;
    assert_eq!(
        details(&r),
        json!([{"field": "highlights[1].start", "issue": "overlap"}])
    );

    // A paragraph inside a `fallback`, through the stored document.
    let r = app
        .call(
            Method::PUT,
            &path,
            Some(json!({"highlights": [hl("mc_sim_p", 0, 4)]})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["highlights"], json!([hl("mc_sim_p", 0, 4)]));
    // Its sibling `code` block is not highlightable.
    let r = app
        .call(
            Method::PUT,
            &path,
            Some(json!({"highlights": [hl("mc_sim_code", 0, 1)]})),
        )
        .await;
    assert_eq!(
        details(&r),
        json!([{"field": "highlights[0].block_id", "issue": "invalid"}])
    );

    // Malformed JSON.
    let req = Request::builder()
        .method(Method::PUT)
        .uri(&path)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{\"highlights\": ["))
        .unwrap();
    let res = app.router.clone().oneshot(req).await.unwrap();
    assert!(
        res.status().is_client_error() && res.status() != StatusCode::NOT_FOUND,
        "{}",
        res.status()
    );

    // None of the failures touched what was stored.
    assert_eq!(
        app.get(&format!("/api/v1/lessons/{lesson}")).await.body["highlights"],
        json!([hl("mc_sim_p", 0, 4)])
    );
}

// ------------------------------------------------------------- strokes

const STROKE_A: &str = "5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f10";
const STROKE_B: &str = "5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f11";
const STROKE_C: &str = "5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f12";

fn stroke(id: &str, block: &str, points: &[i64]) -> Value {
    json!({"id": id, "block_id": block, "color": "ink", "width": "medium", "points": points})
}

impl App {
    async fn lesson(&self) -> (String, String) {
        self.lesson_in(None).await
    }

    async fn lesson_in(&self, language: Option<&str>) -> (String, String) {
        let topic = self.topic_in("F", language).await;
        self.make_ready(&topic).await;
        self.post(&format!("/api/v1/topics/{topic}/lessons"), json!({}))
            .await;
        let job = self
            .wait_job(&topic, "lesson_generation", "succeeded")
            .await;
        (
            topic,
            job["result"]["lesson_id"].as_str().unwrap().to_owned(),
        )
    }

    /// A PUT whose body is the given bytes, as a client that sends them as is.
    async fn put_raw(&self, path: &str, body: Vec<u8>) -> Res {
        let req = Request::builder()
            .method(Method::PUT)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        Res {
            status: parts.status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }
}

#[tokio::test]
async fn strokes_are_saved_whole_served_in_drawing_order_and_go_with_the_topic() {
    let app = App::new(vec![say(&plan()), say(&document())]).await;
    let (topic, lesson) = app.lesson().await;
    let path = format!("/api/v1/lessons/{lesson}/strokes");
    let view_path = format!("/api/v1/lessons/{lesson}");
    assert_eq!(app.get(&view_path).await.body["strokes"], json!([]));

    // The order sent is the order kept, not the order of the document; a
    // block with a `fallback` is one block (`mc_sim`), its children are not.
    let set = json!([
        stroke(STROKE_B, "s1_p1", &[1204, 380, 1311, 402, 1498, 415]),
        stroke(STROKE_A, "intro", &[-40000, 200_000]),
        stroke(STROKE_C, "mc_sim", &[50000, -40000]),
    ]);
    let r = app
        .call(Method::PUT, &path, Some(json!({"strokes": set})))
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body, json!({"strokes": set}));
    assert_eq!(app.get(&view_path).await.body["strokes"], set);

    // Resending the same body changes nothing.
    let again = app
        .call(Method::PUT, &path, Some(json!({"strokes": set})))
        .await;
    assert_eq!(again.body, json!({"strokes": set}));
    assert_eq!(app.get(&view_path).await.body["strokes"], set);

    // A rejected request leaves the stored set alone.
    let bad =
        json!({"strokes": [stroke(STROKE_A, "intro", &[1, 2]), stroke(STROKE_B, "zz", &[1, 2])]});
    let r = app.call(Method::PUT, &path, Some(bad)).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(app.get(&view_path).await.body["strokes"], set);

    // Allowed on a completed lesson, which stays completed.
    sqlx::query("UPDATE lessons SET status = 'completed', completed_at = ? WHERE id = ?")
        .bind("2026-11-09T15:30:00.000Z")
        .bind(&lesson)
        .execute(&app.pool)
        .await
        .unwrap();
    let one = json!([stroke(STROKE_A, "intro", &[7, 8])]);
    let r = app
        .call(Method::PUT, &path, Some(json!({"strokes": one})))
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let view = app.get(&view_path).await;
    assert_eq!(view.body["lesson"]["status"], "completed");
    assert_eq!(view.body["strokes"], one);

    // Ids and missing lessons.
    let app_ref = &app;
    let empty = |p: String| async move {
        app_ref
            .call(Method::PUT, &p, Some(json!({"strokes": []})))
            .await
            .status
    };
    assert_eq!(
        empty("/api/v1/lessons/nope/strokes".into()).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        empty(format!("/api/v1/lessons/{}/strokes", Uuid::from_u128(9))).await,
        StatusCode::NOT_FOUND
    );

    // An empty set clears; deleting the topic takes the rest away.
    let r = app
        .call(Method::PUT, &path, Some(json!({"strokes": []})))
        .await;
    assert_eq!(r.body, json!({"strokes": []}));
    app.call(Method::PUT, &path, Some(json!({"strokes": one})))
        .await;
    let gone = app
        .call(Method::DELETE, &format!("/api/v1/topics/{topic}"), None)
        .await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT, "{}", gone.body);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM strokes")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn strokes_reject_bad_requests_with_the_exact_errors() {
    let app = App::new(vec![say(&plan()), say(&document())]).await;
    let (_, lesson) = app.lesson().await;
    let path = format!("/api/v1/lessons/{lesson}/strokes");
    let details = |r: &Res| r.body["error"]["details"].clone();
    let ok = || stroke(STROKE_A, "intro", &[1, 2]);
    let with = |key: &str, value: Value| {
        let mut s = ok();
        s[key] = value;
        s
    };
    let without = |key: &str| {
        let mut s = ok();
        s.as_object_mut().unwrap().remove(key);
        s
    };
    let cases: Vec<(Value, &str, &str)> = vec![
        (json!({}), "strokes", "invalid"),
        (json!({"strokes": {}}), "strokes", "invalid"),
        (
            json!({"strokes": [without("id")]}),
            "strokes[0].id",
            "invalid",
        ),
        (
            json!({"strokes": [with("id", json!("x"))]}),
            "strokes[0].id",
            "invalid",
        ),
        (
            json!({"strokes": [with("id", json!(STROKE_A.replace('-', "")))]}),
            "strokes[0].id",
            "invalid",
        ),
        (
            json!({"strokes": [ok(), ok()]}),
            "strokes[1].id",
            "duplicate",
        ),
        (
            json!({"strokes": [without("block_id")]}),
            "strokes[0].block_id",
            "invalid",
        ),
        (
            json!({"strokes": [with("block_id", json!("zz"))]}),
            "strokes[0].block_id",
            "invalid",
        ),
        // Nested in a `fallback`: not a top-level block.
        (
            json!({"strokes": [with("block_id", json!("mc_sim_p"))]}),
            "strokes[0].block_id",
            "invalid",
        ),
        (
            json!({"strokes": [without("color")]}),
            "strokes[0].color",
            "invalid",
        ),
        (
            json!({"strokes": [with("color", json!("pink"))]}),
            "strokes[0].color",
            "invalid",
        ),
        (
            json!({"strokes": [without("width")]}),
            "strokes[0].width",
            "invalid",
        ),
        (
            json!({"strokes": [with("width", json!("huge"))]}),
            "strokes[0].width",
            "invalid",
        ),
        (
            json!({"strokes": [without("points")]}),
            "strokes[0].points",
            "invalid",
        ),
        (
            json!({"strokes": [with("points", json!("1,2"))]}),
            "strokes[0].points",
            "invalid",
        ),
        (
            json!({"strokes": [with("points", json!([1, "2"]))]}),
            "strokes[0].points",
            "invalid",
        ),
        (
            json!({"strokes": [with("points", json!([1.5, 2]))]}),
            "strokes[0].points",
            "invalid",
        ),
        (
            json!({"strokes": [with("points", json!([]))]}),
            "strokes[0].points",
            "invalid",
        ),
        (
            json!({"strokes": [with("points", json!([1, 2, 3]))]}),
            "strokes[0].points",
            "invalid",
        ),
        (
            json!({"strokes": [with("points", json!([-40001, 0]))]}),
            "strokes[0].points",
            "out_of_range",
        ),
        (
            json!({"strokes": [with("points", json!([50001, 0]))]}),
            "strokes[0].points",
            "out_of_range",
        ),
        (
            json!({"strokes": [with("points", json!([0, 200_001]))]}),
            "strokes[0].points",
            "out_of_range",
        ),
        (
            json!({"strokes": [with("points", json!([0, 1e30]))]}),
            "strokes[0].points",
            "out_of_range",
        ),
        (
            json!({"strokes": [with("points", json!(vec![0; 2002]))]}),
            "strokes[0].points",
            "too_many",
        ),
    ];
    for (body, field, issue) in cases {
        let r = app.call(Method::PUT, &path, Some(body.clone())).await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(
            details(&r),
            json!([{"field": field, "issue": issue}]),
            "{body}"
        );
    }

    // 2001 strokes.
    let many: Vec<Value> = (0..2001)
        .map(|i| {
            stroke(
                &format!("00000000-0000-4000-8000-{i:012}"),
                "intro",
                &[0, 0],
            )
        })
        .collect();
    let r = app
        .call(Method::PUT, &path, Some(json!({"strokes": many})))
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        details(&r),
        json!([{"field": "strokes", "issue": "too_many"}])
    );

    // 201 strokes of 1000 points: 201 000 points in all.
    let heavy: Vec<Value> = (0..201)
        .map(|i| {
            stroke(
                &format!("00000000-0000-4000-8000-{i:012}"),
                "intro",
                &vec![0; 2000],
            )
        })
        .collect();
    let r = app
        .call(Method::PUT, &path, Some(json!({"strokes": heavy})))
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        details(&r),
        json!([{"field": "strokes", "issue": "too_many_points"}])
    );

    // Malformed JSON is a client error and nothing was stored.
    let r = app.put_raw(&path, b"{\"strokes\": [".to_vec()).await;
    assert!(r.status.is_client_error() && r.status != StatusCode::NOT_FOUND);
    assert_eq!(
        app.get(&format!("/api/v1/lessons/{lesson}")).await.body["strokes"],
        json!([])
    );
}

#[tokio::test]
async fn the_strokes_body_limit_is_4_mib_and_only_there() {
    const LIMIT: usize = 4 * 1024 * 1024;
    let app = App::new(vec![say(&plan()), say(&document())]).await;
    let (_, lesson) = app.lesson().await;
    let path = format!("/api/v1/lessons/{lesson}/strokes");

    // Valid JSON padded with spaces to an exact size.
    let padded = |size: usize| {
        let set: Vec<Value> = (0..200)
            .map(|i| {
                // Wide values so the real content is already around 2.8 MB.
                let points: Vec<i64> = (0..2000)
                    .map(|j| if j % 2 == 0 { -39999 } else { 199_999 })
                    .collect();
                stroke(
                    &format!("00000000-0000-4000-8000-{i:012}"),
                    "intro",
                    &points,
                )
            })
            .collect();
        let mut body = json!({"strokes": set}).to_string().into_bytes();
        assert!(body.len() < size);
        body.resize(size, b' ');
        body
    };

    let r = app.put_raw(&path, padded(LIMIT)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["strokes"].as_array().unwrap().len(), 200);
    let view = app.get(&format!("/api/v1/lessons/{lesson}")).await;
    assert_eq!(view.body["strokes"].as_array().unwrap().len(), 200);

    let r = app.put_raw(&path, padded(LIMIT + 1)).await;
    assert_eq!(r.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(r.body["error"]["code"], "payload_too_large");

    // Another route keeps the 1 MiB limit.
    let hl = format!("/api/v1/lessons/{lesson}/highlights");
    let mut big = json!({"highlights": []}).to_string().into_bytes();
    big.resize(1024 * 1024 + 1, b' ');
    let r = app.put_raw(&hl, big).await;
    assert_eq!(r.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn the_lesson_view_carries_the_language_of_its_topic() {
    let app = App::new(vec![say(&plan()), say(&document())]).await;
    let (_, lesson) = app.lesson_in(Some("en")).await;
    let view = app.get(&format!("/api/v1/lessons/{lesson}")).await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.body["language"], "en");
}
