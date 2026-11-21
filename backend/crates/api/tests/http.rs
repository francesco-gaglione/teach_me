//! HTTP-level tests driving the router with `oneshot` over in-memory SQLite.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use api::app::{Limits, assemble, build_router};
use api::error::ApiJson;
use api::state::{AppOptions, AppState};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::post;
use domain::{Clock, IdGenerator};
use http_body_util::BodyExt;
use infrastructure::sqlite;
use serde_json::Value;
use time::OffsetDateTime;
use time::macros::datetime;
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = common::TOKEN;

struct FixedClock(OffsetDateTime);
impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        self.0
    }
}

/// Sequential ids so the generated request id is predictable.
struct SeqIds(AtomicU64);
impl IdGenerator for SeqIds {
    fn new_id(&self) -> Uuid {
        Uuid::from_u128(u128::from(self.0.fetch_add(1, Ordering::SeqCst)) + 1)
    }
}

async fn state() -> AppState {
    let pool = sqlite::connect("sqlite::memory:").await.unwrap();
    sqlite::migrate(&pool).await.unwrap();
    let state = AppState::new(
        Arc::new(FixedClock(datetime!(2026-11-08 14:36:01.129_999 UTC))),
        Arc::new(SeqIds(AtomicU64::new(0))),
        common::auth(),
        &pool,
        common::ScriptedLlm::with(vec![]),
        AppOptions::default(),
    );
    common::sign_in_owner(&state, &pool).await;
    state
}

async fn router() -> Router {
    build_router(state().await, Limits::default())
}

async fn send(app: Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Value) {
    let res = app.oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (parts.status, parts.headers, json)
}

fn get(path: &str) -> axum::http::request::Builder {
    Request::builder().uri(path)
}

fn authed(path: &str) -> axum::http::request::Builder {
    get(path).header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
}

/// Asserts the contract §7 shape and that `request_id` matches the header.
fn assert_error(headers: &axum::http::HeaderMap, body: &Value, code: &str) {
    let err = &body["error"];
    assert_eq!(err["code"], code, "{body}");
    assert!(err["message"].as_str().is_some_and(|m| !m.is_empty()));
    assert!(err["details"].is_null());
    assert_eq!(
        err["request_id"].as_str().unwrap(),
        headers["x-request-id"].to_str().unwrap()
    );
}

#[tokio::test]
async fn public_health_is_ok_without_token() {
    let (status, headers, body) =
        send(router().await, get("/health").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, serde_json::json!({"status": "ok"}));
    assert!(headers.contains_key("x-request-id"));
}

#[tokio::test]
async fn authenticated_health_matches_contract() {
    let (status, _, body) = send(
        router().await,
        authed("/api/v1/health").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["api_version"], 1);
    assert_eq!(body["catalog_version"], 1);
    assert_eq!(body["server_time"], "2026-11-08T14:36:01.129Z");
}

