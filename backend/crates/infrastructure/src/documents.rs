//! SQLite adapter for uploaded documents (`docs/contracts/documents.md`) and
//! the local directory the uploads wait in.

use std::path::PathBuf;

use async_trait::async_trait;
use domain::document::MAX_DOCUMENTS_PER_TOPIC;
use domain::{
    Document, DocumentFiles, DocumentId, DocumentRepository, DocumentSection, DocumentStatus,
    DocumentView, ImportedDocument, Job, JobError, JobErrorCode, JobStatus, RepositoryError,
    ResourceId, TopicId, UserId,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use crate::jobs::insert_job;
use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};

macro_rules! document_select {
    () => {
        "SELECT d.id, d.topic_id, d.resource_id, d.filename, d.title, d.status, d.size_bytes, \
         d.page_count, d.language, d.error_code, d.error_message, d.created_at, \
         j.status AS job_status, j.error_code AS job_error_code, \
         j.error_message AS job_error_message \
         FROM documents d LEFT JOIN jobs j ON j.document_id = d.id"
    };
}

fn error_of(
    code: Option<String>,
    message: Option<String>,
    id: &str,
) -> Result<Option<JobError>, RepositoryError> {
    match (code, message) {
        (Some(code), Some(message)) => Ok(Some(JobError {
            code: code
                .parse::<JobErrorCode>()
                .map_err(|_| corrupt("document error code", id))?,
            message,
        })),
        (None, None) => Ok(None),
        _ => Err(corrupt("document error", id)),
    }
}

fn document_from_row(row: &SqliteRow) -> Result<Document, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic_id: String = row.try_get("topic_id").map_err(storage)?;
    let resource_id: Option<String> = row.try_get("resource_id").map_err(storage)?;
    let status: String = row.try_get("status").map_err(storage)?;
    let size: i64 = row.try_get("size_bytes").map_err(storage)?;
    let pages: Option<i64> = row.try_get("page_count").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let stored = status
        .parse::<DocumentStatus>()
        .map_err(|_| corrupt("document status", &id))?;
    let stored_error = error_of(
        row.try_get("error_code").map_err(storage)?,
        row.try_get("error_message").map_err(storage)?,
        &id,
    )?;
    let job_status: Option<String> = row.try_get("job_status").map_err(storage)?;
    let job_status = job_status
        .as_deref()
        .map(|s| {
            s.parse::<JobStatus>()
                .map_err(|_| corrupt("document job status", &id))
        })
        .transpose()?;
    // The import job decides while the document is not ready: it is the one
    // that is always up to date (a retry, a restart, an interruption).
    let (status, error) = match (stored, job_status) {
        (DocumentStatus::Ready, _) => (DocumentStatus::Ready, None),
        (_, Some(JobStatus::Queued | JobStatus::Running | JobStatus::Succeeded)) => {
            (DocumentStatus::Importing, None)
        }
        (_, Some(JobStatus::Failed)) => {
            let from_job = error_of(
                row.try_get("job_error_code").map_err(storage)?,
                row.try_get("job_error_message").map_err(storage)?,
                &id,
            )?;
            (DocumentStatus::Failed, from_job.or(stored_error))
        }
        (DocumentStatus::Failed, None) => (DocumentStatus::Failed, stored_error),
        (s, None) => (s, None),
    };
    Ok(Document {
        id: DocumentId::new(parse_id(&id)?),
        topic_id: TopicId::new(parse_id(&topic_id)?),
        resource_id: resource_id
            .as_deref()
            .map(|r| parse_id(r).map(ResourceId::new))
            .transpose()?,
        filename: row.try_get("filename").map_err(storage)?,
        title: row.try_get("title").map_err(storage)?,
        status,
        size_bytes: u64::try_from(size).map_err(|_| corrupt("document size", &id))?,
        page_count: pages
            .map(|p| u32::try_from(p).map_err(|_| corrupt("document pages", &id)))
            .transpose()?,
        language: row.try_get("language").map_err(storage)?,
        error,
        created_at: parse_ts(&created)?,
    })
}

