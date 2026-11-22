//! The resource limits of `api-conventions.md` section 12: SSE streams per
//! user and the daily LLM token budget, over the real router and SQLite.

mod common;

use std::time::Duration;

use api::state::AppOptions;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::harness::Harness;
use common::{Step, TOKEN};
use domain::llm::{LlmEvent, Usage};
use serde_json::json;
use tower::ServiceExt;

fn events_request(topic: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/topics/{topic}/events"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::ACCEPT, "text/event-stream")
        .body(Body::empty())
        .unwrap()
}

async fn owner_topic(h: &Harness) -> String {
    let folder = h.post("/api/v1/folders", json!({ "name": "F" })).await.body["id"]
        .as_str()
        .unwrap()
        .to_owned();
    h.post(
        &format!("/api/v1/folders/{folder}/topics"),
        json!({ "title": "Rust" }),
    )
    .await
    .body["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn a_user_over_the_stream_cap_gets_429_and_a_closed_stream_frees_its_slot() {
    let options = AppOptions {
        max_sse_per_user: 2,
        ..AppOptions::default()
    };
    let h = Harness::with_options(vec![], options).await;
    let topic = owner_topic(&h).await;

    let first = h
        .router
        .clone()
        .oneshot(events_request(&topic))
        .await
        .unwrap();
    let second = h
        .router
        .clone()
        .oneshot(events_request(&topic))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);

    let third = h
        .router
        .clone()
        .oneshot(events_request(&topic))
        .await
        .unwrap();
    assert_eq!(third.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(third.headers().contains_key(header::RETRY_AFTER));
    let body = http_body_util::BodyExt::collect(third.into_body())
        .await
        .unwrap()
        .to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"]["code"], "rate_limited");

    // Dropping the response body ends the stream and releases the slot.
    drop(first);
    let again = h
        .router
        .clone()
        .oneshot(events_request(&topic))
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::OK);
}

#[tokio::test]
async fn the_cap_does_not_count_other_users_streams() {
    let options = AppOptions {
        max_sse_per_user: 1,
        ..AppOptions::default()
    };
    let h = Harness::with_options(vec![], options).await;
    let owner_topic_id = owner_topic(&h).await;
    let ann = h.user("ann", true).await;
    let folder = h.folder(&ann, "F").await;
    let ann_topic = h.topic(&ann, &folder, "T").await;

    let _owner = h
        .router
        .clone()
        .oneshot(events_request(&owner_topic_id))
        .await
        .unwrap();
    let mut req = events_request(&ann_topic);
    req.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {ann}").parse().unwrap(),
    );
    let res = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

fn spend(input: u32, output: u32) -> Vec<Step> {
    vec![
        Step::Delta("ok"),
        Step::Event(LlmEvent::Usage(Usage {
            input_tokens: input,
            output_tokens: output,
            ..Usage::default()
        })),
    ]
}

#[tokio::test]
async fn once_the_daily_budget_is_spent_new_llm_requests_get_429_until_midnight_utc() {
    let options = AppOptions {
        daily_llm_tokens: 100,
        ..AppOptions::default()
    };
    let h = Harness::with_options(vec![spend(60, 50)], options).await;
    let topic = owner_topic(&h).await;
    let path = format!("/api/v1/topics/{topic}/messages");

    let first = h.post(&path, json!({ "text": "hello" })).await;
    assert_eq!(first.status, StatusCode::ACCEPTED, "{}", first.body);
    // The reply runs in the background; its usage lands when its stream ends.
    let mut spent = 0_i64;
    for _ in 0..400 {
        let row: Option<(i64,)> = sqlx::query_as("SELECT SUM(tokens) FROM llm_usage")
            .fetch_optional(&h.pool)
            .await
            .unwrap();
        spent = row.map_or(0, |r| r.0);
        if spent > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(spent, 110);

    let refused = h.post(&path, json!({ "text": "again" })).await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(refused.body["error"]["code"], "llm_budget_exceeded");
    // 14:36:01.129 UTC to 00:00 UTC, rounded up.
    assert_eq!(refused.headers[header::RETRY_AFTER], "33839");

    // Requests that do not use the model are not affected.
    let list = h.get(&format!("/api/v1/topics/{topic}/messages")).await;
    assert_eq!(list.status, StatusCode::OK);

    // The next UTC day the count starts again.
    h.set_time(time::macros::datetime!(2026-11-09 00:00:00 UTC));
    let next_day = h.post(&path, json!({ "text": "good morning" })).await;
    assert_eq!(next_day.status, StatusCode::ACCEPTED, "{}", next_day.body);
}

#[tokio::test]
async fn without_a_budget_nothing_is_counted_or_refused() {
    let h = Harness::new(vec![spend(1_000_000, 1_000_000), spend(1, 1)]).await;
    let topic = owner_topic(&h).await;
    let path = format!("/api/v1/topics/{topic}/messages");
    assert_eq!(
        h.post(&path, json!({ "text": "a" })).await.status,
        StatusCode::ACCEPTED
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        h.post(&path, json!({ "text": "b" })).await.status,
        StatusCode::ACCEPTED
    );
    let (rows,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM llm_usage")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}
