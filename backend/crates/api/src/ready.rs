//! `GET /ready`: readiness for the orchestration (contract api-conventions
//! section 3). Unlike `/health` (liveness), it says whether the service can
//! do its job: the database answers and the job runner is alive. The speech
//! engine is optional and only reported.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use application::Heartbeat;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use domain::{Clock, SpeechSynthesizer};
use infrastructure::sqlite::{self, Pool};
use serde::Serialize;

use crate::state::AppState;

/// A probe result is reused for this long, so polling `/ready` stays cheap.
const TTS_CACHE_MS: i64 = 5_000;
const DB_TIMEOUT: Duration = Duration::from_secs(2);
const TTS_TIMEOUT: Duration = Duration::from_secs(3);

pub struct Readiness {
    pool: Pool,
    heartbeat: Arc<Heartbeat>,
    tts: Option<Arc<dyn SpeechSynthesizer>>,
    clock: Arc<dyn Clock>,
    /// (unix ms of the probe, answer).
    tts_seen: Mutex<Option<(i64, bool)>>,
}

#[derive(Serialize)]
pub struct Checks {
    database: &'static str,
    jobs: &'static str,
    tts: &'static str,
}

#[derive(Serialize)]
pub struct ReadyBody {
    status: &'static str,
    checks: Checks,
}

fn ok_or_down(ok: bool) -> &'static str {
    if ok { "ok" } else { "down" }
}

impl Readiness {
    #[must_use]
    pub fn new(
        pool: Pool,
        heartbeat: Arc<Heartbeat>,
        tts: Option<Arc<dyn SpeechSynthesizer>>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            pool,
            heartbeat,
            tts,
            clock,
            tts_seen: Mutex::new(None),
        }
    }

    pub async fn check(&self) -> (StatusCode, ReadyBody) {
        let database = tokio::time::timeout(DB_TIMEOUT, sqlite::ping(&self.pool))
            .await
            .unwrap_or(false);
        let jobs = self.heartbeat.is_alive(self.clock.now());
        let tts = match &self.tts {
            None => "disabled",
            Some(engine) => ok_or_down(self.tts_up(engine.as_ref()).await),
        };
        let ready = database && jobs;
        let status = if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        };
        let body = ReadyBody {
            status: if ready { "ready" } else { "not_ready" },
            checks: Checks {
                database: ok_or_down(database),
                jobs: ok_or_down(jobs),
                tts,
            },
        };
        (status, body)
    }

    async fn tts_up(&self, engine: &dyn SpeechSynthesizer) -> bool {
        let now =
            i64::try_from(self.clock.now().unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX);
        if let Ok(seen) = self.tts_seen.lock()
            && let Some((at, up)) = *seen
            && (0..TTS_CACHE_MS).contains(&now.saturating_sub(at))
        {
            return up;
        }
        let up = tokio::time::timeout(TTS_TIMEOUT, engine.is_up())
            .await
            .unwrap_or(false);
        if let Ok(mut seen) = self.tts_seen.lock() {
            *seen = Some((now, up));
        }
        up
    }
}

/// `GET /ready`, public. The 503 has the same body with `not_ready`, not the
/// error shape of section 7.
pub async fn ready(State(state): State<AppState>) -> Response {
    let (status, body) = state.ready.check().await;
    let mut response = (status, Json(body)).into_response();
    response
        .extensions_mut()
        .insert(crate::middleware::KeepBody);
    response
}