fn section_from_row(row: &SqliteRow, doc: &str) -> Result<DocumentSection, RepositoryError> {
    let level: i64 = row.try_get("level").map_err(storage)?;
    let start: i64 = row.try_get("page_start").map_err(storage)?;
    let end: i64 = row.try_get("page_end").map_err(storage)?;
    Ok(DocumentSection {
        id: row.try_get("section_id").map_err(storage)?,
        title: row.try_get("title").map_err(storage)?,
        level: u8::try_from(level).map_err(|_| corrupt("section level", doc))?,
        page_start: u32::try_from(start).map_err(|_| corrupt("section page", doc))?,
        page_end: u32::try_from(end).map_err(|_| corrupt("section page", doc))?,
    })
}

#[derive(Clone)]
pub struct SqliteDocumentRepository {
    pool: SqlitePool,
}

impl SqliteDocumentRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn sections(&self, id: DocumentId) -> Result<Vec<DocumentSection>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT section_id, title, level, page_start, page_end FROM document_sections \
             WHERE document_id = ? ORDER BY position",
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|r| section_from_row(r, &id.to_string()))
            .collect()
    }
}

fn to_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

#[async_trait]
impl DocumentRepository for SqliteDocumentRepository {
    async fn insert_with_job(&self, document: &Document, job: &Job) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // One conditional write: the transaction takes the lock up front and
        // the count cannot change between the check and the insert.
        let inserted = sqlx::query(
            "INSERT INTO documents (id, topic_id, filename, title, status, size_bytes, created_at) \
             SELECT ?, ?, ?, ?, 'importing', ?, ? \
             WHERE (SELECT COUNT(*) FROM documents WHERE topic_id = ?) < ?",
        )
        .bind(document.id.to_string())
        .bind(document.topic_id.to_string())
        .bind(&document.filename)
        .bind(&document.title)
        .bind(i64::try_from(document.size_bytes).unwrap_or(i64::MAX))
        .bind(format_ts(document.created_at))
        .bind(document.topic_id.to_string())
        .bind(to_i64(MAX_DOCUMENTS_PER_TOPIC))
        .execute(&mut *tx)
        .await
        .map_err(constraint)?
        .rows_affected();
        if inserted == 0 {
            return Err(RepositoryError::Conflict);
        }
        insert_job(&mut tx, job).await?;
        tx.commit().await.map_err(storage)
    }

    async fn get(&self, user: UserId, id: DocumentId) -> Result<Option<Document>, RepositoryError> {
        let row = sqlx::query(concat!(
            document_select!(),
            " WHERE d.id = ? AND d.topic_id IN (SELECT id FROM topics WHERE user_id = ?)"
        ))
        .bind(id.to_string())
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(document_from_row).transpose()
    }

    async fn ids_for_user(&self, user: UserId) -> Result<Vec<DocumentId>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT d.id FROM documents d JOIN topics t ON t.id = d.topic_id WHERE t.user_id = ?",
        )
        .bind(user.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|row| {
                let id: String = row.try_get("id").map_err(storage)?;
                Ok(DocumentId::new(parse_id(&id)?))
            })
            .collect()
    }

    async fn total_bytes_for_user(&self, user: UserId) -> Result<u64, RepositoryError> {
        let (total,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(d.size_bytes), 0) FROM documents d \
             JOIN topics t ON t.id = d.topic_id WHERE t.user_id = ?",
        )
        .bind(user.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(storage)?;
        Ok(u64::try_from(total).unwrap_or(0))
    }

    async fn list(&self, topic_id: TopicId) -> Result<Vec<DocumentView>, RepositoryError> {
        let rows = sqlx::query(concat!(
            document_select!(),
            " WHERE d.topic_id = ? ORDER BY d.created_at DESC, d.id DESC"
        ))
        .bind(topic_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let mut views = Vec::with_capacity(rows.len());
        for row in &rows {
            let document = document_from_row(row)?;
            let sections = if document.status == DocumentStatus::Ready {
                self.sections(document.id).await?
            } else {
                Vec::new()
            };
            views.push(DocumentView { document, sections });
        }
        Ok(views)
    }

    async fn mark_importing(&self, id: DocumentId) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE documents SET status = 'importing', error_code = NULL, error_message = NULL \
             WHERE id = ? AND status <> 'ready'",
        )
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }

    async fn complete(
        &self,
        import: &ImportedDocument,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        let id = import.document_id.to_string();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // First statement is a write (see `insert_with_job`); it doubles as
        // the existence check.
        let topic: Option<(String,)> =
            sqlx::query_as("UPDATE documents SET status = status WHERE id = ? RETURNING topic_id")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
        let Some((topic,)) = topic else {
            return Err(RepositoryError::NotFound);
        };
        for delete in [
            "DELETE FROM document_pages WHERE document_id = ?",
            "DELETE FROM document_sections WHERE document_id = ?",
            "DELETE FROM resources WHERE document_id = ?",
        ] {
            sqlx::query(delete)
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
        }
        for (i, text) in import.pages.iter().enumerate() {
            sqlx::query("INSERT INTO document_pages (document_id, page, text) VALUES (?, ?, ?)")
                .bind(&id)
                .bind(to_i64(i + 1))
                .bind(text)
                .execute(&mut *tx)
                .await
                .map_err(constraint)?;
        }
        for (i, s) in import.sections.iter().enumerate() {
            sqlx::query(
                "INSERT INTO document_sections (document_id, position, section_id, title, level, \
                 page_start, page_end) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(to_i64(i))
            .bind(&s.id)
            .bind(&s.title)
            .bind(i64::from(s.level))
            .bind(i64::from(s.page_start))
            .bind(i64::from(s.page_end))
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
        }
        let r = &import.resource;
        sqlx::query(
            "INSERT INTO resources (id, topic_id, position, kind, source_type, title, url, \
             language, annotation, use_for, document_id, created_at) \
             VALUES (?, ?, 0, ?, ?, ?, NULL, ?, ?, ?, ?, ?)",
        )
        .bind(import.resource_id.to_string())
        .bind(&topic)
        .bind(r.kind.as_str())
        .bind(r.source_type.as_str())
        .bind(&r.title)
        .bind(&r.language)
        .bind(&r.annotation)
        .bind(&r.use_for)
        .bind(&id)
        .bind(format_ts(at))
        .execute(&mut *tx)
        .await
        .map_err(constraint)?;
        sqlx::query(
            "UPDATE documents SET status = 'ready', error_code = NULL, error_message = NULL, \
             title = ?, language = ?, page_count = ?, resource_id = ? WHERE id = ?",
        )
        .bind(&import.title)
        .bind(&import.language)
        .bind(to_i64(import.pages.len()))
        .bind(import.resource_id.to_string())
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)
    }

    async fn fail(&self, id: DocumentId, error: &JobError) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE documents SET status = 'failed', error_code = ?, error_message = ? \
             WHERE id = ? AND status <> 'ready'",
        )
        .bind(error.code.as_str())
        .bind(&error.message)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }

    async fn delete(&self, user: UserId, id: DocumentId) -> Result<(), RepositoryError> {
        // One statement: the check for a running import and the delete cannot
        // be separated by the runner claiming the job.
        let deleted = sqlx::query(
            "DELETE FROM documents WHERE id = ? \
             AND topic_id IN (SELECT id FROM topics WHERE user_id = ?) AND NOT EXISTS \
             (SELECT 1 FROM jobs WHERE jobs.document_id = documents.id \
              AND jobs.status IN ('queued', 'running'))",
        )
        .bind(id.to_string())
        .bind(user.to_string())
        .execute(&self.pool)
        .await
        .map_err(storage)?
        .rows_affected();
        if deleted > 0 {
            return Ok(());
        }
        let exists = sqlx::query(
            "SELECT 1 FROM documents WHERE id = ? \
             AND topic_id IN (SELECT id FROM topics WHERE user_id = ?)",
        )
        .bind(id.to_string())
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?
        .is_some();
        Err(if exists {
            RepositoryError::Conflict
        } else {
            RepositoryError::NotFound
        })
    }

    async fn pages(
        &self,
        id: DocumentId,
        first: u32,
        last: u32,
    ) -> Result<Vec<(u32, String)>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT page, text FROM document_pages \
             WHERE document_id = ? AND page BETWEEN ? AND ? ORDER BY page",
        )
        .bind(id.to_string())
        .bind(i64::from(first))
        .bind(i64::from(last))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|r| {
                let page: i64 = r.try_get("page").map_err(storage)?;
                Ok((
                    u32::try_from(page).map_err(|_| corrupt("document page", &id.to_string()))?,
                    r.try_get("text").map_err(storage)?,
                ))
            })
            .collect()
    }
}

