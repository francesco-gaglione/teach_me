//! SQLite adapter for the job repository.

use async_trait::async_trait;
use domain::{
    DocumentId, Job, JobCursor, JobError, JobErrorCode, JobId, JobKind, JobProgress, JobRepository,
    JobResult, JobStatus, LessonId, RepositoryError, TopicId, UserId,
};
use serde_json::{Value, json};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};

macro_rules! job_select {
    () => {
        "SELECT id, topic_id, lesson_id, document_id, kind, status, progress, error_code, error_message, attempt, \
         result, created_at, started_at, finished_at FROM jobs"
    };
}

fn progress_to_json(p: &JobProgress) -> String {
    json!({ "step": p.step, "message": p.message, "done": p.done, "total": p.total }).to_string()
}

fn result_to_json(result: JobResult) -> String {
    match result {
        JobResult::Research {
            resource_count,
            gap_count,
        } => json!({ "resource_count": resource_count, "gap_count": gap_count }),
        JobResult::LessonGeneration {
            lesson_id,
            lesson_number,
        } => json!({ "lesson_id": lesson_id.to_string(), "lesson_number": lesson_number }),
        JobResult::LessonCompletion {
            lesson_id,
            record_count,
            term_count,
            review_card_count,
        } => json!({
            "lesson_id": lesson_id.to_string(),
            "record_count": record_count,
            "term_count": term_count,
            "review_card_count": review_card_count,
        }),
        JobResult::DocumentImport {
            document_id,
            page_count,
            section_count,
        } => json!({
            "document_id": document_id.to_string(),
            "page_count": page_count,
            "section_count": section_count,
        }),
    }
    .to_string()
}

fn result_from_json(kind: JobKind, raw: &str, row_id: &str) -> Result<JobResult, RepositoryError> {
    let value: Value = serde_json::from_str(raw).map_err(|_| corrupt("job result", row_id))?;
    let count = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| corrupt("job result", row_id))
    };
    let lesson_id = || {
        value
            .get("lesson_id")
            .and_then(Value::as_str)
            .and_then(domain::parse_uuid)
            .map(LessonId::new)
            .ok_or_else(|| corrupt("job result", row_id))
    };
    let document_id = || {
        value
            .get("document_id")
            .and_then(Value::as_str)
            .and_then(domain::parse_uuid)
            .map(DocumentId::new)
            .ok_or_else(|| corrupt("job result", row_id))
    };
    match kind {
        JobKind::DocumentImport => Ok(JobResult::DocumentImport {
            document_id: document_id()?,
            page_count: count("page_count")?,
            section_count: count("section_count")?,
        }),
        JobKind::Research => Ok(JobResult::Research {
            resource_count: count("resource_count")?,
            gap_count: count("gap_count")?,
        }),
        JobKind::LessonGeneration => Ok(JobResult::LessonGeneration {
            lesson_id: lesson_id()?,
            lesson_number: count("lesson_number")?,
        }),
        JobKind::LessonCompletion => Ok(JobResult::LessonCompletion {
            lesson_id: lesson_id()?,
            record_count: count("record_count")?,
            term_count: count("term_count")?,
            review_card_count: count("review_card_count")?,
        }),
    }
}

fn progress_from_json(raw: &str, row_id: &str) -> Result<JobProgress, RepositoryError> {
    let value: Value = serde_json::from_str(raw).map_err(|_| corrupt("job progress", row_id))?;
    let count = |key: &str| -> Result<Option<u32>, RepositoryError> {
        match value.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(Some)
                .ok_or_else(|| corrupt("job progress", row_id)),
        }
    };
    Ok(JobProgress {
        step: value
            .get("step")
            .and_then(Value::as_str)
            .ok_or_else(|| corrupt("job progress", row_id))?
            .to_owned(),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned),
        done: count("done")?,
        total: count("total")?,
    })
}

