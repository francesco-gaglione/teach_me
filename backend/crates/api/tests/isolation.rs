//! The safety net of the multi-user design (ADR 0008 §3): two users, and for
//! every route of the router the second one, holding the first one's ids, is
//! told `404 not_found`, exactly as for an id that does not exist.
//!
//! The list of routes is read from the router. A route without its case here
//! fails `every_route_of_the_router_has_an_isolation_case`.

mod common;

use std::collections::BTreeSet;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::harness::{Harness, Res};
use common::{Step, TOKEN};
use serde_json::{Value, json};
use uuid::Uuid;

const AT: &str = "2026-11-09T14:00:00.000Z";

// Ids of user A's data (valid UUIDs, in the form the API accepts).
const LESSON: &str = "01933b4e-0000-7000-8000-00000000a001";
const JOB: &str = "01933b4e-0000-7000-8000-00000000a002";
const MESSAGE: &str = "01933b4e-0000-7000-8000-00000000a003";
const DOCUMENT: &str = "01933b4e-0000-7000-8000-00000000a004";
const PREF_GLOBAL: &str = "01933b4e-0000-7000-8000-00000000a005";
const PREF_TOPIC: &str = "01933b4e-0000-7000-8000-00000000a006";
const CARD: &str = "01933b4e-0000-7000-8000-00000000a007";
const CLIP: &str = "01933b4e-0000-7000-8000-00000000a00a";
const LESSON_IN_PROGRESS: &str = "01933b4e-0000-7000-8000-00000000a00b";
const ATTEMPT: &str = "01933b4e-0000-7000-8000-00000000a00c";
const TERM: &str = "01933b4e-0000-7000-8000-00000000a008";

struct Ids {
    folder: String,
    topic: String,
}

/// User A: everything a user can own, so that every id in the routes is real.
async fn seed_user_a(h: &Harness) -> Ids {
    let folder = h.folder(TOKEN, "Lavoro").await;
    let topic = h.topic(TOKEN, &folder, "Rust").await;
    h.sql("UPDATE topics SET state = 'ready' WHERE id = ?", &[&topic])
        .await;
    h.sql(
        "INSERT INTO missions (topic_id, version, status, why, success_criteria, mission_constraints, \
         out_of_scope, created_at, confirmed_at) VALUES (?, 1, 'confirmed', 'A-segreto', '[\"c\"]', '[]', '[]', ?, ?)",
        &[&topic, AT, AT],
    )
    .await;
    h.sql(
        "INSERT INTO chat_messages (id, topic_id, role, status, parts, error_code, error_message, created_at, updated_at) \
         VALUES (?, ?, 'assistant', 'failed', '[]', 'internal', 'x', ?, ?)",
        &[MESSAGE, &topic, AT, AT],
    )
    .await;
    h.sql(
        "INSERT INTO jobs (id, topic_id, kind, status, error_code, error_message, attempt, created_at) \
         VALUES (?, ?, 'research', 'failed', 'internal', 'x', 1, ?)",
        &[JOB, &topic, AT],
    )
    .await;
    h.sql(
        "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, catalog_version, \
         mission_version, document, plan, generation_job_id, created_at) \
         VALUES (?, ?, 1, 'Lezione A', 'o', 5, 1, 1, '{}', '{}', 'gj1', ?)",
        &[LESSON, &topic, AT],
    )
    .await;
    h.sql(
        "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, status, \
         catalog_version, mission_version, document, plan, generation_job_id, created_at) \
         VALUES (?, ?, 2, 'Lezione A in corso', 'o', 5, 'in_progress', 1, 1, '{}', '{}', 'gj2', ?)",
        &[LESSON_IN_PROGRESS, &topic, AT],
    )
    .await;
    h.sql(
        "INSERT INTO attempts (id, lesson_id, block_id, kind, attempt_number, answer, outcome, \
         answered_at, created_at) VALUES (?, ?, 'q1', 'multiple_choice', 1, '{}', 'correct', ?, ?)",
        &[ATTEMPT, LESSON_IN_PROGRESS, AT, AT],
    )
    .await;
    h.sql(
        "INSERT INTO highlights (lesson_id, position, block_id, range_start, range_end, color) \
         VALUES (?, 0, 'p9', 2, 7, 'green')",
        &[LESSON],
    )
    .await;
    h.sql(
        "INSERT INTO strokes (lesson_id, position, id, block_id, color, width, points) \
         VALUES (?, 0, '5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f10', 'p9', 'red', 'thin', x'0100000002000000')",
        &[LESSON],
    )
    .await;
    h.sql(
        "INSERT INTO component_requests (id, topic_id, lesson_id, block_id, wanted, purpose, fallback, created_at) \
         VALUES ('01933b4e-0000-7000-8000-00000000a009', ?, ?, 'b1', 'slider', 'p', 'f', ?)",
        &[&topic, LESSON, AT],
    )
    .await;
    h.sql(
        "INSERT INTO glossary_terms (id, topic_id, key, term, term_key, definition, avoid, lesson_id, created_at, updated_at) \
         VALUES (?, ?, 'k', 'Termine A', 'termine a', 'd', '[]', ?, ?, ?)",
        &[TERM, &topic, LESSON, AT, AT],
    )
    .await;
    h.sql(
        "INSERT INTO review_cards (id, topic_id, lesson_id, term_id, front, back, created_at, due) \
         VALUES (?, ?, ?, ?, 'f', 'b', ?, ?)",
        &[CARD, &topic, LESSON, TERM, AT, "2026-11-01T00:00:00.000Z"],
    )
    .await;
    h.sql(
        "INSERT INTO documents (id, topic_id, filename, title, status, size_bytes, created_at) \
         VALUES (?, ?, 'a.pdf', 'Documento A', 'ready', 10, ?)",
        &[DOCUMENT, &topic, AT],
    )
    .await;
    h.sql(
        "INSERT INTO preferences (id, user_id, scope, topic_id, text, text_key, created_at) \
         VALUES (?, '00000000-0000-7000-8000-000000000001', 'global', NULL, 'Preferenza globale di A', 'a1', ?)",
        &[PREF_GLOBAL, AT],
    )
    .await;
    h.sql(
        "INSERT INTO preferences (id, user_id, scope, topic_id, text, text_key, created_at) \
         VALUES (?, '00000000-0000-7000-8000-000000000001', 'topic', ?, 'Preferenza di argomento di A', 'a2', ?)",
        &[PREF_TOPIC, &topic, AT],
    )
    .await;
    Ids { folder, topic }
}

