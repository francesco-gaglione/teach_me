//! `GET /ready` (contract api-conventions section 3) over the real router.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use api::app::{Limits, build_router};
use api::state::{AppOptions, AppState, AudioSetup, DocumentSetup};
use application::AudioSettings;
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use domain::{Clock, IdGenerator};
use domain::{ContentLanguage, EngineInfo, SpeechError, SpeechSynthesizer, SynthesizedClip, Voice};
use http_body_util::BodyExt;
use infrastructure::FsClipStore;
use infrastructure::sqlite;
use serde_json::{Value, json};
use std::sync::Mutex;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;
use uuid::Uuid;

struct Engine {
    info: EngineInfo,
    voices: Vec<Voice>,
    up: bool,
    probes: AtomicUsize,
}

#[async_trait]
impl SpeechSynthesizer for Engine {
    fn engine(&self) -> &EngineInfo {
        &self.info
    }
    fn voices(&self) -> &[Voice] {
        &self.voices
    }
    fn default_voice(&self) -> &'static str {
        "v"
    }
    async fn is_up(&self) -> bool {
        self.probes.fetch_add(1, Ordering::SeqCst);
        self.up
    }
    async fn synthesize(
        &self,
        _: &str,
        _: &str,
        _: ContentLanguage,
    ) -> Result<SynthesizedClip, SpeechError> {
        Err(SpeechError::Unavailable("test".into()))
    }
}

struct TestClock(Mutex<OffsetDateTime>);
impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        *self.0.lock().unwrap()
    }
}
struct Ids;
impl IdGenerator for Ids {
    fn new_id(&self) -> Uuid {
        Uuid::now_v7()
    }
}

struct App {
    router: axum::Router,
    state: AppState,
    pool: sqlite::Pool,
    clock: Arc<TestClock>,
    engine: Option<Arc<Engine>>,
}

impl App {
    async fn new(tts_up: Option<bool>) -> Self {
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        let clock = Arc::new(TestClock(Mutex::new(datetime!(2026-11-22 10:00 UTC))));
        let engine = tts_up.map(|up| {
            Arc::new(Engine {
                info: EngineInfo {
                    engine: "fake".into(),
                    model: "m".into(),
                    params: "p".into(),
                },
                voices: vec![Voice {
                    id: "v".into(),
                    label: "V".into(),
                }],
                up,
                probes: AtomicUsize::new(0),
            })
        });
        let audio = match &engine {
            Some(engine) => {
                let dir = std::env::temp_dir().join(format!("teach_me_ready_{}", Uuid::now_v7()));
                let store = FsClipStore::new(dir);
                store.prepare().await.unwrap();
                Some(AudioSetup {
                    synthesizer: engine.clone(),
                    store: Arc::new(store),
                    settings: AudioSettings::default(),
                })
            }
            None => None,
        };
        let state = AppState::with_audio(
            clock.clone(),
            Arc::new(Ids),
            common::auth(),
            &pool,
            common::ScriptedLlm::with(vec![]),
            AppOptions::default(),
            DocumentSetup::unavailable(),
            audio,
        );
        Self {
            router: build_router(state.clone(), Limits::default()),
            state,
            pool,
            clock,
            engine,
        }
    }

    fn advance(&self, secs: i64) {
        let mut now = self.clock.0.lock().unwrap();
        *now += Duration::seconds(secs);
    }

    /// No credentials: the endpoint is public.
    async fn ready(&self) -> (StatusCode, Value) {
        let req = Request::builder()
            .uri("/ready")
            .body(Body::empty())
            .unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    fn beat(&self) {
        self.state.heartbeat.beat(self.clock.now());
    }
}

#[tokio::test]
async fn not_ready_until_the_job_runner_has_beaten() {
    let app = App::new(None).await;
    let (status, body) = app.ready().await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body,
        json!({"status": "not_ready", "checks": {"database": "ok", "jobs": "down", "tts": "disabled"}})
    );

    app.beat();
    let (status, body) = app.ready().await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({"status": "ready", "checks": {"database": "ok", "jobs": "ok", "tts": "disabled"}})
    );
}

#[tokio::test]
async fn a_runner_that_stops_beating_makes_the_service_not_ready() {
    let app = App::new(None).await;
    app.beat();
    app.advance(25);
    assert_eq!(app.ready().await.0, StatusCode::OK, "within the tolerance");
    app.advance(10);
    let (status, body) = app.ready().await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["checks"]["jobs"], "down");
}

#[tokio::test]
async fn a_database_that_does_not_answer_makes_the_service_not_ready() {
    let app = App::new(None).await;
    app.beat();
    app.pool.close().await;
    let (status, body) = app.ready().await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "not_ready");
    assert_eq!(body["checks"]["database"], "down");
}

#[tokio::test]
async fn a_speech_engine_that_is_down_is_reported_but_does_not_block_readiness() {
    let app = App::new(Some(false)).await;
    app.beat();
    let (status, body) = app.ready().await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ready");
    assert_eq!(body["checks"]["tts"], "down");
}

#[tokio::test]
async fn the_speech_probe_is_cached_for_a_few_seconds() {
    let app = App::new(Some(true)).await;
    app.beat();
    let probes = || app.engine.as_ref().unwrap().probes.load(Ordering::SeqCst);
    let (_, body) = app.ready().await;
    assert_eq!(body["checks"]["tts"], "ok");
    app.ready().await;
    assert_eq!(probes(), 1, "the second call reuses the answer");
    app.advance(6);
    app.beat();
    app.ready().await;
    assert_eq!(probes(), 2);
}
