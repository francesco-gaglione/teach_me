//! End-to-end tests of the mission contract (`docs/contracts/mission.md`) over
//! the real router and SQLite, in process (no sockets), with a scripted model.

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
use domain::{Clock, IdGenerator};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use infrastructure::sqlite::{self, Pool};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;
use tokio::sync::Notify;
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = common::TOKEN;

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

struct App {
    router: Router,
    pool: Pool,
    llm: Arc<ScriptedLlm>,
}

struct Setup {
    scripts: Vec<Vec<Step>>,
    options: AppOptions,
    limits: Limits,
}

impl Setup {
    fn new(scripts: Vec<Vec<Step>>) -> Self {
        let mut options = AppOptions::default();
        options.chat.flush_interval = Duration::ZERO;
        Self {
            scripts,
            options,
            limits: Limits::default(),
        }
    }
}

impl App {
    async fn new(scripts: Vec<Vec<Step>>) -> Self {
        Self::with(Setup::new(scripts)).await
    }

    async fn with(setup: Setup) -> Self {
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        Self::on(pool, setup).await
    }

    async fn on(pool: Pool, setup: Setup) -> Self {
        let llm = ScriptedLlm::with(setup.scripts);
        let state = AppState::new(
            Arc::new(TestClock(Mutex::new(
                datetime!(2026-11-08 14:36:01.129 UTC),
            ))),
            Arc::new(SeqIds(AtomicU64::new(0))),
            common::auth(),
            &pool,
            llm.clone(),
            setup.options,
        );
        common::sign_in_owner(&state, &pool).await;
        Self {
            router: build_router(state.clone(), setup.limits),
            pool,
            llm,
        }
    }