// ---------- the route table ----------

/// What B sends, and what has to come back.
struct Probe {
    method: Method,
    path: String,
    body: Option<Value>,
    multipart: bool,
    expect: Expect,
}

enum Expect {
    /// B holds A's id: `404 not_found`, the same as for an id that is not there.
    Gone,
    /// B's own resource: the status, and a check on the body.
    Own(StatusCode, fn(&Value)),
}

/// `{id}` means a different resource on each route family: this is the id of
/// A's resource the route is about.
fn fill(template: &str, ids: &Ids) -> String {
    let id = if template.contains("/folders/{id}") {
        ids.folder.as_str()
    } else if template.contains("/lessons/{id}") {
        LESSON
    } else if template.contains("/documents/{id}") {
        DOCUMENT
    } else if template.contains("/preferences/{id}") {
        PREF_GLOBAL
    } else {
        ids.topic.as_str()
    };
    template
        .replace("{id}", id)
        .replace("{message_id}", MESSAGE)
        .replace("{job_id}", JOB)
        .replace("{card_id}", CARD)
        .replace("{clip_id}", CLIP)
}

fn body_of(method: &Method, template: &str) -> Option<Value> {
    let key = format!("{method} {template}");
    let body = match key.as_str() {
        "PATCH /api/v1/folders/{id}" => json!({"name": "intruso"}),
        "POST /api/v1/folders/{id}/topics" | "PATCH /api/v1/topics/{id}" => {
            json!({"title": "intruso"})
        }
        "POST /api/v1/topics/{id}/messages" => json!({"text": "intruso"}),
        "POST /api/v1/topics/{id}/mission/confirm" => json!({"version": 1}),
        "POST /api/v1/lessons/{id}/attempts" => json!({"attempts": []}),
        "PUT /api/v1/lessons/{id}/highlights" => json!({"highlights": [
            {"block_id": "p1", "start": 0, "end": 1, "color": "yellow"}
        ]}),
        "PUT /api/v1/lessons/{id}/strokes" => json!({"strokes": [
            {"id": "5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f99", "block_id": "p1", "color": "ink",
             "width": "thin", "points": [1, 2]}
        ]}),
        "POST /api/v1/reviews/{card_id}" => json!({"rating": "good", "review_count": 0}),
        "POST /api/v1/topics/{id}/messages/{message_id}/retry"
        | "POST /api/v1/topics/{id}/jobs/{job_id}/retry"
        | "POST /api/v1/topics/{id}/resources/research"
        | "POST /api/v1/topics/{id}/lessons"
        | "POST /api/v1/lessons/{id}/complete"
        | "POST /api/v1/reviews/{card_id}/undo"
        | "POST /api/v1/reviews/{card_id}/suspend"
        | "POST /api/v1/reviews/{card_id}/unsuspend" => json!({}),
        _ => return None,
    };
    Some(body)
}

