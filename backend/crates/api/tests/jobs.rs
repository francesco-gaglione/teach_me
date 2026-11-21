//! End-to-end tests of the background jobs (proposal in the phase 4 report) over
//! the real router and SQLite, in process (no sockets), with a scripted model
//! and fake job handlers.

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
            llm,
            setup.options,
        );
        common::sign_in_owner(&state, &pool).await;
        Self {
            router: build_router(state.clone(), setup.limits),
            state,
            pool,
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
            headers: parts.headers,
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
    headers: axum::http::HeaderMap,
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

    /// Reads events until a `job_updated` with the given job status.
    async fn until_job(&mut self, status: &str) -> Vec<(Option<u64>, String, Value)> {
        let mut seen = Vec::new();
        loop {
            let e = self.next_event().await;
            let done = e.1 == "job_updated" && e.2["payload"]["job"]["status"] == status;
            seen.push(e);
            if done {
                return seen;
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

use application::{JobContext, JobFailure, JobHandler};
use async_trait::async_trait;
use domain::{JobErrorCode, JobKind, JobProgress, JobResult};
use std::sync::atomic::AtomicBool;

fn proposing() -> Vec<Vec<Step>> {
    vec![
        vec![
            Step::Delta("Ho capito. "),
            Step::Tool {
                id: "tu_1",
                name: "save_mission_draft",
                input: json!({
                    "why": "Per il nuovo lavoro",
                    "starting_point": "Programmo in Kotlin; di Rust ho letto solo i primi capitoli",
                    "success_criteria": ["Leggo e modifico un servizio axum"],
                    "constraints": [],
                    "out_of_scope": []
                }),
            },
        ],
        vec![],
    ]
}

/// Reports one progress step, then succeeds or fails depending on `fail`.
struct Researcher {
    fail: AtomicBool,
}

#[async_trait]
impl JobHandler for Researcher {
    async fn run(&self, ctx: &JobContext) -> Result<JobResult, JobFailure> {
        ctx.progress(JobProgress::step("searching").with_done(1))
            .await;
        if self.fail.load(Ordering::SeqCst) {
            Err(JobFailure(JobErrorCode::UpstreamFailed))
        } else {
            Ok(JobResult::Research {
                resource_count: 2,
                gap_count: 1,
            })
        }
    }
}

fn researcher(fail: bool) -> Arc<Researcher> {
    Arc::new(Researcher {
        fail: AtomicBool::new(fail),
    })
}

impl App {
    async fn fresh() -> Self {
        Self::with(Setup::new(proposing())).await
    }

    /// A topic with a confirmed first mission (so a research job exists).
    async fn confirmed_topic(&self) -> String {
        let topic = self.topic().await;
        let sent = self.send(&topic, "Voglio imparare Rust").await;
        let id = sent.body["assistant_message"]["id"].as_str().unwrap();
        self.wait_status(&topic, id, "complete").await;
        let r = self
            .post(
                &format!("/api/v1/topics/{topic}/mission/confirm"),
                json!({ "version": 1 }),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        topic
    }

    async fn jobs(&self, topic: &str) -> Vec<Value> {
        let r = self.get(&format!("/api/v1/topics/{topic}/jobs")).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        r.body["items"].as_array().unwrap().clone()
    }

    async fn wait_job(&self, topic: &str, status: &str) -> Value {
        for _ in 0..400 {
            if let Some(job) = self.jobs(topic).await.into_iter().next()
                && job["status"] == status
            {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("job never reached {status}");
    }

    /// Starts the production loop with `handler` for research jobs.
    fn run_jobs(&self, handler: Arc<dyn JobHandler>) -> tokio::task::JoinHandle<()> {
        let runner = self.state.job_runner(vec![(JobKind::Research, handler)]);
        tokio::spawn(Arc::new(runner).run())
    }

    async fn state_of(&self, topic: &str) -> String {
        let r = self.get(&format!("/api/v1/topics/{topic}")).await;
        r.body["state"].as_str().unwrap().to_owned()
    }
}

fn job_events(events: &[(Option<u64>, String, Value)]) -> Vec<String> {
    events
        .iter()
        .filter(|e| e.1 == "job_updated")
        .map(|e| e.2["payload"]["job"]["status"].as_str().unwrap().to_owned())
        .collect()
}

// ---------- tests ----------

#[tokio::test]
async fn jobs_endpoints_need_the_token_and_validate_ids() {
    let app = App::fresh().await;
    let topic = app.topic().await;
    let no_token = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/topics/{topic}/jobs"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_token.status(), StatusCode::UNAUTHORIZED);

    let r = app.get("/api/v1/topics/nope/jobs").await;
    assert_eq!(
        (r.status, r.body["error"]["code"].clone()),
        (StatusCode::BAD_REQUEST, json!("invalid_id"))
    );
    let missing = "01933b4e-0000-7000-8000-00000000ffff";
    assert_eq!(
        app.get(&format!("/api/v1/topics/{missing}/jobs"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let r = app
        .get(&format!("/api/v1/topics/{topic}/jobs?cursor=zzz"))
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = app
        .post(
            &format!("/api/v1/topics/{topic}/jobs/nope/retry"),
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = app
        .post(
            &format!("/api/v1/topics/{topic}/jobs/{missing}/retry"),
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    // A topic that never confirmed a mission has no jobs.
    let r = app.get(&format!("/api/v1/topics/{topic}/jobs")).await;
    assert_eq!(r.body, json!({ "items": [], "next_cursor": null }));
    assert!(r.headers.contains_key("x-last-event-id"));
}

#[tokio::test]
async fn confirming_the_mission_queues_a_research_job_and_without_a_handler_it_just_waits() {
    let app = App::fresh().await;
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    let sent = app.send(&topic, "Voglio imparare Rust").await;
    let id = sent.body["assistant_message"]["id"].as_str().unwrap();
    app.wait_status(&topic, id, "complete").await;
    sse.until("message_completed").await;
    let r = app
        .post(
            &format!("/api/v1/topics/{topic}/mission/confirm"),
            json!({ "version": 1 }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);

    // The same order every time: mission, topic, then the job.
    let events = sse.until("job_updated").await;
    let seen = names(&events);
    assert!(seen.ends_with(&["mission_confirmed", "topic_updated", "job_updated"]));
    let job = &events.last().unwrap().2["payload"]["job"];
    assert_eq!(job["kind"], "research");
    assert_eq!(job["status"], "queued");

    let jobs = app.jobs(&topic).await;
    assert_eq!(jobs.len(), 1);
    let listed = &jobs[0];
    assert_eq!(listed["id"], job["id"]);
    assert_eq!(listed["topic_id"], topic);
    assert_eq!(listed["attempt"], 0);
    assert_eq!(listed["progress"], Value::Null);
    assert_eq!(listed["error"], Value::Null);
    assert_eq!(listed["started_at"], Value::Null);
    assert_eq!(listed["finished_at"], Value::Null);

    // The binary registers no handler yet: nothing runs, nothing fails.
    let runner = app.state.job_runner(Vec::new());
    assert!(!runner.run_next().await.unwrap());
    assert_eq!(app.jobs(&topic).await[0]["status"], "queued");
    assert_eq!(app.state_of(&topic).await, "researching");
    assert!(sse.next_within(Duration::from_millis(100)).await.is_none());
}

#[tokio::test]
async fn a_handler_drives_the_job_and_the_events_tell_the_story() {
    let app = App::fresh().await;
    let _runner = app.run_jobs(researcher(false));
    let topic = app.topic().await;
    let mut sse = app.events(&topic, None).await;
    let sent = app.send(&topic, "Voglio imparare Rust").await;
    let id = sent.body["assistant_message"]["id"].as_str().unwrap();
    app.wait_status(&topic, id, "complete").await;
    app.post(
        &format!("/api/v1/topics/{topic}/mission/confirm"),
        json!({ "version": 1 }),
    )
    .await;

    let events = sse.until_job("succeeded").await;
    assert_eq!(
        job_events(&events),
        ["queued", "running", "running", "succeeded"]
    );
    let progress: Vec<_> = events
        .iter()
        .filter(|e| e.1 == "job_updated")
        .map(|e| e.2["payload"]["job"]["progress"].clone())
        .collect();
    assert_eq!(
        progress[2],
        json!({ "step": "searching", "message": null, "done": 1, "total": null })
    );

    let job = app.wait_job(&topic, "succeeded").await;
    assert_eq!(job["attempt"], 1);
    assert!(job["started_at"].is_string() && job["finished_at"].is_string());
    // Moving the topic on belongs to the handler, not to the runner.
    assert_eq!(app.state_of(&topic).await, "researching");
}

#[tokio::test]
async fn a_failed_job_shows_its_error_and_can_be_retried() {
    let app = App::fresh().await;
    let handler = researcher(true);
    let _runner = app.run_jobs(handler.clone());
    let topic = app.confirmed_topic().await;

    let failed = app.wait_job(&topic, "failed").await;
    assert_eq!(failed["error"]["code"], "upstream_failed");
    assert!(failed["error"]["message"].is_string());
    assert_eq!(failed["attempt"], 1);

    handler.fail.store(false, Ordering::SeqCst);
    let id = failed["id"].as_str().unwrap();
    let mut sse = app.events(&topic, None).await;
    let r = app
        .post(
            &format!("/api/v1/topics/{topic}/jobs/{id}/retry"),
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);
    assert_eq!(r.body["id"], failed["id"]);
    assert_eq!(r.body["status"], "queued");
    assert_eq!(r.body["attempt"], 0);
    assert_eq!(r.body["error"], Value::Null);

    let events = sse.until_job("succeeded").await;
    assert_eq!(
        job_events(&events).first().map(String::as_str),
        Some("queued")
    );
    let done = app.wait_job(&topic, "succeeded").await;
    assert_eq!(done["attempt"], 1);
    assert_eq!(app.jobs(&topic).await.len(), 1, "the same job ran again");
}

#[tokio::test]
async fn retry_conflicts_unless_the_job_is_failed() {
    let app = App::fresh().await;
    let topic = app.confirmed_topic().await;
    let job = app.jobs(&topic).await.remove(0);
    let id = job["id"].as_str().unwrap();
    let r = app
        .post(
            &format!("/api/v1/topics/{topic}/jobs/{id}/retry"),
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.body["error"]["code"], "conflict");
}

async fn mark_running(app: &App, attempt: i64) {
    sqlx::query("UPDATE jobs SET status = 'running', attempt = ?, started_at = created_at")
        .bind(attempt)
        .execute(&app.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_restart_requeues_an_interrupted_job_which_then_completes() {
    let first = App::fresh().await;
    let topic = first.confirmed_topic().await;
    mark_running(&first, 1).await;

    // "Restart": a new application over the same database.
    let second = App::on(first.pool.clone(), Setup::new(vec![])).await;
    let runner = second
        .state
        .job_runner(vec![(JobKind::Research, researcher(false))]);
    let report = runner.recover_interrupted().await.unwrap();
    assert_eq!((report.requeued, report.failed), (1, 0));
    assert_eq!(second.jobs(&topic).await[0]["status"], "queued");
    let _task = tokio::spawn(Arc::new(runner).run());
    let done = second.wait_job(&topic, "succeeded").await;
    assert_eq!(done["attempt"], 2);
}

#[tokio::test]
async fn a_job_that_keeps_interrupting_is_failed_instead_of_looping() {
    let first = App::fresh().await;
    let topic = first.confirmed_topic().await;
    mark_running(&first, 3).await;

    let second = App::on(first.pool.clone(), Setup::new(vec![])).await;
    let report = second
        .state
        .job_runner(Vec::new())
        .recover_interrupted()
        .await
        .unwrap();
    assert_eq!((report.requeued, report.failed), (0, 1));
    let job = second.jobs(&topic).await.remove(0);
    assert_eq!(job["status"], "failed");
    assert_eq!(job["error"]["code"], "interrupted");
}

#[tokio::test]
async fn jobs_go_with_their_topic() {
    let app = App::fresh().await;
    let topic = app.confirmed_topic().await;
    let r = app.delete(&format!("/api/v1/topics/{topic}")).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

// ---------- the Researcher ----------

use domain::llm::LlmEvent;

fn research_script() -> Vec<Step> {
    let reply = json!({
        "resources": [{
            "kind": "knowledge", "source_type": "documentation", "title": "axum docs",
            "url": "https://docs.rs/axum/latest/axum/", "language": "en",
            "annotation": "Documentazione ufficiale del crate.", "use_for": "leggere un servizio axum"
        }],
        "gaps": [{ "area": "Deploy", "note": "Nessuna guida recente." }],
        "communities_skipped": true
    });
    vec![
        Step::Event(LlmEvent::ServerToolUse {
            block: 0,
            name: "web_search".into(),
            input: json!({ "query": "axum documentation" }),
        }),
        Step::Event(LlmEvent::ServerSearchResults {
            block: 1,
            urls: vec!["https://docs.rs/axum/latest/axum".into()],
        }),
        Step::Event(LlmEvent::TextDelta {
            block: 2,
            text: reply.to_string(),
        }),
    ]
}

fn resources_path(topic: &str) -> String {
    format!("/api/v1/topics/{topic}/resources")
}

#[tokio::test]
async fn confirming_the_mission_runs_the_researcher_and_the_sources_appear() {
    let mut scripts = proposing();
    scripts.push(research_script());
    let app = App::with(Setup::new(scripts)).await;
    let _runner = app.run_jobs(app.state.research_handler());
    let topic = app.topic().await;

    let empty = app.get(&resources_path(&topic)).await;
    assert_eq!(empty.status, StatusCode::OK);
    assert_eq!(
        empty.body,
        json!({ "resources": [], "gaps": [], "communities_skipped": false,
                "research": null, "job": null })
    );

    let mut sse = app.events(&topic, None).await;
    let sent = app.send(&topic, "Voglio imparare Rust").await;
    let id = sent.body["assistant_message"]["id"].as_str().unwrap();
    app.wait_status(&topic, id, "complete").await;
    app.post(
        &format!("/api/v1/topics/{topic}/mission/confirm"),
        json!({ "version": 1 }),
    )
    .await;

    let events = sse.until_job("succeeded").await;
    let seen = names(&events);
    let ready = seen.iter().rposition(|n| *n == "topic_updated").unwrap();
    assert_eq!(events[ready].2["payload"]["topic"]["state"], "ready");
    assert!(
        ready < seen.len() - 1,
        "topic_updated comes before the job is done"
    );
    let steps: Vec<_> = events
        .iter()
        .filter(|e| e.1 == "job_updated")
        .filter_map(|e| e.2["payload"]["job"]["progress"]["step"].as_str())
        .collect();
    assert!(
        steps.contains(&"searching") && steps.contains(&"saving"),
        "{steps:?}"
    );
    let done = &events.last().unwrap().2["payload"]["job"];
    assert_eq!(
        done["result"],
        json!({ "resource_count": 1, "gap_count": 1 })
    );
    assert_eq!(app.state_of(&topic).await, "ready");

    let view = app.get(&resources_path(&topic)).await;
    assert_eq!(view.status, StatusCode::OK, "{}", view.body);
    let resource = &view.body["resources"][0];
    // The spelling of the search result wins over the model's.
    assert_eq!(resource["url"], "https://docs.rs/axum/latest/axum");
    assert_eq!(resource["domain"], "docs.rs");
    assert_eq!(resource["kind"], "knowledge");
    assert_eq!(resource["source_type"], "documentation");
    assert!(resource["id"].is_string() && resource["created_at"].is_string());
    assert_eq!(
        view.body["gaps"],
        json!([{ "area": "Deploy", "note": "Nessuna guida recente." }])
    );
    assert_eq!(view.body["communities_skipped"], true);
    assert_eq!(view.body["research"]["mission_version"], 1);
    assert_eq!(view.body["research"]["job_id"], done["id"]);
    assert_eq!(view.body["job"]["status"], "succeeded");
    assert_eq!(view.body["job"]["kind"], "research");
}

#[tokio::test]
async fn a_research_with_an_unusable_answer_fails_as_invalid_output_and_can_be_retried() {
    // Two answers without JSON: the first and the repair.
    let mut scripts = proposing();
    scripts.push(vec![Step::Delta("niente")]);
    scripts.push(vec![Step::Delta("ancora niente")]);
    scripts.push(research_script());
    let app = App::with(Setup::new(scripts)).await;
    let _runner = app.run_jobs(app.state.research_handler());
    let topic = app.confirmed_topic().await;

    let failed = app.wait_job(&topic, "failed").await;
    assert_eq!(failed["error"]["code"], "invalid_output");
    assert!(failed["error"]["message"].is_string());
    assert_eq!(app.state_of(&topic).await, "researching");
    let view = app.get(&resources_path(&topic)).await;
    assert_eq!(view.body["resources"], json!([]));
    assert_eq!(view.body["job"]["error"]["code"], "invalid_output");

    let id = failed["id"].as_str().unwrap();
    let r = app
        .post(
            &format!("/api/v1/topics/{topic}/jobs/{id}/retry"),
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    app.wait_job(&topic, "succeeded").await;
    assert_eq!(app.state_of(&topic).await, "ready");
}

#[tokio::test]
async fn asking_for_a_new_research_is_refused_without_a_mission_or_with_one_running() {
    let app = App::fresh().await;
    let topic = app.topic().await;
    let path = format!("{}/research", resources_path(&topic));

    let r = app.post(&path, json!({})).await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.body);
    assert_eq!(r.body["error"]["code"], "conflict");

    // A confirmed mission queued its job; no runner, so it stays active.
    let app = App::fresh().await;
    let topic = app.confirmed_topic().await;
    let r = app
        .post(&format!("{}/research", resources_path(&topic)), json!({}))
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.body);

    let missing = "01933b4e-0000-7000-8000-00000000ffff";
    for (method, path) in [
        (Method::GET, resources_path(missing)),
        (
            Method::POST,
            format!("{}/research", resources_path(missing)),
        ),
    ] {
        let r = app.call(method, &path, None).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
    }
    for path in [
        "/api/v1/topics/nope/resources",
        "/api/v1/topics/nope/resources/research",
    ] {
        let method = if path.ends_with("research") {
            Method::POST
        } else {
            Method::GET
        };
        let r = app.call(method, path, None).await;
        assert_eq!(
            (r.status, r.body["error"]["code"].clone()),
            (StatusCode::BAD_REQUEST, json!("invalid_id"))
        );
    }
}

#[tokio::test]
async fn a_new_research_is_queued_as_a_202_job_once_the_previous_one_is_over() {
    let app = App::fresh().await;
    let runner = app.run_jobs(researcher(false));
    let topic = app.confirmed_topic().await;
    app.wait_job(&topic, "succeeded").await;
    runner.abort();

    let r = app
        .post(&format!("{}/research", resources_path(&topic)), json!({}))
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);
    assert_eq!(r.body["kind"], "research");
    assert_eq!(r.body["status"], "queued");
    assert_eq!(r.body["result"], Value::Null);
    let view = app.get(&resources_path(&topic)).await;
    assert_eq!(view.body["job"]["id"], r.body["id"]);

    let again = app
        .post(&format!("{}/research", resources_path(&topic)), json!({}))
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn an_older_failed_job_cannot_be_retried_once_a_newer_one_exists() {
    let app = App::fresh().await;
    let _runner = app.run_jobs(researcher(true));
    let topic = app.confirmed_topic().await;
    let old = app.wait_job(&topic, "failed").await;

    // A new research is asked for (the old one is over, so it is accepted).
    let r = app
        .post(&format!("{}/research", resources_path(&topic)), json!({}))
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);
    let newer = app.wait_job(&topic, "failed").await;
    assert_ne!(newer["id"], old["id"]);

    let r = app
        .post(
            &format!(
                "/api/v1/topics/{topic}/jobs/{}/retry",
                old["id"].as_str().unwrap()
            ),
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.body);
    assert_eq!(r.body["error"]["code"], "conflict");
    // The newest one can still be retried.
    let r = app
        .post(
            &format!(
                "/api/v1/topics/{topic}/jobs/{}/retry",
                newer["id"].as_str().unwrap()
            ),
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);
}
