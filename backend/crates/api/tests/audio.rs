#![allow(clippy::too_many_lines, clippy::needless_pass_by_value)] // one scenario over the whole flow
//! Read-aloud triggers and wiring (ADR 0013 §5, §11) over the real router,
//! SQLite and a clip directory, with a scripted model and a fake speech engine.

mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::app::{Limits, build_router};
use api::state::{AppOptions, AppState, AudioSetup};
use application::{AudioSettings, AudioStep};
use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::{ScriptedLlm, Step};
use domain::{Clock, IdGenerator, JobKind};
use domain::{EngineInfo, SpeechError, SpeechSynthesizer, SynthesizedClip, Voice};
use http_body_util::BodyExt;
use infrastructure::FsClipStore;
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

struct Raw {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    bytes: Vec<u8>,
}

struct App {
    router: Router,
    pool: Pool,
    state: AppState,
    dir: std::path::PathBuf,
}

struct FakeEngine {
    info: EngineInfo,
    voices: Vec<Voice>,
}

#[async_trait]
impl SpeechSynthesizer for FakeEngine {
    fn engine(&self) -> &EngineInfo {
        &self.info
    }
    fn voices(&self) -> &[Voice] {
        &self.voices
    }
    fn default_voice(&self) -> &'static str {
        "vivian"
    }
    async fn synthesize(
        &self,
        text: &str,
        voice: &str,
        _language: domain::ContentLanguage,
    ) -> Result<SynthesizedClip, SpeechError> {
        Ok(SynthesizedClip {
            audio: format!("{voice}:{text}").into_bytes(),
            duration_ms: 1000,
        })
    }
}

fn fake_engine() -> Arc<FakeEngine> {
    Arc::new(FakeEngine {
        info: EngineInfo {
            engine: "fake".into(),
            model: "m".into(),
            params: "p".into(),
        },
        voices: ["vivian", "serena"]
            .map(|id| Voice {
                id: id.into(),
                label: id[..1].to_uppercase() + &id[1..],
            })
            .to_vec(),
    })
}

fn say(text: &str) -> Vec<Step> {
    vec![Step::Delta(Box::leak(text.to_owned().into_boxed_str()))]
}

impl App {
    async fn new(scripts: Vec<Vec<Step>>, audio: bool) -> Self {
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        let llm = ScriptedLlm::with(scripts);
        let mut options = AppOptions::default();
        options.chat.flush_interval = Duration::ZERO;
        let dir = std::env::temp_dir().join(format!("teach_me_audio_{}", Uuid::now_v7()));
        let audio = if audio {
            let store = FsClipStore::new(dir.clone());
            store.prepare().await.unwrap();
            Some(AudioSetup {
                synthesizer: fake_engine(),
                store: Arc::new(store),
                settings: AudioSettings::default(),
            })
        } else {
            None
        };
        let state = AppState::with_audio(
            Arc::new(TestClock(Mutex::new(
                datetime!(2026-11-09 15:00:00.000 UTC),
            ))),
            Arc::new(SeqIds(AtomicU64::new(0))),
            common::auth(),
            &pool,
            llm.clone(),
            options,
            api::state::DocumentSetup::unavailable(),
            audio,
        );
        common::sign_in_owner(&state, &pool).await;
        let runner = state.job_runner(vec![
            (JobKind::LessonGeneration, state.generation_handler()),
            (JobKind::LessonCompletion, state.completion_handler()),
        ]);
        tokio::spawn(Arc::new(runner).run());
        Self {
            router: build_router(state.clone(), Limits::default()),
            pool,
            state,
            dir,
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

    /// A request with the given token, keeping the raw body and the headers
    /// (the clips are not JSON).
    async fn fetch(&self, token: &str, path: &str, range: Option<&str>) -> Raw {
        let mut req = Request::builder()
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"));
        if let Some(range) = range {
            req = req.header(header::RANGE, range);
        }
        let res = self
            .router
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (parts, body) = res.into_parts();
        Raw {
            status: parts.status,
            headers: parts.headers,
            bytes: body.collect().await.unwrap().to_bytes().to_vec(),
        }
    }

    /// A second user with a session, and its token. It owns nothing.
    async fn other_user(&self) -> &'static str {
        const OTHER: &str = "tm_other-token-0123456789-abcdefghijklmnop";
        sqlx::query(
            "INSERT INTO users (id, username, display_name, created_at) \
             VALUES ('00000000-0000-7000-8000-0000000000b2', 'other', 'Other', '2026-01-01T00:00:00.000Z')",
        )
        .execute(&self.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, user_id, token_digest, device_name, created_at, \
             last_seen_at, expires_at) VALUES ('00000000-0000-7000-8000-0000000000bb', \
             '00000000-0000-7000-8000-0000000000b2', ?, NULL, '2026-01-01T00:00:00.000Z', \
             '2026-01-01T00:00:00.000Z', '2099-01-01T00:00:00.000Z')",
        )
        .bind(domain::TokenIssuer::digest(&infrastructure::OsTokenIssuer, OTHER).as_bytes())
        .execute(&self.pool)
        .await
        .unwrap();
        OTHER
    }