/// Every route of the API, with what to do with it as user B. A route
/// missing from here, or listed here and gone from the router, fails the test.
#[allow(clippy::too_many_lines)] // the table of routes
fn cases(ids: &Ids) -> Vec<(String, Probe)> {
    use Expect::{Gone, Own};
    let id_routes = [
        "GET /api/v1/folders/{id}",
        "PATCH /api/v1/folders/{id}",
        "DELETE /api/v1/folders/{id}",
        "GET /api/v1/folders/{id}/topics",
        "POST /api/v1/folders/{id}/topics",
        "GET /api/v1/topics/{id}",
        "PATCH /api/v1/topics/{id}",
        "DELETE /api/v1/topics/{id}",
        "GET /api/v1/topics/{id}/messages",
        "POST /api/v1/topics/{id}/messages",
        "POST /api/v1/topics/{id}/messages/{message_id}/retry",
        "GET /api/v1/topics/{id}/events",
        "GET /api/v1/topics/{id}/mission",
        "GET /api/v1/topics/{id}/mission/versions",
        "POST /api/v1/topics/{id}/mission/confirm",
        "GET /api/v1/topics/{id}/jobs",
        "POST /api/v1/topics/{id}/jobs/{job_id}/retry",
        "GET /api/v1/topics/{id}/resources",
        "POST /api/v1/topics/{id}/resources/research",
        "GET /api/v1/topics/{id}/documents",
        "POST /api/v1/topics/{id}/documents",
        "DELETE /api/v1/documents/{id}",
        "GET /api/v1/topics/{id}/lessons",
        "POST /api/v1/topics/{id}/lessons",
        "GET /api/v1/lessons/{id}",
        "GET /api/v1/lessons/{id}/audio",
        "GET /api/v1/audio/clips/{clip_id}",
        "POST /api/v1/lessons/{id}/attempts",
        "PUT /api/v1/lessons/{id}/highlights",
        "PUT /api/v1/lessons/{id}/strokes",
        "POST /api/v1/lessons/{id}/complete",
        "GET /api/v1/topics/{id}/glossary",
        "GET /api/v1/topics/{id}/learning-records",
        "DELETE /api/v1/preferences/{id}",
        "POST /api/v1/reviews/{card_id}",
        "POST /api/v1/reviews/{card_id}/undo",
        "POST /api/v1/reviews/{card_id}/suspend",
        "POST /api/v1/reviews/{card_id}/unsuspend",
    ];
    let mut out: Vec<(String, Probe)> = id_routes
        .iter()
        .map(|key| {
            let (method, template) = key.split_once(' ').unwrap();
            let method: Method = method.parse().unwrap();
            let probe = Probe {
                path: fill(template, ids),
                body: body_of(&method, template),
                multipart: template.ends_with("/documents") && method == Method::POST,
                method,
                expect: Gone,
            };
            ((*key).to_owned(), probe)
        })
        .collect();
    // Routes with no id in them: B sees and changes only its own.
    let own =
        |key: &str, path: &str, body: Option<Value>, status: StatusCode, check: fn(&Value)| {
            let (method, _) = key.split_once(' ').unwrap();
            (
                key.to_owned(),
                Probe {
                    method: method.parse().unwrap(),
                    path: path.to_owned(),
                    body,
                    multipart: false,
                    expect: Own(status, check),
                },
            )
        };
    out.extend([
        own(
            "GET /api/v1/folders",
            "/api/v1/folders",
            None,
            StatusCode::OK,
            |b| {
                assert_eq!(b["items"], json!([]), "B's folders: nothing of A's");
            },
        ),
        own(
            "POST /api/v1/folders",
            "/api/v1/folders",
            Some(json!({"name": "Lavoro"})),
            StatusCode::CREATED,
            |b| assert_eq!(b["name"], "Lavoro", "the same name as A's is B's to use"),
        ),
        own(
            "GET /api/v1/lessons",
            "/api/v1/lessons?status=in_progress",
            None,
            StatusCode::OK,
            |b| {
                assert_eq!(
                    b["items"],
                    json!([]),
                    "B sees none of A's lessons in progress"
                );
            },
        ),
        own(
            "GET /api/v1/preferences",
            "/api/v1/preferences",
            None,
            StatusCode::OK,
            |b| {
                assert_eq!(b["items"], json!([]));
            },
        ),
        own(
            "GET /api/v1/reviews/suspended",
            "/api/v1/reviews/suspended",
            None,
            StatusCode::OK,
            |b| {
                assert_eq!(
                    (b["items"].clone(), b["suspended_count"].clone()),
                    (json!([]), json!(0)),
                    "B sees none of A's suspended cards"
                );
            },
        ),
        own(
            "GET /api/v1/reviews/due",
            "/api/v1/reviews/due",
            None,
            StatusCode::OK,
            |b| {
                assert_eq!(
                    (b["items"].clone(), b["due_count"].clone()),
                    (json!([]), json!(0))
                );
            },
        ),
        own(
            "GET /api/v1/component-requests",
            "/api/v1/component-requests",
            None,
            StatusCode::OK,
            |b| assert_eq!(b["items"], json!([])),
        ),
        own("GET /api/v1/me", "/api/v1/me", None, StatusCode::OK, |b| {
            assert_eq!(b["username"], "bea");
        }),
        own(
            "PUT /api/v1/me/password",
            "/api/v1/me/password",
            Some(
                json!({"current_password": "a long password", "new_password": "another long one"}),
            ),
            StatusCode::NO_CONTENT,
            |_| {},
        ),
        own(
            "PUT /api/v1/me/llm-key",
            "/api/v1/me/llm-key",
            Some(json!({"api_key": "sk-bea-new"})),
            StatusCode::OK,
            |b| assert_eq!(b["username"], "bea"),
        ),
        own(
            "DELETE /api/v1/me/llm-key",
            "/api/v1/me/llm-key",
            None,
            StatusCode::NO_CONTENT,
            |_| {},
        ),
        // The harness has no speech engine: the feature is off.
        own(
            "GET /api/v1/tts/voices",
            "/api/v1/tts/voices",
            None,
            StatusCode::OK,
            |b| assert_eq!(b["available"], false),
        ),
        own(
            "PUT /api/v1/me/tts-voice",
            "/api/v1/me/tts-voice",
            Some(json!({"voice": "vivian"})),
            StatusCode::CONFLICT,
            |b| assert_eq!(b["error"]["code"], "conflict"),
        ),
        own(
            "POST /api/v1/auth/logout",
            "/api/v1/auth/logout",
            None,
            StatusCode::NO_CONTENT,
            |_| {},
        ),
    ]);
    out
}