    fn request(method: Method, path: &str, body: Option<Value>) -> Request<Body> {
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
        req.body(body).unwrap()
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Res {
        let res = self
            .router
            .clone()
            .oneshot(Self::request(method, path, body))
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
    async fn delete(&self, path: &str) -> Res {
        self.call(Method::DELETE, path, None).await
    }

    async fn topic(&self) -> String {
        let f = self.post("/api/v1/folders", json!({ "name": "F" })).await;
        let folder = f.body["id"].as_str().unwrap().to_owned();
        let t = self
            .post(
                &format!("/api/v1/folders/{folder}/topics"),
                json!({ "title": "Rust" }),
            )
            .await;
        assert_eq!(t.status, StatusCode::CREATED);
        t.body["id"].as_str().unwrap().to_owned()
    }

    async fn send(&self, topic: &str, text: &str) -> Res {
        self.post(
            &format!("/api/v1/topics/{topic}/messages"),
            json!({ "text": text }),
        )
        .await
    }

    async fn messages(&self, topic: &str) -> Vec<Value> {
        let r = self.get(&format!("/api/v1/topics/{topic}/messages")).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        r.body["items"].as_array().unwrap().clone()
    }

    /// Polls until the message reaches `status`; returns it.
    async fn wait_status(&self, topic: &str, id: &str, status: &str) -> Value {
        for _ in 0..400 {
            let found = self
                .messages(topic)
                .await
                .into_iter()
                .find(|m| m["id"] == id && m["status"] == status);
            if let Some(m) = found {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("message {id} never reached {status}");
    }

    /// Opens the SSE stream. The returned reader owns the response body.
    async fn events(&self, topic: &str, last_event_id: Option<&str>) -> Sse {
        let mut req = Self::request(Method::GET, &format!("/api/v1/topics/{topic}/events"), None);
        req.headers_mut()
            .insert(header::ACCEPT, "text/event-stream".parse().unwrap());
        if let Some(id) = last_event_id {
            req.headers_mut()
                .insert("last-event-id", id.parse().unwrap());
        }
        let res = self.router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers()[header::CONTENT_TYPE],
            "text/event-stream",
            "content type"
        );
        Sse {
            body: Box::pin(res.into_body().into_data_stream()),
            buffer: String::new(),
        }
    }
}

struct Res {
    status: StatusCode,
    body: Value,
}

#[derive(Debug, PartialEq)]
enum Frame {
    Comment(String),
    Event {
        id: Option<u64>,
        name: String,
        data: Value,
    },
}

struct Sse {
    body: std::pin::Pin<
        Box<dyn futures_util::Stream<Item = Result<axum::body::Bytes, axum::Error>> + Send>,
    >,
    buffer: String,
}

impl Sse {
    /// Next frame, or `None` if the stream ended or stayed silent for `wait`.
    async fn next_within(&mut self, wait: Duration) -> Option<Frame> {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let raw: String = self.buffer.drain(..end + 2).collect();
                return Some(parse_frame(raw.trim_end_matches('\n')));
            }
            match tokio::time::timeout(wait, self.body.next()).await {
                Ok(Some(Ok(bytes))) => self.buffer.push_str(std::str::from_utf8(&bytes).unwrap()),
                _ => return None,
            }
        }
    }

    async fn next(&mut self) -> Frame {
        self.next_within(Duration::from_secs(5))
            .await
            .expect("an SSE frame")
    }

    /// Next non-comment event.
    async fn next_event(&mut self) -> (Option<u64>, String, Value) {
        loop {
            if let Frame::Event { id, name, data } = self.next().await {
                return (id, name, data);
            }
        }
    }

    /// Reads events until one named `name`; returns all of them.
    async fn until(&mut self, name: &str) -> Vec<(Option<u64>, String, Value)> {
        let mut seen = Vec::new();
        loop {
            let e = self.next_event().await;
            let done = e.1 == name;
            seen.push(e);
            if done {
                return seen;
            }
        }
    }
}

fn parse_frame(raw: &str) -> Frame {
    let (mut id, mut name, mut data) = (None, String::new(), String::new());
    if raw.starts_with(':') {
        return Frame::Comment(raw.to_owned());
    }
    for line in raw.lines() {
        if let Some(v) = line.strip_prefix("id: ") {
            id = Some(v.parse().unwrap());
        } else if let Some(v) = line.strip_prefix("event: ") {
            v.clone_into(&mut name);
        } else if let Some(v) = line.strip_prefix("data: ") {
            data.push_str(v);
        }
    }
    Frame::Event {
        id,
        name,
        data: serde_json::from_str(&data).unwrap(),
    }
}

fn names(events: &[(Option<u64>, String, Value)]) -> Vec<&str> {
    events.iter().map(|e| e.1.as_str()).collect()
}

// ---------- helpers ----------

fn draft(why: &str) -> Value {
    json!({
        "why": why,
        "starting_point": "Programmo in Kotlin; di Rust ho letto solo i primi capitoli",
        "success_criteria": ["Leggo e modifico un servizio axum", "Scrivo test con tokio"],
        "constraints": ["30 minuti al giorno"],
        "out_of_scope": ["Rust embedded"]
    })
}

fn tool(name: &'static str, input: Value) -> Step {
    Step::Tool {
        id: "tu_1",
        name,
        input,
    }
}

fn proposing(why: &str) -> Vec<Vec<Step>> {
    vec![
        vec![
            Step::Delta("Ho capito. "),
            tool("save_mission_draft", draft(why)),
        ],
        vec![],
    ]
}

impl App {
    /// Sends a message and waits for the reply to finish.
    async fn turn(&self, topic: &str, text: &str) -> Value {
        let sent = self.send(topic, text).await;
        assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.body);
        let id = sent.body["assistant_message"]["id"].as_str().unwrap();
        self.wait_status(topic, id, "complete").await
    }

    async fn topic_named(&self, folder_name: &str, title: &str) -> String {
        let f = self
            .post("/api/v1/folders", json!({ "name": folder_name }))
            .await;
        let folder = f.body["id"].as_str().unwrap().to_owned();
        let t = self
            .post(
                &format!("/api/v1/folders/{folder}/topics"),
                json!({ "title": title }),
            )
            .await;
        t.body["id"].as_str().unwrap().to_owned()
    }

    async fn confirm(&self, topic: &str, body: Value) -> Res {
        self.post(&format!("/api/v1/topics/{topic}/mission/confirm"), body)
            .await
    }

    async fn state(&self, topic: &str) -> String {
        let r = self.get(&format!("/api/v1/topics/{topic}")).await;
        r.body["state"].as_str().unwrap().to_owned()
    }

    async fn mission(&self, topic: &str) -> Value {
        let r = self.get(&format!("/api/v1/topics/{topic}/mission")).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        r.body
    }
}

// ---------- endpoints ----------