fn job_from_row(row: &SqliteRow) -> Result<Job, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic_id: String = row.try_get("topic_id").map_err(storage)?;
    let lesson_id: Option<String> = row.try_get("lesson_id").map_err(storage)?;
    let document_id: Option<String> = row.try_get("document_id").map_err(storage)?;
    let kind: String = row.try_get("kind").map_err(storage)?;
    let status: String = row.try_get("status").map_err(storage)?;
    let progress: Option<String> = row.try_get("progress").map_err(storage)?;
    let error_code: Option<String> = row.try_get("error_code").map_err(storage)?;
    let error_message: Option<String> = row.try_get("error_message").map_err(storage)?;
    let result: Option<String> = row.try_get("result").map_err(storage)?;
    let attempt: i64 = row.try_get("attempt").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let started: Option<String> = row.try_get("started_at").map_err(storage)?;
    let finished: Option<String> = row.try_get("finished_at").map_err(storage)?;
    let error = match (error_code, error_message) {
        (Some(code), Some(message)) => Some(JobError {
            code: code
                .parse::<JobErrorCode>()
                .map_err(|_| corrupt("job error code", &id))?,
            message,
        }),
        (None, _) => None,
        _ => return Err(corrupt("job error", &id)),
    };
    let kind = kind
        .parse::<JobKind>()
        .map_err(|_| corrupt("job kind", &id))?;
    Ok(Job {
        id: JobId::new(parse_id(&id)?),
        topic_id: TopicId::new(parse_id(&topic_id)?),
        lesson_id: lesson_id
            .as_deref()
            .map(|l| parse_id(l).map(LessonId::new))
            .transpose()?,
        document_id: document_id
            .as_deref()
            .map(|d| parse_id(d).map(DocumentId::new))
            .transpose()?,
        kind,
        status: status
            .parse::<JobStatus>()
            .map_err(|_| corrupt("job status", &id))?,
        progress: progress
            .as_deref()
            .map(|p| progress_from_json(p, &id))
            .transpose()?,
        error,
        result: result
            .as_deref()
            .map(|r| result_from_json(kind, r, &id))
            .transpose()?,
        attempt: u32::try_from(attempt).map_err(|_| corrupt("job attempt", &id))?,
        created_at: parse_ts(&created)?,
        started_at: started.as_deref().map(parse_ts).transpose()?,
        finished_at: finished.as_deref().map(parse_ts).transpose()?,
    })
}

/// Inserts inside the caller's connection/transaction. A unique violation is
/// the one-active-job rule (`Conflict`); a missing topic is `MissingParent`.
pub(crate) async fn insert_job(
    conn: &mut SqliteConnection,
    job: &Job,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "INSERT INTO jobs (id, topic_id, lesson_id, document_id, kind, status, progress, error_code, \
         error_message, result, attempt, created_at, started_at, finished_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(job.id.to_string())
    .bind(job.topic_id.to_string())
    .bind(job.lesson_id.map(|l| l.to_string()))
    .bind(job.document_id.map(|d| d.to_string()))
    .bind(job.kind.as_str())
    .bind(job.status.as_str())
    .bind(job.progress.as_ref().map(progress_to_json))
    .bind(job.error.as_ref().map(|e| e.code.as_str()))
    .bind(job.error.as_ref().map(|e| e.message.as_str()))
    .bind(job.result.map(result_to_json))
    .bind(i64::from(job.attempt))
    .bind(format_ts(job.created_at))
    .bind(job.started_at.map(format_ts))
    .bind(job.finished_at.map(format_ts))
    .execute(conn)
    .await
    .map_err(|e| match constraint(e) {
        RepositoryError::Duplicate => RepositoryError::Conflict,
        other => other,
    })?;
    Ok(())
}

#[derive(Clone)]
pub struct SqliteJobRepository {
    pool: SqlitePool,
}

