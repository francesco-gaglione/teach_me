//! End-to-end tests of the folders/topics contract (`docs/contracts/folders-topics.md`),
//! driving the real router over SQLite, in process (no sockets).

mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use api::app::{Limits, build_router};
use api::state::{AppOptions, AppState};
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use domain::{Clock, IdGenerator};
use http_body_util::BodyExt;
use infrastructure::sqlite::{self, Pool};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = common::TOKEN;
const T0: OffsetDateTime = datetime!(2026-11-08 14:36:01.129_999 UTC);

struct TestClock(Mutex<OffsetDateTime>);
impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        *self.0.lock().unwrap()
    }
}

struct SeqIds(AtomicU64);
impl IdGenerator for SeqIds {
    fn new_id(&self) -> Uuid {
        // Counter in the low bits keeps ids increasing, like UUID v7.
        Uuid::from_u128(
            0x0193_3b4e_0000_7000_8000_0000_0000_0000
                + u128::from(self.0.fetch_add(1, Ordering::SeqCst)),
        )
    }
}

struct App {
    router: Router,
    clock: Arc<TestClock>,
    pool: Pool,
}

impl App {
    async fn new() -> Self {
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        Self::on(pool, Arc::new(SeqIds(AtomicU64::new(0)))).await
    }

    async fn on(pool: Pool, ids: Arc<SeqIds>) -> Self {
        let clock = Arc::new(TestClock(Mutex::new(T0)));
        let state = AppState::new(
            clock.clone(),
            ids,
            common::auth(),
            &pool,
            common::ScriptedLlm::with(vec![]),
            AppOptions::default(),
        );
        common::sign_in_owner(&state, &pool).await;
        Self {
            router: build_router(state, Limits::default()),
            clock,
            pool,
        }
    }

    fn set_time(&self, at: OffsetDateTime) {
        *self.clock.0.lock().unwrap() = at;
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
        self.send(req.body(body).unwrap()).await
    }

