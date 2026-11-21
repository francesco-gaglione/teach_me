//! End-to-end tests of the chat contract (`docs/contracts/chat.md`) over the
//! real router and SQLite, in process (no sockets), with a scripted model.

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
use domain::llm::LlmError;
use domain::{Clock, IdGenerator};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use infrastructure::EventHubConfig;
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
    state: AppState,
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
            state,
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
            headers: parts.headers,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }

    async fn get(&self, path: &str) -> Res {
        self.call(Method::GET, path, None).await
    }
    async fn post(&self, path: &str, body: Value) -> Res {
        self.call(Method::POST, path, Some(body)).await
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
    headers: axum::http::HeaderMap,
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

fn gate() -> Arc<Notify> {
    Arc::new(Notify::new())
}

// ---------- REST ----------

#[tokio::test]
async fn send_returns_202_with_both_messages_and_the_reply_completes() {
    let app = App::new(vec![vec![Step::Delta("Ciao "), Step::Delta("mondo")]]).await;
    let topic = app.topic().await;
    let r = app.send(&topic, "  Voglio imparare Rust  ").await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);

    let user = &r.body["user_message"];
    assert_eq!(user["role"], "user");
    assert_eq!(user["status"], "complete");
    assert_eq!(
        user["parts"],
        json!([{ "type": "text", "text": "Voglio imparare Rust" }])
    );
    assert_eq!(user["error"], Value::Null);
    assert_eq!(user["topic_id"], topic);
    let assistant = &r.body["assistant_message"];
    assert_eq!(assistant["role"], "assistant");
    assert_eq!(assistant["status"], "streaming");
    assert_eq!(assistant["parts"], json!([]));
    assert_eq!(assistant["created_at"], "2026-11-08T14:36:01.129Z");

    let id = assistant["id"].as_str().unwrap();
    let done = app.wait_status(&topic, id, "complete").await;
    assert_eq!(
        done["parts"],
        json!([{ "type": "text", "text": "Ciao mondo" }])
    );

    // What the model was asked: the user's text, in a request for the Tutor.
    let seen = app.llm.requests.lock().unwrap()[0].clone();
    assert_eq!(seen.messages.len(), 1);
    assert!(seen.system[1].text.contains("\"Rust\""));
}

#[tokio::test]
async fn first_message_moves_the_topic_to_interviewing() {
    let app = App::new(vec![vec![Step::Delta("a")]]).await;
    let topic = app.topic().await;
    assert_eq!(
        app.get(&format!("/api/v1/topics/{topic}")).await.body["state"],
        "new"
    );
    let r = app.send(&topic, "ciao").await;
    let id = r.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.wait_status(&topic, &id, "complete").await;
    assert_eq!(
        app.get(&format!("/api/v1/topics/{topic}")).await.body["state"],
        "interviewing"
    );
}

