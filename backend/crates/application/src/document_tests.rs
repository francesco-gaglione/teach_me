#![allow(
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::assert_is_empty
)]
//! Uploaded documents over in-memory fakes: the service, the import job and
//! how the documents reach the agents. No PDF is read: the extractor is a fake.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use domain::{
    Document, DocumentFiles, DocumentId, DocumentRepository, DocumentStatus, ExtractedPdf,
    IdGenerator, Job, JobErrorCode, JobKind, JobResult, JobStatus, OutlineEntry, PdfError,
    PdfTextExtractor, ResourceKind, TopicId,
};
use uuid::Uuid;

use crate::chat_tests::{Fixture, SLOW, fixture_with};
use crate::context::{HistoryLimits, TutorContext, assemble_tutor_request};
use crate::error::{AppError, ConflictReason};
use crate::fakes::SeqIds;
use crate::{DocumentService, ImportDocument, JobRunner, JobSettings, NewUpload, TutorPrompts};

/// Files that "exist" are the ones in the set.
#[derive(Default)]
struct FakeFiles(Mutex<HashSet<DocumentId>>);

#[async_trait]
impl DocumentFiles for FakeFiles {
    fn path(&self, id: DocumentId) -> PathBuf {
        PathBuf::from(format!("/uploads/{id}.pdf"))
    }
    fn partial_path(&self, id: DocumentId) -> PathBuf {
        PathBuf::from(format!("/uploads/{id}.part"))
    }
    async fn remove(&self, id: DocumentId) {
        self.0.lock().unwrap().remove(&id);
    }
}

type Reader = Box<dyn Fn() -> Result<ExtractedPdf, PdfError> + Send + Sync>;

struct FakeExtractor(Reader);

impl PdfTextExtractor for FakeExtractor {
    fn extract(
        &self,
        _path: &Path,
        _max_pages: u32,
        on_page: &mut dyn FnMut(u32, u32),
    ) -> Result<ExtractedPdf, PdfError> {
        let pdf = (self.0)()?;
        let total = u32::try_from(pdf.pages.len()).unwrap();
        on_page(total, total);
        Ok(pdf)
    }
}

fn text_pages(n: usize) -> Vec<String> {
    (1..=n)
        .map(|p| format!("Pagina {p}. {}", "testo di prova ".repeat(40)))
        .collect()
}

fn outline(title: &str, level: u8, page: u32) -> OutlineEntry {
    OutlineEntry {
        title: title.into(),
        level,
        page,
    }
}

struct Env {
    f: Fixture,
    files: Arc<FakeFiles>,
    ids: Arc<SeqIds>,
}

impl Env {
    async fn new() -> Self {
        let f = fixture_with(vec![], SLOW, 4, |llm| llm).await;
        let ids = Arc::new(SeqIds::default());
        for _ in 0..1000 {
            ids.new_id();
        }
        Self {
            f,
            files: Arc::new(FakeFiles::default()),
            ids,
        }
    }

    fn service(&self) -> DocumentService {
        DocumentService::new(
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.files.clone(),
            self.f.jobs.clone(),
            self.ids.clone(),
            self.f.clock.clone(),
        )
    }

    fn runner(&self, read: Reader) -> JobRunner {
        let handler = Arc::new(ImportDocument::new(
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.files.clone(),
            Arc::new(FakeExtractor(read)),
            self.f.clock.clone(),
            self.ids.clone(),
        ));
        JobRunner::new(
            self.f.jobs.clone(),
            JobSettings {
                max_concurrent: 1,
                job_timeout: Duration::from_secs(30),
                max_attempts: 3,
                store_retry_delay: Duration::from_millis(1),
            },
        )
        .with_handler(JobKind::DocumentImport, handler)
    }

    /// Receives a file (it "exists" until the import deletes it).
    async fn upload(&self, name: &str) -> Result<(Document, Job), AppError> {
        let svc = self.service();
        let id = svc.new_document_id();
        self.files.0.lock().unwrap().insert(id);
        svc.create(
            domain::UserId::OWNER,
            self.f.topic,
            NewUpload {
                id,
                filename: name.to_owned(),
                size_bytes: 1234,
            },
        )
        .await
    }

