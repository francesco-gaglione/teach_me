//! Uploaded documents: the use cases behind `/topics/{id}/documents`
//! and the `document_import` job handler. Contract: `docs/contracts/documents.md`.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use domain::document::{
    MAX_DOCUMENTS_PER_TOPIC, MAX_PAGES, build_sections, check_has_text, clean_filename,
    guess_language, plausible_title, resource_draft, title_from_filename,
};
use domain::{
    Clock, Document, DocumentFiles, DocumentId, DocumentRepository, DocumentView, IdGenerator,
    IssueKind, Job, JobError, JobErrorCode, JobKind, JobProgress, JobResult, PdfError,
    PdfTextExtractor, RepositoryError, ResourceId, TopicId, TopicRepository, UserId,
    ValidationIssue,
};
use tokio::sync::mpsc;

use crate::error::{AppError, ConflictReason};
use crate::jobs::{JobContext, JobFailure, JobHandler, JobService};
use crate::time_util::now_millis;

/// A file fully received, waiting for its import.
#[derive(Debug, Clone)]
pub struct NewUpload {
    pub id: DocumentId,
    /// As sent by the client; cleaned here.
    pub filename: String,
    pub size_bytes: u64,
}

/// The user's side of the documents: upload, list, delete.
pub struct DocumentService {
    topics: Arc<dyn TopicRepository>,
    documents: Arc<dyn DocumentRepository>,
    files: Arc<dyn DocumentFiles>,
    jobs: Arc<JobService>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    /// Bytes of documents a user may keep; 0 = unlimited.
    storage_quota_bytes: u64,
}

impl DocumentService {
    #[must_use]
    pub fn new(
        topics: Arc<dyn TopicRepository>,
        documents: Arc<dyn DocumentRepository>,
        files: Arc<dyn DocumentFiles>,
        jobs: Arc<JobService>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            topics,
            documents,
            files,
            jobs,
            ids,
            clock,
            storage_quota_bytes: 0,
        }
    }

    /// Caps the bytes of documents each user may keep (0 = unlimited).
    #[must_use]
    pub fn with_storage_quota(mut self, bytes: u64) -> Self {
        self.storage_quota_bytes = bytes;
        self
    }

    /// `StorageQuotaExceeded` when `extra` more bytes would take the user's
    /// documents over the quota.
    async fn check_quota(&self, user: UserId, extra: u64) -> Result<(), AppError> {
        if self.storage_quota_bytes == 0 {
            return Ok(());
        }
        let stored = self.documents.total_bytes_for_user(user).await?;
        if stored.saturating_add(extra) > self.storage_quota_bytes {
            return Err(AppError::StorageQuotaExceeded);
        }
        Ok(())
    }

    /// A fresh id for an upload, to name its file before the document exists.
    #[must_use]
    pub fn new_document_id(&self) -> DocumentId {
        DocumentId::new(self.ids.new_id())
    }

    /// Where the upload of `id` is streamed to, and where it ends up.
    #[must_use]
    pub fn upload_paths(&self, id: DocumentId) -> (PathBuf, PathBuf) {
        (self.files.partial_path(id), self.files.path(id))
    }

    /// Deletes an upload that never became a document.
    pub async fn discard_upload(&self, id: DocumentId) {
        self.files.remove(id).await;
    }

    /// Checked before the first byte is read, so a refusal does not cost a
    /// 50 MB transfer: the topic exists, has room, and no import is active.
    /// `declared_bytes` is the size the client announced (`Content-Length`),
    /// when it did: it only makes the refusal earlier, `create` checks the
    /// real size.
    pub async fn check_upload(
        &self,
        user: UserId,
        topic_id: TopicId,
        declared_bytes: Option<u64>,
    ) -> Result<(), AppError> {
        self.require_topic(user, topic_id).await?;
        if self.documents.list(topic_id).await?.len() >= MAX_DOCUMENTS_PER_TOPIC {
            return Err(AppError::Conflict(ConflictReason::DocumentLimit));
        }
        if self
            .jobs
            .latest(topic_id, JobKind::DocumentImport)
            .await?
            .is_some_and(|j| j.status.is_active())
        {
            return Err(AppError::Conflict(ConflictReason::JobActive));
        }
        self.check_quota(user, declared_bytes.unwrap_or(0)).await?;
        Ok(())
    }

    /// Registers a received file: the document and its queued import job, in
    /// one transaction. On any error the caller deletes the file.
    pub async fn create(
        &self,
        user: UserId,
        topic_id: TopicId,
        upload: NewUpload,
    ) -> Result<(Document, Job), AppError> {
        // Checked again here: the upload took a while, and the topic must
        // still be the user's.
        self.require_topic(user, topic_id).await?;
        self.check_quota(user, upload.size_bytes).await?;
        let filename = clean_filename(&upload.filename).ok_or_else(|| {
            AppError::validation(ValidationIssue::new("filename", IssueKind::Invalid))
        })?;
        let document = Document::importing(
            upload.id,
            topic_id,
            filename,
            upload.size_bytes,
            now_millis(self.clock.as_ref()),
        );
        let job = self.jobs.new_document_job(topic_id, document.id);
        match self.documents.insert_with_job(&document, &job).await {
            Ok(()) => {}
            Err(RepositoryError::MissingParent) => return Err(AppError::NotFound),
            Err(RepositoryError::Conflict) => {
                // Two rules share the conflict: say which one.
                let full = self.documents.list(topic_id).await?.len() >= MAX_DOCUMENTS_PER_TOPIC;
                return Err(AppError::Conflict(if full {
                    ConflictReason::DocumentLimit
                } else {
                    ConflictReason::JobActive
                }));
            }
            Err(other) => return Err(other.into()),
        }
        self.jobs.announce(&job);
        Ok((document, job))
    }

    /// Newest first.
    pub async fn list(
        &self,
        user: UserId,
        topic_id: TopicId,
    ) -> Result<Vec<DocumentView>, AppError> {
        self.require_topic(user, topic_id).await?;
        Ok(self.documents.list(topic_id).await?)
    }

    /// Deletes the document with its pages, sections and resource, and the
    /// file if it is still there. `Conflict` while the import runs.
    pub async fn delete(&self, user: UserId, id: DocumentId) -> Result<(), AppError> {
        match self.documents.delete(user, id).await {
            Ok(()) => {}
            Err(RepositoryError::NotFound) => return Err(AppError::NotFound),
            Err(RepositoryError::Conflict) => {
                return Err(AppError::Conflict(ConflictReason::ImportInProgress));
            }
            Err(other) => return Err(other.into()),
        }
        self.files.remove(id).await;
        Ok(())
    }

    async fn require_topic(&self, user: UserId, id: TopicId) -> Result<(), AppError> {
        self.topics
            .get(user, id)
            .await?
            .map(|_| ())
            .ok_or(AppError::NotFound)
    }
}