/// Routes that are not about anybody's data.
const EXEMPT: [&str; 4] = [
    "GET /health",
    "GET /ready",
    "GET /api/v1/health",
    "POST /api/v1/auth/login",
];

/// The routes of the router, as "METHOD /path", read from the router's own
/// description: the path of each route and the methods its handler answers.
fn routes_of_the_router(h: &Harness) -> BTreeSet<String> {
    let described = format!("{:?}", h.router);
    // The part after `fallback_router` describes the fallbacks, not routes.
    let main = described.split("fallback_router").next().unwrap();
    let (routes_part, node_part) = main.split_once("node: Node").unwrap();

    // `RouteId(n): "path"` in the node, `RouteId(n): MethodRouter(...)` in the routes.
    let mut paths = std::collections::HashMap::new();
    for chunk in node_part.split("RouteId(").skip(1) {
        let (id, rest) = chunk.split_once("): \"").unwrap();
        paths.insert(id.to_owned(), rest[..rest.find('"').unwrap()].to_owned());
    }
    let mut routes = BTreeSet::new();
    for chunk in routes_part.split("RouteId(").skip(1) {
        let id = &chunk[..chunk.find(')').unwrap()];
        let path = &paths[id];
        for method in ["get", "post", "put", "patch", "delete"] {
            if chunk.contains(&format!(" {method}: Route")) {
                routes.insert(format!("{} {path}", method.to_uppercase()));
            }
        }
    }
    routes
}