#[tokio::test]
async fn missing_malformed_or_wrong_token_is_401_in_contract_format() {
    let cases = [
        None,
        Some("Bearer wrong-token-wrong-token-wrong-token".to_owned()),
        Some(format!("Basic {TOKEN}")),
        Some(TOKEN.to_owned()),
    ];
    for auth in cases {
        let mut req = get("/api/v1/health");
        if let Some(a) = &auth {
            req = req.header(header::AUTHORIZATION, a);
        }
        let (status, headers, body) = send(router().await, req.body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{auth:?}");
        assert_error(&headers, &body, "unauthorized");
        assert!(!body.to_string().contains(TOKEN));
    }
}

#[tokio::test]
async fn unknown_route_is_404_in_contract_format() {
    let (status, headers, body) =
        send(router().await, get("/nope").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_error(&headers, &body, "not_found");

    let (status, headers, body) = send(
        router().await,
        authed("/api/v1/nope").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_error(&headers, &body, "not_found");
}

#[tokio::test]
async fn unknown_api_route_requires_token_first() {
    let (status, _, _) = send(
        router().await,
        get("/api/v1/nope").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_method_is_405_in_contract_format() {
    let req = authed("/api/v1/health")
        .method("DELETE")
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = send(router().await, req).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_error(&headers, &body, "method_not_allowed");
    assert!(headers.contains_key(header::ALLOW));
}

#[tokio::test]
async fn client_request_id_is_reused_when_valid() {
    let id = "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f";
    let req = authed("/api/v1/health")
        .header("x-request-id", id)
        .body(Body::empty())
        .unwrap();
    let (_, headers, _) = send(router().await, req).await;
    assert_eq!(headers["x-request-id"], id);

    // Also on errors, where it must appear in the body.
    let req = get("/nope")
        .header("x-request-id", id)
        .body(Body::empty())
        .unwrap();
    let (_, headers, body) = send(router().await, req).await;
    assert_eq!(headers["x-request-id"], id);
    assert_eq!(body["error"]["request_id"], id);
}

#[tokio::test]
async fn invalid_request_id_is_replaced_by_generated_one() {
    let req = get("/health")
        .header("x-request-id", "not-a-uuid")
        .body(Body::empty())
        .unwrap();
    let (_, headers, _) = send(router().await, req).await;
    assert_eq!(
        headers["x-request-id"],
        "00000000-0000-0000-0000-000000000001"
    );
}

#[derive(serde::Deserialize)]
struct Echo {
    #[allow(dead_code)]
    name: String,
}

async fn echo(ApiJson(_): ApiJson<Echo>) -> &'static str {
    "ok"
}

async fn body_router(limit: usize) -> Router {
    let limits = Limits {
        body_bytes: limit,
        ..Limits::default()
    };
    assemble(
        state().await,
        limits,
        Router::new().route("/echo", post(echo)),
    )
}

fn post_json(body: impl Into<Body>, len: Option<usize>) -> Request<Body> {
    let mut req = authed("/api/v1/echo")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(len) = len {
        req = req.header(header::CONTENT_LENGTH, len);
    }
    req.body(body.into()).unwrap()
}

#[tokio::test]
async fn malformed_json_is_400_in_contract_format() {
    let (status, headers, body) = send(body_router(1024).await, post_json("{not json", None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_error(&headers, &body, "bad_request");
}

#[tokio::test]
async fn json_with_wrong_shape_is_422() {
    let (status, headers, body) =
        send(body_router(1024).await, post_json(r#"{"x":1}"#, None)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&headers, &body, "validation_failed");
}

#[tokio::test]
async fn valid_json_is_accepted() {
    let (status, _, _) = send(body_router(1024).await, post_json(r#"{"name":"a"}"#, None)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn oversized_body_is_413_in_contract_format() {
    let big = format!(r#"{{"name":"{}"}}"#, "a".repeat(4096));
    // With Content-Length: rejected by the limit layer before the handler.
    let len = big.len();
    let (status, headers, body) =
        send(body_router(1024).await, post_json(big.clone(), Some(len))).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_error(&headers, &body, "payload_too_large");

    // Unknown size (stream): only detectable while the extractor reads.
    let streamed = big.clone().into_bytes();
    let chunk = futures_util::stream::once(async move { Ok::<_, std::io::Error>(streamed) });
    let (status, headers, body) = send(
        body_router(1024).await,
        post_json(Body::from_stream(chunk), None),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_error(&headers, &body, "payload_too_large");

    // Known size, without Content-Length header: the limit still applies.
    let (status, headers, body) = send(body_router(1024).await, post_json(big, None)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_error(&headers, &body, "payload_too_large");
}

async fn boom() -> &'static str {
    panic!("handler exploded");
}

#[tokio::test]
async fn handler_panic_is_500_in_contract_format() {
    let app = assemble(
        state().await,
        Limits::default(),
        Router::new().route("/boom", axum::routing::get(boom)),
    );
    let (status, headers, body) =
        send(app, authed("/api/v1/boom").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_error(&headers, &body, "internal");
}

async fn slow() -> &'static str {
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    "late"
}

#[tokio::test]
async fn slow_handler_times_out_with_503_in_contract_format() {
    let limits = Limits {
        request_timeout: std::time::Duration::from_millis(50),
        ..Limits::default()
    };
    let app = assemble(
        state().await,
        limits,
        Router::new().route("/slow", axum::routing::get(slow)),
    );
    let (status, headers, body) =
        send(app, authed("/api/v1/slow").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_error(&headers, &body, "unavailable");
}

#[tokio::test]
async fn error_message_follows_accept_language_and_code_stays_the_same() {
    for (header_value, message) in [
        (Some("en-US,en;q=0.9"), "Resource not found."),
        (Some("fr, en;q=0.5"), "Resource not found."),
        (Some("it-IT"), "Risorsa non trovata."),
        (Some("de"), "Risorsa non trovata."),
        (None, "Risorsa non trovata."),
    ] {
        let mut req = authed("/api/v1/nope");
        if let Some(v) = header_value {
            req = req.header(header::ACCEPT_LANGUAGE, v);
        }
        let (status, headers, body) = send(router().await, req.body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_error(&headers, &body, "not_found");
        assert_eq!(body["error"]["message"], message, "{header_value:?}");
    }
}
