//! A real router over in-memory SQLite with several users, for the tests of
//! accounts, keys and isolation.
#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::app::{Limits, build_router};
use api::state::{AppOptions, AppState};
use application::{CreateUser, NewAccount};
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use domain::{Clock, IdGenerator};
use http_body_util::BodyExt;
use infrastructure::sqlite::{self, Pool};
use infrastructure::{SqliteUserRepository, SystemClock};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;
use tower::ServiceExt;
use uuid::Uuid;

use super::{ScriptedLlm, Step, TOKEN, auth, sign_in_owner};

pub const T0: OffsetDateTime = datetime!(2026-11-08 14:36:01.129 UTC);

pub struct TestClock(pub Mutex<OffsetDateTime>);

impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        *self.0.lock().unwrap()
    }
}

pub struct SeqIds(AtomicU64);

impl IdGenerator for SeqIds {
    fn new_id(&self) -> Uuid {
        Uuid::from_u128(
            0x0193_3b4e_0000_7000_8000_0000_0000_0000
                + u128::from(self.0.fetch_add(1, Ordering::SeqCst)),
        )
    }
}

pub struct Res {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

pub struct Harness {
    pub router: Router,
    pub state: AppState,
    pub pool: Pool,
    pub llm: Arc<ScriptedLlm>,
    pub clock: Arc<TestClock>,
    ids: Arc<SeqIds>,
}

impl Harness {
    /// `owner` is signed in (token [`TOKEN`]) and has a key (`sk-test-owner`).
    pub async fn new(scripts: Vec<Vec<Step>>) -> Self {
        Self::with_ping(scripts, Duration::from_secs(15)).await
    }

    /// With the interval of the SSE ping (which is also when streams re-check
    /// their session).
    pub async fn with_ping(scripts: Vec<Vec<Step>>, ping: Duration) -> Self {
        let mut options = AppOptions::default();
        options.chat.flush_interval = Duration::ZERO;
        options.sse_ping = ping;
        Self::with_options(scripts, options).await
    }

    /// With explicit options (limits, settings); the caller sets the chat
    /// flush interval and ping if it needs them.
    pub async fn with_options(scripts: Vec<Vec<Step>>, options: AppOptions) -> Self {
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        let llm = ScriptedLlm::with(scripts);
        let clock = Arc::new(TestClock(Mutex::new(T0)));
        let ids = Arc::new(SeqIds(AtomicU64::new(0)));
        let state = AppState::new(
            clock.clone(),
            ids.clone(),
            auth(),
            &pool,
            llm.clone(),
            options,
        );
        sign_in_owner(&state, &pool).await;
        Self {
            router: build_router(state.clone(), Limits::default()),
            state,
            pool,
            llm,
            clock,
            ids,
        }
    }

    pub fn set_time(&self, at: OffsetDateTime) {
        *self.clock.0.lock().unwrap() = at;
    }

    pub async fn request(
        &self,
        token: Option<&str>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Res {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let body = match body {
            Some(v) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        self.send(req.body(body).unwrap()).await
    }

    pub async fn send(&self, req: Request<Body>) -> Res {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        Res {
            status: parts.status,
            headers: parts.headers,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }

    /// `owner`, signed in with [`TOKEN`].
    pub async fn get(&self, path: &str) -> Res {
        self.request(Some(TOKEN), Method::GET, path, None).await
    }
    pub async fn post(&self, path: &str, body: Value) -> Res {
        self.request(Some(TOKEN), Method::POST, path, Some(body))
            .await
    }

    /// Creates a user through the same use case the `user add` command runs.
    pub async fn create_user(&self, name: &str, password: &str) {
        let auth = auth();
        let create = CreateUser::new(
            Arc::new(SqliteUserRepository::new(self.pool.clone())),
            auth.hasher,
            auth.tokens,
            Arc::new(SystemClock),
            self.ids.clone(),
        );
        create
            .execute(NewAccount {
                username: name.into(),
                display_name: None,
                password: Some(password.into()),
            })
            .await
            .unwrap();
    }

    pub async fn login(&self, name: &str, password: &str) -> Res {
        self.request(
            None,
            Method::POST,
            "/api/v1/auth/login",
            Some(json!({ "username": name, "password": password, "device_name": "test" })),
        )
        .await
    }

    /// A new user, logged in; the token.
    pub async fn user(&self, name: &str, with_key: bool) -> String {
        self.create_user(name, "a long password").await;
        let res = self.login(name, "a long password").await;
        assert_eq!(res.status, StatusCode::OK, "{}", res.body);
        let token = res.body["token"].as_str().unwrap().to_owned();
        if with_key {
            let res = self
                .request(
                    Some(&token),
                    Method::PUT,
                    "/api/v1/me/llm-key",
                    Some(json!({ "api_key": format!("sk-of-{name}") })),
                )
                .await;
            assert_eq!(res.status, StatusCode::OK, "{}", res.body);
        }
        token
    }

    pub async fn folder(&self, token: &str, name: &str) -> String {
        let r = self
            .request(
                Some(token),
                Method::POST,
                "/api/v1/folders",
                Some(json!({ "name": name })),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        r.body["id"].as_str().unwrap().to_owned()
    }

    pub async fn topic(&self, token: &str, folder: &str, title: &str) -> String {
        let r = self
            .request(
                Some(token),
                Method::POST,
                &format!("/api/v1/folders/{folder}/topics"),
                Some(json!({ "title": title })),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        r.body["id"].as_str().unwrap().to_owned()
    }

    pub async fn sql(&self, query: &'static str, binds: &[&str]) {
        let mut q = sqlx::query(query);
        for b in binds {
            q = q.bind(*b);
        }
        q.execute(&self.pool).await.unwrap();
    }
}
