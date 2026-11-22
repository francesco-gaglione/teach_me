//! End-to-end tests of the document upload and import (`documents.md`) over
//! the real router and SQLite, in process, with a fake PDF engine.

mod common;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use api::app::{Limits, build_router};
use api::state::{AppOptions, AppState, DocumentSetup};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::ScriptedLlm;
use domain::{Clock, ExtractedPdf, IdGenerator, JobKind, OutlineEntry, PdfError, PdfTextExtractor};
use http_body_util::BodyExt;
use infrastructure::LocalDocumentFiles;
use infrastructure::sqlite::{self, Pool};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = common::TOKEN;
const BOUNDARY: &str = "X-TEST-BOUNDARY";

struct TestClock(Mutex<OffsetDateTime>);
impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        *self.0.lock().unwrap()
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

type Reader = Box<dyn Fn() -> Result<ExtractedPdf, PdfError> + Send + Sync>;

struct FakeEngine(Reader);

impl PdfTextExtractor for FakeEngine {
    fn extract(
        &self,
        _path: &Path,
        _max_pages: u32,
        on_page: &mut dyn FnMut(u32, u32),
    ) -> Result<ExtractedPdf, PdfError> {
        let pdf = (self.0)()?;
        let n = u32::try_from(pdf.pages.len()).unwrap();
        on_page(n, n);
        Ok(pdf)
    }
}

fn good_pdf() -> Reader {
    Box::new(|| {
        Ok(ExtractedPdf {
            title: Some("Rust in Action".into()),
            pages: (1..=14)
                .map(|p| format!("Pagina {p}. {}", "testo di prova ".repeat(40)))
                .collect(),
            outline: vec![
                OutlineEntry {
                    title: "1 Intro".into(),
                    level: 1,
                    page: 1,
                },
                OutlineEntry {
                    title: "2 Basi".into(),
                    level: 1,
                    page: 8,
                },
            ],
        })
    })
}

struct App {
    router: Router,
    state: AppState,
    pool: Pool,
    dir: std::path::PathBuf,
}

impl App {
    async fn new(read: Reader) -> Self {
        Self::with_limits(read, Limits::default()).await
    }

    async fn with_limits(read: Reader, limits: Limits) -> Self {
        Self::build(read, limits, AppOptions::default()).await
    }

    async fn build(read: Reader, limits: Limits, options: AppOptions) -> Self {
        let dir = std::env::temp_dir().join(format!("teach_me_doc_test_{}", Uuid::now_v7()));
        let files = LocalDocumentFiles::new(dir.clone());
        files.prepare().await.unwrap();
        let pool = sqlite::connect("sqlite::memory:").await.unwrap();
        sqlite::migrate(&pool).await.unwrap();
        let state = AppState::with_documents(
            Arc::new(TestClock(Mutex::new(
                datetime!(2026-11-09 14:36:01.129 UTC),
            ))),
            Arc::new(SeqIds(AtomicU64::new(0))),
            common::auth(),
            &pool,
            ScriptedLlm::with(vec![]),
            options,
            DocumentSetup {
                extractor: Arc::new(FakeEngine(read)),
                files: Arc::new(files),
            },
        );
        common::sign_in_owner(&state, &pool).await;
        Self {
            router: build_router(state.clone(), limits),
            state,
            pool,
            dir,
        }
    }