/// Characters of text the language is guessed from: enough to be sure, little
/// enough to be instant.
const LANGUAGE_SAMPLE_CHARS: usize = 60_000;

/// Use case `ImportDocument`, registered as the handler of `document_import`
/// jobs. Safe to run again: the file is kept until the import succeeds and
/// the result replaces any earlier partial one.
pub struct ImportDocument {
    topics: Arc<dyn TopicRepository>,
    documents: Arc<dyn DocumentRepository>,
    files: Arc<dyn DocumentFiles>,
    extractor: Arc<dyn PdfTextExtractor>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl ImportDocument {
    #[must_use]
    pub fn new(
        topics: Arc<dyn TopicRepository>,
        documents: Arc<dyn DocumentRepository>,
        files: Arc<dyn DocumentFiles>,
        extractor: Arc<dyn PdfTextExtractor>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            topics,
            documents,
            files,
            extractor,
            clock,
            ids,
        }
    }

    async fn import(&self, ctx: &JobContext, job: &Job) -> Result<JobResult, JobErrorCode> {
        let internal = |what: &str, e: &dyn std::fmt::Display| {
            tracing::error!(job = %job.id, error = %e, "document import: cannot {what}");
            JobErrorCode::Internal
        };
        let id = job
            .document_id
            .ok_or_else(|| internal("find the document", &"the job has none"))?;
        // The import acts for the owner of the document's topic.
        let owner = self
            .topics
            .get_system(job.topic_id)
            .await
            .map_err(|e| internal("read the topic", &e))?
            .ok_or_else(|| internal("read the topic", &"it is gone"))?
            .user_id;
        let document = self
            .documents
            .get(owner, id)
            .await
            .map_err(|e| internal("read the document", &e))?
            .ok_or_else(|| internal("read the document", &"it is gone"))?;
        self.documents
            .mark_importing(id)
            .await
            .map_err(|e| internal("mark the document", &e))?;

        ctx.progress(JobProgress::step("reading")).await;
        let pdf = self.extract(ctx, job, id).await?;
        check_has_text(&pdf.pages)?;

        ctx.progress(JobProgress::step("sectioning")).await;
        let pages: Vec<String> = pdf.pages.iter().map(|p| clean_page(p)).collect();
        let page_count = u32::try_from(pages.len()).unwrap_or(u32::MAX);
        let sections = build_sections(&pdf.outline, page_count);
        let title = plausible_title(pdf.title.as_deref())
            .unwrap_or_else(|| title_from_filename(&document.filename));
        let language = guess_language(&language_sample(&pages));
        let section_count = u32::try_from(sections.len()).unwrap_or(u32::MAX);

        ctx.progress(JobProgress::step("saving")).await;
        let resource = resource_draft(&title, page_count, language.as_deref());
        let import = domain::ImportedDocument {
            document_id: id,
            title,
            language,
            pages,
            sections,
            resource_id: ResourceId::new(self.ids.new_id()),
            resource,
        };
        self.documents
            .complete(&import, now_millis(self.clock.as_ref()))
            .await
            .map_err(|e| internal("store the text", &e))?;
        // The text is in the database: the file has done its job.
        self.files.remove(id).await;
        tracing::info!(job = %job.id, document = %id, page_count, section_count, "document imported");
        Ok(JobResult::DocumentImport {
            document_id: id,
            page_count,
            section_count,
        })
    }

