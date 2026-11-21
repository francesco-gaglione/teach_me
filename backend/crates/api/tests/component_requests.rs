//! `GET /component-requests` over the real router and SQLite
//! (`docs/contracts/component-requests.md`). Rows are seeded in SQL: the
//! generation flow that writes them is covered by `lessons.rs`.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use api::app::{Limits, build_router};
use api::state::{AppOptions, AppState};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::ScriptedLlm;
use domain::{Clock, IdGenerator};
use http_body_util::BodyExt;
use infrastructure::sqlite::{self, Pool};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = common::TOKEN;

struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        datetime!(2026-11-09 15:00:00.000 UTC)
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
}

impl App {
    async fn new() -> Self {
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        let state = AppState::new(
            Arc::new(TestClock),
            Arc::new(SeqIds(AtomicU64::new(0))),
            common::auth(),
            &pool,
            ScriptedLlm::with(vec![]),
            AppOptions::default(),
        );
        common::sign_in_owner(&state, &pool).await;
        Self {
            router: build_router(state, Limits::default()),
            pool,
        }
    }

    async fn get(&self, path: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(Body::empty())
            .unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        (
            parts.status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn topic(&self) -> String {
        let f = Request::builder()
            .method("POST")
            .uri("/api/v1/folders")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"name": "F"}).to_string()))
            .unwrap();
        let res = self.router.clone().oneshot(f).await.unwrap();
        let folder: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let t = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/folders/{}/topics",
                folder["id"].as_str().unwrap()
            ))
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"title": "Rust"}).to_string()))
            .unwrap();
        let res = self.router.clone().oneshot(t).await.unwrap();
        let topic: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        topic["id"].as_str().unwrap().to_owned()
    }

    async fn lesson(&self, topic: &str, n: u32, title: &str, version: u32) -> String {
        let id = Uuid::from_u128(0x100 + u128::from(n)).to_string();
        sqlx::query(
            "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, \
             catalog_version, mission_version, document, plan, generation_job_id, created_at) \
             VALUES (?, ?, ?, ?, 'o', 5, ?, 1, '{}', '{}', ?, '2026-11-09T10:00:00.000Z')",
        )
        .bind(&id)
        .bind(topic)
        .bind(n)
        .bind(title)
        .bind(version)
        .bind(Uuid::from_u128(0x200 + u128::from(n)).to_string())
        .execute(&self.pool)
        .await
        .unwrap();
        id
    }

    async fn ask(&self, topic: &str, lesson: &str, n: u32, wanted: &str, at: &str) {
        sqlx::query(
            "INSERT INTO component_requests (id, topic_id, lesson_id, block_id, wanted, purpose, \
             fallback, created_at) VALUES (?, ?, ?, ?, ?, ?, '[]', ?)",
        )
        .bind(Uuid::from_u128(0x300 + u128::from(n)).to_string())
        .bind(topic)
        .bind(lesson)
        .bind(format!("b{n}"))
        .bind(wanted)
        .bind(format!("purpose {n}"))
        .bind(at)
        .execute(&self.pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn the_ranking_is_empty_then_grouped_and_paged() {
    let app = App::new().await;
    let (status, body) = app.get("/api/v1/component-requests").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"items": [], "next_cursor": null}));

    let topic = app.topic().await;
    let l1 = app.lesson(&topic, 1, "Ownership", 1).await;
    let l2 = app.lesson(&topic, 2, "Borrow", 2).await;
    app.ask(&topic, &l1, 1, "timeline", "2026-11-09T11:00:00.000Z")
        .await;
    app.ask(&topic, &l2, 2, "timeline", "2026-11-09T12:00:00.000Z")
        .await;
    app.ask(&topic, &l1, 3, "map", "2026-11-09T13:00:00.000Z")
        .await;

    let (status, body) = app.get("/api/v1/component-requests?limit=1").await;
    assert_eq!(status, StatusCode::OK);
    let first = &body["items"][0];
    assert_eq!(first["wanted"], "timeline");
    assert_eq!(first["count"], 2);
    assert_eq!(first["catalog_version"], 2);
    assert_eq!(first["first_requested_at"], "2026-11-09T11:00:00.000Z");
    assert_eq!(first["last_requested_at"], "2026-11-09T12:00:00.000Z");
    assert_eq!(first["examples"].as_array().unwrap().len(), 2);
    assert_eq!(first["examples"][0]["lesson_title"], "Borrow");
    assert_eq!(first["examples"][0]["topic_title"], "Rust");
    assert_eq!(first["examples"][0]["purpose"], "purpose 2");
    assert_eq!(first["examples"][0]["lesson_id"], l2);
    let cursor = body["next_cursor"].as_str().unwrap().to_owned();

    let (status, body) = app
        .get(&format!(
            "/api/v1/component-requests?limit=1&cursor={cursor}"
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"][0]["wanted"], "map");
    assert_eq!(body["next_cursor"], Value::Null);
}

#[tokio::test]
async fn a_bad_cursor_or_limit_is_a_bad_request_and_a_token_is_required() {
    let app = App::new().await;
    let (status, body) = app.get("/api/v1/component-requests?cursor=!!").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
    let (status, _) = app.get("/api/v1/component-requests?limit=0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/component-requests")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