    /// Uploads and runs the import to its end.
    async fn import(&self, name: &str, read: Reader) -> (Document, Job) {
        let (doc, _) = self.upload(name).await.unwrap();
        assert!(self.runner(read).run_next().await.unwrap());
        let doc = self
            .f
            .repo
            .get(domain::UserId::OWNER, doc.id)
            .await
            .unwrap()
            .unwrap();
        let job = {
            let s = self.f.repo.0.lock().unwrap();
            s.jobs
                .iter()
                .find(|j| j.document_id == Some(doc.id))
                .unwrap()
                .clone()
        };
        (doc, job)
    }
}

fn pdf(pages: Vec<String>, outline: Vec<OutlineEntry>) -> Reader {
    Box::new(move || {
        Ok(ExtractedPdf {
            title: Some("Rust in Action".into()),
            pages: pages.clone(),
            outline: outline.clone(),
        })
    })
}

#[tokio::test]
async fn a_pdf_with_bookmarks_becomes_a_ready_document_with_sections_and_a_resource() {
    let env = Env::new().await;
    let (doc, job) = env
        .import(
            "../rust-in-action.pdf",
            pdf(
                text_pages(30),
                vec![
                    outline("1 Intro", 1, 1),
                    outline("1.1 Why", 2, 3),
                    outline("2 Basics", 1, 12),
                ],
            ),
        )
        .await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(
        job.result,
        Some(JobResult::DocumentImport {
            document_id: doc.id,
            page_count: 30,
            section_count: 3
        })
    );
    assert_eq!(doc.status, DocumentStatus::Ready);
    assert_eq!(doc.filename, "rust-in-action.pdf", "the path is dropped");
    assert_eq!(doc.title, "Rust in Action");
    assert_eq!(doc.page_count, Some(30));
    assert!(doc.error.is_none());
    assert!(
        !env.files.0.lock().unwrap().contains(&doc.id),
        "file deleted"
    );

    let views = env
        .service()
        .list(domain::UserId::OWNER, env.f.topic)
        .await
        .unwrap();
    let sections = &views[0].sections;
    let spans: Vec<_> = sections
        .iter()
        .map(|s| (s.id.as_str(), s.level, s.page_start, s.page_end))
        .collect();
    assert_eq!(
        spans,
        [("s1", 1, 1, 11), ("s2", 2, 3, 11), ("s3", 1, 12, 30)]
    );

    let resource = {
        let s = env.f.repo.0.lock().unwrap();
        s.resources[0].1.resources[0].clone()
    };
    assert_eq!(resource.kind, ResourceKind::Document);
    assert_eq!(resource.document_id, Some(doc.id));
    assert_eq!(doc.resource_id, Some(resource.id));
    assert_eq!(resource.url, None);
    assert_eq!(resource.annotation, "PDF caricato da te, 30 pagine.");
    let pages = env.f.repo.pages(doc.id, 2, 3).await.unwrap();
    assert_eq!(pages.len(), 2);
    assert!(pages[0].1.starts_with("Pagina 2."));
}

#[tokio::test]
async fn without_bookmarks_the_sections_are_blocks_of_ten_pages() {
    let env = Env::new().await;
    let mut reader = pdf(text_pages(23), vec![]);
    // A junk title falls back to the file name.
    reader = {
        let inner = reader;
        Box::new(move || {
            let mut p = inner()?;
            p.title = Some("Microsoft Word - cap1.doc".into());
            Ok(p)
        })
    };
    let (doc, job) = env.import("guida-rust.pdf", reader).await;
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(doc.title, "guida-rust");
    let sections = env
        .service()
        .list(domain::UserId::OWNER, env.f.topic)
        .await
        .unwrap()[0]
        .sections
        .clone();
    let titles: Vec<_> = sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Pagine 1-10", "Pagine 11-20", "Pagine 21-23"]);
}