#[tokio::test]
async fn mission_and_preference_endpoints_need_the_token() {
    let app = App::new(vec![]).await;
    let topic = app.topic().await;
    let any = Uuid::nil();
    for (method, path) in [
        (Method::GET, format!("/api/v1/topics/{topic}/mission")),
        (
            Method::GET,
            format!("/api/v1/topics/{topic}/mission/versions"),
        ),
        (
            Method::POST,
            format!("/api/v1/topics/{topic}/mission/confirm"),
        ),
        (Method::GET, "/api/v1/preferences".to_owned()),
        (Method::DELETE, format!("/api/v1/preferences/{any}")),
    ] {
        let req = Request::builder()
            .method(method.clone())
            .uri(&path)
            .body(Body::empty())
            .unwrap();
        let res = app.router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn ids_and_missing_topics_follow_the_error_conventions() {
    let app = App::new(vec![]).await;
    let ghost = Uuid::from_u128(0xdead);
    for path in ["mission", "mission/versions"] {
        let bad = app.get(&format!("/api/v1/topics/nope/{path}")).await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
        assert_eq!(bad.body["error"]["code"], "invalid_id");
        let missing = app.get(&format!("/api/v1/topics/{ghost}/{path}")).await;
        assert_eq!(missing.status, StatusCode::NOT_FOUND);
    }
    let bad = app.confirm("nope", json!({ "version": 1 })).await;
    assert_eq!(bad.body["error"]["code"], "invalid_id");
    // Existence is checked before the body.
    let missing = app.confirm(&ghost.to_string(), json!({})).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let bad = app.delete("/api/v1/preferences/nope").await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.body["error"]["code"], "invalid_id");
    let missing = app.delete(&format!("/api/v1/preferences/{ghost}")).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_topic_without_mission_has_nothing_current_or_drafted() {
    let app = App::new(vec![]).await;
    let topic = app.topic().await;
    assert_eq!(
        app.mission(&topic).await,
        json!({ "current": null, "draft": null })
    );
    let versions = app
        .get(&format!("/api/v1/topics/{topic}/mission/versions"))
        .await;
    assert_eq!(versions.body, json!({ "items": [], "next_cursor": null }));
}

// ---------- the flow, with events ----------

#[tokio::test]
async fn proposing_then_confirming_a_mission_end_to_end() {
    let app = App::new(proposing("Per il nuovo lavoro")).await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;

    let sent = app.send(&topic, "Voglio imparare Rust").await;
    let assistant_id = sent.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let events = sse.until("message_completed").await;
    assert_eq!(
        names(&events),
        [
            "message_created",
            "message_created",
            "topic_updated",
            "message_delta",
            "mission_proposed",
            "message_completed"
        ]
    );
    let after = sse.next_event().await;
    assert_eq!(after.1, "topic_updated");
    assert_eq!(after.2["payload"]["topic"]["state"], "mission_proposed");

    // The event carries the Mission of the contract.
    let proposed = &events[4].2["payload"]["mission"];
    assert_eq!(proposed["version"], 1);
    assert_eq!(proposed["status"], "draft");
    assert_eq!(proposed["why"], "Per il nuovo lavoro");
    assert_eq!(
        proposed["starting_point"],
        "Programmo in Kotlin; di Rust ho letto solo i primi capitoli"
    );
    assert_eq!(proposed["constraints"], json!(["30 minuti al giorno"]));
    assert_eq!(proposed["out_of_scope"], json!(["Rust embedded"]));
    assert_eq!(proposed["confirmed_at"], Value::Null);
    assert_eq!(proposed["superseded_at"], Value::Null);
    assert_eq!(proposed["topic_id"], topic.as_str());

    // The card is the last part of the message, after the text, in the REST
    // snapshot and in the completed event alike.
    let message = app.wait_status(&topic, &assistant_id, "complete").await;
    let parts = message["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], json!({ "type": "text", "text": "Ho capito. " }));
    assert_eq!(parts[1]["type"], "mission_proposal");
    assert_eq!(parts[1]["replaces_version"], Value::Null);
    assert_eq!(parts[1]["mission"], *proposed);
    assert_eq!(events[5].2["payload"]["message"]["parts"], message["parts"]);

    // The view shows the draft, no current mission.
    let view = app.mission(&topic).await;
    assert_eq!(view["current"], Value::Null);
    assert_eq!(view["draft"], *proposed);

    // Confirm.
    let confirmed = app.confirm(&topic, json!({ "version": 1 })).await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.body);
    assert_eq!(confirmed.body["draft"], Value::Null);
    let current = &confirmed.body["current"];
    assert_eq!(current["status"], "confirmed");
    assert_eq!(current["confirmed_at"], "2026-11-08T14:36:01.129Z");
    assert_eq!(app.state(&topic).await, "researching");
    let e1 = sse.next_event().await;
    let e2 = sse.next_event().await;
    assert_eq!(
        (e1.1.as_str(), e2.1.as_str()),
        ("mission_confirmed", "topic_updated")
    );
    assert_eq!(e1.2["payload"]["mission"]["status"], "confirmed");
    assert_eq!(
        e1.2["payload"]["mission"]["starting_point"],
        proposed["starting_point"]
    );
    assert_eq!(e2.2["payload"]["topic"]["state"], "researching");
    // No chat message was added by the confirmation.
    assert_eq!(app.messages(&topic).await.len(), 2);

    // The message snapshot did not change.
    let again = app.wait_status(&topic, &assistant_id, "complete").await;
    assert_eq!(again["parts"][1]["mission"]["status"], "draft");
}

/// Missions and chat snapshots saved before contract v1.2 have no starting
/// point: the HTTP answers still carry the key (`null`) and stay valid.
#[tokio::test]
async fn versions_saved_before_the_starting_point_are_served_with_null() {
    let app = App::new(vec![]).await;
    let topic = app.topic().await;
    let at = "2026-11-08T10:00:00.000Z";
    // A row as the Phase 11 backend wrote it: no `starting_point` column value.
    sqlx::query(
        "INSERT INTO missions (topic_id, version, status, why, success_criteria, \
         mission_constraints, out_of_scope, created_at, confirmed_at) \
         VALUES (?, 1, 'confirmed', 'Per lavoro', '[\"So fare X\"]', '[]', '[]', ?, ?)",
    )
    .bind(&topic)
    .bind(at)
    .bind(at)
    .execute(&app.pool)
    .await
    .unwrap();
    // A saved chat snapshot of an older draft: the mission has no such key.
    let snapshot = json!([{
        "type": "mission_proposal",
        "replaces_version": null,
        "mission": {
            "topic_id": topic, "version": 1, "status": "superseded",
            "why": "Vecchia", "success_criteria": ["So fare Y"],
            "constraints": [], "out_of_scope": [],
            "created_at": at, "confirmed_at": null, "superseded_at": at
        }
    }]);
    sqlx::query(
        "INSERT INTO chat_messages (id, topic_id, role, status, parts, created_at, updated_at) \
         VALUES ('01933b4e-0000-7000-8000-0000000000aa', ?, 'assistant', 'complete', ?, ?, ?)",
    )
    .bind(&topic)
    .bind(snapshot.to_string())
    .bind(at)
    .bind(at)
    .execute(&app.pool)
    .await
    .unwrap();

    let view = app.mission(&topic).await;
    assert_eq!(view["current"]["why"], "Per lavoro");
    assert_eq!(view["current"]["starting_point"], Value::Null);
    assert!(
        view["current"]
            .as_object()
            .unwrap()
            .contains_key("starting_point")
    );
    let versions = app
        .get(&format!("/api/v1/topics/{topic}/mission/versions"))
        .await;
    assert_eq!(versions.body["items"][0]["starting_point"], Value::Null);

    let messages = app.messages(&topic).await;
    assert_eq!(messages.len(), 1);
    let mission = &messages[0]["parts"][0]["mission"];
    assert_eq!(mission["why"], "Vecchia");
    assert_eq!(mission["starting_point"], Value::Null);
}

#[tokio::test]
async fn confirm_conflicts_and_validation() {
    let app = App::new(proposing("x")).await;
    let topic = app.topic().await;
    app.turn(&topic, "ciao").await;

    // A body that does not carry a usable integer version: 422 on `version`.
    for body in [
        json!({}),
        json!({ "version": null }),
        json!({ "version": "1" }),
        json!({ "version": 1.5 }),
        json!({ "version": 0 }),
        json!({ "version": -1 }),
        json!({ "version": true }),
    ] {
        let r = app.confirm(&topic, body.clone()).await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(r.body["error"]["code"], "validation_failed");
        assert_eq!(
            r.body["error"]["details"],
            json!([{ "field": "version", "issue": "invalid" }]),
            "{body}"
        );
    }
    // Nothing happened.
    assert_eq!(app.state(&topic).await, "mission_proposed");

    // Not the open draft: 409, the app reloads.
    let wrong = app.confirm(&topic, json!({ "version": 9 })).await;
    assert_eq!(wrong.status, StatusCode::CONFLICT);
    assert_eq!(wrong.body["error"]["code"], "conflict");
    assert_eq!(app.mission(&topic).await["draft"]["version"], 1);

    assert_eq!(
        app.confirm(&topic, json!({ "version": 1 })).await.status,
        StatusCode::OK
    );
    // Confirmed twice.
    let twice = app.confirm(&topic, json!({ "version": 1 })).await;
    assert_eq!(twice.status, StatusCode::CONFLICT);
    // Malformed JSON and wrong content type keep their generic errors.
    let bad = app
        .call(
            Method::POST,
            &format!("/api/v1/topics/{topic}/mission/confirm"),
            None,
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn writing_after_a_proposal_reopens_the_interview_and_the_draft_stays_confirmable() {
    let app = App::new({
        let mut s = proposing("x");
        s.push(vec![Step::Delta("Dimmi.")]);
        s
    })
    .await;
    let topic = app.topic().await;
    app.turn(&topic, "uno").await;
    assert_eq!(app.state(&topic).await, "mission_proposed");

    let mut sse = app.events(&topic, None).await;
    app.turn(&topic, "Vorrei cambiare una cosa").await;
    assert_eq!(app.state(&topic).await, "interviewing");
    let events = sse.until("message_completed").await;
    let updated = events.iter().find(|e| e.1 == "topic_updated").unwrap();
    assert_eq!(updated.2["payload"]["topic"]["state"], "interviewing");

    // Still open, still confirmable.
    assert_eq!(app.mission(&topic).await["draft"]["version"], 1);
    let ok = app.confirm(&topic, json!({ "version": 1 })).await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);
    assert_eq!(app.state(&topic).await, "researching");
}

#[tokio::test]
async fn a_new_draft_supersedes_the_open_one_and_versions_list_newest_first() {
    let mut scripts = proposing("Uno");
    scripts.extend(proposing("Due"));
    scripts.extend(proposing("Tre"));
    let app = App::new(scripts).await;
    let topic = app.topic().await;
    for text in ["a", "b", "c"] {
        app.turn(&topic, text).await;
    }
    assert_eq!(app.mission(&topic).await["draft"]["why"], "Tre");
    let r = app
        .get(&format!("/api/v1/topics/{topic}/mission/versions"))
        .await;
    let items = r.body["items"].as_array().unwrap();
    let got: Vec<_> = items
        .iter()
        .map(|m| {
            (
                m["version"].as_u64().unwrap(),
                m["status"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(got, [(3, "draft"), (2, "superseded"), (1, "superseded")]);
    assert_eq!(items[1]["confirmed_at"], Value::Null);
    assert!(items[1]["superseded_at"].is_string());

    // Pagination, newest first, with an opaque cursor.
    let first = app
        .get(&format!("/api/v1/topics/{topic}/mission/versions?limit=2"))
        .await;
    assert_eq!(first.body["items"].as_array().unwrap().len(), 2);
    let cursor = first.body["next_cursor"].as_str().unwrap();
    let rest = app
        .get(&format!(
            "/api/v1/topics/{topic}/mission/versions?limit=2&cursor={cursor}"
        ))
        .await;
    assert_eq!(rest.body["items"][0]["version"], 1);
    assert_eq!(rest.body["next_cursor"], Value::Null);
    let bad = app
        .get(&format!(
            "/api/v1/topics/{topic}/mission/versions?cursor=%%"
        ))
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    // The superseded draft cannot be confirmed any more.
    assert_eq!(
        app.confirm(&topic, json!({ "version": 2 })).await.status,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn changing_a_confirmed_mission_leaves_the_topic_state_alone() {
    let mut scripts = proposing("Prima");
    scripts.push(vec![
        Step::Delta("Va bene. "),
        tool("propose_mission_change", draft("Dopo")),
    ]);
    scripts.push(vec![]);
    let app = App::new(scripts).await;
    let topic = app.topic().await;
    app.turn(&topic, "uno").await;
    app.confirm(&topic, json!({ "version": 1 })).await;
    assert_eq!(app.state(&topic).await, "researching");

    let mut sse = app.events(&topic, None).await;
    let message = app.turn(&topic, "Cambiamo la mission").await;
    let card = &message["parts"][1];
    assert_eq!(card["type"], "mission_proposal");
    assert_eq!(card["replaces_version"], 1);
    assert_eq!(card["mission"]["version"], 2);
    assert_eq!(app.state(&topic).await, "researching");
    let view = app.mission(&topic).await;
    assert_eq!(view["current"]["version"], 1);
    assert_eq!(view["draft"]["version"], 2);

    let events = sse.until("message_completed").await;
    assert!(events.iter().any(|e| e.1 == "mission_proposed"));
    // No state change: no topic_updated after the completion.
    let tail = sse.next_within(Duration::from_millis(200)).await;
    assert_eq!(tail, None);

    let done = app.confirm(&topic, json!({ "version": 2 })).await;
    assert_eq!(done.body["current"]["version"], 2);
    assert_eq!(done.body["draft"], Value::Null);
    assert_eq!(app.state(&topic).await, "researching");
    let confirmed = sse.next_event().await;
    assert_eq!(confirmed.1, "mission_confirmed");
    assert_eq!(sse.next_within(Duration::from_millis(200)).await, None);

    let versions = app
        .get(&format!("/api/v1/topics/{topic}/mission/versions"))
        .await;
    let items = versions.body["items"].as_array().unwrap();
    assert_eq!(items[0]["status"], "confirmed");
    assert_eq!(items[1]["status"], "superseded");
    assert!(items[1]["confirmed_at"].is_string());
    assert!(items[1]["superseded_at"].is_string());
}

#[tokio::test]
async fn a_tool_error_is_invisible_to_the_user() {
    let mut bad = draft("x");
    bad["success_criteria"] = json!([]);
    let app = App::new(vec![
        vec![tool("save_mission_draft", bad)],
        vec![Step::Delta("Riformulo.")],
    ])
    .await;
    let topic = app.topic().await;
    let message = app.turn(&topic, "ciao").await;
    assert_eq!(
        message["parts"],
        json!([{ "type": "text", "text": "Riformulo." }])
    );
    assert_eq!(app.mission(&topic).await["draft"], Value::Null);
    // The model did get the explanation.
    let requests = app.llm.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
}

// ---------- preferences ----------

fn pref_tool(scope: &str, text: &str) -> Step {
    tool("save_preference", json!({ "scope": scope, "text": text }))
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn the_tutor_saves_preferences_and_the_api_lists_filters_and_deletes_them() {
    let app = App::new(vec![
        vec![
            pref_tool("global", "Risposte brevi"),
            pref_tool("topic", "Esempi in Python"),
        ],
        vec![Step::Delta("Ok.")],
        vec![pref_tool("topic", "Esempi in Go")],
        vec![Step::Delta("Ok.")],
    ])
    .await;
    let rust = app.topic().await;
    let go = app.topic_named("Altra", "Go").await;
    let mut rust_sse = app.events(&rust, None).await;
    let mut go_sse = app.events(&go, None).await;
    app.turn(&rust, "uno").await;
    app.turn(&go, "due").await;

    let saved = rust_sse.until("message_completed").await;
    let saved: Vec<_> = saved.iter().filter(|e| e.1 == "preference_saved").collect();
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0].2["payload"]["preference"]["scope"], "global");
    assert_eq!(saved[0].2["payload"]["preference"]["topic_id"], Value::Null);
    assert_eq!(
        saved[1].2["payload"]["preference"]["topic_id"],
        rust.as_str()
    );
    assert_eq!(
        saved[1].2["payload"]["preference"]["text"],
        "Esempi in Python"
    );

    // All of them, oldest first; the shape of the contract.
    let all = app.get("/api/v1/preferences").await;
    assert_eq!(all.status, StatusCode::OK);
    let items = all.body["items"].as_array().unwrap();
    let texts: Vec<_> = items.iter().map(|p| p["text"].as_str().unwrap()).collect();
    assert_eq!(
        texts,
        ["Risposte brevi", "Esempi in Python", "Esempi in Go"]
    );
    assert_eq!(
        items[1],
        json!({
            "id": items[1]["id"],
            "scope": "topic",
            "topic_id": rust,
            "text": "Esempi in Python",
            "created_at": "2026-11-08T14:36:01.129Z"
        })
    );
    assert_eq!(all.body["next_cursor"], Value::Null);

    // With a topic: the global ones and that topic's.
    let mine = app
        .get(&format!("/api/v1/preferences?topic_id={rust}"))
        .await;
    let texts: Vec<_> = mine.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["text"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(texts, ["Risposte brevi", "Esempi in Python"]);
    let unknown = app
        .get(&format!(
            "/api/v1/preferences?topic_id={}",
            Uuid::from_u128(77)
        ))
        .await;
    assert_eq!(unknown.body["items"].as_array().unwrap().len(), 1);

    // Pagination.
    let page = app.get("/api/v1/preferences?limit=2").await;
    assert_eq!(page.body["items"].as_array().unwrap().len(), 2);
    let cursor = page.body["next_cursor"].as_str().unwrap();
    let following = app
        .get(&format!("/api/v1/preferences?limit=2&cursor={cursor}"))
        .await;
    assert_eq!(following.body["items"][0]["text"], "Esempi in Go");

    // Query errors.
    for q in ["topic_id=nope", "limit=0", "limit=201", "cursor=%%"] {
        let r = app.get(&format!("/api/v1/preferences?{q}")).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{q}");
        assert_eq!(r.body["error"]["code"], "bad_request", "{q}");
    }

    // Deleting a topic preference: event on its topic only.
    let topic_pref = items[1]["id"].as_str().unwrap().to_owned();
    let gone = app
        .delete(&format!("/api/v1/preferences/{topic_pref}"))
        .await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    let e = rust_sse.until("preference_deleted").await.pop().unwrap();
    assert_eq!(
        e.2["payload"],
        json!({ "preference_id": topic_pref, "scope": "topic", "topic_id": rust })
    );
    let again = app
        .delete(&format!("/api/v1/preferences/{topic_pref}"))
        .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);

    // Deleting a global one: every open stream hears it.
    let global = items[0]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        app.delete(&format!("/api/v1/preferences/{global}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let on_rust = rust_sse.until("preference_deleted").await.pop().unwrap();
    assert_eq!(
        on_rust.2["payload"],
        json!({ "preference_id": global, "scope": "global", "topic_id": null })
    );
    let on_go = go_sse.until("preference_deleted").await;
    assert_eq!(
        on_go.last().unwrap().2["payload"]["preference_id"],
        global.as_str()
    );
    // The Go stream never heard about the deleted Rust preference.
    assert!(
        !on_go
            .iter()
            .any(|e| e.2["payload"]["preference_id"] == topic_pref.as_str())
    );
    let left = app.get("/api/v1/preferences").await;
    assert_eq!(left.body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn the_same_preference_twice_is_stored_once_and_announced_once() {
    let app = App::new(vec![
        vec![
            pref_tool("topic", "Esempi in Python"),
            pref_tool("topic", "  esempi in PYTHON "),
        ],
        vec![Step::Delta("Ok.")],
    ])
    .await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    app.turn(&topic, "uno").await;
    let events = sse.until("message_completed").await;
    assert_eq!(
        events.iter().filter(|e| e.1 == "preference_saved").count(),
        1
    );
    let all = app.get("/api/v1/preferences").await;
    assert_eq!(all.body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn deleting_a_topic_or_its_folder_removes_missions_and_topic_preferences() {
    let mut scripts = proposing("x");
    scripts.push(vec![
        pref_tool("topic", "Solo qui"),
        pref_tool("global", "Ovunque"),
    ]);
    scripts.push(vec![Step::Delta("ok")]);
    let app = App::new(scripts).await;
    let topic = app.topic().await;
    app.turn(&topic, "uno").await;
    app.turn(&topic, "due").await;
    let count = |table: &'static str| {
        let pool = app.pool.clone();
        async move {
            let sql = match table {
                "missions" => "SELECT COUNT(*) FROM missions",
                _ => "SELECT COUNT(*) FROM preferences",
            };
            let (n,): (i64,) = sqlx::query_as(sql).fetch_one(&pool).await.unwrap();
            n
        }
    };
    assert_eq!(count("missions").await, 1);
    assert_eq!(count("preferences").await, 2);

    let del = app.delete(&format!("/api/v1/topics/{topic}")).await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    assert_eq!(count("missions").await, 0);
    assert_eq!(count("preferences").await, 1, "the global one stays");
    let left = app.get("/api/v1/preferences").await;
    assert_eq!(left.body["items"][0]["text"], "Ovunque");

    // Same through the folder.
    let app = App::new({
        let mut s = proposing("y");
        s.push(vec![pref_tool("topic", "Solo qui")]);
        s.push(vec![Step::Delta("ok")]);
        s
    })
    .await;
    let f = app.post("/api/v1/folders", json!({ "name": "F" })).await;
    let folder = f.body["id"].as_str().unwrap().to_owned();
    let t = app
        .post(
            &format!("/api/v1/folders/{folder}/topics"),
            json!({ "title": "T" }),
        )
        .await;
    let topic = t.body["id"].as_str().unwrap().to_owned();
    app.turn(&topic, "uno").await;
    app.turn(&topic, "due").await;
    app.delete(&format!("/api/v1/folders/{folder}")).await;
    let (m,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM missions")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let (p,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM preferences")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!((m, p), (0, 0));
}

#[tokio::test]
async fn the_model_sees_the_mission_and_preferences_in_its_context() {
    let mut scripts = proposing("Per lavoro");
    scripts.push(vec![pref_tool("global", "Risposte brevi")]);
    scripts.push(vec![Step::Delta("ok")]);
    scripts.push(vec![Step::Delta("ok")]);
    let app = App::new(scripts).await;
    let topic = app.topic().await;
    app.turn(&topic, "uno").await;
    app.confirm(&topic, json!({ "version": 1 })).await;
    app.turn(&topic, "due").await;
    app.turn(&topic, "tre").await;
    let requests = app.llm.requests.lock().unwrap();
    let last = requests.last().unwrap();
    let context = &last.system[1].text;
    assert!(
        context.contains("Mission confermata dalla persona (versione 1):"),
        "{context}"
    );
    assert!(
        context.contains("- [tutti gli argomenti] Risposte brevi"),
        "{context}"
    );
    assert_eq!(last.tools.len(), 4);
}

#[tokio::test]
async fn a_mid_turn_snapshot_holds_only_text_and_the_card_arrives_with_the_end() {
    let gate = Arc::new(Notify::new());
    let app = App::new(vec![
        vec![
            Step::Delta("Ho capito. "),
            tool("save_mission_draft", draft("x")),
        ],
        vec![Step::Wait(gate.clone()), Step::Delta("Fine.")],
    ])
    .await;
    let topic = app.topic().await;
    let sent = app.send(&topic, "ciao").await;
    let id = sent.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // The tool has run (the draft exists) while the reply is still streaming.
    let mut mid = None;
    for _ in 0..400 {
        if app.mission(&topic).await["draft"].is_object() {
            let found = app
                .messages(&topic)
                .await
                .into_iter()
                .find(|m| m["id"] == id.as_str());
            mid = found;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mid = mid.expect("the draft appears");
    assert_eq!(mid["status"], "streaming");
    assert_eq!(
        mid["parts"],
        json!([{ "type": "text", "text": "Ho capito. " }]),
        "no card until the message closes, so part indexes match the deltas"
    );

    gate.notify_one();
    let done = app.wait_status(&topic, &id, "complete").await;
    let parts = done["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(
        parts[0],
        json!({ "type": "text", "text": "Ho capito. Fine." })
    );
    assert_eq!(parts[1]["type"], "mission_proposal");
}

// ---------- request_lesson (lessons.md §12) ----------

#[tokio::test]
async fn the_tutor_requests_a_lesson_and_the_message_points_to_the_job() {
    let app = App::new(vec![
        vec![
            Step::Delta("Preparo la prossima lezione."),
            tool("request_lesson", json!({})),
        ],
        vec![],
    ])
    .await;
    let topic = app.topic().await;
    sqlx::query("UPDATE topics SET state = 'ready' WHERE id = ?")
        .bind(&topic)
        .execute(&app.pool)
        .await
        .unwrap();
    let mut sse = app.events(&topic, None).await;

    let message = app.turn(&topic, "Passiamo alla prossima lezione").await;

    let jobs = app.get(&format!("/api/v1/topics/{topic}/jobs")).await;
    let items = jobs.body["items"].as_array().unwrap();
    let generation: Vec<_> = items
        .iter()
        .filter(|j| j["kind"] == "lesson_generation")
        .collect();
    assert_eq!(generation.len(), 1, "{}", jobs.body);
    assert_eq!(generation[0]["status"], "queued");
    let parts = message["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["type"], "text");
    assert_eq!(
        parts[1],
        json!({ "type": "lesson_request", "job_id": generation[0]["id"] })
    );

    // `job_updated` comes before the message closes, as for the endpoint.
    let events = sse.until("message_completed").await;
    let all = names(&events);
    let job_at = all.iter().position(|n| *n == "job_updated").unwrap();
    assert!(job_at < all.len() - 1, "{all:?}");
    assert_eq!(
        events[job_at].2["payload"]["job"]["id"],
        generation[0]["id"]
    );
    assert_eq!(
        events.last().unwrap().2["payload"]["message"]["parts"],
        message["parts"]
    );
}