    async fn send(&self, req: Request<Body>) -> (StatusCode, Value) {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        (
            parts.status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn builder(method: Method, path: &str) -> axum::http::request::Builder {
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut b = Self::builder(method, path);
        let body = match body {
            Some(v) => {
                b = b.header(header::CONTENT_TYPE, "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        self.send(b.body(body).unwrap()).await
    }

    async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.call(Method::GET, path, None).await
    }

    async fn topic(&self) -> String {
        let (_, f) = self
            .call(
                Method::POST,
                "/api/v1/folders",
                Some(json!({ "name": "F" })),
            )
            .await;
        let folder = f["id"].as_str().unwrap().to_owned();
        let (status, t) = self
            .call(
                Method::POST,
                &format!("/api/v1/folders/{folder}/topics"),
                Some(json!({ "title": "Rust" })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
        t["id"].as_str().unwrap().to_owned()
    }

    fn multipart(field: &str, filename: &str, content: &[u8]) -> Vec<u8> {
        let mut body = format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\n\
             Content-Type: application/pdf\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
        body
    }

    fn upload_request(topic: &str, body: Body) -> Request<Body> {
        Self::builder(Method::POST, &format!("/api/v1/topics/{topic}/documents"))
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .body(body)
            .unwrap()
    }

    async fn upload(&self, topic: &str, filename: &str, content: &[u8]) -> (StatusCode, Value) {
        let body = Self::multipart("file", filename, content);
        self.send(Self::upload_request(topic, Body::from(body)))
            .await
    }

    async fn run_import(&self) {
        let runner = self
            .state
            .job_runner(vec![(JobKind::DocumentImport, self.state.import_handler())]);
        assert!(runner.run_next().await.unwrap(), "an import was queued");
    }

    fn files_left(&self) -> usize {
        std::fs::read_dir(&self.dir).unwrap().count()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn pdf_bytes(extra: usize) -> Vec<u8> {
    let mut v = b"%PDF-1.7\n".to_vec();
    v.extend(std::iter::repeat_n(b'x', extra));
    v
}

#[tokio::test]
async fn a_valid_upload_is_imported_into_a_ready_document_with_sections_and_a_resource() {
    let app = App::new(good_pdf()).await;
    let topic = app.topic().await;

    let (status, body) = app
        .upload(&topic, "../../rust-in-action.pdf", &pdf_bytes(500))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let doc = &body["document"];
    let id = doc["id"].as_str().unwrap().to_owned();
    assert_eq!(doc["status"], "importing");
    assert_eq!(
        doc["filename"], "rust-in-action.pdf",
        "directories are dropped"
    );
    assert_eq!(doc["size_bytes"], 509);
    assert_eq!(doc["sections"], json!([]));
    assert_eq!(doc["error"], Value::Null);
    assert_eq!(doc["resource_id"], Value::Null);
    assert_eq!(body["job"]["kind"], "document_import");
    assert_eq!(body["job"]["document_id"], id.as_str(), "set from creation");
    assert_eq!(body["job"]["lesson_id"], Value::Null);
    assert_eq!(body["job"]["status"], "queued");
    assert_eq!(app.files_left(), 1, "the file waits for the import");

    app.run_import().await;

    let (status, list) = app.get(&format!("/api/v1/topics/{topic}/documents")).await;
    assert_eq!(status, StatusCode::OK);
    let doc = &list["documents"][0];
    assert_eq!(doc["id"], id.as_str());
    assert_eq!(doc["status"], "ready");
    assert_eq!(doc["title"], "Rust in Action");
    assert_eq!(doc["page_count"], 14);
    assert_eq!(
        doc["sections"],
        json!([
            {"id": "s1", "title": "1 Intro", "level": 1, "page_start": 1, "page_end": 7},
            {"id": "s2", "title": "2 Basi", "level": 1, "page_start": 8, "page_end": 14}
        ])
    );
    assert_eq!(app.files_left(), 0, "the file is deleted once imported");

    // The resource is a `document`, first in the list, with no address.
    let (_, res) = app.get(&format!("/api/v1/topics/{topic}/resources")).await;
    let r = &res["resources"][0];
    assert_eq!(r["kind"], "document");
    assert_eq!(r["source_type"], "pdf");
    assert_eq!(r["document_id"], id.as_str());
    assert_eq!(r["id"], doc["resource_id"]);
    assert_eq!(r["url"], Value::Null);
    assert_eq!(r["domain"], Value::Null);
    assert_eq!(r["annotation"], "PDF caricato da te, 14 pagine.");

    let (_, jobs) = app.get(&format!("/api/v1/topics/{topic}/jobs")).await;
    assert_eq!(jobs["items"][0]["status"], "succeeded");
    assert_eq!(jobs["items"][0]["document_id"], id.as_str());
    assert_eq!(jobs["items"][0]["result"]["page_count"], 14);
    assert_eq!(jobs["items"][0]["result"]["section_count"], 2);
    assert_eq!(jobs["items"][0]["result"]["document_id"], id.as_str());
}

#[tokio::test]
async fn an_upload_over_the_storage_quota_is_refused_with_409_and_leaves_nothing_behind() {
    let options = AppOptions {
        storage_quota_bytes: 1000,
        ..AppOptions::default()
    };
    let app = App::build(good_pdf(), Limits::default(), options).await;
    let topic = app.topic().await;

    let (status, body) = app.upload(&topic, "a.pdf", &pdf_bytes(500)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    app.run_import().await;

    // 509 bytes are kept; 509 more would pass 1000.
    let (status, body) = app.upload(&topic, "b.pdf", &pdf_bytes(500)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "storage_quota_exceeded");
    assert_eq!(
        body["error"]["details"],
        json!([{ "field": "quota", "issue": "exceeded" }])
    );
    assert_eq!(app.files_left(), 0, "the refused file is removed");
    let (_, list) = app.get(&format!("/api/v1/topics/{topic}/documents")).await;
    assert_eq!(list["documents"].as_array().unwrap().len(), 1);

    // What still fits is accepted.
    let (status, body) = app.upload(&topic, "c.pdf", &pdf_bytes(100)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
}

#[tokio::test]
async fn files_over_the_limit_are_refused_with_413_and_leave_nothing_behind() {
    let app = App::new(good_pdf()).await;
    let topic = app.topic().await;
    // 51 MB, with an exact Content-Length: refused by the route's limit.
    let (status, body) = app
        .upload(&topic, "big.pdf", &pdf_bytes(51 * 1024 * 1024))
        .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"]["code"], "payload_too_large");
    assert_eq!(app.files_left(), 0);
    let (_, list) = app.get(&format!("/api/v1/topics/{topic}/documents")).await;
    assert_eq!(list["documents"], json!([]));
}

#[tokio::test]
async fn a_streamed_upload_without_a_length_is_stopped_as_it_arrives() {
    let limits = Limits {
        upload_bytes: 100 * 1024,
        ..Limits::default()
    };
    let app = App::with_limits(good_pdf(), limits).await;
    let topic = app.topic().await;
    let body = App::multipart("file", "big.pdf", &pdf_bytes(300 * 1024));
    let chunks: Vec<Result<Vec<u8>, std::io::Error>> =
        body.chunks(16 * 1024).map(|c| Ok(c.to_vec())).collect();
    let stream = futures_util::stream::iter(chunks);
    let (status, body) = app
        .send(App::upload_request(&topic, Body::from_stream(stream)))
        .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert_eq!(body["error"]["code"], "payload_too_large");
    assert_eq!(app.files_left(), 0, "the partial file is removed");

    // A file within the limit still goes through on the same route.
    let (status, _) = app.upload(&topic, "ok.pdf", &pdf_bytes(1000)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn only_pdfs_are_accepted_by_signature_not_by_name() {
    let app = App::new(good_pdf()).await;
    let topic = app.topic().await;
    for (name, content) in [
        ("book.pdf", b"PK\x03\x04 this is a zip".to_vec()),
        ("empty.pdf", Vec::new()),
        (
            "late.pdf",
            [vec![b' '; 2000], b"%PDF-1.4".to_vec()].concat(),
        ),
    ] {
        let (status, body) = app.upload(&topic, name, &content).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{name}: {body}");
        assert_eq!(body["error"]["code"], "unsupported_media_type");
    }
    assert_eq!(app.files_left(), 0);
    // The content type is not trusted either way: a PDF sent as octet-stream is fine.
    let body = App::multipart("file", "x.bin", &pdf_bytes(10));
    let (status, _) = app
        .send(App::upload_request(&topic, Body::from(body)))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn bad_requests_get_the_usual_errors() {
    let app = App::new(good_pdf()).await;
    let topic = app.topic().await;
    // No `file` field.
    let body = App::multipart("other", "x.pdf", &pdf_bytes(10));
    let (status, b) = app
        .send(App::upload_request(&topic, Body::from(body)))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(b["error"]["code"], "bad_request");
    // Not multipart at all.
    let req = App::builder(Method::POST, &format!("/api/v1/topics/{topic}/documents"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(app.send(req).await.0, StatusCode::BAD_REQUEST);
    // Unknown topic, malformed id, no token.
    let ghost = Uuid::from_u128(9);
    let (status, _) = app
        .upload(&ghost.to_string(), "x.pdf", &pdf_bytes(10))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, b) = app.upload("nope", "x.pdf", &pdf_bytes(10)).await;
    assert_eq!(
        (status, b["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_id"))
    );
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/topics/{topic}/documents"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(App::multipart("file", "x.pdf", &pdf_bytes(10))))
        .unwrap();
    assert_eq!(app.send(req).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(app.files_left(), 0);
}

#[tokio::test]
async fn one_import_at_a_time_and_five_documents_per_topic() {
    let app = App::new(good_pdf()).await;
    let topic = app.topic().await;
    assert_eq!(
        app.upload(&topic, "a.pdf", &pdf_bytes(10)).await.0,
        StatusCode::ACCEPTED
    );
    let (status, b) = app.upload(&topic, "b.pdf", &pdf_bytes(10)).await;
    assert_eq!(
        (status, b["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict"))
    );
    assert_eq!(app.files_left(), 1, "the refused file was not kept");
    app.run_import().await;
    for name in ["b", "c", "d", "e"] {
        assert_eq!(
            app.upload(&topic, &format!("{name}.pdf"), &pdf_bytes(10))
                .await
                .0,
            StatusCode::ACCEPTED
        );
        app.run_import().await;
    }
    let (status, b) = app.upload(&topic, "f.pdf", &pdf_bytes(10)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        b["error"]["message"]
            .as_str()
            .unwrap()
            .contains("5 documenti")
    );
    let (_, list) = app.get(&format!("/api/v1/topics/{topic}/documents")).await;
    assert_eq!(list["documents"].as_array().unwrap().len(), 5);
    assert_eq!(app.files_left(), 0);
}

#[tokio::test]
async fn a_protected_or_scanned_pdf_fails_with_a_clear_error_on_the_document() {
    let cases: Vec<(Reader, &str)> = vec![
        (Box::new(|| Err(PdfError::Encrypted)), "encrypted_pdf"),
        (
            Box::new(|| {
                Ok(ExtractedPdf {
                    title: None,
                    pages: vec![String::new(); 30],
                    outline: vec![],
                })
            }),
            "no_text",
        ),
    ];
    for (read, code) in cases {
        let app = App::new(read).await;
        let topic = app.topic().await;
        let (_, up) = app.upload(&topic, "x.pdf", &pdf_bytes(10)).await;
        let job_id = up["job"]["id"].as_str().unwrap().to_owned();
        app.run_import().await;

        let (_, list) = app.get(&format!("/api/v1/topics/{topic}/documents")).await;
        let doc = &list["documents"][0];
        assert_eq!(doc["status"], "failed");
        assert_eq!(doc["error"]["code"], code);
        assert!(doc["error"]["message"].as_str().unwrap().len() > 10);
        assert_eq!(doc["resource_id"], Value::Null);
        let (_, jobs) = app.get(&format!("/api/v1/topics/{topic}/jobs")).await;
        assert_eq!(jobs["items"][0]["id"], job_id.as_str());
        assert_eq!(jobs["items"][0]["status"], "failed");
        assert_eq!(jobs["items"][0]["error"]["code"], code);
        let (_, res) = app.get(&format!("/api/v1/topics/{topic}/resources")).await;
        assert_eq!(res["resources"], json!([]));
        // Retrying fails the same way, and the user can delete the document.
        let id = doc["id"].as_str().unwrap();
        let (status, _) = app
            .call(Method::DELETE, &format!("/api/v1/documents/{id}"), None)
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(app.files_left(), 0, "the file goes with the document");
    }
}

#[tokio::test]
async fn deleting_a_document_removes_its_pages_sections_and_resource() {
    let app = App::new(good_pdf()).await;
    let topic = app.topic().await;
    let (_, up) = app.upload(&topic, "a.pdf", &pdf_bytes(10)).await;
    let id = up["document"]["id"].as_str().unwrap().to_owned();
    let path = format!("/api/v1/documents/{id}");

    // While the import is queued or running it cannot be deleted.
    let (status, b) = app.call(Method::DELETE, &path, None).await;
    assert_eq!(
        (status, b["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict"))
    );
    app.run_import().await;

    let count = |table: &'static str| {
        let pool = app.pool.clone();
        async move {
            let (n,): (i64,) =
                sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            n
        }
    };
    assert_eq!(count("document_pages").await, 14);
    assert_eq!(count("document_sections").await, 2);
    assert_eq!(count("resources").await, 1);

    let (status, _) = app.call(Method::DELETE, &path, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    for table in [
        "documents",
        "document_pages",
        "document_sections",
        "resources",
        "jobs",
    ] {
        assert_eq!(count(table).await, 0, "{table}");
    }
    assert_eq!(
        app.call(Method::DELETE, &path, None).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, b) = app
        .call(Method::DELETE, "/api/v1/documents/nope", None)
        .await;
    assert_eq!(
        (status, b["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_id"))
    );

    // After the delete a new document can be uploaded.
    assert_eq!(
        app.upload(&topic, "b.pdf", &pdf_bytes(10)).await.0,
        StatusCode::ACCEPTED
    );
}

#[tokio::test]
async fn deleting_the_topic_deletes_its_documents() {
    let app = App::new(good_pdf()).await;
    let topic = app.topic().await;
    app.upload(&topic, "a.pdf", &pdf_bytes(10)).await;
    app.run_import().await;
    let (status, _) = app
        .call(Method::DELETE, &format!("/api/v1/topics/{topic}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (n,): (i64,) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM documents) + (SELECT COUNT(*) FROM document_pages)",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn ordinary_routes_keep_the_small_body_limit() {
    let app = App::new(good_pdf()).await;
    let big = "x".repeat(2 * 1024 * 1024);
    let (status, body) = app
        .call(
            Method::POST,
            "/api/v1/folders",
            Some(json!({ "name": big })),
        )
        .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"]["code"], "payload_too_large");
}

#[tokio::test]
async fn the_limit_is_50_mib_exactly() {
    const LIMIT: usize = 52_428_800;
    assert_eq!(domain::document::MAX_UPLOAD_BYTES, LIMIT as u64);
    let app = App::new(good_pdf()).await;
    let topic = app.topic().await;

    let (status, body) = app
        .upload(&topic, "big.pdf", &pdf_bytes(LIMIT + 1 - 9))
        .await;
    assert_eq!(
        status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "one byte over: {body}"
    );
    assert_eq!(app.files_left(), 0);

    let (status, body) = app.upload(&topic, "edge.pdf", &pdf_bytes(LIMIT - 9)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "exactly 50 MiB: {body}");
    assert_eq!(body["document"]["size_bytes"], LIMIT as u64);
}

/// Serves the app on a real socket, for tests of what a client sees.
async fn serve(app: &App) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = app.router.clone();
    tokio::spawn(async move { axum::serve(listener, router).await });
    addr
}

/// A raw HTTP/1.1 upload of `size` file bytes, sent in chunks. Returns how many
/// bytes the client managed to send and the status line it read back.
async fn raw_upload(
    addr: std::net::SocketAddr,
    topic: &str,
    size: usize,
    first: &[u8],
) -> (usize, Option<String>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let head = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.pdf\"\r\n\r\n"
    );
    let tail = format!("\r\n--{BOUNDARY}--\r\n");
    let total = head.len() + size + tail.len();
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "POST /api/v1/topics/{topic}/documents HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {TOKEN}\r\n\
         Content-Type: multipart/form-data; boundary={BOUNDARY}\r\nContent-Length: {total}\r\n\r\n{head}"
    );
    let mut sent = 0;
    let mut ok = s.write_all(req.as_bytes()).await.is_ok() && s.write_all(first).await.is_ok();
    sent += first.len();
    let chunk = vec![b'x'; 64 * 1024];
    while ok && sent < size {
        let n = chunk.len().min(size - sent);
        ok = s.write_all(&chunk[..n]).await.is_ok();
        if ok {
            sent += n;
        }
    }
    if ok {
        let _ = s.write_all(tail.as_bytes()).await;
    }
    let mut buf = vec![0u8; 512];
    let n = tokio::time::timeout(std::time::Duration::from_secs(5), s.read(&mut buf))
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(0);
    let line = String::from_utf8_lossy(&buf[..n])
        .lines()
        .next()
        .map(str::to_owned);
    (sent, line)
}

#[tokio::test]
async fn a_refused_upload_is_answered_after_the_client_has_finished_sending() {
    let app = App::new(good_pdf()).await;
    let addr = serve(&app).await;
    let topic = app.topic().await;
    let size = 20 * 1024 * 1024;

    // 415: not a PDF, the client is still sending 20 MB.
    let (sent, line) = raw_upload(addr, &topic, size, b"PK\x03\x04").await;
    assert_eq!(sent, size, "the client could send everything");
    assert!(line.unwrap().contains("415"));

    // 404 and 409 are decided before the body is read.
    let ghost = Uuid::from_u128(9).to_string();
    let (sent, line) = raw_upload(addr, &ghost, size, b"%PDF-1.4").await;
    assert_eq!(sent, size);
    assert!(line.unwrap().contains("404"));
    app.upload(&topic, "a.pdf", &pdf_bytes(10)).await;
    let (sent, line) = raw_upload(addr, &topic, size, b"%PDF-1.4").await;
    assert_eq!(sent, size);
    assert!(line.unwrap().contains("409"));
}

#[tokio::test]
async fn a_declared_size_over_the_quota_is_refused_before_the_body_is_read() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let options = AppOptions {
        storage_quota_bytes: 1000,
        ..AppOptions::default()
    };
    let app = App::build(good_pdf(), Limits::default(), options).await;
    let addr = serve(&app).await;
    let topic = app.topic().await;

    // The body is not a multipart form at all: were it read, the answer would
    // be 400. Only the Content-Length check (2 MB against a 1000-byte quota)
    // can make it a 409. The server drains the body before answering.
    let size = 2_000_000;
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "POST /api/v1/topics/{topic}/documents HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {TOKEN}\r\n\
         Content-Type: multipart/form-data; boundary={BOUNDARY}\r\nContent-Length: {size}\r\n\r\n"
    );
    s.write_all(req.as_bytes()).await.unwrap();
    s.write_all(&vec![b'x'; size]).await.unwrap();
    let mut raw = Vec::new();
    let mut buf = vec![0u8; 4096];
    let (head, body) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let n = s.read(&mut buf).await.unwrap();
            assert!(n > 0, "closed before a full answer");
            raw.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&raw).into_owned();
            if let Some((head, body)) = text.split_once("\r\n\r\n")
                && body.trim_end().ends_with('}')
            {
                return (head.to_owned(), body.to_owned());
            }
        }
    })
    .await
    .expect("an answer");
    assert!(head.starts_with("HTTP/1.1 409"), "{head}");
    let body: Value = serde_json::from_str(body.trim()).unwrap();
    assert_eq!(body["error"]["code"], "storage_quota_exceeded");
    assert_eq!(
        body["error"]["details"],
        json!([{ "field": "quota", "issue": "exceeded" }])
    );
    assert_eq!(app.files_left(), 0);
}

#[tokio::test]
async fn a_client_that_disconnects_mid_upload_leaves_no_file_behind() {
    use tokio::io::AsyncWriteExt;
    let app = App::new(good_pdf()).await;
    let addr = serve(&app).await;
    let topic = app.topic().await;
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let head = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.pdf\"\r\n\r\n%PDF-1.4"
    );
    let req = format!(
        "POST /api/v1/topics/{topic}/documents HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {TOKEN}\r\n\
         Content-Type: multipart/form-data; boundary={BOUNDARY}\r\nContent-Length: 10000000\r\n\r\n{head}"
    );
    s.write_all(req.as_bytes()).await.unwrap();
    s.write_all(&vec![b'x'; 200_000]).await.unwrap();
    // Wait until the server has started writing the file, then hang up.
    for _ in 0..200 {
        if app.files_left() > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        app.files_left(),
        1,
        "the partial file exists while receiving"
    );
    drop(s);
    for _ in 0..200 {
        if app.files_left() == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(app.files_left(), 0, "the partial file is removed");
    let (_, list) = app.get(&format!("/api/v1/topics/{topic}/documents")).await;
    assert_eq!(list["documents"], json!([]));
}
