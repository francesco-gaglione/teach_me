//! Accounts, sessions and the LLM key over HTTP (`docs/contracts/users.md`).

mod common;

use axum::http::{Method, StatusCode, header};
use common::harness::{Harness, Res, T0};
use common::{Step, TOKEN};
use domain::llm::LlmError;
use serde_json::{Value, json};

fn code(res: &Res) -> &str {
    res.body["error"]["code"].as_str().unwrap_or("")
}

fn without_request_id(res: &Res) -> Value {
    let mut body = res.body.clone();
    body["error"]["request_id"] = Value::Null;
    body
}

#[tokio::test]
async fn login_returns_a_session_and_the_user() {
    let h = Harness::new(vec![]).await;
    h.create_user("Ann", "correct horse").await;
    let res = h.login("ann", "correct horse").await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let token = res.body["token"].as_str().unwrap();
    assert!(token.starts_with("tm_") && token.len() > 40);
    assert_eq!(res.body["expires_at"], "2027-02-06T14:36:01.129Z");
    let user = &res.body["user"];
    assert_eq!(user["username"], "ann");
    assert_eq!(user["display_name"], "Ann");
    assert_eq!(user["has_llm_key"], false);
    assert_eq!(user["llm_provider"], "openrouter");
    assert!(user["id"].as_str().is_some() && user["created_at"].as_str().is_some());

    let me = h
        .request(Some(token), Method::GET, "/api/v1/me", None)
        .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body, *user);
    // The password and the token are nowhere in what comes back.
    assert!(!me.body.to_string().contains("correct horse"));
}

#[tokio::test]
async fn login_is_the_only_route_without_a_session_and_everything_else_says_401() {
    let h = Harness::new(vec![]).await;
    for (method, path) in [
        (Method::GET, "/api/v1/me"),
        (Method::GET, "/api/v1/folders"),
        (Method::POST, "/api/v1/auth/logout"),
        (Method::GET, "/api/v1/nothing-here"),
    ] {
        for token in [None, Some("tm_not-a-session"), Some("")] {
            let res = h.request(token, method.clone(), path, None).await;
            assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{method} {path}");
            assert_eq!(code(&res), "unauthorized");
            assert_eq!(res.headers[header::WWW_AUTHENTICATE], "Bearer");
        }
    }
    assert_eq!(h.get("/health").await.status, StatusCode::OK);
    let res = h.request(None, Method::GET, "/health", None).await;
    assert_eq!(res.status, StatusCode::OK);
}

#[tokio::test]
async fn unknown_user_wrong_password_and_disabled_user_get_the_same_answer() {
    let h = Harness::new(vec![]).await;
    h.create_user("ann", "correct horse").await;
    h.create_user("off", "correct horse").await;
    h.sql(
        "UPDATE users SET disabled_at = '2026-11-01T00:00:00.000Z' WHERE username = 'off'",
        &[],
    )
    .await;
    let answers = [
        h.login("ann", "wrong horse!!").await,
        h.login("nobody", "correct horse").await,
        h.login("off", "correct horse").await,
        h.login("owner", "correct horse").await, // exists, has no password yet
    ];
    for res in &answers {
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
        assert_eq!(code(res), "invalid_credentials");
    }
    let first = without_request_id(&answers[0]);
    assert!(answers.iter().all(|r| without_request_id(r) == first));
    assert!(!first.to_string().contains("ann"), "the name is not echoed");
}