#[tokio::test]
async fn refused_pdfs_fail_with_their_code_keep_the_file_and_explain_why() {
    let cases: Vec<(Reader, JobErrorCode, &str)> = vec![
        (
            Box::new(|| Err(PdfError::Encrypted)),
            JobErrorCode::EncryptedPdf,
            "Il PDF è protetto da password.",
        ),
        (
            Box::new(|| Err(PdfError::Invalid)),
            JobErrorCode::InvalidPdf,
            "Il file non è un PDF valido.",
        ),
        (
            Box::new(|| Err(PdfError::TooManyPages(2000))),
            JobErrorCode::TooManyPages,
            "Il PDF ha troppe pagine (massimo 1.500).",
        ),
        (
            // A scan: pages with almost no text.
            pdf(vec!["12".into(); 40], vec![]),
            JobErrorCode::NoText,
            "Il PDF non contiene testo selezionabile (forse è una scansione).",
        ),
        (
            Box::new(|| Err(PdfError::Internal("boom".into()))),
            JobErrorCode::Internal,
            "Errore interno.",
        ),
    ];
    for (read, code, message) in cases {
        let env = Env::new().await;
        let (doc, job) = env.import("x.pdf", read).await;
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.error.as_ref().unwrap().code, code);
        assert_eq!(doc.status, DocumentStatus::Failed, "{code:?}");
        let error = doc.error.expect("the document carries the error");
        assert_eq!((error.code, error.message.as_str()), (code, message));
        assert!(doc.resource_id.is_none() && doc.page_count.is_none());
        // The file stays: a retry may succeed (and the user deletes it otherwise).
        assert!(env.files.0.lock().unwrap().contains(&doc.id), "{code:?}");
        let s = env.f.repo.0.lock().unwrap();
        assert!(s.resources.is_empty());
    }
}