    async fn send(&self, req: Request<Body>) -> Res {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        Res {
            status: parts.status,
            headers: parts.headers,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }

    async fn get(&self, path: &str) -> Res {
        self.call(Method::GET, path, None).await
    }
    async fn post(&self, path: &str, body: Value) -> Res {
        self.call(Method::POST, path, Some(body)).await
    }
    async fn patch(&self, path: &str, body: Value) -> Res {
        self.call(Method::PATCH, path, Some(body)).await
    }
    async fn delete(&self, path: &str) -> Res {
        self.call(Method::DELETE, path, None).await
    }

    async fn folder(&self, name: &str) -> String {
        let r = self.post("/api/v1/folders", json!({ "name": name })).await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        r.body["id"].as_str().unwrap().to_owned()
    }

    async fn topic(&self, folder: &str, title: &str) -> String {
        let r = self
            .post(
                &format!("/api/v1/folders/{folder}/topics"),
                json!({ "title": title }),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        r.body["id"].as_str().unwrap().to_owned()
    }
}

struct Res {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

impl Res {
    fn assert_validation(&self, expected: &[(&str, &str)]) {
        assert_eq!(
            self.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            self.body
        );
        let err = &self.body["error"];
        assert_eq!(err["code"], "validation_failed");
        assert_eq!(
            err["request_id"].as_str().unwrap(),
            self.headers["x-request-id"].to_str().unwrap()
        );
        let details: Vec<(String, String)> = err["details"]
            .as_array()
            .unwrap_or_else(|| panic!("details missing: {}", self.body))
            .iter()
            .map(|d| {
                (
                    d["field"].as_str().unwrap().to_owned(),
                    d["issue"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let expected: Vec<_> = expected
            .iter()
            .map(|(f, i)| ((*f).to_owned(), (*i).to_owned()))
            .collect();
        assert_eq!(details, expected);
    }

    fn assert_error(&self, status: StatusCode, code: &str) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(self.body["error"]["code"], code, "{}", self.body);
        assert!(
            self.body["error"]["message"]
                .as_str()
                .is_some_and(|m| !m.is_empty())
        );
    }
}

fn names(r: &Res, key: &str) -> Vec<String> {
    r.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i[key].as_str().unwrap().to_owned())
        .collect()
}

// ---------------------------------------------------------------- auth

#[tokio::test]
async fn every_endpoint_requires_the_token() {
    let app = App::new().await;
    let id = "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f";
    for (method, path) in [
        (Method::GET, "/api/v1/folders".to_owned()),
        (Method::POST, "/api/v1/folders".to_owned()),
        (Method::GET, format!("/api/v1/folders/{id}")),
        (Method::PATCH, format!("/api/v1/folders/{id}")),
        (Method::DELETE, format!("/api/v1/folders/{id}")),
        (Method::GET, format!("/api/v1/folders/{id}/topics")),
        (Method::POST, format!("/api/v1/folders/{id}/topics")),
        (Method::GET, format!("/api/v1/topics/{id}")),
        (Method::PATCH, format!("/api/v1/topics/{id}")),
        (Method::DELETE, format!("/api/v1/topics/{id}")),
    ] {
        let req = Request::builder()
            .method(method)
            .uri(&path)
            .body(Body::empty())
            .unwrap();
        let r = app.send(req).await;
        r.assert_error(StatusCode::UNAUTHORIZED, "unauthorized");
    }
}

// ------------------------------------------------------------- folders

#[tokio::test]
async fn folder_crud_matches_the_contract_shape() {
    let app = App::new().await;
    let created = app
        .post("/api/v1/folders", json!({ "name": "  Lavoro " }))
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let id = created.body["id"].as_str().unwrap().to_owned();
    assert_eq!(
        created.headers[header::LOCATION].to_str().unwrap(),
        format!("/api/v1/folders/{id}")
    );
    assert_eq!(
        created.body,
        json!({
            "id": id,
            "name": "Lavoro",
            "position": 0,
            "topic_count": 0,
            "created_at": "2026-11-08T14:36:01.129Z",
            "updated_at": "2026-11-08T14:36:01.129Z",
        })
    );
    // UUID v7: version nibble and canonical lowercase hyphenated form.
    assert_eq!(id.len(), 36);
    assert_eq!(&id[14..15], "7");

    assert_eq!(
        app.get(&format!("/api/v1/folders/{id}")).await.body,
        created.body
    );

    app.set_time(datetime!(2026-11-09 08:00:00.5 UTC));
    let renamed = app
        .patch(
            &format!("/api/v1/folders/{id}"),
            json!({ "name": "Lavoro e studio" }),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    assert_eq!(renamed.body["name"], "Lavoro e studio");
    assert_eq!(renamed.body["created_at"], "2026-11-08T14:36:01.129Z");
    assert_eq!(renamed.body["updated_at"], "2026-11-09T08:00:00.500Z");

    let list = app.get("/api/v1/folders").await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body["items"], json!([renamed.body]));
    assert!(list.body["next_cursor"].is_null());
    assert!(list.body.as_object().unwrap().contains_key("next_cursor"));

    let deleted = app.delete(&format!("/api/v1/folders/{id}")).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(deleted.body, Value::Null);
    app.get(&format!("/api/v1/folders/{id}"))
        .await
        .assert_error(StatusCode::NOT_FOUND, "not_found");
    app.delete(&format!("/api/v1/folders/{id}"))
        .await
        .assert_error(StatusCode::NOT_FOUND, "not_found");
}

#[tokio::test]
async fn folder_positions_increase_and_leave_gaps_after_delete() {
    let app = App::new().await;
    let a = app.folder("A").await;
    let b = app.folder("B").await;
    app.delete(&format!("/api/v1/folders/{a}")).await;
    let c = app.folder("C").await;
    let list = app.get("/api/v1/folders").await;
    let positions: Vec<_> = list.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["position"].as_i64().unwrap())
        .collect();
    assert_eq!(positions, [1, 2]);
    assert_eq!(names(&list, "id"), [b, c]);
}

#[tokio::test]
async fn folder_name_validation() {
    let app = App::new().await;
    for body in [
        json!({}),
        json!({ "name": "" }),
        json!({ "name": "   \n\t" }),
        json!({ "name": null }),
    ] {
        app.post("/api/v1/folders", body)
            .await
            .assert_validation(&[("name", "empty")]);
    }
    app.post("/api/v1/folders", json!({ "name": "a".repeat(81) }))
        .await
        .assert_validation(&[("name", "too_long")]);
    assert_eq!(
        app.post("/api/v1/folders", json!({ "name": "a".repeat(80) }))
            .await
            .status,
        StatusCode::CREATED
    );
    // 80 code points, 320 bytes; counts are in code points, after trim.
    assert_eq!(
        app.post(
            "/api/v1/folders",
            json!({ "name": format!(" {} ", "😀".repeat(80)) })
        )
        .await
        .status,
        StatusCode::CREATED
    );
    app.post("/api/v1/folders", json!({ "name": "😀".repeat(81) }))
        .await
        .assert_validation(&[("name", "too_long")]);
}

#[tokio::test]
async fn wrong_json_type_is_422_without_details_and_malformed_json_is_400() {
    let app = App::new().await;
    let r = app.post("/api/v1/folders", json!({ "name": 5 })).await;
    r.assert_error(StatusCode::UNPROCESSABLE_ENTITY, "validation_failed");
    assert!(r.body["error"]["details"].is_null());
    let r = app.post("/api/v1/folders", json!(["x"])).await;
    r.assert_error(StatusCode::UNPROCESSABLE_ENTITY, "validation_failed");

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/folders")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    app.send(req)
        .await
        .assert_error(StatusCode::BAD_REQUEST, "bad_request");
}

#[tokio::test]
async fn folder_names_are_unique_case_insensitively_after_trim() {
    let app = App::new().await;
    app.folder("Economía").await;
    for dup in ["economía", "  ECONOMÍA ", "Economía"] {
        app.post("/api/v1/folders", json!({ "name": dup }))
            .await
            .assert_validation(&[("name", "duplicate")]);
    }
}

#[tokio::test]
async fn renaming_to_own_name_with_different_case_is_not_a_duplicate() {
    let app = App::new().await;
    let id = app.folder("lavoro").await;
    let r = app
        .patch(
            &format!("/api/v1/folders/{id}"),
            json!({ "name": "LAVORO" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["name"], "LAVORO");
    let other = app.folder("Altro").await;
    app.patch(
        &format!("/api/v1/folders/{other}"),
        json!({ "name": "lavoro" }),
    )
    .await
    .assert_validation(&[("name", "duplicate")]);
}

#[tokio::test]
async fn folder_patch_validation_and_null() {
    let app = App::new().await;
    let id = app.folder("A").await;
    let path = format!("/api/v1/folders/{id}");
    app.patch(&path, json!({ "name": null }))
        .await
        .assert_validation(&[("name", "empty")]);
    app.patch(&path, json!({ "name": "  " }))
        .await
        .assert_validation(&[("name", "empty")]);
    app.patch(&path, json!({ "name": "x".repeat(81) }))
        .await
        .assert_validation(&[("name", "too_long")]);
    // Unchanged after all the failures.
    assert_eq!(app.get(&path).await.body["name"], "A");
}

#[tokio::test]
async fn unknown_and_read_only_fields_are_ignored() {
    let app = App::new().await;
    let r = app
        .post(
            "/api/v1/folders",
            json!({
                "name": "A", "position": 99, "topic_count": 5, "id": "x",
                "created_at": "2000-01-01T00:00:00.000Z", "whatever": [1]
            }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert_eq!(r.body["position"], 0);
    assert_eq!(r.body["topic_count"], 0);
    assert_eq!(r.body["created_at"], "2026-11-08T14:36:01.129Z");
    let id = r.body["id"].as_str().unwrap().to_owned();
    assert_ne!(id, "x");

    let p = app
        .patch(
            &format!("/api/v1/folders/{id}"),
            json!({ "name": "B", "position": 7, "id": "y" }),
        )
        .await;
    assert_eq!(p.body["position"], 0);
    assert_eq!(p.body["id"], id);
}

#[tokio::test]
async fn empty_patch_returns_the_resource_without_touching_updated_at() {
    let app = App::new().await;
    let f = app.folder("A").await;
    let t = app.topic(&f, "T").await;
    app.set_time(datetime!(2026-12-01 00:00:00 UTC));

    let before = app.get(&format!("/api/v1/folders/{f}")).await.body;
    let after = app.patch(&format!("/api/v1/folders/{f}"), json!({})).await;
    assert_eq!(after.status, StatusCode::OK);
    assert_eq!(after.body, before);

    let before = app.get(&format!("/api/v1/topics/{t}")).await.body;
    let after = app.patch(&format!("/api/v1/topics/{t}"), json!({})).await;
    assert_eq!(after.status, StatusCode::OK);
    assert_eq!(after.body, before);

    // Same values are also a no-op.
    let same = app
        .patch(
            &format!("/api/v1/topics/{t}"),
            json!({ "title": "T", "folder_id": f }),
        )
        .await;
    assert_eq!(same.body["updated_at"], before["updated_at"]);
}

#[tokio::test]
async fn invalid_ids_in_paths_are_400_invalid_id() {
    let app = App::new().await;
    let good = "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f";
    for bad in [
        "abc",
        "123",
        "01933b4e8f2a7c3d9e1f2a3b4c5d6e7f",
        "%7B01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f%7D",
    ] {
        for (method, path) in [
            (Method::GET, format!("/api/v1/folders/{bad}")),
            (Method::PATCH, format!("/api/v1/folders/{bad}")),
            (Method::DELETE, format!("/api/v1/folders/{bad}")),
            (Method::GET, format!("/api/v1/folders/{bad}/topics")),
            (Method::POST, format!("/api/v1/folders/{bad}/topics")),
            (Method::GET, format!("/api/v1/topics/{bad}")),
            (Method::PATCH, format!("/api/v1/topics/{bad}")),
            (Method::DELETE, format!("/api/v1/topics/{bad}")),
        ] {
            let body = matches!(method, Method::PATCH | Method::POST).then(|| json!({}));
            let r = app.call(method.clone(), &path, body).await;
            assert_eq!(
                r.status,
                StatusCode::BAD_REQUEST,
                "{method} {path}: {}",
                r.body
            );
            assert_eq!(r.body["error"]["code"], "invalid_id");
        }
    }
    // A well-formed id that does not exist is 404, not 400.
    app.get(&format!("/api/v1/folders/{good}"))
        .await
        .assert_error(StatusCode::NOT_FOUND, "not_found");
}

#[tokio::test]
async fn missing_resources_are_404() {
    let app = App::new().await;
    let id = "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f";
    for (method, path) in [
        (Method::GET, format!("/api/v1/folders/{id}")),
        (Method::PATCH, format!("/api/v1/folders/{id}")),
        (Method::DELETE, format!("/api/v1/folders/{id}")),
        (Method::GET, format!("/api/v1/folders/{id}/topics")),
        (Method::POST, format!("/api/v1/folders/{id}/topics")),
        (Method::GET, format!("/api/v1/topics/{id}")),
        (Method::PATCH, format!("/api/v1/topics/{id}")),
        (Method::DELETE, format!("/api/v1/topics/{id}")),
    ] {
        // Bodies are deliberately invalid: existence is reported first.
        let body = matches!(method, Method::PATCH | Method::POST)
            .then(|| json!({ "name": "", "title": "" }));
        let r = app.call(method.clone(), &path, body).await;
        assert_eq!(
            r.status,
            StatusCode::NOT_FOUND,
            "{method} {path}: {}",
            r.body
        );
        assert_eq!(r.body["error"]["code"], "not_found");
    }
}

// -------------------------------------------------------------- topics

#[tokio::test]
async fn topic_crud_matches_the_contract_shape() {
    let app = App::new().await;
    let f = app.folder("A").await;
    let created = app
        .post(
            &format!("/api/v1/folders/{f}/topics"),
            json!({ "title": " Rust ", "state": "ready" }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let id = created.body["id"].as_str().unwrap().to_owned();
    assert_eq!(
        created.headers[header::LOCATION].to_str().unwrap(),
        format!("/api/v1/topics/{id}")
    );
    assert_eq!(
        created.body,
        json!({
            "id": id,
            "folder_id": f,
            "title": "Rust",
            "language": "it",
            "state": "new",
            "created_at": "2026-11-08T14:36:01.129Z",
            "updated_at": "2026-11-08T14:36:01.129Z",
        })
    );
    assert_eq!(
        app.get(&format!("/api/v1/topics/{id}")).await.body,
        created.body
    );

    app.set_time(datetime!(2026-11-09 08:00:00 UTC));
    let renamed = app
        .patch(
            &format!("/api/v1/topics/{id}"),
            json!({ "title": "Rust avanzato", "state": "ready" }),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    assert_eq!(renamed.body["title"], "Rust avanzato");
    assert_eq!(renamed.body["state"], "new", "state is not editable");
    assert_eq!(renamed.body["updated_at"], "2026-11-09T08:00:00.000Z");

    let list = app.get(&format!("/api/v1/folders/{f}/topics")).await;
    assert_eq!(list.body["items"], json!([renamed.body]));
    assert!(list.body["next_cursor"].is_null());

    assert_eq!(
        app.delete(&format!("/api/v1/topics/{id}")).await.status,
        StatusCode::NO_CONTENT
    );
    app.get(&format!("/api/v1/topics/{id}"))
        .await
        .assert_error(StatusCode::NOT_FOUND, "not_found");
    app.delete(&format!("/api/v1/topics/{id}"))
        .await
        .assert_error(StatusCode::NOT_FOUND, "not_found");
}

#[tokio::test]
async fn topic_title_validation() {
    let app = App::new().await;
    let f = app.folder("A").await;
    let path = format!("/api/v1/folders/{f}/topics");
    for body in [
        json!({}),
        json!({ "title": "" }),
        json!({ "title": "  " }),
        json!({ "title": null }),
    ] {
        app.post(&path, body)
            .await
            .assert_validation(&[("title", "empty")]);
    }
    app.post(&path, json!({ "title": "a".repeat(121) }))
        .await
        .assert_validation(&[("title", "too_long")]);
    assert_eq!(
        app.post(&path, json!({ "title": "a".repeat(120) }))
            .await
            .status,
        StatusCode::CREATED
    );
    app.post(&path, json!({ "title": "日".repeat(121) }))
        .await
        .assert_validation(&[("title", "too_long")]);
    assert_eq!(
        app.post(&path, json!({ "title": "日".repeat(120) }))
            .await
            .status,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn the_language_of_a_topic_is_chosen_at_creation_and_never_changes() {
    let app = App::new().await;
    let f = app.folder("A").await;
    let path = format!("/api/v1/folders/{f}/topics");

    // Absent and null mean Italian.
    for (title, body) in [
        ("Uno", json!({ "title": "Uno" })),
        ("Due", json!({ "title": "Due", "language": null })),
        ("Tre", json!({ "title": "Tre", "language": "it" })),
    ] {
        let r = app.post(&path, body).await;
        assert_eq!(r.status, StatusCode::CREATED, "{title}");
        assert_eq!(r.body["language"], "it", "{title}");
    }

    let en = app
        .post(&path, json!({ "title": "Rust", "language": "en" }))
        .await;
    assert_eq!(en.status, StatusCode::CREATED);
    assert_eq!(en.body["language"], "en");
    let id = en.body["id"].as_str().unwrap().to_owned();

    // Every representation carries it.
    assert_eq!(
        app.get(&format!("/api/v1/topics/{id}")).await.body["language"],
        "en"
    );
    let list = app.get(&path).await;
    let languages: Vec<_> = list.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["language"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(languages, ["it", "it", "it", "en"]);

    // PATCH ignores it, as it does state.
    let patched = app
        .patch(
            &format!("/api/v1/topics/{id}"),
            json!({ "title": "Rust avanzato", "language": "it" }),
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK);
    assert_eq!(patched.body["language"], "en");
    assert_eq!(patched.body["title"], "Rust avanzato");
}

#[tokio::test]
async fn an_unknown_language_is_a_validation_error_on_the_field() {
    let app = App::new().await;
    let f = app.folder("A").await;
    let path = format!("/api/v1/folders/{f}/topics");
    for language in ["fr", "", "IT", "italiano"] {
        app.post(&path, json!({ "title": "T", "language": language }))
            .await
            .assert_validation(&[("language", "invalid")]);
    }
    // Together with another problem: both are reported.
    app.post(&path, json!({ "language": "fr" }))
        .await
        .assert_validation(&[("title", "empty"), ("language", "invalid")]);
    // A wrong JSON type is the generic 422 without details.
    let r = app
        .post(&path, json!({ "title": "T", "language": 5 }))
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(r.body["error"]["details"].is_null());
    // Nothing was created.
    assert_eq!(app.get(&path).await.body["items"], json!([]));
}

#[tokio::test]
async fn topic_titles_are_unique_per_folder_only() {
    let app = App::new().await;
    let a = app.folder("A").await;
    let b = app.folder("B").await;
    app.topic(&a, "Rust").await;
    app.post(
        &format!("/api/v1/folders/{a}/topics"),
        json!({ "title": "  rUST " }),
    )
    .await
    .assert_validation(&[("title", "duplicate")]);
    app.topic(&b, "Rust").await;
}

#[tokio::test]
async fn move_topic_between_folders() {
    let app = App::new().await;
    let a = app.folder("A").await;
    let b = app.folder("B").await;
    let t = app.topic(&a, "Rust").await;
    app.topic(&b, "Python").await;
    let path = format!("/api/v1/topics/{t}");

    app.set_time(datetime!(2026-11-09 08:00:00 UTC));
    let moved = app.patch(&path, json!({ "folder_id": b })).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    assert_eq!(moved.body["folder_id"], b);
    assert_eq!(moved.body["updated_at"], "2026-11-09T08:00:00.000Z");

    assert_eq!(
        app.get(&format!("/api/v1/folders/{a}")).await.body["topic_count"],
        0
    );
    assert_eq!(
        app.get(&format!("/api/v1/folders/{b}")).await.body["topic_count"],
        2
    );
    // Moving a topic does not modify either folder's own updated_at.
    assert_eq!(
        app.get(&format!("/api/v1/folders/{b}")).await.body["updated_at"],
        "2026-11-08T14:36:01.129Z"
    );

    // Title and move together.
    let both = app
        .patch(&path, json!({ "title": "Rust avanzato", "folder_id": a }))
        .await;
    assert_eq!(both.body["title"], "Rust avanzato");
    assert_eq!(both.body["folder_id"], a);
}

#[tokio::test]
async fn move_checks_title_uniqueness_in_the_destination() {
    let app = App::new().await;
    let a = app.folder("A").await;
    let b = app.folder("B").await;
    let t = app.topic(&a, "Rust").await;
    app.topic(&b, "RUST").await;
    app.patch(&format!("/api/v1/topics/{t}"), json!({ "folder_id": b }))
        .await
        .assert_validation(&[("title", "duplicate")]);
    // Renaming in the same request resolves the clash.
    let ok = app
        .patch(
            &format!("/api/v1/topics/{t}"),
            json!({ "folder_id": b, "title": "Rust 2" }),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);
}

#[tokio::test]
async fn topic_patch_folder_id_errors_and_multiple_details() {
    let app = App::new().await;
    let a = app.folder("A").await;
    let t = app.topic(&a, "Rust").await;
    let path = format!("/api/v1/topics/{t}");
    let ghost = "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f";

    app.patch(&path, json!({ "folder_id": ghost }))
        .await
        .assert_validation(&[("folder_id", "not_found")]);
    app.patch(&path, json!({ "folder_id": "nope" }))
        .await
        .assert_validation(&[("folder_id", "invalid")]);
    app.patch(&path, json!({ "folder_id": null }))
        .await
        .assert_validation(&[("folder_id", "invalid")]);
    app.patch(&path, json!({ "title": null }))
        .await
        .assert_validation(&[("title", "empty")]);
    app.patch(&path, json!({ "title": "", "folder_id": ghost }))
        .await
        .assert_validation(&[("title", "empty"), ("folder_id", "not_found")]);
    app.patch(
        &path,
        json!({ "title": "x".repeat(121), "folder_id": "bad" }),
    )
    .await
    .assert_validation(&[("title", "too_long"), ("folder_id", "invalid")]);

    let unchanged = app.get(&path).await.body;
    assert_eq!(unchanged["title"], "Rust");
    assert_eq!(unchanged["folder_id"], a);
}

#[tokio::test]
async fn deleting_a_folder_cascades_to_its_topics() {
    let app = App::new().await;
    let a = app.folder("A").await;
    let b = app.folder("B").await;
    let t1 = app.topic(&a, "T1").await;
    let t2 = app.topic(&a, "T2").await;
    let kept = app.topic(&b, "Keep").await;

    assert_eq!(
        app.get(&format!("/api/v1/folders/{a}")).await.body["topic_count"],
        2
    );
    assert_eq!(
        app.delete(&format!("/api/v1/folders/{a}")).await.status,
        StatusCode::NO_CONTENT
    );
    for t in [t1, t2] {
        app.get(&format!("/api/v1/topics/{t}"))
            .await
            .assert_error(StatusCode::NOT_FOUND, "not_found");
    }
    assert_eq!(
        app.get(&format!("/api/v1/topics/{kept}")).await.status,
        StatusCode::OK
    );
    app.get(&format!("/api/v1/folders/{a}/topics"))
        .await
        .assert_error(StatusCode::NOT_FOUND, "not_found");
    // Nothing is left behind in the table either.
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM topics")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn creating_and_deleting_topics_does_not_touch_the_folder_updated_at() {
    let app = App::new().await;
    let f = app.folder("A").await;
    app.set_time(datetime!(2026-12-01 00:00:00 UTC));
    let t = app.topic(&f, "T").await;
    app.delete(&format!("/api/v1/topics/{t}")).await;
    let folder = app.get(&format!("/api/v1/folders/{f}")).await.body;
    assert_eq!(folder["updated_at"], "2026-11-08T14:36:01.129Z");
    assert_eq!(folder["topic_count"], 0);
}

// ---------------------------------------------------------- pagination

#[tokio::test]
async fn folders_paginate_over_several_pages_surviving_changes() {
    let app = App::new().await;
    let mut ids = Vec::new();
    for n in 0..7 {
        ids.push(app.folder(&format!("F{n}")).await);
    }
    let p1 = app.get("/api/v1/folders?limit=3").await;
    assert_eq!(names(&p1, "name"), ["F0", "F1", "F2"]);
    let c1 = p1.body["next_cursor"].as_str().unwrap().to_owned();

    // Between pages: the last seen item is deleted, an upcoming one too, and a new one is added.
    app.delete(&format!("/api/v1/folders/{}", ids[2])).await;
    app.delete(&format!("/api/v1/folders/{}", ids[4])).await;
    app.folder("F7").await;

    let p2 = app
        .get(&format!("/api/v1/folders?limit=3&cursor={c1}"))
        .await;
    assert_eq!(names(&p2, "name"), ["F3", "F5", "F6"]);
    let c2 = p2.body["next_cursor"].as_str().unwrap().to_owned();
    let p3 = app
        .get(&format!("/api/v1/folders?limit=3&cursor={c2}"))
        .await;
    assert_eq!(names(&p3, "name"), ["F7"]);
    assert!(p3.body["next_cursor"].is_null());
}

#[tokio::test]
async fn topics_paginate_in_creation_order_surviving_changes() {
    let app = App::new().await;
    let f = app.folder("A").await;
    let mut ids = Vec::new();
    for n in 0..5 {
        app.set_time(T0 + time::Duration::seconds(n));
        ids.push(app.topic(&f, &format!("T{n}")).await);
    }
    let base = format!("/api/v1/folders/{f}/topics");
    let p1 = app.get(&format!("{base}?limit=2")).await;
    assert_eq!(names(&p1, "title"), ["T0", "T1"]);
    let c1 = p1.body["next_cursor"].as_str().unwrap().to_owned();

    app.delete(&format!("/api/v1/topics/{}", ids[1])).await; // last item of page 1
    app.set_time(T0 + time::Duration::seconds(10));
    app.topic(&f, "T5").await;

    let p2 = app.get(&format!("{base}?limit=2&cursor={c1}")).await;
    assert_eq!(names(&p2, "title"), ["T2", "T3"]);
    let c2 = p2.body["next_cursor"].as_str().unwrap().to_owned();
    let p3 = app.get(&format!("{base}?limit=2&cursor={c2}")).await;
    assert_eq!(names(&p3, "title"), ["T4", "T5"]);
    assert!(p3.body["next_cursor"].is_null());
}

#[tokio::test]
async fn topics_created_in_the_same_millisecond_keep_a_stable_order() {
    let app = App::new().await;
    let f = app.folder("A").await;
    for n in 0..5 {
        app.topic(&f, &format!("T{n}")).await; // clock is frozen: identical created_at
    }
    let base = format!("/api/v1/folders/{f}/topics");
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let url = match &cursor {
            None => format!("{base}?limit=2"),
            Some(c) => format!("{base}?limit=2&cursor={c}"),
        };
        let page = app.get(&url).await;
        seen.extend(names(&page, "title"));
        match page.body["next_cursor"].as_str() {
            Some(c) => cursor = Some(c.to_owned()),
            None => break,
        }
    }
    assert_eq!(seen, ["T0", "T1", "T2", "T3", "T4"]);
}

#[tokio::test]
async fn default_limit_is_50_and_max_is_200() {
    let app = App::new().await;
    for n in 0..51 {
        app.folder(&format!("F{n}")).await;
    }
    let page = app.get("/api/v1/folders").await;
    assert_eq!(page.body["items"].as_array().unwrap().len(), 50);
    assert!(page.body["next_cursor"].is_string());
    let all = app.get("/api/v1/folders?limit=200").await;
    assert_eq!(all.body["items"].as_array().unwrap().len(), 51);
    assert!(all.body["next_cursor"].is_null());
    // Exactly one full page: no cursor.
    let exact = app.get("/api/v1/folders?limit=51").await;
    assert!(exact.body["next_cursor"].is_null());
}

#[tokio::test]
async fn bad_query_parameters_are_400_bad_request() {
    let app = App::new().await;
    let f = app.folder("A").await;
    let folder_cursor = app.folder("B").await; // only to have two folders
    let _ = folder_cursor;
    let cursor = app.get("/api/v1/folders?limit=1").await.body["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    for query in [
        "limit=0",
        "limit=201",
        "limit=-1",
        "limit=abc",
        "limit=",
        "limit=1.5",
        "cursor=%25%25%25",
        "cursor=bm90LWEtY3Vyc29y",
        "cursor=",
    ] {
        for path in [
            "/api/v1/folders".to_owned(),
            format!("/api/v1/folders/{f}/topics"),
        ] {
            let r = app.get(&format!("{path}?{query}")).await;
            r.assert_error(StatusCode::BAD_REQUEST, "bad_request");
            assert!(r.body["error"]["details"].is_null());
        }
    }
    // A folder cursor cannot be replayed on a topic list.
    app.get(&format!("/api/v1/folders/{f}/topics?cursor={cursor}"))
        .await
        .assert_error(StatusCode::BAD_REQUEST, "bad_request");
    // Unknown parameters are ignored.
    assert_eq!(
        app.get("/api/v1/folders?foo=bar").await.status,
        StatusCode::OK
    );
}

// --------------------------------------------------------- persistence

#[tokio::test]
async fn data_survives_reopening_the_database() {
    let dir = std::env::temp_dir().join(format!("teach_me_http_{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("t.db").display());

    let (folder, topic, snapshot) = {
        let pool = sqlite::connect(&url).await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        let app = App::on(pool.clone(), Arc::new(SeqIds(AtomicU64::new(0)))).await;
        let folder = app.folder("Lavoro").await;
        let topic = app.topic(&folder, "Rust").await;
        let snapshot = app.get(&format!("/api/v1/folders/{folder}")).await.body;
        pool.close().await;
        (folder, topic, snapshot)
    };

    let pool = sqlite::connect(&url).await.unwrap();
    sqlite::migrate(&pool).await.unwrap();
    // Fresh id counter starting past the old ones, as a restarted process would (UUID v7).
    let app = App::on(pool.clone(), Arc::new(SeqIds(AtomicU64::new(1000)))).await;
    assert_eq!(
        app.get(&format!("/api/v1/folders/{folder}")).await.body,
        snapshot
    );
    let t = app.get(&format!("/api/v1/topics/{topic}")).await;
    assert_eq!(t.body["title"], "Rust");
    assert_eq!(t.body["folder_id"], folder);
    assert_eq!(t.body["state"], "new");
    // Uniqueness still enforced from the stored keys.
    app.post("/api/v1/folders", json!({ "name": "LAVORO" }))
        .await
        .assert_validation(&[("name", "duplicate")]);
    // Cascade still works on the reopened file (foreign keys on every connection).
    app.delete(&format!("/api/v1/folders/{folder}")).await;
    app.get(&format!("/api/v1/topics/{topic}"))
        .await
        .assert_error(StatusCode::NOT_FOUND, "not_found");

    pool.close().await;
    std::fs::remove_dir_all(dir).unwrap();
}

// ------------------------------------------- precedence of path/existence/body

async fn raw(app: &App, method: Method, path: &str, ctype: Option<&str>, body: &str) -> Res {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    if let Some(c) = ctype {
        req = req.header(header::CONTENT_TYPE, c);
    }
    app.send(req.body(Body::from(body.to_owned())).unwrap())
        .await
}

#[tokio::test]
async fn existence_is_checked_before_the_body_is_parsed() {
    let app = App::new().await;
    let ghost = "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f";
    for (method, path) in [
        (Method::PATCH, format!("/api/v1/folders/{ghost}")),
        (Method::POST, format!("/api/v1/folders/{ghost}/topics")),
        (Method::PATCH, format!("/api/v1/topics/{ghost}")),
    ] {
        for body in [r#"{"name": 5, "title": 5}"#, "[]", r#"["x"]"#, "{oops"] {
            let r = raw(&app, method.clone(), &path, Some("application/json"), body).await;
            r.assert_error(StatusCode::NOT_FOUND, "not_found");
        }
    }
}

#[tokio::test]
async fn invalid_id_wins_over_a_bad_body() {
    let app = App::new().await;
    for (method, path) in [
        (Method::PATCH, "/api/v1/folders/zzz"),
        (Method::POST, "/api/v1/folders/zzz/topics"),
        (Method::PATCH, "/api/v1/topics/zzz"),
    ] {
        for body in [r#"{"name": 5}"#, "[]", "{oops"] {
            let r = raw(&app, method.clone(), path, Some("application/json"), body).await;
            r.assert_error(StatusCode::BAD_REQUEST, "invalid_id");
        }
    }
}

#[tokio::test]
async fn existing_resource_with_bad_body_still_reports_body_errors() {
    let app = App::new().await;
    let f = app.folder("A").await;
    let t = app.topic(&f, "T").await;
    let r = raw(
        &app,
        Method::PATCH,
        &format!("/api/v1/topics/{t}"),
        Some("application/json"),
        r#"{"title": 5}"#,
    )
    .await;
    r.assert_error(StatusCode::UNPROCESSABLE_ENTITY, "validation_failed");
    let r = raw(
        &app,
        Method::PATCH,
        &format!("/api/v1/folders/{f}"),
        Some("application/json"),
        "{oops",
    )
    .await;
    r.assert_error(StatusCode::BAD_REQUEST, "bad_request");
}

#[tokio::test]
async fn missing_content_type_is_a_contract_formatted_error() {
    let app = App::new().await;
    let f = app.folder("A").await;
    let t = app.topic(&f, "T").await;
    for (method, path) in [
        (Method::POST, "/api/v1/folders".to_owned()),
        (Method::PATCH, format!("/api/v1/folders/{f}")),
        (Method::POST, format!("/api/v1/folders/{f}/topics")),
        (Method::PATCH, format!("/api/v1/topics/{t}")),
    ] {
        let r = raw(&app, method, &path, None, r#"{"name":"x","title":"y"}"#).await;
        r.assert_error(StatusCode::BAD_REQUEST, "bad_request");
        assert!(r.body["error"]["details"].is_null());
        assert_eq!(
            r.body["error"]["request_id"].as_str().unwrap(),
            r.headers["x-request-id"].to_str().unwrap()
        );
    }
}