#[tokio::test]
async fn list_is_newest_first_with_cursor_pagination() {
    let app = App::new(vec![vec![Step::Delta("a1")], vec![Step::Delta("a2")]]).await;
    let topic = app.topic().await;
    for text in ["q1", "q2"] {
        let r = app.send(&topic, text).await;
        let id = r.body["assistant_message"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        app.wait_status(&topic, &id, "complete").await;
    }
    let all = app.messages(&topic).await;
    let texts: Vec<_> = all
        .iter()
        .map(|m| m["parts"][0]["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["a2", "q2", "a1", "q1"]);

    let page = app
        .get(&format!("/api/v1/topics/{topic}/messages?limit=3"))
        .await;
    assert_eq!(page.body["items"].as_array().unwrap().len(), 3);
    let cursor = page.body["next_cursor"].as_str().unwrap().to_owned();
    let rest = app
        .get(&format!(
            "/api/v1/topics/{topic}/messages?limit=3&cursor={cursor}"
        ))
        .await;
    assert_eq!(rest.body["items"].as_array().unwrap().len(), 1);
    assert_eq!(rest.body["items"][0]["parts"][0]["text"], "q1");
    assert_eq!(rest.body["next_cursor"], Value::Null);

    for bad in ["limit=0", "limit=201", "cursor=%25%25", "cursor=Zg"] {
        let r = app
            .get(&format!("/api/v1/topics/{topic}/messages?{bad}"))
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(r.body["error"]["code"], "bad_request");
    }
}

#[tokio::test]
async fn path_ids_and_missing_topics() {
    let app = App::new(vec![]).await;
    let ghost = "01933b4e-0000-7000-8000-00000000ffff";
    for path in [
        "/api/v1/topics/nope/messages",
        "/api/v1/topics/nope/events",
        &format!("/api/v1/topics/{ghost}/messages/nope/retry"),
    ] {
        let method = if path.ends_with("retry") {
            Method::POST
        } else {
            Method::GET
        };
        let r = app.call(method, path, None).await;
        assert_eq!(
            (r.status, r.body["error"]["code"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_id")),
            "{path}"
        );
    }
    assert_eq!(
        app.get(&format!("/api/v1/topics/{ghost}/messages"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(app.send(ghost, "x").await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.get(&format!("/api/v1/topics/{ghost}/events"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let retry = app
        .call(
            Method::POST,
            &format!("/api/v1/topics/{ghost}/messages/{ghost}/retry"),
            None,
        )
        .await;
    assert_eq!(retry.status, StatusCode::NOT_FOUND);
    // The endpoints are behind the token.
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/topics/{ghost}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn text_validation_errors_are_422_with_details() {
    let app = App::new(vec![]).await;
    let topic = app.topic().await;
    let path = format!("/api/v1/topics/{topic}/messages");
    for body in [
        json!({}),
        json!({ "text": "" }),
        json!({ "text": " \n " }),
        json!({ "text": null }),
    ] {
        let r = app.post(&path, body).await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            r.body["error"]["details"],
            json!([{ "field": "text", "issue": "empty" }])
        );
    }
    let r = app.send(&topic, &"a".repeat(8001)).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        r.body["error"]["details"],
        json!([{ "field": "text", "issue": "too_long" }])
    );
    // 8000 code points (24000 bytes) are fine.
    assert_eq!(
        app.send(&topic, &"日".repeat(8000)).await.status,
        StatusCode::ACCEPTED
    );

    // Wrong type: 422 without details. Nothing from the failed attempts is stored.
    let r = app.post(&path, json!({ "text": 5 })).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.body["error"]["details"], Value::Null);
}

#[tokio::test]
async fn a_second_send_while_streaming_is_409_and_stores_nothing() {
    let g = gate();
    let app = App::new(vec![
        vec![Step::Delta("a"), Step::Wait(g.clone())],
        vec![Step::Delta("b")],
    ])
    .await;
    let topic = app.topic().await;
    let first = app.send(&topic, "uno").await;
    let id = first.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let second = app.send(&topic, "due").await;
    assert_eq!(second.status, StatusCode::CONFLICT);
    assert_eq!(second.body["error"]["code"], "conflict");
    assert_eq!(app.messages(&topic).await.len(), 2);

    g.notify_one();
    app.wait_status(&topic, &id, "complete").await;
    assert_eq!(app.send(&topic, "due").await.status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn failed_replies_keep_partial_text_and_can_be_retried_once_last() {
    let app = App::new(vec![
        vec![
            Step::Delta("parz"),
            Step::Fail(LlmError::RateLimited { retry_after: None }),
        ],
        vec![Step::Delta("ok")],
    ])
    .await;
    let topic = app.topic().await;
    let sent = app.send(&topic, "uno").await;
    let id = sent.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let user_id = sent.body["user_message"]["id"].as_str().unwrap().to_owned();

    let failed = app.wait_status(&topic, &id, "failed").await;
    assert_eq!(failed["parts"], json!([{ "type": "text", "text": "parz" }]));
    assert_eq!(failed["error"]["code"], "rate_limited");
    assert!(failed["error"]["message"].as_str().is_some());
    assert_eq!(
        failed["error"].as_object().unwrap().len(),
        2,
        "code and message only"
    );

    // User messages and unknown ids cannot be retried.
    let retry_path = |m: &str| format!("/api/v1/topics/{topic}/messages/{m}/retry");
    let r = app.call(Method::POST, &retry_path(&user_id), None).await;
    assert_eq!(
        (r.status, r.body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict"))
    );

    let r = app.call(Method::POST, &retry_path(&id), None).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);
    assert_eq!(r.body["id"], id.as_str());
    assert_eq!(r.body["status"], "streaming");
    assert_eq!(r.body["parts"], json!([]));
    assert_eq!(r.body["error"], Value::Null);

    let done = app.wait_status(&topic, &id, "complete").await;
    assert_eq!(done["parts"][0]["text"], "ok");
    assert_eq!(done["error"], Value::Null);

    // Complete now: not retryable.
    let r = app.call(Method::POST, &retry_path(&id), None).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn only_the_last_message_can_be_retried() {
    let app = App::new(vec![
        vec![Step::Fail(LlmError::Overloaded)],
        vec![Step::Delta("ok")],
    ])
    .await;
    let topic = app.topic().await;
    let first = app.send(&topic, "uno").await;
    let first_id = first.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.wait_status(&topic, &first_id, "failed").await;
    let second = app.send(&topic, "due").await;
    let second_id = second.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.wait_status(&topic, &second_id, "complete").await;
    let r = app
        .call(
            Method::POST,
            &format!("/api/v1/topics/{topic}/messages/{first_id}/retry"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
}

// ---------- SSE ----------

#[tokio::test]
async fn stream_delivers_the_documented_event_sequence() {
    let app = App::new(vec![vec![Step::Delta("Perché "), Step::Delta("Rust?")]]).await;
    let topic = app.topic().await;
    let base = app.state.events.id_base();
    let mut sse = app.events(&topic, None).await;

    let sent = app.send(&topic, "ciao").await;
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
            "message_delta",
            "message_completed"
        ]
    );
    // Consecutive ids from 1; `event` equals `data.type`; envelope shape.
    for (i, (id, name, data)) in events.iter().enumerate() {
        assert_eq!(*id, Some(base + i as u64 + 1));
        assert_eq!(data["type"], name.as_str());
        assert_eq!(data["topic_id"], topic.as_str());
        assert!(data["at"].as_str().unwrap().ends_with('Z'));
        assert!(data["payload"].is_object());
    }
    assert_eq!(events[0].2["payload"]["message"]["role"], "user");
    assert_eq!(events[1].2["payload"]["message"]["status"], "streaming");
    assert_eq!(events[1].2["payload"]["message"]["parts"], json!([]));
    assert_eq!(events[2].2["payload"]["topic"]["state"], "interviewing");
    assert_eq!(
        events[3].2["payload"],
        json!({ "message_id": assistant_id, "part_index": 0, "text": "Perché " })
    );
    assert_eq!(events[4].2["payload"]["text"], "Rust?");
    let completed = &events[5].2["payload"]["message"];
    assert_eq!(completed["status"], "complete");
    assert_eq!(
        completed["parts"],
        json!([{ "type": "text", "text": "Perché Rust?" }])
    );
}

#[tokio::test]
async fn failed_generation_ends_with_message_failed_and_partial_text() {
    let app = App::new(vec![vec![
        Step::Delta("meta"),
        Step::Fail(LlmError::Unavailable),
    ]])
    .await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    app.send(&topic, "ciao").await;
    let events = sse.until("message_failed").await;
    assert!(!names(&events).contains(&"message_completed"));
    let failed = &events.last().unwrap().2["payload"]["message"];
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["error"]["code"], "upstream_failed");
    assert_eq!(failed["parts"][0]["text"], "meta");
}

#[tokio::test]
async fn retry_announces_the_restart_with_message_created() {
    let app = App::new(vec![
        vec![Step::Fail(LlmError::Overloaded)],
        vec![Step::Delta("ok")],
    ])
    .await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    let sent = app.send(&topic, "ciao").await;
    let id = sent.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    sse.until("message_failed").await;
    app.call(
        Method::POST,
        &format!("/api/v1/topics/{topic}/messages/{id}/retry"),
        None,
    )
    .await;
    let events = sse.until("message_completed").await;
    assert_eq!(
        names(&events),
        ["message_created", "message_delta", "message_completed"]
    );
    assert_eq!(events[0].2["payload"]["message"]["id"], id.as_str());
    assert_eq!(events[0].2["payload"]["message"]["status"], "streaming");
}

#[tokio::test]
async fn reconnecting_with_last_event_id_replays_only_what_was_missed() {
    let app = App::new(vec![vec![Step::Delta("a"), Step::Delta("b")]]).await;
    let topic = app.topic().await;
    let mut first = app.events(&topic, None).await;
    app.send(&topic, "ciao").await;
    // Read up to the first delta (event 4) and "lose the connection".
    let seen = first.until("message_delta").await;
    let last_seen = seen.last().unwrap().0.unwrap();
    let base = app.state.events.id_base();
    assert_eq!(last_seen, base + 4);
    drop(first);

    let mut second = app.events(&topic, Some(&last_seen.to_string())).await;
    let rest = second.until("message_completed").await;
    assert_eq!(
        rest.iter().map(|e| e.0.unwrap()).collect::<Vec<_>>(),
        [base + 5, base + 6]
    );
    assert_eq!(names(&rest), ["message_delta", "message_completed"]);
    assert_eq!(rest[0].2["payload"]["text"], "b");
    // Fully caught up: nothing replayed, then live events only.
    let mut third = app.events(&topic, Some(&(base + 6).to_string())).await;
    assert!(
        third
            .next_within(Duration::from_millis(100))
            .await
            .is_none()
    );
}

#[tokio::test]
async fn out_of_window_ids_get_resync_required_first() {
    let mut setup = Setup::new(vec![vec![
        Step::Delta("a"),
        Step::Delta("b"),
        Step::Delta("c"),
    ]]);
    // Keep only 3 events: ids 1-3 fall out of the buffer.
    setup.options.event_hub = EventHubConfig {
        min_events: 3,
        hard_cap: 3,
        ..EventHubConfig::default()
    };
    let app = App::with(setup).await;
    let topic = app.topic().await;
    let mut live = app.events(&topic, None).await;
    let sent = app.send(&topic, "ciao").await;
    let id = sent.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    live.until("message_completed").await; // 7 events published; ids 5-7 stay buffered
    app.wait_status(&topic, &id, "complete").await;

    let base = app.state.events.id_base();
    let mut sse = app.events(&topic, Some(&(base + 1).to_string())).await;
    let (rid, name, data) = sse.next_event().await;
    assert_eq!(name, "resync_required");
    assert_eq!(data["type"], "resync_required");
    assert_eq!(data["payload"], json!({}));
    assert_eq!(
        rid,
        Some(base + 7),
        "carries the latest id so the client adopts it"
    );
    // Reconnecting with that id is clean.
    let mut again = app.events(&topic, Some(&(base + 7).to_string())).await;
    assert!(
        again
            .next_within(Duration::from_millis(100))
            .await
            .is_none()
    );

    // Inside the buffer: replay, no resync.
    let mut inside = app.events(&topic, Some(&(base + 4).to_string())).await;
    assert_eq!(inside.next_event().await.0, Some(base + 5));
}

#[tokio::test]
async fn ids_from_before_a_restart_get_resync_required() {
    let app = App::new(vec![]).await;
    let topic = app.topic().await;
    // Ids from a previous run (below this run's base) or from the future resync.
    for header in ["7", "99999999999999999999999", "not-a-number"] {
        let mut sse = app.events(&topic, Some(header)).await;
        let (id, name, _) = sse.next_event().await;
        assert_eq!(
            (id, name.as_str()),
            (Some(app.state.events.id_base()), "resync_required"),
            "{header}"
        );
    }
    // An empty header means no header.
    let mut sse = app.events(&topic, Some(" ")).await;
    assert!(sse.next_within(Duration::from_millis(100)).await.is_none());
}

#[tokio::test]
async fn heartbeat_is_a_ping_comment() {
    let mut setup = Setup::new(vec![]);
    setup.options.sse_ping = Duration::from_millis(40);
    let app = App::with(setup).await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    assert_eq!(sse.next().await, Frame::Comment(": ping".into()));
    assert_eq!(sse.next().await, Frame::Comment(": ping".into()));
}

#[tokio::test]
async fn default_heartbeat_interval_is_fifteen_seconds() {
    assert_eq!(AppOptions::default().sse_ping, Duration::from_secs(15));
}

#[tokio::test]
async fn the_stream_is_not_cut_by_the_rest_timeout() {
    let mut setup = Setup::new(vec![vec![Step::Delta("tardi")]]);
    setup.limits.request_timeout = Duration::from_millis(100);
    setup.options.sse_ping = Duration::from_secs(60);
    let app = App::with(setup).await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    app.send(&topic, "ciao").await;
    let events = sse.until("message_completed").await;
    assert!(names(&events).contains(&"message_delta"));
}

#[tokio::test]
async fn several_clients_see_the_same_events() {
    let app = App::new(vec![vec![Step::Delta("x")]]).await;
    let topic = app.topic().await;
    let mut a = app.events(&topic, None).await;
    let mut b = app.events(&topic, None).await;
    app.send(&topic, "ciao").await;
    let ea = a.until("message_completed").await;
    let eb = b.until("message_completed").await;
    assert_eq!(
        ea.iter().map(|e| (e.0, e.1.clone())).collect::<Vec<_>>(),
        eb.iter().map(|e| (e.0, e.1.clone())).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn events_of_other_topics_are_not_delivered() {
    let app = App::new(vec![vec![Step::Delta("x")]]).await;
    let one = app.topic().await;
    let other = app
        .post(
            &format!(
                "/api/v1/folders/{}/topics",
                app.get("/api/v1/folders").await.body["items"][0]["id"]
                    .as_str()
                    .unwrap()
            ),
            json!({ "title": "Altro" }),
        )
        .await
        .body["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut quiet = app.events(&other, None).await;
    app.send(&one, "ciao").await;
    assert!(
        quiet
            .next_within(Duration::from_millis(150))
            .await
            .is_none()
    );
}

#[tokio::test]
async fn patching_a_topic_emits_topic_updated() {
    let app = App::new(vec![]).await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    let r = app
        .call(
            Method::PATCH,
            &format!("/api/v1/topics/{topic}"),
            Some(json!({ "title": "Rust avanzato" })),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let (id, name, data) = sse.next_event().await;
    assert_eq!(
        (id, name.as_str()),
        (Some(app.state.events.id_base() + 1), "topic_updated")
    );
    assert_eq!(data["payload"]["topic"]["title"], "Rust avanzato");
    assert_eq!(data["payload"]["topic"], r.body);

    // A no-op patch emits nothing.
    app.call(
        Method::PATCH,
        &format!("/api/v1/topics/{topic}"),
        Some(json!({})),
    )
    .await;
    assert!(sse.next_within(Duration::from_millis(100)).await.is_none());
}

// ---------- lifecycle ----------

#[tokio::test]
async fn generation_survives_the_client_disconnecting() {
    let g = gate();
    let app = App::new(vec![vec![
        Step::Delta("a"),
        Step::Wait(g.clone()),
        Step::Delta("b"),
    ]])
    .await;
    let topic = app.topic().await;
    let sse = app.events(&topic, None).await;
    let sent = app.send(&topic, "ciao").await;
    let id = sent.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(sse); // the app goes away mid-reply
    g.notify_one();
    let done = app.wait_status(&topic, &id, "complete").await;
    assert_eq!(done["parts"][0]["text"], "ab");
}

#[tokio::test]
async fn deleting_the_topic_removes_messages_and_stops_the_reply_quietly() {
    let g = gate();
    let app = App::new(vec![vec![
        Step::Delta("a"),
        Step::Wait(g.clone()),
        Step::Delta("b"),
    ]])
    .await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    app.send(&topic, "ciao").await;
    sse.until("message_delta").await;

    let deleted = app
        .call(Method::DELETE, &format!("/api/v1/topics/{topic}"), None)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    g.notify_one();

    // The worker notices the topic is gone and closes the stream: no panic,
    // no completion event.
    let rest = tokio::time::timeout(Duration::from_secs(5), async {
        let mut out = Vec::new();
        while let Some(Frame::Event { name, .. }) = sse.next_within(Duration::from_secs(5)).await {
            out.push(name);
        }
        out
    })
    .await
    .unwrap();
    assert!(
        !rest
            .iter()
            .any(|n| n == "message_completed" || n == "message_failed"),
        "{rest:?}"
    );

    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chat_messages")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    assert_eq!(
        app.get(&format!("/api/v1/topics/{topic}/messages"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn deleting_the_folder_cascades_to_messages() {
    let app = App::new(vec![vec![Step::Delta("a")]]).await;
    let topic = app.topic().await;
    let sent = app.send(&topic, "ciao").await;
    let id = sent.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.wait_status(&topic, &id, "complete").await;
    let folder = app.get("/api/v1/folders").await.body["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.call(Method::DELETE, &format!("/api/v1/folders/{folder}"), None)
        .await;
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chat_messages")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn restart_turns_streaming_messages_into_retryable_internal_failures() {
    let g = gate();
    let first = App::new(vec![vec![Step::Delta("meta"), Step::Wait(g.clone())]]).await;
    let topic = first.topic().await;
    let sent = first.send(&topic, "ciao").await;
    let id = sent.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for _ in 0..200 {
        if first.messages(&topic).await[0]["parts"][0]["text"] == "meta" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // "Restart": a new application over the same database.
    let second = App::on(
        first.pool.clone(),
        Setup::new(vec![vec![Step::Delta("ok")]]),
    )
    .await;
    assert_eq!(second.state.chat.recover_interrupted().await.unwrap(), 1);
    let failed = second.wait_status(&topic, &id, "failed").await;
    assert_eq!(failed["error"]["code"], "internal");
    assert_eq!(failed["parts"][0]["text"], "meta");

    let r = second
        .call(
            Method::POST,
            &format!("/api/v1/topics/{topic}/messages/{id}/retry"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    let done = second.wait_status(&topic, &id, "complete").await;
    assert_eq!(done["parts"][0]["text"], "ok");
    g.notify_one(); // let the orphaned worker of the "old process" end
}

#[tokio::test]
async fn closing_the_hub_ends_open_streams_for_graceful_shutdown() {
    let app = App::new(vec![]).await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    app.state.events.close_all();
    assert!(sse.next_within(Duration::from_secs(2)).await.is_none());
}

// ---------- review follow-ups ----------

#[tokio::test]
async fn two_parallel_sends_give_one_202_one_409_and_one_turn() {
    let g = gate();
    let app = App::new(vec![vec![Step::Wait(g.clone()), Step::Delta("x")]]).await;
    let topic = app.topic().await;
    let (a, b) = tokio::join!(app.send(&topic, "uno"), app.send(&topic, "due"));
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::ACCEPTED, StatusCode::CONFLICT]);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chat_messages")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 2, "one user message and one assistant message");
    g.notify_one();
}

#[tokio::test]
async fn deleting_a_topic_or_folder_without_generation_closes_open_streams() {
    let app = App::new(vec![]).await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    let r = app
        .call(Method::DELETE, &format!("/api/v1/topics/{topic}"), None)
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert!(sse.next_within(Duration::from_secs(2)).await.is_none());

    let other = App::new(vec![]).await;
    let topic = other.topic().await;
    let folder = other.get("/api/v1/folders").await.body["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut sse = other.events(&topic, None).await;
    other
        .call(Method::DELETE, &format!("/api/v1/folders/{folder}"), None)
        .await;
    assert!(sse.next_within(Duration::from_secs(2)).await.is_none());
}

#[tokio::test]
async fn a_late_delta_after_deletion_leaves_no_channel_behind() {
    let g = gate();
    let app = App::new(vec![vec![
        Step::Delta("a"),
        Step::Wait(g.clone()),
        Step::Delta("b"),
    ]])
    .await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    app.send(&topic, "ciao").await;
    sse.until("message_delta").await;
    app.call(Method::DELETE, &format!("/api/v1/topics/{topic}"), None)
        .await;
    g.notify_one(); // the worker still tries to publish "b"
    tokio::time::sleep(Duration::from_millis(100)).await;
    // The closed topic stays closed: HTTP says 404, and the hub itself hands
    // out an already-ended subscription instead of a fresh channel.
    let res = app
        .router
        .clone()
        .oneshot(App::request(
            Method::GET,
            &format!("/api/v1/topics/{topic}/events"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let id = domain::TopicId::new(topic.parse().unwrap());
    let mut sub = app.state.events.subscribe(domain::UserId::OWNER, id, None);
    assert!(matches!(
        sub.live.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Closed)
    ));
}

#[tokio::test]
async fn rest_snapshot_and_replay_line_up_on_last_event_id() {
    // Nothing is flushed mid-stream, so the stored text lags the deltas.
    let g = gate();
    let mut setup = Setup::new(vec![vec![
        Step::Delta("a"),
        Step::Delta("b"),
        Step::Wait(g.clone()),
        Step::Delta("c"),
    ]]);
    setup.options.chat.flush_interval = Duration::from_secs(3600);
    let app = App::with(setup).await;
    let topic = app.topic().await;
    let base = app.state.events.id_base();

    // Before any event: the run's base, which is itself a valid position.
    let empty = app.get(&format!("/api/v1/topics/{topic}/messages")).await;
    assert_eq!(empty.headers["x-last-event-id"], base.to_string().as_str());

    let mut live = app.events(&topic, None).await;
    let sent = app.send(&topic, "ciao").await;
    let assistant_id = sent.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    live.until("message_delta").await; // a
    live.until("message_delta").await; // b -> id base + 5

    let snapshot = app.get(&format!("/api/v1/topics/{topic}/messages")).await;
    let header: u64 = snapshot.headers["x-last-event-id"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(header, base + 5);
    let streaming = snapshot.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == assistant_id.as_str())
        .unwrap();
    assert_eq!(streaming["parts"], json!([]), "text not flushed yet");
    let reflected = streaming["last_event_id"].as_u64().unwrap();
    assert_eq!(reflected, base + 2, "the message_created of the assistant");
    assert!(reflected < header);

    // The client opens the stream from min(header, last_event_id) and gets
    // every delta the snapshot does not contain.
    let from = header.min(reflected);
    let mut replay = app.events(&topic, Some(&from.to_string())).await;
    let (id, name, _) = replay.next_event().await;
    assert_eq!((id, name.as_str()), (Some(base + 3), "topic_updated"));
    let texts: Vec<String> = {
        let mut out = Vec::new();
        for _ in 0..2 {
            let (_, name, data) = replay.next_event().await;
            assert_eq!(name, "message_delta");
            out.push(data["payload"]["text"].as_str().unwrap().to_owned());
        }
        out
    };
    assert_eq!(texts, ["a", "b"]);

    // Once complete, the row says which event closed it.
    g.notify_one();
    let done = app.wait_status(&topic, &assistant_id, "complete").await;
    let completed_id = replay
        .until("message_completed")
        .await
        .last()
        .unwrap()
        .0
        .unwrap();
    for _ in 0..200 {
        let again = app.wait_status(&topic, &assistant_id, "complete").await;
        if again["last_event_id"].as_u64() == Some(completed_id) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("last_event_id never reached {completed_id}: {done}");
}

#[tokio::test]
async fn the_list_header_is_present_on_every_page() {
    let app = App::new(vec![vec![Step::Delta("a")]]).await;
    let topic = app.topic().await;
    let sent = app.send(&topic, "ciao").await;
    let id = sent.body["assistant_message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.wait_status(&topic, &id, "complete").await;
    let page = app
        .get(&format!("/api/v1/topics/{topic}/messages?limit=1"))
        .await;
    assert!(
        page.headers["x-last-event-id"]
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 0
    );
}

#[tokio::test]
async fn a_new_topic_after_a_restart_reports_the_base_and_streams_from_it() {
    // "Restart": a second app over the same database, started a second later.
    let first = App::new(vec![]).await;
    let topic = first.topic().await;
    let second = App::on(first.pool.clone(), Setup::new(vec![vec![Step::Delta("x")]])).await;
    let base = second.state.events.id_base();

    let listed = second
        .get(&format!("/api/v1/topics/{topic}/messages"))
        .await;
    let header = listed.headers["x-last-event-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(header, base.to_string());

    // The header value is a valid position: no resync, then live events.
    let mut sse = second.events(&topic, Some(&header)).await;
    assert!(sse.next_within(Duration::from_millis(100)).await.is_none());
    second.send(&topic, "ciao").await;
    let (id, name, _) = sse.next_event().await;
    assert_eq!((id, name.as_str()), (Some(base + 1), "message_created"));
    sse.until("message_completed").await;
}