#[tokio::test]
async fn the_sixth_attempt_in_fifteen_minutes_is_429_with_retry_after() {
    let h = Harness::new(vec![]).await;
    h.create_user("ann", "correct horse").await;
    for _ in 0..5 {
        assert_eq!(
            h.login("ann", "wrong horse!!").await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    let blocked = h.login("ann", "correct horse").await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(code(&blocked), "rate_limited");
    let wait: u64 = blocked.headers[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=900).contains(&wait), "{wait}");
    assert_eq!(
        h.login("bob-the-other", "x").await.status,
        StatusCode::UNAUTHORIZED
    );

    h.set_time(T0 + time::Duration::minutes(15));
    assert_eq!(h.login("ann", "correct horse").await.status, StatusCode::OK);
}

#[tokio::test]
async fn a_badly_shaped_login_is_a_validation_error() {
    let h = Harness::new(vec![]).await;
    for body in [
        json!({}),
        json!({"username": "ann"}),
        json!({"username": 5, "password": "x"}),
    ] {
        let res = h
            .request(None, Method::POST, "/api/v1/auth/login", Some(body))
            .await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&res), "validation_failed");
    }
    let long = "d".repeat(61);
    let res = h
        .request(
            None,
            Method::POST,
            "/api/v1/auth/login",
            Some(json!({"username": "ann", "password": "x", "device_name": long})),
        )
        .await;
    assert_eq!(
        res.body["error"]["details"],
        json!([{"field": "device_name", "issue": "too_long"}])
    );
}

#[tokio::test]
async fn logout_ends_only_the_session_that_was_used() {
    let h = Harness::new(vec![]).await;
    let phone = h.user("ann", false).await;
    let tablet = h.login("ann", "a long password").await.body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let out = h
        .request(Some(&phone), Method::POST, "/api/v1/auth/logout", None)
        .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let again = h
        .request(Some(&phone), Method::GET, "/api/v1/me", None)
        .await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED);
    let other = h
        .request(Some(&tablet), Method::GET, "/api/v1/me", None)
        .await;
    assert_eq!(other.status, StatusCode::OK);
}

#[tokio::test]
async fn a_session_dies_after_ninety_idle_days() {
    let h = Harness::new(vec![]).await;
    let token = h.user("ann", false).await;
    h.set_time(T0 + time::Duration::days(60));
    assert_eq!(
        h.request(Some(&token), Method::GET, "/api/v1/me", None)
            .await
            .status,
        StatusCode::OK
    );
    h.set_time(T0 + time::Duration::days(60 + 89));
    assert_eq!(
        h.request(Some(&token), Method::GET, "/api/v1/me", None)
            .await
            .status,
        StatusCode::OK
    );
    h.set_time(T0 + time::Duration::days(60 + 89 + 90));
    let res = h
        .request(Some(&token), Method::GET, "/api/v1/me", None)
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(code(&res), "unauthorized");
}