#[tokio::test]
async fn every_route_of_the_router_has_an_isolation_case() {
    let h = Harness::new(vec![]).await;
    let found = routes_of_the_router(&h);
    assert!(
        found.len() > 40,
        "the router should describe its routes: {found:?}"
    );

    let ids = Ids {
        folder: String::new(),
        topic: String::new(),
    };
    let covered: BTreeSet<String> = cases(&ids)
        .into_iter()
        .map(|(key, _)| key)
        .chain(EXEMPT.iter().map(|s| (*s).to_owned()))
        .collect();
    let missing: Vec<_> = found.difference(&covered).collect();
    let stale: Vec<_> = covered.difference(&found).collect();
    assert!(
        missing.is_empty(),
        "routes without an isolation case (add one to `cases`): {missing:?}"
    );
    assert!(
        stale.is_empty(),
        "cases for routes that are not in the router: {stale:?}"
    );
}

fn strip_request_id(mut body: Value) -> Value {
    body["error"]["request_id"] = Value::Null;
    body
}

async fn send_probe(h: &Harness, token: &str, probe: &Probe, path: &str) -> Res {
    if probe.multipart {
        let boundary = "isolationboundary";
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.pdf\"\r\n\
             Content-Type: application/pdf\r\n\r\n%PDF-1.4 tiny\r\n--{boundary}--\r\n"
        );
        let req = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap();
        return h.send(req).await;
    }
    h.request(Some(token), probe.method.clone(), path, probe.body.clone())
        .await
}