    /// Runs the extractor off the async runtime, relaying its progress.
    async fn extract(
        &self,
        ctx: &JobContext,
        job: &Job,
        id: DocumentId,
    ) -> Result<domain::ExtractedPdf, JobErrorCode> {
        let path = self.files.path(id);
        let extractor = self.extractor.clone();
        let (tx, mut rx) = mpsc::unbounded_channel::<(u32, u32)>();
        let task = tokio::task::spawn_blocking(move || {
            extractor.extract(&path, MAX_PAGES, &mut |done, total| {
                let _ = tx.send((done, total));
            })
        });
        tokio::pin!(task);
        let extracting = |done: u32, total: u32| {
            let mut p = JobProgress::step("extracting").with_done(done);
            p.total = Some(total);
            p
        };
        let result = loop {
            tokio::select! {
                Some((done, total)) = rx.recv() => ctx.progress(extracting(done, total)).await,
                joined = &mut task => break joined,
            }
        };
        // Ticks sent just before the task ended: report the last one.
        if let Some((done, total)) = std::iter::from_fn(|| rx.try_recv().ok()).last() {
            ctx.progress(extracting(done, total)).await;
        }
        match result {
            Ok(Ok(pdf)) => Ok(pdf),
            Ok(Err(e)) => {
                match &e {
                    PdfError::Unavailable(_) | PdfError::Internal(_) => {
                        tracing::error!(job = %job.id, error = %e, "document import: extraction failed");
                    }
                    _ => tracing::warn!(job = %job.id, error = %e, "document import: PDF refused"),
                }
                Err(e.job_code())
            }
            Err(join) => {
                tracing::error!(job = %job.id, error = %join, "document import: extraction died");
                Err(JobErrorCode::Internal)
            }
        }
    }
}

/// Page text as stored: `\n` line ends and no control characters (SQLite
/// text cannot hold a NUL, and the prompt has no use for the others).
fn clean_page(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect::<String>()
        .trim()
        .to_owned()
}

fn language_sample(pages: &[String]) -> String {
    let mut sample = String::new();
    for page in pages {
        if sample.chars().count() >= LANGUAGE_SAMPLE_CHARS {
            break;
        }
        sample.push_str(page);
        sample.push('\n');
    }
    sample
}

#[async_trait]
impl JobHandler for ImportDocument {
    async fn run(&self, ctx: &JobContext) -> Result<JobResult, JobFailure> {
        let job = ctx.job().await;
        self.import(ctx, &job).await.map_err(JobFailure)
    }

    async fn on_failed(&self, job: &Job, error: &JobError) {
        let Some(id) = job.document_id else { return };
        if let Err(e) = self.documents.fail(id, error).await {
            tracing::warn!(job = %job.id, error = %e, "cannot record why the import failed");
        }
    }
}