impl SqliteJobRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl SqliteJobRepository {
    /// Marks a row that cannot be read as failed (`internal`), so it frees
    /// its (topic, kind) slot and is never claimed again. The columns are
    /// copied as they are: whatever is wrong with the row stays visible.
    async fn quarantine(&self, row: &SqliteRow) -> Result<(), RepositoryError> {
        let id: String = row.try_get("id").map_err(storage)?;
        // Italian, the default language: the row is unreadable, so its topic
        // (and the topic's language) cannot be trusted or looked up here.
        let message =
            JobError::from_code(JobErrorCode::Internal, domain::ContentLanguage::It).message;
        sqlx::query(
            "UPDATE jobs SET status = 'failed', progress = NULL, error_code = 'internal', \
             error_message = ?, result = NULL, finished_at = COALESCE(started_at, created_at) \
             WHERE id = ?",
        )
        .bind(message)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }
}

#[async_trait]
impl JobRepository for SqliteJobRepository {
    async fn insert(&self, job: &Job) -> Result<(), RepositoryError> {
        let mut conn = self.pool.acquire().await.map_err(storage)?;
        insert_job(&mut conn, job).await
    }

    async fn get(
        &self,
        user: UserId,
        topic_id: TopicId,
        id: JobId,
    ) -> Result<Option<Job>, RepositoryError> {
        let row = sqlx::query(concat!(
            job_select!(),
            " WHERE topic_id = ? AND id = ? \
             AND topic_id IN (SELECT id FROM topics WHERE user_id = ?)"
        ))
        .bind(topic_id.to_string())
        .bind(id.to_string())
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(job_from_row).transpose()
    }