/// The second user's world: a user with a key and nothing else.
async fn user_b(h: &Harness) -> String {
    h.create_user("bea", "a long password").await;
    let token = h.login("bea", "a long password").await.body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let put = h
        .request(
            Some(&token),
            Method::PUT,
            "/api/v1/me/llm-key",
            Some(json!({"api_key": "sk-of-bea"})),
        )
        .await;
    assert_eq!(put.status, StatusCode::OK);
    token
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // one scenario over every route
async fn user_b_with_the_ids_of_user_a_gets_404_on_every_route_and_changes_nothing() {
    let h = Harness::new(vec![]).await;
    let ids = seed_user_a(&h).await;
    let b = user_b(&h).await;

    let mut checked = 0;
    for (key, probe) in cases(&ids) {
        match &probe.expect {
            Expect::Gone => {
                let res = send_probe(&h, &b, &probe, &probe.path).await;
                assert_eq!(res.status, StatusCode::NOT_FOUND, "{key}: {}", res.body);
                assert_eq!(res.body["error"]["code"], "not_found", "{key}");

                // The answer is the one for an id that does not exist at all.
                let nowhere = probe
                    .path
                    .replace(&ids.folder, &Uuid::now_v7().to_string())
                    .replace(&ids.topic, &Uuid::now_v7().to_string())
                    .replace(LESSON, &Uuid::now_v7().to_string())
                    .replace(DOCUMENT, &Uuid::now_v7().to_string())
                    .replace(PREF_GLOBAL, &Uuid::now_v7().to_string())
                    .replace(CARD, &Uuid::now_v7().to_string());
                let ghost = send_probe(&h, &b, &probe, &nowhere).await;
                assert_eq!(
                    (ghost.status, strip_request_id(ghost.body)),
                    (res.status, strip_request_id(res.body)),
                    "{key}: an id of A's looks like an id that is not there"
                );
                checked += 1;
            }
            Expect::Own(..) => {}
        }
    }
    assert!(checked >= 30, "{checked}");

    // B's own routes, last: the password change and the logout end its session.
    for (key, probe) in cases(&ids) {
        if let Expect::Own(status, check) = &probe.expect {
            let res = send_probe(&h, &b, &probe, &probe.path).await;
            assert_eq!(res.status, *status, "{key}: {}", res.body);
            check(&res.body);
        }
    }

    // A's world is exactly as it was.
    let folders = h.get("/api/v1/folders").await;
    assert_eq!(folders.body["items"].as_array().unwrap().len(), 1);
    assert_eq!(folders.body["items"][0]["name"], "Lavoro");
    assert_eq!(folders.body["items"][0]["topic_count"], 1);
    let topic = h.get(&format!("/api/v1/topics/{}", ids.topic)).await;
    assert_eq!(
        (topic.body["title"].clone(), topic.body["state"].clone()),
        (json!("Rust"), json!("ready"))
    );
    let mine = h.get("/api/v1/lessons?status=in_progress").await;
    assert_eq!(mine.status, StatusCode::OK);
    assert_eq!(
        mine.body["items"].as_array().unwrap().len(),
        1,
        "{}",
        mine.body
    );
    assert_eq!(mine.body["items"][0]["id"], LESSON_IN_PROGRESS);
    assert_eq!(mine.body["items"][0]["topic_title"], "Rust");
    assert_eq!(mine.body["items"][0]["document_sources"], json!([]));
    assert_eq!(mine.body["items"][0]["last_activity_at"], AT);
    for bad in ["/api/v1/lessons", "/api/v1/lessons?status=completed"] {
        let r = h.get(bad).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(r.body["error"]["code"], "bad_request");
    }
    let lesson = h.get(&format!("/api/v1/lessons/{LESSON}")).await;
    assert_eq!(lesson.status, StatusCode::OK);
    assert_eq!(lesson.body["lesson"]["status"], "not_started");
    assert_eq!(
        lesson.body["highlights"],
        json!([{"block_id": "p9", "start": 2, "end": 7, "color": "green"}]),
        "B's highlights did not replace A's"
    );
    assert_eq!(
        lesson.body["strokes"],
        json!([{"id": "5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f10", "block_id": "p9", "color": "red",
                "width": "thin", "points": [1, 2]}]),
        "B's strokes did not replace A's"
    );
    assert_eq!(
        h.get("/api/v1/preferences").await.body["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let due = h.get("/api/v1/reviews/due").await;
    assert_eq!(due.body["due_count"], 1);
    assert_eq!(due.body["items"][0]["state"], "new");
    assert_eq!(
        h.get(&format!("/api/v1/topics/{}/documents", ids.topic))
            .await
            .body["documents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let jobs = h.get(&format!("/api/v1/topics/{}/jobs", ids.topic)).await;
    assert_eq!(
        jobs.body["items"][0]["status"], "failed",
        "B's retry did not touch A's job"
    );
    let messages = h
        .get(&format!("/api/v1/topics/{}/messages", ids.topic))
        .await;
    assert_eq!(
        messages.body["items"].as_array().unwrap().len(),
        1,
        "B wrote nothing in A's chat"
    );
    assert_eq!(
        h.get("/api/v1/me").await.body["has_llm_key"],
        true,
        "B's key removal is B's"
    );
    assert_eq!(h.get("/api/v1/me").await.body["username"], "owner");
    assert!(h.llm.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_lists_of_user_b_hold_nothing_of_user_a() {
    let h = Harness::new(vec![]).await;
    let ids = seed_user_a(&h).await;
    let b = user_b(&h).await;
    let folder = h.folder(&b, "Lavoro").await;
    let topic = h.topic(&b, &folder, "Rust").await;

    let get = |path: String| {
        let (h, b) = (&h, b.clone());
        async move { h.request(Some(&b), Method::GET, &path, None).await }
    };
    // Same names as A's, and not one thing of A's in any list.
    let list = get("/api/v1/folders".into()).await;
    assert_eq!(list.body["items"].as_array().unwrap().len(), 1);
    assert_eq!(list.body["items"][0]["id"], folder.as_str());
    assert_eq!(
        list.body["items"][0]["position"], 0,
        "positions are per user"
    );
    assert_eq!(list.body["items"][0]["topic_count"], 1);
    let topics = get(format!("/api/v1/folders/{folder}/topics")).await;
    assert_eq!(topics.body["items"].as_array().unwrap().len(), 1);
    assert_eq!(topics.body["items"][0]["id"], topic.as_str());
    for sub in [
        "messages",
        "jobs",
        "lessons",
        "glossary",
        "learning-records",
        "documents",
    ] {
        let res = get(format!("/api/v1/topics/{topic}/{sub}")).await;
        assert_eq!(res.status, StatusCode::OK, "{sub}");
        let key = if sub == "documents" {
            "documents"
        } else {
            "items"
        };
        assert_eq!(res.body[key], json!([]), "{sub}");
    }
    let mission = get(format!("/api/v1/topics/{topic}/mission")).await;
    assert_eq!(mission.body["current"], Value::Null);
    assert!(!mission.body.to_string().contains("A-segreto"));
    let resources = get(format!("/api/v1/topics/{topic}/resources")).await;
    assert_eq!(resources.body["resources"], json!([]));
    // The preferences, also when asked about A's topic.
    for path in [
        "/api/v1/preferences".to_owned(),
        format!("/api/v1/preferences?topic_id={}", ids.topic),
    ] {
        assert_eq!(get(path).await.body["items"], json!([]));
    }
    // Moving a topic to A's folder, or A's topic into B's, is "not found".
    let moved = h
        .request(
            Some(&b),
            Method::PATCH,
            &format!("/api/v1/topics/{topic}"),
            Some(json!({"folder_id": ids.folder})),
        )
        .await;
    assert_eq!(moved.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        moved.body["error"]["details"],
        json!([{"field": "folder_id", "issue": "not_found"}])
    );
    // A still sees one folder with one topic.
    let mine = h.get("/api/v1/folders").await;
    assert_eq!(
        (
            mine.body["items"].as_array().unwrap().len(),
            mine.body["items"][0]["topic_count"].clone()
        ),
        (1, json!(1))
    );
}

#[tokio::test]
async fn the_global_preference_of_b_never_reaches_the_prompt_of_a() {
    let secret = "Rispondi sempre in rima baciata";
    let h = Harness::new(vec![
        // B's first turn: the Tutor saves a global preference, then answers.
        vec![Step::Tool {
            id: "t1",
            name: "save_preference",
            input: json!({"scope": "global", "text": secret}),
        }],
        vec![Step::Delta("Fatto.")],
        // A's turn.
        vec![Step::Delta("Ciao A.")],
        // B's second turn.
        vec![Step::Delta("Ciao B.")],
    ])
    .await;
    let b = user_b(&h).await;
    let a_topic = {
        let f = h.folder(TOKEN, "F").await;
        h.topic(TOKEN, &f, "T").await
    };
    let b_topic = {
        let f = h.folder(&b, "F").await;
        h.topic(&b, &f, "T").await
    };
    for (token, topic, text) in [
        (b.as_str(), &b_topic, "uno"),
        (TOKEN, &a_topic, "due"),
        (b.as_str(), &b_topic, "tre"),
    ] {
        let sent = h
            .request(
                Some(token),
                Method::POST,
                &format!("/api/v1/topics/{topic}/messages"),
                Some(json!({"text": text})),
            )
            .await;
        assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.body);
        for _ in 0..200 {
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
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
    let requests = h.llm.requests.lock().unwrap().clone();
    let system = |i: usize| {
        requests[i]
            .system
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(requests.len(), 4, "B's tool turn takes two calls");
    // Call 2 is B's turn after the tool ran; call 3 is A's; call 4 is B's again.
    assert!(
        !system(2).contains(secret),
        "A's prompt holds B's preference"
    );
    assert!(system(3).contains(secret), "B's own prompt has it");
    // And A's world has none: the API agrees.
    assert_eq!(h.get("/api/v1/preferences").await.body["items"], json!([]));
    let bs = h
        .request(Some(&b), Method::GET, "/api/v1/preferences", None)
        .await;
    assert_eq!(bs.body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_stream_of_a_topic_of_another_user_is_not_opened() {
    let h = Harness::new(vec![]).await;
    let ids = seed_user_a(&h).await;
    let b = user_b(&h).await;
    let res = h
        .request(
            Some(&b),
            Method::GET,
            &format!("/api/v1/topics/{}/events", ids.topic),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.body["error"]["code"], "not_found");
}

#[tokio::test]
async fn deleting_a_global_preference_is_announced_only_to_the_streams_of_its_owner() {
    let h = Harness::new(vec![]).await;
    let ids = seed_user_a(&h).await;
    let b = user_b(&h).await;
    let b_folder = h.folder(&b, "F").await;
    let b_topic = h.topic(&b, &b_folder, "T").await;
    let b_user = domain::UserId::new(
        sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE username = 'bea'")
            .fetch_one(&h.pool)
            .await
            .map(|s| s.parse().unwrap())
            .unwrap(),
    );
    let mut theirs = h
        .state
        .events
        .subscribe(b_user, domain::TopicId::new(b_topic.parse().unwrap()), None)
        .live;
    let mut mine = h
        .state
        .events
        .subscribe(
            domain::UserId::OWNER,
            domain::TopicId::new(ids.topic.parse().unwrap()),
            None,
        )
        .live;
    let res = h
        .request(
            Some(TOKEN),
            Method::DELETE,
            &format!("/api/v1/preferences/{PREF_GLOBAL}"),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    assert!(mine.try_recv().is_ok());
    assert!(
        theirs.try_recv().is_err(),
        "B's stream heard about A's preference"
    );
}

/// The 404 of the routes above can come from the topic alone. Here B has a
/// topic of its own, so only the message or job lookup can refuse.
#[tokio::test]
async fn a_message_or_job_of_a_is_not_found_even_inside_a_topic_of_b() {
    let h = Harness::new(vec![]).await;
    seed_user_a(&h).await;
    let b = user_b(&h).await;
    let folder = h.folder(&b, "F").await;
    let topic = h.topic(&b, &folder, "T").await;
    for path in [
        format!("/api/v1/topics/{topic}/messages/{MESSAGE}/retry"),
        format!("/api/v1/topics/{topic}/jobs/{JOB}/retry"),
    ] {
        let res = h
            .request(Some(&b), Method::POST, &path, Some(json!({})))
            .await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{path}: {}", res.body);
        assert_eq!(res.body["error"]["code"], "not_found");
    }
    // A's failed message and job were not touched.
    let status: String = sqlx::query_scalar("SELECT status FROM chat_messages WHERE id = ?")
        .bind(MESSAGE)
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(status, "failed");
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = ?")
        .bind(JOB)
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(status, "failed");
}

#[tokio::test]
async fn the_lesson_tool_of_a_creates_the_job_only_on_the_topic_of_a() {
    let h = Harness::new(vec![
        vec![Step::Tool {
            id: "t1",
            name: "request_lesson",
            input: json!({}),
        }],
        vec![Step::Delta("Preparo.")],
    ])
    .await;
    let b = user_b(&h).await;
    let a_topic = {
        let f = h.folder(TOKEN, "F").await;
        h.topic(TOKEN, &f, "T").await
    };
    let b_topic = {
        let f = h.folder(&b, "F").await;
        h.topic(&b, &f, "T").await
    };
    for topic in [&a_topic, &b_topic] {
        h.sql("UPDATE topics SET state = 'ready' WHERE id = ?", &[topic])
            .await;
    }

    let sent = h
        .request(
            Some(TOKEN),
            Method::POST,
            &format!("/api/v1/topics/{a_topic}/messages"),
            Some(json!({"text": "La prossima lezione"})),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.body);
    let mut a_messages = Value::Null;
    for _ in 0..200 {
        a_messages = h
            .request(
                Some(TOKEN),
                Method::GET,
                &format!("/api/v1/topics/{a_topic}/messages"),
                None,
            )
            .await
            .body;
        if a_messages["items"][0]["status"] == "complete" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(a_messages["items"][0]["status"], "complete", "{a_messages}");

    // A has the job and the part pointing to it.
    let a_jobs = h.get(&format!("/api/v1/topics/{a_topic}/jobs")).await.body;
    let generation: Vec<_> = a_jobs["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|j| j["kind"] == "lesson_generation")
        .collect();
    assert_eq!(generation.len(), 1, "{a_jobs}");
    let part = a_messages["items"][0]["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["type"] == "lesson_request")
        .expect("lesson_request part");
    assert_eq!(part["job_id"], generation[0]["id"]);

    // B sees neither on its own topic, and cannot open A's.
    let b_jobs = h
        .request(
            Some(&b),
            Method::GET,
            &format!("/api/v1/topics/{b_topic}/jobs"),
            None,
        )
        .await;
    assert_eq!(b_jobs.body["items"], json!([]), "{}", b_jobs.body);
    for path in [
        format!("/api/v1/topics/{a_topic}/jobs"),
        format!("/api/v1/topics/{a_topic}/messages"),
    ] {
        let res = h.request(Some(&b), Method::GET, &path, None).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{path}: {}", res.body);
    }
    let b_messages = h
        .request(
            Some(&b),
            Method::GET,
            &format!("/api/v1/topics/{b_topic}/messages"),
            None,
        )
        .await;
    assert_eq!(b_messages.body["items"], json!([]));
}
