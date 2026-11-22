//! Nothing sensitive reaches the logs: passwords, tokens, API keys, message
//! text, model output, provider error text and query strings are exercised
//! through the real router with a capture subscriber at TRACE level.

mod common;

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::{Method, Request, header};
use common::harness::Harness;
use common::{Step, TOKEN};
use domain::llm::LlmError;
use serde_json::json;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

const PASSWORD: &str = "PWD-MARKER-correct-horse";
const WRONG_PASSWORD: &str = "PWD-MARKER-wrong-horse";
const API_KEY: &str = "sk-KEY-MARKER-0123456789";
const USER_TEXT: &str = "TEXT-MARKER-what-the-learner-typed";
const MODEL_TEXT: &str = "REPLY-MARKER-what-the-model-said";
const PROVIDER_ECHO: &str = "ECHO-MARKER-provider-echoed-the-prompt";
const QUERY_SECRET: &str = "QUERY-MARKER-in-the-url";
const KEPT_MARKER: &str = "KEPT-MARKER-short-provider-diagnosis";
const CUT_MARKER: &str = "CUT-MARKER-beyond-the-limit";
const COOKIE: &str = "COOKIE-MARKER";

#[tokio::test(flavor = "current_thread")]
async fn credentials_content_and_provider_text_never_reach_the_logs() {
    let capture = Capture::default();
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(capture.clone())
            .with_ansi(false)
            .finish(),
    );
    let h = Harness::new(vec![
        vec![Step::Fail(LlmError::Network(PROVIDER_ECHO.into()))],
        // The first 200 characters of an invalid-request text are kept for
        // diagnosis; what follows is cut.
        vec![Step::Fail(LlmError::InvalidRequest(format!(
            "{KEPT_MARKER}{}{CUT_MARKER}",
            "x".repeat(250)
        )))],
        vec![Step::Delta(MODEL_TEXT)],
    ])
    .await;

    // Login path, good and bad, then the LLM key.
    h.create_user("ann", PASSWORD).await;
    h.login("ann", WRONG_PASSWORD).await;
    let token = h.login("ann", PASSWORD).await.body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    h.request(
        Some(&token),
        Method::PUT,
        "/api/v1/me/llm-key",
        Some(json!({ "api_key": API_KEY })),
    )
    .await;

    // A request with a query string and a cookie.
    let req = Request::builder()
        .uri(format!("/api/v1/me?access_token={QUERY_SECRET}"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::COOKIE, COOKIE)
        .body(axum::body::Body::empty())
        .unwrap();
    h.send(req).await;

    // The LLM path: one failing reply (provider text), one good one.
    let folder = h.post("/api/v1/folders", json!({ "name": "F" })).await.body["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let topic = h
        .post(
            &format!("/api/v1/folders/{folder}/topics"),
            json!({ "title": "Rust" }),
        )
        .await
        .body["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for _ in 0..3 {
        h.post(
            &format!("/api/v1/topics/{topic}/messages"),
            json!({ "text": USER_TEXT }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let logs = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("http"),
        "the capture saw the requests: {logs}"
    );
    for (what, marker) in [
        ("password", "PWD-MARKER"),
        ("API key", "KEY-MARKER"),
        ("session token", TOKEN),
        ("user text", "TEXT-MARKER"),
        ("model text", "REPLY-MARKER"),
        ("provider text", "ECHO-MARKER"),
        ("query string", "QUERY-MARKER"),
        ("cookie", COOKIE),
        ("invalid-request text past 200 chars", "CUT-MARKER"),
    ] {
        assert!(
            !logs.contains(marker),
            "{what} leaked into the logs:\n{logs}"
        );
    }
    assert!(
        logs.contains(KEPT_MARKER),
        "the short provider text is kept:\n{logs}"
    );
    assert!(!logs.contains(&token), "the login token leaked");
}