#[tokio::test]
async fn a_retry_makes_the_document_importing_again_and_can_succeed() {
    let env = Env::new().await;
    let (doc, job) = env
        .import("x.pdf", Box::new(|| Err(PdfError::Internal("boom".into()))))
        .await;
    assert_eq!(doc.status, DocumentStatus::Failed);

    env.f
        .jobs
        .retry(domain::UserId::OWNER, env.f.topic, job.id)
        .await
        .unwrap();
    let during = env
        .f
        .repo
        .get(domain::UserId::OWNER, doc.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(during.status, DocumentStatus::Importing);
    assert!(during.error.is_none(), "the error is cleared by the retry");

    assert!(
        env.runner(pdf(text_pages(12), vec![]))
            .run_next()
            .await
            .unwrap()
    );
    let done = env
        .f
        .repo
        .get(domain::UserId::OWNER, doc.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.status, DocumentStatus::Ready);
    assert!(done.error.is_none());
}

#[tokio::test]
async fn retrying_a_document_is_not_blocked_by_a_newer_upload() {
    let env = Env::new().await;
    let (a, job_a) = env
        .import("a.pdf", Box::new(|| Err(PdfError::Internal("boom".into()))))
        .await;
    // B is uploaded after A failed; its import fails too.
    let (b, job_b) = env
        .import("b.pdf", Box::new(|| Err(PdfError::Internal("boom".into()))))
        .await;
    assert_eq!(b.status, DocumentStatus::Failed);

    // The older failure can be retried (and so can the newer one).
    env.f
        .jobs
        .retry(domain::UserId::OWNER, env.f.topic, job_a.id)
        .await
        .unwrap();
    assert_eq!(
        env.f
            .repo
            .get(domain::UserId::OWNER, a.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        DocumentStatus::Importing
    );
    // Only one import is active in the topic: B waits for A.
    assert!(matches!(
        env.f
            .jobs
            .retry(domain::UserId::OWNER, env.f.topic, job_b.id)
            .await,
        Err(AppError::Conflict(ConflictReason::JobNotRetryable))
    ));
    assert!(
        env.runner(pdf(text_pages(12), vec![]))
            .run_next()
            .await
            .unwrap()
    );
    assert_eq!(
        env.f
            .repo
            .get(domain::UserId::OWNER, a.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        DocumentStatus::Ready
    );
    env.f
        .jobs
        .retry(domain::UserId::OWNER, env.f.topic, job_b.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn progress_reports_the_pages_read() {
    let env = Env::new().await;
    env.import("x.pdf", pdf(text_pages(12), vec![])).await;
    let mut steps: Vec<(String, Option<u32>, Option<u32>)> = Vec::new();
    for e in env.f.events.snapshot() {
        if let domain::TopicEventPayload::JobUpdated(j) = e.payload
            && let Some(p) = j.progress
            && steps.last().map(|s| &s.0) != Some(&p.step)
        {
            steps.push((p.step, p.done, p.total));
        }
    }
    let names: Vec<_> = steps.iter().map(|s| s.0.as_str()).collect();
    assert_eq!(names, ["reading", "extracting", "sectioning", "saving"]);
    assert_eq!((steps[1].1, steps[1].2), (Some(12), Some(12)));
}

#[tokio::test]
async fn at_most_five_documents_and_one_active_import() {
    let env = Env::new().await;
    // Only one import at a time.
    env.upload("a.pdf").await.unwrap();
    assert!(matches!(
        env.upload("b.pdf").await,
        Err(AppError::Conflict(ConflictReason::JobActive))
    ));
    assert!(matches!(
        env.service()
            .check_upload(domain::UserId::OWNER, env.f.topic)
            .await,
        Err(AppError::Conflict(ConflictReason::JobActive))
    ));
    let runner = env.runner(pdf(text_pages(12), vec![]));
    runner.run_next().await.unwrap();

    for name in ["b", "c", "d", "e"] {
        env.upload(&format!("{name}.pdf")).await.unwrap();
        runner.run_next().await.unwrap();
    }
    // The sixth is refused, before and after the file is received.
    assert!(matches!(
        env.service()
            .check_upload(domain::UserId::OWNER, env.f.topic)
            .await,
        Err(AppError::Conflict(ConflictReason::DocumentLimit))
    ));
    assert!(matches!(
        env.upload("f.pdf").await,
        Err(AppError::Conflict(ConflictReason::DocumentLimit))
    ));
    assert_eq!(
        env.service()
            .list(domain::UserId::OWNER, env.f.topic)
            .await
            .unwrap()
            .len(),
        5
    );
}

#[tokio::test]
async fn unknown_topics_and_unusable_names_are_refused() {
    let env = Env::new().await;
    let svc = env.service();
    let ghost = TopicId::new(Uuid::from_u128(0xdead));
    assert!(matches!(
        svc.check_upload(domain::UserId::OWNER, ghost).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        svc.list(domain::UserId::OWNER, ghost).await,
        Err(AppError::NotFound)
    ));
    let id = svc.new_document_id();
    let upload = NewUpload {
        id,
        filename: "..".into(),
        size_bytes: 1,
    };
    assert!(matches!(
        svc.create(domain::UserId::OWNER, env.f.topic, upload).await,
        Err(AppError::Validation(_))
    ));
}

#[tokio::test]
async fn deleting_cascades_but_not_while_the_import_runs() {
    let env = Env::new().await;
    let svc = env.service();
    let (doc, _) = env.upload("a.pdf").await.unwrap();
    assert!(matches!(
        svc.delete(domain::UserId::OWNER, doc.id).await,
        Err(AppError::Conflict(ConflictReason::ImportInProgress))
    ));
    env.runner(pdf(text_pages(12), vec![]))
        .run_next()
        .await
        .unwrap();
    assert_eq!(
        svc.list(domain::UserId::OWNER, env.f.topic)
            .await
            .unwrap()
            .len(),
        1
    );

    svc.delete(domain::UserId::OWNER, doc.id).await.unwrap();
    assert!(
        svc.list(domain::UserId::OWNER, env.f.topic)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        env.f
            .repo
            .get(domain::UserId::OWNER, doc.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(env.f.repo.pages(doc.id, 1, 5).await.is_err());
    {
        let s = env.f.repo.0.lock().unwrap();
        assert!(s.resources[0].1.resources.is_empty(), "resource deleted");
        assert!(s.jobs.is_empty());
    }
    assert!(matches!(
        svc.delete(domain::UserId::OWNER, doc.id).await,
        Err(AppError::NotFound)
    ));

    // A failed document can be deleted too, and its file goes with it.
    let (failed, _) = env
        .import("b.pdf", Box::new(|| Err(PdfError::Encrypted)))
        .await;
    assert!(env.files.0.lock().unwrap().contains(&failed.id));
    svc.delete(domain::UserId::OWNER, failed.id).await.unwrap();
    assert!(!env.files.0.lock().unwrap().contains(&failed.id));
}

#[tokio::test]
async fn the_tutor_sees_the_index_of_ready_documents() {
    let env = Env::new().await;
    env.import(
        "libro.pdf",
        pdf(
            text_pages(12),
            vec![outline("1 Intro", 1, 1), outline("2 Basi", 1, 6)],
        ),
    )
    .await;
    let views = env
        .service()
        .list(domain::UserId::OWNER, env.f.topic)
        .await
        .unwrap();
    let topic = env
        .f
        .topics
        .get(domain::UserId::OWNER, env.f.topic)
        .await
        .unwrap();
    let request = assemble_tutor_request(
        &TutorPrompts::embedded(),
        &topic,
        &TutorContext {
            documents: &views,
            ..TutorContext::default()
        },
        &[domain::ChatMessage::user(
            domain::MessageId::new(Uuid::from_u128(0xabc)),
            topic.id,
            domain::MessageText::parse("Ciao").unwrap(),
            topic.created_at,
        )],
        HistoryLimits { token_budget: 8000 },
    )
    .unwrap();
    let context = &request.system[1].text;
    assert!(context.contains("## Documenti caricati"), "{context}");
    assert!(context.contains("\"Rust in Action\" · 12 pagine"));
    assert!(context.contains("- 1 Intro (pp. 1-5)"));
    // Without documents the heading stays, with an explicit "none".
    let plain = assemble_tutor_request(
        &TutorPrompts::embedded(),
        &topic,
        &TutorContext::default(),
        &[domain::ChatMessage::user(
            domain::MessageId::new(Uuid::from_u128(0xabd)),
            topic.id,
            domain::MessageText::parse("Ciao").unwrap(),
            topic.created_at,
        )],
        HistoryLimits { token_budget: 8000 },
    )
    .unwrap();
    let plain = &plain.system[1].text;
    assert!(plain.contains("## Documenti caricati"), "{plain}");
    assert!(plain.contains("Nessuno."), "{plain}");
}

/// A ready document stored directly (no import), with its resource.
pub(crate) fn seed_ready_document(
    f: &Fixture,
    n: u128,
    title: &str,
    pages: Vec<String>,
    sections: Vec<domain::DocumentSection>,
) -> (DocumentId, domain::ResourceId) {
    use crate::fakes::DocumentRow;
    let id = DocumentId::new(Uuid::from_u128(0xd000 + n));
    let resource_id = domain::ResourceId::new(Uuid::from_u128(0xe000 + n));
    let at = time::macros::datetime!(2026-11-08 10:00:00 UTC)
        + time::Duration::minutes(i64::try_from(n).unwrap());
    let mut document = Document::importing(id, f.topic, format!("{title}.pdf"), 100, at);
    document.title = title.to_owned();
    document.status = DocumentStatus::Ready;
    document.page_count = Some(u32::try_from(pages.len()).unwrap());
    document.language = Some("en".into());
    document.resource_id = Some(resource_id);
    let resource = domain::Resource {
        id: resource_id,
        topic_id: f.topic,
        kind: ResourceKind::Document,
        source_type: domain::SourceType::Pdf,
        title: title.to_owned(),
        url: None,
        language: "en".into(),
        annotation: format!("PDF caricato da te, {} pagine.", pages.len()),
        use_for: "seguire il tuo PDF, citandone le pagine".into(),
        document_id: Some(id),
        created_at: at,
    };
    let mut s = f.repo.0.lock().unwrap();
    s.documents.push(DocumentRow {
        document,
        sections,
        pages,
    });
    match s.resources.iter_mut().find(|(t, _)| *t == f.topic) {
        Some((_, set)) => set.resources.insert(0, resource),
        None => s.resources.push((
            f.topic,
            domain::TopicResources {
                resources: vec![resource],
                ..domain::TopicResources::default()
            },
        )),
    }
    (id, resource_id)
}

pub(crate) fn section(id: &str, title: &str, level: u8, a: u32, b: u32) -> domain::DocumentSection {
    domain::DocumentSection {
        id: id.into(),
        title: title.into(),
        level,
        page_start: a,
        page_end: b,
    }
}