    async fn list(
        &self,
        topic_id: TopicId,
        after: Option<JobCursor>,
        limit: u32,
    ) -> Result<Vec<Job>, RepositoryError> {
        let at = after.map(|c| format_ts(c.created_at));
        let id = after.map(|c| c.id.to_string());
        let rows = sqlx::query(concat!(
            job_select!(),
            " WHERE topic_id = ? AND (? IS NULL OR created_at < ? OR (created_at = ? AND id < ?)) \
             ORDER BY created_at DESC, id DESC LIMIT ?"
        ))
        .bind(topic_id.to_string())
        .bind(&at)
        .bind(&at)
        .bind(&at)
        .bind(&id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(job_from_row).collect()
    }

    async fn latest(
        &self,
        topic_id: TopicId,
        kind: JobKind,
        lesson_id: Option<LessonId>,
    ) -> Result<Option<Job>, RepositoryError> {
        let row = sqlx::query(concat!(
            job_select!(),
            " WHERE topic_id = ? AND kind = ? AND lesson_id IS ? \
             ORDER BY created_at DESC, id DESC LIMIT 1"
        ))
        .bind(topic_id.to_string())
        .bind(kind.as_str())
        .bind(lesson_id.map(|l| l.to_string()))
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(job_from_row).transpose()
    }

    async fn next_queued(&self, kinds: &[JobKind]) -> Result<Option<Job>, RepositoryError> {
        // Few kinds: scan the queue in order and keep the first that matches,
        // which avoids building SQL with a dynamic IN list.
        let rows = sqlx::query(concat!(
            job_select!(),
            " WHERE status = 'queued' ORDER BY created_at, id"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        for row in &rows {
            // An unknown kind (written by a newer version) is skipped, not fatal.
            let kind: String = row.try_get("kind").map_err(storage)?;
            if kind.parse::<JobKind>().is_ok_and(|k| kinds.contains(&k)) {
                match job_from_row(row) {
                    Ok(job) => return Ok(Some(job)),
                    // One bad row must not stop the queue.
                    Err(RepositoryError::Storage(e)) => {
                        tracing::error!(error = %e, "unreadable queued job; failing it");
                        self.quarantine(row).await?;
                    }
                    Err(other) => return Err(other),
                }
            }
        }
        Ok(None)
    }

    async fn running(&self) -> Result<Vec<Job>, RepositoryError> {
        let rows = sqlx::query(concat!(
            job_select!(),
            " WHERE status = 'running' ORDER BY created_at, id"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let mut jobs = Vec::new();
        for row in &rows {
            match job_from_row(row) {
                Ok(job) => jobs.push(job),
                // One bad row must not stop the startup.
                Err(RepositoryError::Storage(e)) => {
                    tracing::error!(error = %e, "unreadable running job; failing it");
                    self.quarantine(row).await?;
                }
                Err(other) => return Err(other),
            }
        }
        Ok(jobs)
    }

    async fn update(&self, job: &Job, expected: JobStatus) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE jobs SET status = ?, progress = ?, error_code = ?, error_message = ?, \
             result = ?, attempt = ?, started_at = ?, finished_at = ? WHERE id = ? AND status = ?",
        )
        .bind(job.status.as_str())
        .bind(job.progress.as_ref().map(progress_to_json))
        .bind(job.error.as_ref().map(|e| e.code.as_str()))
        .bind(job.error.as_ref().map(|e| e.message.as_str()))
        .bind(job.result.map(result_to_json))
        .bind(i64::from(job.attempt))
        .bind(job.started_at.map(format_ts))
        .bind(job.finished_at.map(format_ts))
        .bind(job.id.to_string())
        .bind(expected.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| match constraint(e) {
            RepositoryError::Duplicate => RepositoryError::Conflict,
            other => other,
        })?;
        if result.rows_affected() > 0 {
            return Ok(());
        }
        // Nothing matched: gone, or in another status.
        let exists = sqlx::query("SELECT 1 FROM jobs WHERE id = ?")
            .bind(job.id.to_string())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::{SqliteFolderRepository, SqliteTopicRepository};
    use crate::sqlite::{connect, migrate};
    use domain::{
        FolderId, FolderName, FolderRepository, NewFolder, Topic, TopicRepository, TopicTitle,
    };
    use time::OffsetDateTime;
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-09 10:00:00.5 UTC);

    struct Env {
        jobs: SqliteJobRepository,
        topics: SqliteTopicRepository,
        topic: TopicId,
    }

    async fn env() -> Env {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let folders = SqliteFolderRepository::new(pool.clone());
        let topics = SqliteTopicRepository::new(pool.clone());
        let folder = FolderId::new(Uuid::from_u128(1));
        folders
            .insert(NewFolder {
                user_id: domain::UserId::OWNER,
                id: folder,
                name: FolderName::parse("F").unwrap(),
                created_at: T0,
            })
            .await
            .unwrap();
        let topic = Topic::new(
            TopicId::new(Uuid::from_u128(2)),
            domain::UserId::OWNER,
            folder,
            TopicTitle::parse("T").unwrap(),
            T0,
        );
        topics.insert(&topic).await.unwrap();
        Env {
            jobs: SqliteJobRepository::new(pool),
            topics,
            topic: topic.id,
        }
    }

    fn job(env: &Env, n: u128, secs: i64) -> Job {
        Job::queued(
            JobId::new(Uuid::from_u128(100 + n)),
            env.topic,
            JobKind::Research,
            T0 + time::Duration::seconds(secs),
        )
    }

    #[tokio::test]
    async fn round_trips_every_field() {
        let env = env().await;
        let mut j = job(&env, 1, 0);
        env.jobs.insert(&j).await.unwrap();
        assert_eq!(
            env.jobs
                .get(domain::UserId::OWNER, env.topic, j.id)
                .await
                .unwrap(),
            Some(j.clone())
        );

        j.start(T0).unwrap();
        j.set_progress(
            JobProgress::step("searching")
                .with_message("Ho cercato: axum")
                .with_done(1),
        )
        .unwrap();
        env.jobs.update(&j, JobStatus::Queued).await.unwrap();
        assert_eq!(
            env.jobs
                .get(domain::UserId::OWNER, env.topic, j.id)
                .await
                .unwrap(),
            Some(j.clone())
        );

        j.fail(JobErrorCode::InvalidOutput, domain::ContentLanguage::It, T0)
            .unwrap();
        env.jobs.update(&j, JobStatus::Running).await.unwrap();
        assert_eq!(
            env.jobs
                .get(domain::UserId::OWNER, env.topic, j.id)
                .await
                .unwrap(),
            Some(j)
        );
    }

    #[tokio::test]
    async fn a_result_survives_the_round_trip() {
        let env = env().await;
        let mut j = job(&env, 1, 0);
        env.jobs.insert(&j).await.unwrap();
        finish(&env, &mut j).await;
        let stored = env
            .jobs
            .get(domain::UserId::OWNER, env.topic, j.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.result,
            Some(JobResult::Research {
                resource_count: 5,
                gap_count: 2
            })
        );
    }

    /// Walks a stored queued job through running to succeeded.
    async fn finish(env: &Env, j: &mut Job) {
        j.start(T0).unwrap();
        env.jobs.update(j, JobStatus::Queued).await.unwrap();
        j.succeed(
            JobResult::Research {
                resource_count: 5,
                gap_count: 2,
            },
            T0,
        )
        .unwrap();
        env.jobs.update(j, JobStatus::Running).await.unwrap();
    }

    #[tokio::test]
    async fn a_job_failed_with_credit_exhausted_is_read_back_with_its_code() {
        let env = env().await;
        let mut j = job(&env, 1, 0);
        env.jobs.insert(&j).await.unwrap();
        j.start(T0).unwrap();
        env.jobs.update(&j, JobStatus::Queued).await.unwrap();
        j.fail(
            JobErrorCode::LlmCreditExhausted,
            domain::ContentLanguage::It,
            T0,
        )
        .unwrap();
        env.jobs.update(&j, JobStatus::Running).await.unwrap();
        let stored = env
            .jobs
            .get(domain::UserId::OWNER, env.topic, j.id)
            .await
            .unwrap()
            .unwrap();
        let error = stored.error.unwrap();
        assert_eq!(error.code, JobErrorCode::LlmCreditExhausted);
        assert_eq!(
            error.message,
            JobError::from_code(error.code, domain::ContentLanguage::It).message
        );
        let raw: (String,) = sqlx::query_as("SELECT error_code FROM jobs WHERE id = ?")
            .bind(j.id.to_string())
            .fetch_one(&env.jobs.pool)
            .await
            .unwrap();
        assert_eq!(raw.0, "llm_credit_exhausted");
    }

    #[tokio::test]
    async fn one_active_job_per_topic_and_kind() {
        let env = env().await;
        let mut first = job(&env, 1, 0);
        env.jobs.insert(&first).await.unwrap();
        assert!(matches!(
            env.jobs.insert(&job(&env, 2, 1)).await,
            Err(RepositoryError::Conflict)
        ));
        // A finished job frees the slot.
        finish(&env, &mut first).await;
        env.jobs.insert(&job(&env, 2, 1)).await.unwrap();
    }

    #[tokio::test]
    async fn retrying_into_an_occupied_slot_conflicts() {
        let env = env().await;
        let mut failed = job(&env, 1, 0);
        env.jobs.insert(&failed).await.unwrap();
        failed.start(T0).unwrap();
        failed
            .fail(JobErrorCode::Internal, domain::ContentLanguage::It, T0)
            .unwrap();
        env.jobs.update(&failed, JobStatus::Queued).await.unwrap();
        env.jobs.insert(&job(&env, 2, 1)).await.unwrap();
        failed.retry().unwrap();
        assert!(matches!(
            env.jobs.update(&failed, JobStatus::Failed).await,
            Err(RepositoryError::Conflict)
        ));
    }

    #[tokio::test]
    async fn update_is_conditional_on_the_stored_status() {
        let env = env().await;
        let mut j = job(&env, 1, 0);
        env.jobs.insert(&j).await.unwrap();
        j.start(T0).unwrap();
        assert!(matches!(
            env.jobs.update(&j, JobStatus::Running).await,
            Err(RepositoryError::Conflict)
        ));
        let ghost = job(&env, 9, 0);
        assert!(matches!(
            env.jobs.update(&ghost, JobStatus::Queued).await,
            Err(RepositoryError::NotFound)
        ));
    }

    #[tokio::test]
    async fn queue_is_fifo_and_filtered_by_kind() {
        let env = env().await;
        assert!(
            env.jobs
                .next_queued(&[JobKind::Research])
                .await
                .unwrap()
                .is_none()
        );
        let j = job(&env, 1, 0);
        env.jobs.insert(&j).await.unwrap();
        assert!(env.jobs.next_queued(&[]).await.unwrap().is_none());
        assert_eq!(
            env.jobs.next_queued(&[JobKind::Research]).await.unwrap(),
            Some(j)
        );
        assert_eq!(env.jobs.running().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn lists_newest_first_with_keyset_paging_and_cascades_with_the_topic() {
        let env = env().await;
        for n in 1..=3u128 {
            let mut j = job(&env, n, i64::try_from(n).unwrap());
            env.jobs.insert(&j).await.unwrap();
            finish(&env, &mut j).await;
        }
        let first = env.jobs.list(env.topic, None, 2).await.unwrap();
        assert_eq!(
            first.iter().map(|j| j.id).collect::<Vec<_>>(),
            vec![
                JobId::new(Uuid::from_u128(103)),
                JobId::new(Uuid::from_u128(102))
            ]
        );
        let rest = env
            .jobs
            .list(env.topic, Some(JobCursor::after(&first[1])), 2)
            .await
            .unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].id, JobId::new(Uuid::from_u128(101)));

        assert!(
            env.topics
                .delete(domain::UserId::OWNER, env.topic)
                .await
                .unwrap()
        );
        assert_eq!(env.jobs.list(env.topic, None, 10).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn an_unreadable_row_is_failed_instead_of_blocking_the_queue_and_the_startup() {
        let env = env().await;
        let good = job(&env, 2, 1);
        // The bad one is older, so it is met first. Its status is valid but
        // its progress is not JSON.
        let bad = job(&env, 1, 0);
        env.jobs.insert(&bad).await.unwrap();
        sqlx::query("UPDATE jobs SET progress = 'not json' WHERE id = ?")
            .bind(bad.id.to_string())
            .execute(&env.jobs.pool)
            .await
            .unwrap();
        assert!(
            env.jobs
                .next_queued(&[JobKind::Research])
                .await
                .unwrap()
                .is_none()
        );
        let stored: (String, String) =
            sqlx::query_as("SELECT status, error_code FROM jobs WHERE id = ?")
                .bind(bad.id.to_string())
                .fetch_one(&env.jobs.pool)
                .await
                .unwrap();
        assert_eq!(stored, ("failed".into(), "internal".into()));
        // Its slot is free again and the next job is served.
        env.jobs.insert(&good).await.unwrap();
        assert_eq!(
            env.jobs.next_queued(&[JobKind::Research]).await.unwrap(),
            Some(good.clone())
        );

        // Same for a running job at startup.
        let mut started = good;
        started.start(T0).unwrap();
        env.jobs.update(&started, JobStatus::Queued).await.unwrap();
        sqlx::query("UPDATE jobs SET progress = '{' WHERE id = ?")
            .bind(started.id.to_string())
            .execute(&env.jobs.pool)
            .await
            .unwrap();
        assert_eq!(env.jobs.running().await.unwrap().len(), 0);
        env.jobs.insert(&job(&env, 3, 2)).await.unwrap();
    }

    #[tokio::test]
    async fn missing_topic_is_a_missing_parent() {
        let env = env().await;
        let mut j = job(&env, 1, 0);
        j.topic_id = TopicId::new(Uuid::from_u128(77));
        assert!(matches!(
            env.jobs.insert(&j).await,
            Err(RepositoryError::MissingParent)
        ));
    }
}