/// Uploads wait in one directory, one file per document, named after its id.
#[derive(Debug, Clone)]
pub struct LocalDocumentFiles {
    dir: PathBuf,
}

impl LocalDocumentFiles {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Creates the directory if needed and removes the partial uploads a
    /// crash left behind (they belong to no document).
    pub async fn prepare(&self) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let mut entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().is_some_and(|e| e == "part") {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl DocumentFiles for LocalDocumentFiles {
    fn path(&self, id: DocumentId) -> PathBuf {
        self.dir.join(format!("{id}.pdf"))
    }

    fn partial_path(&self, id: DocumentId) -> PathBuf {
        self.dir.join(format!("{id}.part"))
    }

    async fn remove(&self, id: DocumentId) {
        for path in [self.path(id), self.partial_path(id)] {
            if let Err(e) = tokio::fs::remove_file(&path).await
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(error = %e, "cannot delete an uploaded file");
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::assert_is_empty)]
mod tests {
    use super::*;
    use crate::jobs::SqliteJobRepository;
    use crate::repositories::{SqliteFolderRepository, SqliteTopicRepository};
    use crate::resources::SqliteResourceRepository;
    use crate::sqlite::{connect, migrate};
    use domain::document::{MAX_DOCUMENTS_PER_TOPIC, resource_draft};
    use domain::{
        FolderId, FolderName, FolderRepository, JobId, JobRepository, JobResult, NewFolder,
        ResourceDraft, ResourceKind, ResourceRepository, SourceType, Topic, TopicRepository,
        TopicState, TopicTitle,
    };
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-09 10:00:00.5 UTC);

    struct Env {
        docs: SqliteDocumentRepository,
        jobs: SqliteJobRepository,
        resources: SqliteResourceRepository,
        topics: SqliteTopicRepository,
        pool: SqlitePool,
        topic: TopicId,
    }

    async fn env() -> Env {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let folder = FolderId::new(Uuid::from_u128(1));
        SqliteFolderRepository::new(pool.clone())
            .insert(NewFolder {
                user_id: domain::UserId::OWNER,
                id: folder,
                name: FolderName::parse("F").unwrap(),
                created_at: T0,
            })
            .await
            .unwrap();
        let topics = SqliteTopicRepository::new(pool.clone());
        let topic = Topic::new(
            TopicId::new(Uuid::from_u128(2)),
            domain::UserId::OWNER,
            folder,
            TopicTitle::parse("T").unwrap(),
            T0,
        );
        topics.insert(&topic).await.unwrap();
        Env {
            docs: SqliteDocumentRepository::new(pool.clone()),
            jobs: SqliteJobRepository::new(pool.clone()),
            resources: SqliteResourceRepository::new(pool.clone()),
            topics,
            pool,
            topic: topic.id,
        }
    }

    fn document(env: &Env, n: u128) -> (Document, Job) {
        let id = DocumentId::new(Uuid::from_u128(0x100 + n));
        let at = T0 + time::Duration::seconds(i64::try_from(n).unwrap());
        let doc = Document::importing(id, env.topic, format!("book{n}.pdf"), 1000, at);
        let job =
            Job::queued_for_document(JobId::new(Uuid::from_u128(0x200 + n)), env.topic, id, at);
        (doc, job)
    }

    fn imported(doc: &Document, n: u128, pages: usize) -> ImportedDocument {
        ImportedDocument {
            document_id: doc.id,
            title: "Rust in Action".into(),
            language: Some("en".into()),
            pages: (1..=pages).map(|p| format!("page {p}")).collect(),
            sections: vec![
                DocumentSection {
                    id: "s1".into(),
                    title: "1 Intro".into(),
                    level: 1,
                    page_start: 1,
                    page_end: 2,
                },
                DocumentSection {
                    id: "s2".into(),
                    title: "2 Basics".into(),
                    level: 1,
                    page_start: 3,
                    page_end: u32::try_from(pages).unwrap(),
                },
            ],
            resource_id: ResourceId::new(Uuid::from_u128(0x300 + n)),
            resource: resource_draft("Rust in Action", u32::try_from(pages).unwrap(), Some("en")),
        }
    }

    async fn finish_job(env: &Env, job: &Job, outcome: Option<JobErrorCode>) {
        let mut j = job.clone();
        j.start(T0).unwrap();
        env.jobs.update(&j, JobStatus::Queued).await.unwrap();
        match outcome {
            None => j
                .succeed(
                    JobResult::DocumentImport {
                        document_id: job.document_id.unwrap(),
                        page_count: 1,
                        section_count: 1,
                    },
                    T0,
                )
                .unwrap(),
            Some(code) => j.fail(code, domain::ContentLanguage::It, T0).unwrap(),
        }
        env.jobs.update(&j, JobStatus::Running).await.unwrap();
    }

    async fn count(env: &Env, table: &'static str) -> i64 {
        let (n,): (i64,) =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&env.pool)
                .await
                .unwrap();
        n
    }

    #[tokio::test]
    async fn the_total_bytes_of_a_user_sum_their_documents() {
        let e = env().await;
        assert_eq!(
            e.docs
                .total_bytes_for_user(domain::UserId::OWNER)
                .await
                .unwrap(),
            0
        );
        for n in 1..=2 {
            let (doc, job) = document(&e, n);
            e.docs.insert_with_job(&doc, &job).await.unwrap();
            finish_job(&e, &job, None).await;
        }
        assert_eq!(
            e.docs
                .total_bytes_for_user(domain::UserId::OWNER)
                .await
                .unwrap(),
            2000
        );
        let stranger = domain::UserId::new(Uuid::from_u128(0x999));
        assert_eq!(e.docs.total_bytes_for_user(stranger).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn a_document_and_its_job_are_stored_together_and_limited() {
        let e = env().await;
        let (doc, job) = document(&e, 1);
        e.docs.insert_with_job(&doc, &job).await.unwrap();
        let stored = e
            .docs
            .get(domain::UserId::OWNER, doc.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, doc);
        let job_back = e
            .jobs
            .get(domain::UserId::OWNER, e.topic, job.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job_back.document_id, Some(doc.id));
        assert_eq!(job_back.kind, domain::JobKind::DocumentImport);

        // A second active import in the topic is refused, and leaves nothing.
        let (doc2, job2) = document(&e, 2);
        assert!(matches!(
            e.docs.insert_with_job(&doc2, &job2).await,
            Err(RepositoryError::Conflict)
        ));
        assert!(
            e.docs
                .get(domain::UserId::OWNER, doc2.id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(count(&e, "documents").await, 1);

        // A missing topic.
        let mut orphan = document(&e, 3);
        orphan.0.topic_id = TopicId::new(Uuid::from_u128(99));
        orphan.1.topic_id = orphan.0.topic_id;
        assert!(matches!(
            e.docs.insert_with_job(&orphan.0, &orphan.1).await,
            Err(RepositoryError::MissingParent)
        ));

        // The limit: five documents in all.
        finish_job(&e, &job, None).await;
        for n in 2..=u128::try_from(MAX_DOCUMENTS_PER_TOPIC).unwrap() {
            let (d, j) = document(&e, n);
            e.docs.insert_with_job(&d, &j).await.unwrap();
            finish_job(&e, &j, None).await;
        }
        let (d, j) = document(&e, 9);
        assert!(matches!(
            e.docs.insert_with_job(&d, &j).await,
            Err(RepositoryError::Conflict)
        ));
        assert_eq!(
            e.docs.list(e.topic).await.unwrap().len(),
            MAX_DOCUMENTS_PER_TOPIC
        );
    }

    #[tokio::test]
    async fn the_import_job_decides_the_status_until_the_document_is_ready() {
        let e = env().await;
        let (doc, job) = document(&e, 1);
        e.docs.insert_with_job(&doc, &job).await.unwrap();
        let status = |d: Option<Document>| d.map(|d| (d.status, d.error.map(|e| e.code)));
        assert_eq!(
            status(e.docs.get(domain::UserId::OWNER, doc.id).await.unwrap()),
            Some((DocumentStatus::Importing, None))
        );

        // The job failed but nobody recorded it on the document (a crash, an
        // interruption): the document still says why.
        finish_job(&e, &job, Some(JobErrorCode::EncryptedPdf)).await;
        assert_eq!(
            status(e.docs.get(domain::UserId::OWNER, doc.id).await.unwrap()),
            Some((DocumentStatus::Failed, Some(JobErrorCode::EncryptedPdf)))
        );
        // Recorded on the document, it stays after the job is gone from view.
        e.docs
            .fail(
                doc.id,
                &JobError::from_code(JobErrorCode::EncryptedPdf, domain::ContentLanguage::It),
            )
            .await
            .unwrap();
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM documents WHERE error_code = 'encrypted_pdf'")
                .fetch_one(&e.pool)
                .await
                .unwrap();
        assert_eq!(n, 1);

        // A retry: the job is queued again, the document importing, no error.
        let mut retried = e
            .jobs
            .get(domain::UserId::OWNER, e.topic, job.id)
            .await
            .unwrap()
            .unwrap();
        retried.retry().unwrap();
        e.jobs.update(&retried, JobStatus::Failed).await.unwrap();
        assert_eq!(
            status(e.docs.get(domain::UserId::OWNER, doc.id).await.unwrap()),
            Some((DocumentStatus::Importing, None))
        );
        e.docs.mark_importing(doc.id).await.unwrap();
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM documents WHERE error_code IS NULL")
                .fetch_one(&e.pool)
                .await
                .unwrap();
        assert_eq!(n, 1, "the stored error is cleared too");
    }

    #[tokio::test]
    async fn completing_stores_pages_sections_and_the_resource_in_one_go() {
        let e = env().await;
        let (doc, job) = document(&e, 1);
        e.docs.insert_with_job(&doc, &job).await.unwrap();
        let import = imported(&doc, 1, 6);
        e.docs.complete(&import, T0).await.unwrap();
        finish_job(&e, &job, None).await;

        let stored = e
            .docs
            .get(domain::UserId::OWNER, doc.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, DocumentStatus::Ready);
        assert_eq!(stored.title, "Rust in Action");
        assert_eq!(stored.page_count, Some(6));
        assert_eq!(stored.language.as_deref(), Some("en"));
        assert_eq!(stored.resource_id, Some(import.resource_id));
        assert_eq!(stored.filename, "book1.pdf");

        let views = e.docs.list(e.topic).await.unwrap();
        assert_eq!(views[0].sections, import.sections);
        let pages = e.docs.pages(doc.id, 2, 4).await.unwrap();
        assert_eq!(
            pages,
            [
                (2, "page 2".to_owned()),
                (3, "page 3".to_owned()),
                (4, "page 4".to_owned())
            ]
        );
        assert!(e.docs.pages(doc.id, 50, 60).await.unwrap().is_empty());

        let all = e.resources.get(e.topic).await.unwrap();
        assert_eq!(all.resources.len(), 1);
        let r = &all.resources[0];
        assert_eq!(
            (r.kind, r.source_type),
            (ResourceKind::Document, SourceType::Pdf)
        );
        assert_eq!(r.document_id, Some(doc.id));
        assert_eq!(r.url, None);
        assert_eq!(r.domain(), None);

        // Running the import again (a crash after saving) leaves one copy.
        let again = ImportedDocument {
            resource_id: ResourceId::new(Uuid::from_u128(0x999)),
            ..imported(&doc, 1, 4)
        };
        e.docs.complete(&again, T0).await.unwrap();
        assert_eq!(count(&e, "document_pages").await, 4);
        assert_eq!(count(&e, "document_sections").await, 2);
        assert_eq!(count(&e, "resources").await, 1);

        // A deleted document cannot be completed.
        e.docs.delete(domain::UserId::OWNER, doc.id).await.unwrap();
        assert!(matches!(
            e.docs.complete(&import, T0).await,
            Err(RepositoryError::NotFound)
        ));
    }

    #[tokio::test]
    async fn deleting_cascades_and_waits_for_the_import() {
        let e = env().await;
        let (doc, job) = document(&e, 1);
        e.docs.insert_with_job(&doc, &job).await.unwrap();
        assert!(matches!(
            e.docs.delete(domain::UserId::OWNER, doc.id).await,
            Err(RepositoryError::Conflict)
        ));
        e.docs.complete(&imported(&doc, 1, 5), T0).await.unwrap();
        finish_job(&e, &job, None).await;

        e.docs.delete(domain::UserId::OWNER, doc.id).await.unwrap();
        for table in [
            "documents",
            "document_pages",
            "document_sections",
            "resources",
            "jobs",
        ] {
            assert_eq!(count(&e, table).await, 0, "{table}");
        }
        assert!(matches!(
            e.docs.delete(domain::UserId::OWNER, doc.id).await,
            Err(RepositoryError::NotFound)
        ));
    }

    #[tokio::test]
    async fn a_research_replaces_web_sources_and_leaves_documents_first() {
        let e = env().await;
        let (doc, job) = document(&e, 1);
        e.docs.insert_with_job(&doc, &job).await.unwrap();
        e.docs.complete(&imported(&doc, 1, 5), T0).await.unwrap();
        e.topics
            .transition_state(e.topic, TopicState::New, TopicState::Researching, T0)
            .await
            .unwrap();

        let web = |n: u128| {
            (
                ResourceId::new(Uuid::from_u128(0x400 + n)),
                ResourceDraft {
                    kind: ResourceKind::Knowledge,
                    source_type: SourceType::Documentation,
                    title: format!("Web {n}"),
                    url: Some(format!("https://example.org/{n}")),
                    language: "en".into(),
                    annotation: "a".into(),
                    use_for: "u".into(),
                },
            )
        };
        let outcome = |resources| domain::ResearchOutcome {
            topic_id: e.topic,
            info: domain::ResearchInfo {
                job_id: JobId::new(Uuid::from_u128(0x500)),
                mission_version: 1,
                finished_at: T0,
            },
            communities_skipped: false,
            resources,
            gaps: vec![],
        };
        e.resources
            .replace(&outcome(vec![web(1), web(2)]), T0)
            .await
            .unwrap();
        let titles = |set: domain::TopicResources| {
            set.resources
                .into_iter()
                .map(|r| r.title)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            titles(e.resources.get(e.topic).await.unwrap()),
            ["Rust in Action", "Web 1", "Web 2"]
        );
        // Again: the web sources are replaced (positions restart), the PDF stays.
        e.resources
            .replace(&outcome(vec![web(3)]), T0)
            .await
            .unwrap();
        assert_eq!(
            titles(e.resources.get(e.topic).await.unwrap()),
            ["Rust in Action", "Web 3"]
        );
        // And even an empty research does not remove it.
        e.resources.replace(&outcome(vec![]), T0).await.unwrap();
        assert_eq!(
            titles(e.resources.get(e.topic).await.unwrap()),
            ["Rust in Action"]
        );
        assert_eq!(count(&e, "document_pages").await, 5);
    }

    #[tokio::test]
    async fn documents_are_listed_newest_first_with_sections_only_when_ready() {
        let e = env().await;
        let (d1, j1) = document(&e, 1);
        e.docs.insert_with_job(&d1, &j1).await.unwrap();
        e.docs.complete(&imported(&d1, 1, 5), T0).await.unwrap();
        finish_job(&e, &j1, None).await;
        let (d2, j2) = document(&e, 2);
        e.docs.insert_with_job(&d2, &j2).await.unwrap();

        let list = e.docs.list(e.topic).await.unwrap();
        let order: Vec<_> = list.iter().map(|v| v.document.id).collect();
        assert_eq!(order, [d2.id, d1.id]);
        assert!(list[0].sections.is_empty());
        assert_eq!(list[1].sections.len(), 2);
        assert!(
            e.docs
                .list(TopicId::new(Uuid::from_u128(77)))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn files_are_named_after_the_document_and_partial_ones_are_swept() {
        let dir = std::env::temp_dir().join(format!("teach_me_files_{}", Uuid::now_v7()));
        let files = LocalDocumentFiles::new(dir.clone());
        files.prepare().await.unwrap();
        let id = DocumentId::new(Uuid::from_u128(7));
        assert_eq!(files.path(id).extension().unwrap(), "pdf");
        assert_eq!(files.partial_path(id).extension().unwrap(), "part");
        tokio::fs::write(files.path(id), b"%PDF-").await.unwrap();
        tokio::fs::write(files.partial_path(id), b"%PD")
            .await
            .unwrap();

        // A new start removes the partial file and keeps the complete one.
        files.prepare().await.unwrap();
        assert!(files.path(id).exists() && !files.partial_path(id).exists());
        files.remove(id).await;
        files.remove(id).await; // a missing file is not an error
        assert!(!files.path(id).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