    async fn get(&self, path: &str) -> Res {
        self.call(Method::GET, path, None).await
    }

    async fn post(&self, path: &str, body: Value) -> Res {
        self.call(Method::POST, path, Some(body)).await
    }

    async fn topic(&self, folder_name: &str) -> String {
        let f = self
            .post("/api/v1/folders", json!({ "name": folder_name }))
            .await;
        let folder = f.body["id"].as_str().unwrap().to_owned();
        let t = self
            .post(
                &format!("/api/v1/folders/{folder}/topics"),
                json!({ "title": "Fotografia" }),
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

async fn count(app: &App, table: &str) -> i64 {
    let sql = match table {
        "audio_queue" => "SELECT COUNT(*) FROM audio_queue",
        "audio_clips" => "SELECT COUNT(*) FROM audio_clips",
        other => panic!("unknown table {other}"),
    };
    sqlx::query_scalar(sql).fetch_one(&app.pool).await.unwrap()
}

fn readable() -> i64 {
    let doc: Value = serde_json::from_str(&document()).unwrap();
    i64::try_from(domain::readable_blocks(&doc).len()).unwrap()
}

/// Generates a lesson and returns its id.
async fn generated(app: &App) -> String {
    let topic = app.topic("F").await;
    app.make_ready(&topic).await;
    let r = app
        .post(&format!("/api/v1/topics/{topic}/lessons"), json!({}))
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);
    let job = app.wait_job(&topic, "lesson_generation", "succeeded").await;
    job["result"]["lesson_id"].as_str().unwrap().to_owned()
}

async fn drain(app: &App) -> usize {
    let worker = app.state.audio_worker().expect("read-aloud is on");
    let mut made = 0;
    loop {
        match worker.step().await.unwrap() {
            AudioStep::Idle => return made,
            AudioStep::Made => made += 1,
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[tokio::test]
async fn a_generated_lesson_is_queued_and_the_worker_makes_its_clips() {
    let app = App::new(vec![say(&plan()), say(&document())], true).await;
    let lesson = generated(&app).await;
    assert!(readable() > 0);
    assert_eq!(count(&app, "audio_queue").await, readable());
    assert_eq!(count(&app, "audio_clips").await, 0);

    assert_eq!(i64::try_from(drain(&app).await).unwrap(), readable());
    assert_eq!(count(&app, "audio_queue").await, 0);
    assert_eq!(count(&app, "audio_clips").await, readable());
    let files = std::fs::read_dir(&app.dir).unwrap().count();
    assert_eq!(i64::try_from(files).unwrap(), readable());

    // Nothing is missing any more: opening the lesson queues nothing.
    let r = app.get(&format!("/api/v1/lessons/{lesson}")).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(count(&app, "audio_queue").await, 0);
    let _ = std::fs::remove_dir_all(&app.dir);
}

#[tokio::test]
async fn opening_a_lesson_queues_what_is_missing() {
    let app = App::new(vec![say(&plan()), say(&document())], true).await;
    let lesson = generated(&app).await;
    drain(&app).await;

    // A lesson from before the feature, or one whose clips were lost.
    sqlx::query("DELETE FROM audio_clips")
        .execute(&app.pool)
        .await
        .unwrap();
    let r = app.get(&format!("/api/v1/lessons/{lesson}")).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(count(&app, "audio_queue").await, readable());
    assert_eq!(i64::try_from(drain(&app).await).unwrap(), readable());
    let _ = std::fs::remove_dir_all(&app.dir);
}

#[tokio::test]
async fn opening_a_lesson_does_not_fail_when_the_queue_is_broken() {
    let app = App::new(vec![say(&plan()), say(&document())], true).await;
    let lesson = generated(&app).await;
    sqlx::query("DROP TABLE audio_queue")
        .execute(&app.pool)
        .await
        .unwrap();
    let r = app.get(&format!("/api/v1/lessons/{lesson}")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let _ = std::fs::remove_dir_all(&app.dir);
}

#[tokio::test]
async fn with_read_aloud_off_nothing_is_queued_and_there_is_no_worker() {
    let app = App::new(vec![say(&plan()), say(&document())], false).await;
    let lesson = generated(&app).await;
    let r = app.get(&format!("/api/v1/lessons/{lesson}")).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(app.state.audio_worker().is_none());
    assert!(app.state.audio.is_none());
    assert_eq!(count(&app, "audio_queue").await, 0);
    assert_eq!(count(&app, "audio_clips").await, 0);
    assert!(!app.dir.exists());
}

// ---------- manifest, voices, clips ----------

fn blocks(manifest: &Res) -> Vec<(String, String, Option<String>)> {
    manifest.body["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["block_id"].as_str().unwrap().to_owned(),
                b["status"].as_str().unwrap().to_owned(),
                b["clip"]["voice"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

async fn put_voice(app: &App, voice: &str) -> Res {
    app.call(
        Method::PUT,
        "/api/v1/me/tts-voice",
        Some(json!({ "voice": voice })),
    )
    .await
}

#[tokio::test]
async fn the_manifest_tells_every_readable_block_and_queues_nothing() {
    let app = App::new(vec![say(&plan()), say(&document())], true).await;
    let lesson = generated(&app).await;
    let path = format!("/api/v1/lessons/{lesson}/audio");
    let expected: Vec<String> = {
        let doc: Value = serde_json::from_str(&document()).unwrap();
        domain::readable_blocks(&doc)
            .into_iter()
            .map(|b| b.block_id)
            .collect()
    };

    // Queued by the generation: everything is pending, with no clip.
    let m = app.get(&path).await;
    assert_eq!(m.status, StatusCode::OK, "{}", m.body);
    assert_eq!(m.body["voice"], "vivian");
    let seen = blocks(&m);
    assert_eq!(
        seen.iter().map(|b| b.0.clone()).collect::<Vec<_>>(),
        expected,
        "every readable block, in document order"
    );
    assert!(seen.iter().all(|b| b.1 == "pending" && b.2.is_none()));
    assert_eq!(m.body["blocks"][0]["clip"], Value::Null);

    // A lesson that was never opened and never queued (made before the
    // feature) gives the same answer, and asking does not queue it.
    sqlx::query("DELETE FROM audio_queue")
        .execute(&app.pool)
        .await
        .unwrap();
    let m = app.get(&path).await;
    assert_eq!(blocks(&m), seen);
    assert_eq!(count(&app, "audio_queue").await, 0);

    // One block failed for good, one is made.
    sqlx::query(
        "INSERT INTO audio_queue (lesson_id, voice, block_id, text, state, attempts) \
         VALUES (?, 'vivian', ?, 't', 'failed', 3)",
    )
    .bind(&lesson)
    .bind(&expected[1])
    .execute(&app.pool)
    .await
    .unwrap();
    app.get(&format!("/api/v1/lessons/{lesson}")).await;
    // The opening put the failed one back in line, and the rest too.
    assert_eq!(count(&app, "audio_queue").await, readable());
    sqlx::query("UPDATE audio_queue SET state = 'failed' WHERE block_id = ?")
        .bind(&expected[1])
        .execute(&app.pool)
        .await
        .unwrap();
    let worker = app.state.audio_worker().unwrap();
    assert_eq!(worker.step().await.unwrap(), AudioStep::Made);
    let m = app.get(&path).await;
    let seen = blocks(&m);
    assert_eq!(
        seen[0],
        (expected[0].clone(), "ready".into(), Some("vivian".into()))
    );
    assert_eq!(seen[1].1, "failed");
    assert!(seen[2..].iter().all(|b| b.1 == "pending"));
    let clip = &m.body["blocks"][0]["clip"];
    assert!(clip["url"].as_str().unwrap().starts_with("/audio/clips/"));
    assert_eq!(clip["duration_ms"], 1000);
    let queued = count(&app, "audio_queue").await;
    app.get(&path).await;
    assert_eq!(
        count(&app, "audio_queue").await,
        queued,
        "the manifest queues nothing"
    );

    // Unknown and malformed ids.
    let nowhere = app
        .get(&format!("/api/v1/lessons/{}/audio", Uuid::now_v7()))
        .await;
    assert_eq!(nowhere.status, StatusCode::NOT_FOUND);
    let bad = app.get("/api/v1/lessons/nope/audio").await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.body["error"]["code"], "invalid_id");
    let _ = std::fs::remove_dir_all(&app.dir);
}

#[tokio::test]
async fn a_new_voice_plays_the_old_one_meanwhile_and_the_old_queue_is_dropped() {
    let app = App::new(vec![say(&plan()), say(&document())], true).await;
    let lesson = generated(&app).await;
    drain(&app).await;
    let path = format!("/api/v1/lessons/{lesson}/audio");
    assert!(blocks(&app.get(&path).await).iter().all(|b| b.1 == "ready"));

    let me = put_voice(&app, "serena").await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    assert_eq!(me.body["tts_voice"], "serena");
    let before = count(&app, "audio_queue").await;
    let m = app.get(&path).await;
    assert_eq!(m.body["voice"], "serena");
    let seen = blocks(&m);
    assert!(
        seen.iter()
            .all(|b| b.1 == "pending" && b.2.as_deref() == Some("vivian")),
        "{seen:?}"
    );
    assert_eq!(
        count(&app, "audio_queue").await,
        before,
        "changing voice and reading the manifest queue nothing"
    );

    // Opening the lesson queues it in the new voice only.
    app.get(&format!("/api/v1/lessons/{lesson}")).await;
    let voices: Vec<String> = sqlx::query_scalar("SELECT DISTINCT voice FROM audio_queue")
        .fetch_all(&app.pool)
        .await
        .unwrap();
    assert_eq!(voices, ["serena"]);
    assert_eq!(i64::try_from(drain(&app).await).unwrap(), readable());
    let seen = blocks(&app.get(&path).await);
    assert!(
        seen.iter()
            .all(|b| b.1 == "ready" && b.2.as_deref() == Some("serena"))
    );

    // Back to the first voice, whose clips are all still there.
    put_voice(&app, "vivian").await;
    assert!(blocks(&app.get(&path).await).iter().all(|b| b.1 == "ready"));

    // The queued work of an abandoned voice is dropped when the lesson is
    // opened in the new one.
    put_voice(&app, "serena").await;
    sqlx::query("DELETE FROM audio_clips WHERE voice = 'serena'")
        .execute(&app.pool)
        .await
        .unwrap();
    app.get(&format!("/api/v1/lessons/{lesson}")).await;
    put_voice(&app, "vivian").await;
    sqlx::query("DELETE FROM audio_clips WHERE voice = 'vivian'")
        .execute(&app.pool)
        .await
        .unwrap();
    app.get(&format!("/api/v1/lessons/{lesson}")).await;
    let voices: Vec<String> = sqlx::query_scalar("SELECT DISTINCT voice FROM audio_queue")
        .fetch_all(&app.pool)
        .await
        .unwrap();
    assert_eq!(voices, ["vivian"]);
    assert_eq!(count(&app, "audio_queue").await, readable());
    let _ = std::fs::remove_dir_all(&app.dir);
}

#[tokio::test]
async fn the_voice_of_the_user_is_the_saved_one_while_it_is_listed() {
    let app = App::new(vec![], true).await;
    let me = app.get("/api/v1/me").await;
    assert_eq!(me.body["tts_voice"], "vivian", "never chosen: the default");

    let put = put_voice(&app, "serena").await;
    assert_eq!(put.status, StatusCode::OK);
    assert_eq!(put.body["tts_voice"], "serena");
    assert_eq!(app.get("/api/v1/me").await.body["tts_voice"], "serena");

    // The server stopped listing the saved voice.
    sqlx::query("UPDATE users SET tts_voice = 'gone'")
        .execute(&app.pool)
        .await
        .unwrap();
    assert_eq!(app.get("/api/v1/me").await.body["tts_voice"], "vivian");

    let unknown = put_voice(&app, "nobody").await;
    assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(unknown.body["error"]["code"], "validation_failed");
    assert_eq!(
        unknown.body["error"]["details"],
        json!([{"field": "voice", "issue": "unknown_voice"}])
    );
    // A refused choice changes nothing.
    assert_eq!(app.get("/api/v1/me").await.body["tts_voice"], "vivian");
    let _ = std::fs::remove_dir_all(&app.dir);
}

#[tokio::test]
async fn with_read_aloud_off_the_voice_cannot_be_set_and_the_answers_are_empty() {
    let app = App::new(vec![say(&plan()), say(&document())], false).await;
    assert_eq!(app.get("/api/v1/me").await.body["tts_voice"], Value::Null);
    let put = put_voice(&app, "vivian").await;
    assert_eq!(put.status, StatusCode::CONFLICT);
    assert_eq!(put.body["error"]["code"], "conflict");
    // Off is told before the value is judged.
    assert_eq!(put_voice(&app, "nobody").await.status, StatusCode::CONFLICT);

    let v = app.get("/api/v1/tts/voices").await;
    assert_eq!(v.status, StatusCode::OK);
    assert_eq!(
        v.body,
        json!({"available": false, "default_voice": null, "voices": []})
    );

    let lesson = generated(&app).await;
    let m = app.get(&format!("/api/v1/lessons/{lesson}/audio")).await;
    assert_eq!(m.status, StatusCode::OK);
    assert_eq!(m.body["blocks"], json!([]));
    // A lesson that is not there is still `404`.
    let nowhere = app
        .get(&format!("/api/v1/lessons/{}/audio", Uuid::now_v7()))
        .await;
    assert_eq!(nowhere.status, StatusCode::NOT_FOUND);
    let clip = app
        .get(&format!("/api/v1/audio/clips/{}", Uuid::now_v7()))
        .await;
    assert_eq!(clip.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_voices_list_has_a_sample_once_it_is_made_and_anyone_can_play_it() {
    let app = App::new(vec![say(&plan()), say(&document())], true).await;
    let v = app.get("/api/v1/tts/voices").await;
    assert_eq!(v.status, StatusCode::OK);
    assert_eq!(v.body["available"], true);
    assert_eq!(v.body["default_voice"], "vivian");
    assert_eq!(
        v.body["voices"],
        json!([
            {"id": "vivian", "label": "Vivian", "sample_url": null},
            {"id": "serena", "label": "Serena", "sample_url": null}
        ])
    );

    // The samples wait for the lessons: with one queued, it goes first.
    let lesson = generated(&app).await;
    let preparer = app.state.audio.clone().unwrap();
    preparer.prepare_samples().await.unwrap();
    let worker = app.state.audio_worker().unwrap();
    assert_eq!(worker.step().await.unwrap(), AudioStep::Made);
    let v = app.get("/api/v1/tts/voices").await;
    assert!(
        v.body["voices"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["sample_url"].is_null())
    );
    drain(&app).await;

    let v = app.get("/api/v1/tts/voices").await;
    let url = v.body["voices"][1]["sample_url"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(url.starts_with("/audio/clips/"));
    // Any signed-in user reads a sample, with no manifest behind it.
    let other = app.other_user().await;
    let sample = app.fetch(other, &format!("/api/v1{url}"), None).await;
    assert_eq!(sample.status, StatusCode::OK);
    assert_eq!(
        sample.bytes,
        format!("serena:{}", application::audio::SAMPLE_TEXT).into_bytes()
    );

    // The clips of a lesson are not samples.
    let m = app.get(&format!("/api/v1/lessons/{lesson}/audio")).await;
    let lesson_clip = m.body["blocks"][0]["clip"]["url"].as_str().unwrap();
    let denied = app
        .fetch(other, &format!("/api/v1{lesson_clip}"), None)
        .await;
    assert_eq!(denied.status, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_dir_all(&app.dir);
}

#[tokio::test]
async fn a_clip_is_readable_only_after_a_manifest_of_that_user_names_it() {
    let app = App::new(vec![say(&plan()), say(&document())], true).await;
    let lesson = generated(&app).await;
    drain(&app).await;
    let clip_id: String = sqlx::query_scalar("SELECT id FROM audio_clips ORDER BY rowid LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let url = format!("/api/v1/audio/clips/{clip_id}");

    // Made, but never shown to the user: no permission yet.
    let r = app.fetch(TOKEN, &url, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    let m = app.get(&format!("/api/v1/lessons/{lesson}/audio")).await;
    assert_eq!(
        m.body["blocks"][0]["clip"]["url"],
        format!("/audio/clips/{clip_id}")
    );
    let r = app.fetch(TOKEN, &url, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.bytes.starts_with(b"vivian:"));

    // Another user gets neither the clip nor the lesson's manifest.
    let other = app.other_user().await;
    let r = app.fetch(other, &url, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = app
        .fetch(other, &format!("/api/v1/lessons/{lesson}/audio"), None)
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // ...and the answer is the one for a clip that does not exist.
    let ghost = app
        .fetch(
            other,
            &format!("/api/v1/audio/clips/{}", Uuid::now_v7()),
            None,
        )
        .await;
    assert_eq!(ghost.status, StatusCode::NOT_FOUND);
    let clip_of_nobody = app.fetch(other, &url, None).await;
    let strip = |raw: &Raw| {
        let mut v: Value = serde_json::from_slice(&raw.bytes).unwrap();
        v["error"]["request_id"] = Value::Null;
        v
    };
    assert_eq!(
        strip(&clip_of_nobody),
        strip(&ghost),
        "a clip of someone else looks like a clip that is not there"
    );

    // A clip of another voice, shown as the fallback, is granted too.
    put_voice(&app, "serena").await;
    let m = app.get(&format!("/api/v1/lessons/{lesson}/audio")).await;
    assert_eq!(m.body["blocks"][1]["clip"]["voice"], "vivian");
    let fallback = m.body["blocks"][1]["clip"]["url"].as_str().unwrap();
    let r = app.fetch(TOKEN, &format!("/api/v1{fallback}"), None).await;
    assert_eq!(r.status, StatusCode::OK);

    let bad = app.fetch(TOKEN, "/api/v1/audio/clips/nope", None).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    // The row stays but the file is gone (an emptied volume): `404`.
    std::fs::remove_file(app.dir.join(format!("{clip_id}.ogg"))).unwrap();
    let r = app.fetch(TOKEN, &url, None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_dir_all(&app.dir);
}

#[tokio::test]
async fn clips_are_sent_whole_or_by_range_in_chunks() {
    let app = App::new(vec![], true).await;
    // A file bigger than the chunk the server reads at a time.
    let body: Vec<u8> = (0..150_000u32).map(|i| (i % 251) as u8).collect();
    let id = "01933b4e-0000-7000-8000-0000000000c1";
    std::fs::write(app.dir.join(format!("{id}.ogg")), &body).unwrap();
    sqlx::query(
        "INSERT INTO audio_clips (id, key, text_hash, voice, duration_ms, size_bytes, created_at) \
         VALUES (?, 'k', 'h', 'vivian', 1000, ?, '2026-11-13T10:00:00.000Z')",
    )
    .bind(id)
    .bind(150_000_i64)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO audio_clip_grants (user_id, clip_id, granted_at) \
         VALUES ('00000000-0000-7000-8000-000000000001', ?, '2026-11-13T10:00:00.000Z')",
    )
    .bind(id)
    .execute(&app.pool)
    .await
    .unwrap();
    let url = format!("/api/v1/audio/clips/{id}");

    let whole = app.fetch(TOKEN, &url, None).await;
    assert_eq!(whole.status, StatusCode::OK);
    assert_eq!(whole.bytes, body);
    let h = &whole.headers;
    assert_eq!(h[header::CONTENT_TYPE], "audio/ogg");
    assert_eq!(
        h[header::CACHE_CONTROL],
        "private, max-age=31536000, immutable"
    );
    assert_eq!(h[header::ACCEPT_RANGES], "bytes");
    assert_eq!(h[header::CONTENT_LENGTH], "150000");

    let part = app.fetch(TOKEN, &url, Some("bytes=65530-65545")).await;
    assert_eq!(part.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(part.bytes, body[65530..=65545]);
    assert_eq!(
        part.headers[header::CONTENT_RANGE],
        "bytes 65530-65545/150000"
    );
    assert_eq!(part.headers[header::CONTENT_LENGTH], "16");
    assert_eq!(part.headers[header::CONTENT_TYPE], "audio/ogg");
    assert_eq!(
        part.headers[header::CACHE_CONTROL],
        "private, max-age=31536000, immutable"
    );

    // From an offset to the end, across two chunks.
    let tail = app.fetch(TOKEN, &url, Some("bytes=100000-")).await;
    assert_eq!(tail.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(tail.bytes, body[100_000..]);
    assert_eq!(
        tail.headers[header::CONTENT_RANGE],
        "bytes 100000-149999/150000"
    );
    let last = app.fetch(TOKEN, &url, Some("bytes=-10")).await;
    assert_eq!(last.bytes, body[149_990..]);

    let beyond = app.fetch(TOKEN, &url, Some("bytes=150000-")).await;
    assert_eq!(beyond.status, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(beyond.headers[header::CONTENT_RANGE], "bytes */150000");
    let error: Value = serde_json::from_slice(&beyond.bytes).unwrap();
    assert_eq!(error["error"]["code"], "range_not_satisfiable");

    // A header that is not one range is ignored.
    let odd = app.fetch(TOKEN, &url, Some("bytes=0-1,5-6")).await;
    assert_eq!(odd.status, StatusCode::OK);
    assert_eq!(odd.bytes.len(), 150_000);
    let _ = std::fs::remove_dir_all(&app.dir);
}