#[tokio::test]
async fn a_disabled_user_is_out_on_the_next_request() {
    let h = Harness::new(vec![]).await;
    let token = h.user("ann", false).await;
    h.sql(
        "UPDATE users SET disabled_at = '2026-11-08T15:00:00.000Z' WHERE username = 'ann'",
        &[],
    )
    .await;
    let res = h
        .request(Some(&token), Method::GET, "/api/v1/me", None)
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn changing_the_password_signs_out_the_other_devices() {
    let h = Harness::new(vec![]).await;
    let phone = h.user("ann", false).await;
    let tablet = h.login("ann", "a long password").await.body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let put = |token: String, current: &str, new: &str| {
        let body = json!({"current_password": current, "new_password": new});
        let h = &h;
        let current = current.to_owned();
        async move {
            let _ = current;
            h.request(Some(&token), Method::PUT, "/api/v1/me/password", Some(body))
                .await
        }
    };

    let wrong = put(tablet.clone(), "not the password", "new long password").await;
    assert_eq!(wrong.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        wrong.body["error"]["details"],
        json!([{"field": "current_password", "issue": "wrong"}])
    );
    let short = put(tablet.clone(), "a long password", "short").await;
    assert_eq!(short.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        short.body["error"]["details"],
        json!([{"field": "new_password", "issue": "too_short"}])
    );
    let long = put(tablet.clone(), "a long password", &"p".repeat(129)).await;
    assert_eq!(
        long.body["error"]["details"],
        json!([{"field": "new_password", "issue": "too_long"}])
    );
    assert_eq!(
        h.request(Some(&phone), Method::GET, "/api/v1/me", None)
            .await
            .status,
        StatusCode::OK
    );

    assert_eq!(
        put(tablet.clone(), "a long password", "new long password")
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        h.request(Some(&phone), Method::GET, "/api/v1/me", None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        h.request(Some(&tablet), Method::GET, "/api/v1/me", None)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        h.login("ann", "a long password").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        h.login("ann", "new long password").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_llm_key_is_written_checked_and_never_read_back() {
    let h = Harness::new(vec![]).await;
    let token = h.user("ann", false).await;
    let put = |key: Value| {
        h.request(
            Some(&token),
            Method::PUT,
            "/api/v1/me/llm-key",
            Some(json!({"api_key": key})),
        )
    };
    for (bad, issue) in [
        (json!("   "), "empty"),
        (json!("k".repeat(501)), "too_long"),
    ] {
        let res = put(bad).await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            res.body["error"]["details"],
            json!([{"field": "api_key", "issue": issue}])
        );
    }
    let ok = put(json!("  sk-or-very-secret \n")).await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.body["has_llm_key"], true);
    for res in [
        ok,
        h.request(Some(&token), Method::GET, "/api/v1/me", None)
            .await,
    ] {
        let text = res.body.to_string();
        assert!(
            !text.contains("secret") && !text.contains("sk-or"),
            "{text}"
        );
    }
    // In the database it is a sealed blob: neither the key nor a piece of it.
    let raw: Vec<u8> = sqlx::query_scalar("SELECT llm_key FROM users WHERE username = 'ann'")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert!(!raw.windows(6).any(|w| w == b"secret"));

    let gone = h
        .request(Some(&token), Method::DELETE, "/api/v1/me/llm-key", None)
        .await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    let me = h
        .request(Some(&token), Method::GET, "/api/v1/me", None)
        .await;
    assert_eq!(me.body["has_llm_key"], false);
}

// ---------- what needs the key ----------

const AT: &str = "2026-11-09T14:00:00.000Z";

async fn make_ready(h: &Harness, topic: &str) {
    h.sql("UPDATE topics SET state = 'ready' WHERE id = ?", &[topic])
        .await;
    h.sql(
        "INSERT INTO missions (topic_id, version, status, why, success_criteria, mission_constraints, \
         out_of_scope, created_at, confirmed_at) VALUES (?, 1, 'confirmed', 'w', '[\"c\"]', '[]', '[]', ?, ?)",
        &[topic, AT, AT],
    )
    .await;
}

fn is_409_llm_key_missing(res: &Res) {
    assert_eq!(res.status, StatusCode::CONFLICT, "{}", res.body);
    assert_eq!(code(res), "llm_key_missing");
    assert!(
        res.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Impostazioni")
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // one scenario per operation that needs the key
async fn work_that_needs_the_model_is_refused_without_a_key_and_creates_nothing() {
    let h = Harness::new(vec![]).await;
    let bob = h.user("bob", false).await;
    let folder = h.folder(&bob, "F").await;
    let topic = h.topic(&bob, &folder, "T").await;
    make_ready(&h, &topic).await;
    let req = |m: Method, p: String, b: Option<Value>| {
        let (h, bob) = (&h, bob.clone());
        async move { h.request(Some(&bob), m, &p, b).await }
    };

    // A message: nothing is stored.
    is_409_llm_key_missing(
        &req(
            Method::POST,
            format!("/api/v1/topics/{topic}/messages"),
            Some(json!({"text": "ciao"})),
        )
        .await,
    );
    let messages = req(
        Method::GET,
        format!("/api/v1/topics/{topic}/messages"),
        None,
    )
    .await;
    assert_eq!(messages.body["items"], json!([]));

    // A research, a lesson: no job.
    is_409_llm_key_missing(
        &req(
            Method::POST,
            format!("/api/v1/topics/{topic}/resources/research"),
            Some(json!({})),
        )
        .await,
    );
    is_409_llm_key_missing(
        &req(
            Method::POST,
            format!("/api/v1/topics/{topic}/lessons"),
            Some(json!({})),
        )
        .await,
    );
    let jobs = req(Method::GET, format!("/api/v1/topics/{topic}/jobs"), None).await;
    assert_eq!(jobs.body["items"], json!([]));

    // Closing a lesson: it stays open.
    let lesson = "01933b4e-0000-7000-8000-00000000a001";
    h.sql(
        "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, catalog_version, \
         mission_version, document, plan, generation_job_id, created_at) VALUES \
         (?, ?, 1, 'L', 'o', 5, 1, 1, '{}', '{}', 'gj1', ?)",
        &[lesson, &topic, AT],
    )
    .await;
    is_409_llm_key_missing(
        &req(
            Method::POST,
            format!("/api/v1/lessons/{lesson}/complete"),
            Some(json!({})),
        )
        .await,
    );
    let status: String = sqlx::query_scalar("SELECT status FROM lessons WHERE id = ?")
        .bind(lesson)
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(status, "not_started");

    // Retrying a failed job or a failed reply.
    let job = "01933b4e-0000-7000-8000-00000000a002";
    h.sql(
        "INSERT INTO jobs (id, topic_id, kind, status, error_code, error_message, attempt, created_at) \
         VALUES (?, ?, 'research', 'failed', 'internal', 'x', 1, ?)",
        &[job, &topic, AT],
    )
    .await;
    is_409_llm_key_missing(
        &req(
            Method::POST,
            format!("/api/v1/topics/{topic}/jobs/{job}/retry"),
            Some(json!({})),
        )
        .await,
    );
    let msg = "01933b4e-0000-7000-8000-00000000a003";
    h.sql(
        "INSERT INTO chat_messages (id, topic_id, role, status, parts, error_code, error_message, created_at, updated_at) \
         VALUES (?, ?, 'assistant', 'failed', '[]', 'internal', 'x', ?, ?)",
        &[msg, &topic, AT, AT],
    )
    .await;
    is_409_llm_key_missing(
        &req(
            Method::POST,
            format!("/api/v1/topics/{topic}/messages/{msg}/retry"),
            Some(json!({})),
        )
        .await,
    );
    // Nothing was created by any of it.
    assert!(h.llm.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn confirming_the_mission_always_works_and_the_research_waits_for_the_key() {
    let h = Harness::new(vec![]).await;
    let bob = h.user("bob", false).await;
    let folder = h.folder(&bob, "F").await;
    let topic = h.topic(&bob, &folder, "T").await;
    h.sql(
        "UPDATE topics SET state = 'mission_proposed' WHERE id = ?",
        &[&topic],
    )
    .await;
    h.sql(
        "INSERT INTO missions (topic_id, version, status, why, success_criteria, mission_constraints, \
         out_of_scope, created_at) VALUES (?, 1, 'draft', 'w', '[\"c\"]', '[]', '[]', ?)",
        &[&topic, AT],
    )
    .await;
    let res = h
        .request(
            Some(&bob),
            Method::POST,
            &format!("/api/v1/topics/{topic}/mission/confirm"),
            Some(json!({"version": 1})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["current"]["status"], "confirmed");

    // The research job it queued fails when it runs: no key.
    let runner = std::sync::Arc::new(h.state.job_runner(vec![(
        domain::JobKind::Research,
        h.state.research_handler(),
    )]));
    assert!(runner.run_next().await.unwrap());
    let jobs = h
        .request(
            Some(&bob),
            Method::GET,
            &format!("/api/v1/topics/{topic}/jobs"),
            None,
        )
        .await;
    assert_eq!(jobs.body["items"][0]["status"], "failed");
    assert_eq!(jobs.body["items"][0]["error"]["code"], "llm_key_missing");
    assert!(h.llm.requests.lock().unwrap().is_empty());

    // Bob saves a key and retries the job: it queues again.
    h.request(
        Some(&bob),
        Method::PUT,
        "/api/v1/me/llm-key",
        Some(json!({"api_key": "sk-bob"})),
    )
    .await;
    let job_id = jobs.body["items"][0]["id"].as_str().unwrap().to_owned();
    let retry = h
        .request(
            Some(&bob),
            Method::POST,
            &format!("/api/v1/topics/{topic}/jobs/{job_id}/retry"),
            Some(json!({})),
        )
        .await;
    assert_eq!(retry.status, StatusCode::ACCEPTED, "{}", retry.body);
    assert_eq!(retry.body["status"], "queued");
}

#[tokio::test]
async fn a_rejected_key_is_reported_in_the_reply_and_in_the_job() {
    let h = Harness::new(vec![
        vec![Step::Fail(LlmError::Authentication)],
        vec![Step::Fail(LlmError::Authentication)],
    ])
    .await;
    let folder = h.folder(TOKEN, "F").await;
    let topic = h.topic(TOKEN, &folder, "T").await;
    let sent = h
        .post(
            &format!("/api/v1/topics/{topic}/messages"),
            json!({"text": "ciao"}),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.body);
    let mut failed = Value::Null;
    for _ in 0..100 {
        let m = h.get(&format!("/api/v1/topics/{topic}/messages")).await;
        let newest = m.body["items"][0].clone();
        if newest["status"] == "failed" {
            failed = newest;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(failed["error"]["code"], "llm_key_rejected", "{failed}");

    // The same for a job: the key of the topic's owner reached the provider
    // and was refused.
    make_ready(&h, &topic).await;
    let queued = h
        .post(
            &format!("/api/v1/topics/{topic}/resources/research"),
            json!({}),
        )
        .await;
    assert_eq!(queued.status, StatusCode::ACCEPTED, "{}", queued.body);
    let runner = std::sync::Arc::new(h.state.job_runner(vec![(
        domain::JobKind::Research,
        h.state.research_handler(),
    )]));
    assert!(runner.run_next().await.unwrap());
    let jobs = h.get(&format!("/api/v1/topics/{topic}/jobs")).await;
    assert_eq!(jobs.body["items"][0]["error"]["code"], "llm_key_rejected");
    let keys: Vec<_> = h
        .llm
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.api_key.as_ref().map(|k| k.expose().to_owned()))
        .collect();
    assert_eq!(
        keys,
        [Some("sk-test-owner".into()), Some("sk-test-owner".into())]
    );
}

#[tokio::test]
async fn an_exhausted_credit_is_reported_in_the_reply_and_in_the_job() {
    let h = Harness::new(vec![
        vec![Step::Fail(LlmError::CreditExhausted)],
        vec![Step::Fail(LlmError::CreditExhausted)],
    ])
    .await;
    let folder = h.folder(TOKEN, "F").await;
    let topic = h.topic(TOKEN, &folder, "T").await;
    let sent = h
        .post(
            &format!("/api/v1/topics/{topic}/messages"),
            json!({"text": "ciao"}),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.body);
    let mut failed = Value::Null;
    for _ in 0..100 {
        let m = h.get(&format!("/api/v1/topics/{topic}/messages")).await;
        let newest = m.body["items"][0].clone();
        if newest["status"] == "failed" {
            failed = newest;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(failed["error"]["code"], "llm_credit_exhausted", "{failed}");

    // The same for a job: the key of the topic's owner reached the provider
    // and was refused.
    make_ready(&h, &topic).await;
    let queued = h
        .post(
            &format!("/api/v1/topics/{topic}/resources/research"),
            json!({}),
        )
        .await;
    assert_eq!(queued.status, StatusCode::ACCEPTED, "{}", queued.body);
    let runner = std::sync::Arc::new(h.state.job_runner(vec![(
        domain::JobKind::Research,
        h.state.research_handler(),
    )]));
    assert!(runner.run_next().await.unwrap());
    let jobs = h.get(&format!("/api/v1/topics/{topic}/jobs")).await;
    assert_eq!(
        jobs.body["items"][0]["error"]["code"],
        "llm_credit_exhausted"
    );
    let keys: Vec<_> = h
        .llm
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.api_key.as_ref().map(|k| k.expose().to_owned()))
        .collect();
    assert_eq!(
        keys,
        [Some("sk-test-owner".into()), Some("sk-test-owner".into())]
    );
}

#[tokio::test]
async fn each_reply_is_made_with_the_key_of_the_topics_owner() {
    let h = Harness::new(vec![vec![Step::Delta("a")], vec![Step::Delta("b")]]).await;
    let bob = h.user("bob", true).await;
    for (token, name) in [(TOKEN, "A"), (bob.as_str(), "B")] {
        let folder = h.folder(token, "F").await;
        let topic = h.topic(token, &folder, name).await;
        let sent = h
            .request(
                Some(token),
                Method::POST,
                &format!("/api/v1/topics/{topic}/messages"),
                Some(json!({"text": "ciao"})),
            )
            .await;
        assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.body);
        for _ in 0..100 {
            let m = h
                .request(
                    Some(token),
                    Method::GET,
                    &format!("/api/v1/topics/{topic}/messages"),
                    None,
                )
                .await;
            if m.body["items"][0]["status"] == "complete" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
    let keys: Vec<_> = h
        .llm
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.api_key.as_ref().map(|k| k.expose().to_owned()))
        .collect();
    assert_eq!(
        keys,
        [Some("sk-test-owner".into()), Some("sk-of-bob".into())]
    );
}

// ---------- streams do not outlive the session ----------

async fn open_stream(h: &Harness, token: &str, topic: &str) -> axum::body::BodyDataStream {
    use tower::ServiceExt;
    let req = axum::http::Request::builder()
        .uri(format!("/api/v1/topics/{topic}/events"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let res = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    res.into_body().into_data_stream()
}

async fn ends_soon(mut stream: axum::body::BodyDataStream) -> bool {
    use futures_util::StreamExt;
    tokio::time::timeout(std::time::Duration::from_secs(3), async move {
        while stream.next().await.is_some() {}
    })
    .await
    .is_ok()
}

#[tokio::test]
async fn an_open_stream_ends_when_its_session_does() {
    let h = Harness::with_ping(vec![], std::time::Duration::from_millis(50)).await;
    let user = h.user("ann", false).await;
    let other_device = h.login("ann", "a long password").await.body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let folder = h.folder(&user, "F").await;
    let topic = h.topic(&user, &folder, "T").await;

    // Logout.
    let s = open_stream(&h, &user, &topic).await;
    h.request(Some(&user), Method::POST, "/api/v1/auth/logout", None)
        .await;
    assert!(ends_soon(s).await, "after logout");

    // Password change on another device: this one is revoked.
    let again = h.login("ann", "a long password").await.body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let s = open_stream(&h, &again, &topic).await;
    let put = h
        .request(
            Some(&other_device),
            Method::PUT,
            "/api/v1/me/password",
            Some(
                json!({"current_password": "a long password", "new_password": "another long one"}),
            ),
        )
        .await;
    assert_eq!(put.status, StatusCode::NO_CONTENT);
    assert!(ends_soon(s).await, "after a password change");

    // Disabled (as `user disable` does, from another process).
    let s = open_stream(&h, &other_device, &topic).await;
    h.sql(
        "UPDATE users SET disabled_at = '2026-11-08T15:00:00.000Z' WHERE username = 'ann'",
        &[],
    )
    .await;
    assert!(ends_soon(s).await, "after the user was disabled");

    // Deleted.
    h.sql(
        "UPDATE users SET disabled_at = NULL WHERE username = 'ann'",
        &[],
    )
    .await;
    let s = open_stream(&h, &other_device, &topic).await;
    h.sql("DELETE FROM users WHERE username = 'ann'", &[]).await;
    assert!(ends_soon(s).await, "after the user was deleted");
}

#[tokio::test]
async fn a_stream_of_a_live_session_stays_open_across_checks() {
    use futures_util::StreamExt;
    let h = Harness::with_ping(vec![], std::time::Duration::from_millis(30)).await;
    let folder = h.folder(TOKEN, "F").await;
    let topic = h.topic(TOKEN, &folder, "T").await;
    let mut s = open_stream(&h, TOKEN, &topic).await;
    // Several intervals pass: it keeps sending its pings.
    for _ in 0..3 {
        let chunk = tokio::time::timeout(std::time::Duration::from_secs(2), s.next()).await;
        assert!(matches!(chunk, Ok(Some(Ok(_)))), "{chunk:?}");
    }
}
