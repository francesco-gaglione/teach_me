//! Lesson adapter.

use async_trait::async_trait;
use domain::{
    COMPONENT_REQUEST_EXAMPLES, ComponentRequest, ComponentRequestCursor, ComponentRequestExample,
    ComponentRequestGroup, ComponentRequestId, GeneratedLesson, InProgressCursor, InProgressLesson,
    Job, JobId, Lesson, LessonCursor, LessonId, LessonRepository, LessonStatus, RepositoryError,
    StoredLesson, TopicId, UserId,
};
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::{as_conflict, count, json_parse, json_text, to_i64, warnings_json, warnings_parse};
use crate::jobs::insert_job;
use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};

fn lesson_from_row(row: &SqliteRow) -> Result<Lesson, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic: String = row.try_get("topic_id").map_err(storage)?;
    let status: String = row.try_get("status").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let completed: Option<String> = row.try_get("completed_at").map_err(storage)?;
    let n = |col: &str| -> Result<u32, RepositoryError> {
        let v: i64 = row.try_get(col).map_err(storage)?;
        count(v, "lesson number", &id)
    };
    let warnings: String = row.try_get("quality_warnings").map_err(storage)?;
    Ok(Lesson {
        quality_warnings: warnings_parse(&warnings, &id)?,
        id: LessonId::new(parse_id(&id)?),
        topic_id: TopicId::new(parse_id(&topic)?),
        number: n("number")?,
        title: row.try_get("title").map_err(storage)?,
        objective: row.try_get("objective").map_err(storage)?,
        estimated_minutes: n("estimated_minutes")?,
        status: status
            .parse::<LessonStatus>()
            .map_err(|_| corrupt("lesson status", &id))?,
        catalog_version: n("catalog_version")?,
        mission_version: n("mission_version")?,
        created_at: parse_ts(&created)?,
        completed_at: completed.as_deref().map(parse_ts).transpose()?,
    })
}

#[derive(Clone)]
pub struct SqliteLessonRepository {
    pool: SqlitePool,
}

impl SqliteLessonRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

pub(super) async fn fetch_lesson(
    conn: &mut SqliteConnection,
    id: LessonId,
) -> Result<Option<Lesson>, RepositoryError> {
    let row = sqlx::query(concat!(lesson_select!(), " WHERE id = ?"))
        .bind(id.to_string())
        .fetch_optional(conn)
        .await
        .map_err(storage)?;
    row.as_ref().map(lesson_from_row).transpose()
}

#[async_trait]
impl LessonRepository for SqliteLessonRepository {
    async fn insert_generated(&self, new: &GeneratedLesson) -> Result<Lesson, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // The first statement is a write (the number comes from a subquery),
        // so the transaction holds the lock before it reads anything.
        sqlx::query(
            "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, \
             catalog_version, mission_version, document, plan, generation_job_id, created_at, \
             quality_warnings) \
             VALUES (?, ?, (SELECT COALESCE(MAX(number), 0) + 1 FROM lessons WHERE topic_id = ?), \
             ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new.id.to_string())
        .bind(new.topic_id.to_string())
        .bind(new.topic_id.to_string())
        .bind(&new.meta.title)
        .bind(&new.meta.objective)
        .bind(to_i64(new.meta.estimated_minutes))
        .bind(to_i64(new.meta.catalog_version))
        .bind(to_i64(new.mission_version))
        .bind(json_text(&new.document))
        .bind(json_text(&new.plan))
        .bind(new.job_id.to_string())
        .bind(format_ts(new.created_at))
        .bind(warnings_json(&new.quality_warnings))
        .execute(&mut *tx)
        .await
        .map_err(as_conflict)?;
        for request in &new.component_requests {
            sqlx::query(
                "INSERT INTO component_requests (id, topic_id, lesson_id, block_id, wanted, \
                 purpose, fallback, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(request.id.to_string())
            .bind(request.topic_id.to_string())
            .bind(request.lesson_id.to_string())
            .bind(&request.block_id)
            .bind(&request.wanted)
            .bind(&request.purpose)
            .bind(json_text(&request.fallback))
            .bind(format_ts(request.created_at))
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
        }
        let lesson = fetch_lesson(&mut tx, new.id)
            .await?
            .ok_or_else(|| corrupt("lesson", &new.id.to_string()))?;
        tx.commit().await.map_err(storage)?;
        Ok(lesson)
    }

    async fn by_generation_job(&self, job_id: JobId) -> Result<Option<Lesson>, RepositoryError> {
        let row = sqlx::query(concat!(lesson_select!(), " WHERE generation_job_id = ?"))
            .bind(job_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        row.as_ref().map(lesson_from_row).transpose()
    }

    async fn exists(&self, user: UserId, id: LessonId) -> Result<bool, RepositoryError> {
        let row = sqlx::query(
            "SELECT 1 FROM lessons WHERE id = ? \
             AND topic_id IN (SELECT id FROM topics WHERE user_id = ?)",
        )
        .bind(id.to_string())
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(row.is_some())
    }

    async fn get(
        &self,
        user: UserId,
        id: LessonId,
    ) -> Result<Option<StoredLesson>, RepositoryError> {
        let row = sqlx::query(
            "SELECT id, topic_id, number, title, objective, estimated_minutes, status, \
             catalog_version, mission_version, created_at, completed_at, quality_warnings, document, plan \
             FROM lessons WHERE id = ? \
             AND topic_id IN (SELECT id FROM topics WHERE user_id = ?)",
        )
        .bind(id.to_string())
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let Some(row) = row else { return Ok(None) };
        let lesson = lesson_from_row(&row)?;
        let document: String = row.try_get("document").map_err(storage)?;
        let plan: String = row.try_get("plan").map_err(storage)?;
        let row_id = id.to_string();
        Ok(Some(StoredLesson {
            lesson,
            document: json_parse(&document, "lesson document", &row_id)?,
            plan: json_parse(&plan, "lesson plan", &row_id)?,
        }))
    }

    async fn list(
        &self,
        topic_id: TopicId,
        after: Option<LessonCursor>,
        limit: u32,
    ) -> Result<Vec<Lesson>, RepositoryError> {
        let rows = sqlx::query(concat!(
            lesson_select!(),
            " WHERE topic_id = ? AND (? IS NULL OR number < ?) ORDER BY number DESC LIMIT ?"
        ))
        .bind(topic_id.to_string())
        .bind(after.map(|c| to_i64(c.number)))
        .bind(after.map(|c| to_i64(c.number)))
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(lesson_from_row).collect()
    }

    async fn list_in_progress(
        &self,
        user: UserId,
        after: Option<InProgressCursor>,
        limit: u32,
    ) -> Result<Vec<InProgressLesson>, RepositoryError> {
        // Timestamps are fixed-width UTC text, so string order is time order.
        // A lesson without attempts (not expected) falls back to its creation.
        let cursor_at = after.map(|c| format_ts(c.last_activity_at));
        let cursor_id = after.map(|c| c.id.to_string());
        let rows = sqlx::query(
            "SELECT * FROM (\
               SELECT l.id, l.topic_id, l.number, l.title, l.objective, l.estimated_minutes, \
                      l.status, l.catalog_version, l.mission_version, l.created_at, \
                      l.completed_at, l.quality_warnings, l.plan, t.title AS topic_title, \
                      COALESCE((SELECT MAX(a.created_at) FROM attempts a WHERE a.lesson_id = l.id), \
                               l.created_at) AS last_activity_at \
               FROM lessons l JOIN topics t ON t.id = l.topic_id \
               WHERE t.user_id = ? AND l.status = 'in_progress') \
             WHERE (? IS NULL OR last_activity_at < ? OR (last_activity_at = ? AND id > ?)) \
             ORDER BY last_activity_at DESC, id LIMIT ?",
        )
        .bind(user.to_string())
        .bind(&cursor_at)
        .bind(&cursor_at)
        .bind(&cursor_at)
        .bind(&cursor_id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|row| {
                let id: String = row.try_get("id").map_err(storage)?;
                let plan: String = row.try_get("plan").map_err(storage)?;
                let at: String = row.try_get("last_activity_at").map_err(storage)?;
                Ok(InProgressLesson {
                    lesson: lesson_from_row(row)?,
                    topic_title: row.try_get("topic_title").map_err(storage)?,
                    last_activity_at: parse_ts(&at)?,
                    plan: json_parse(&plan, "lesson plan", &id)?,
                })
            })
            .collect()
    }

    async fn all_in_topic(&self, topic_id: TopicId) -> Result<Vec<Lesson>, RepositoryError> {
        let rows = sqlx::query(concat!(
            lesson_select!(),
            " WHERE topic_id = ? ORDER BY number"
        ))
        .bind(topic_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(lesson_from_row).collect()
    }

    async fn plans(&self, topic_id: TopicId) -> Result<Vec<(LessonId, Value)>, RepositoryError> {
        let rows = sqlx::query("SELECT id, plan FROM lessons WHERE topic_id = ? ORDER BY number")
            .bind(topic_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?;
        rows.iter()
            .map(|row| {
                let id: String = row.try_get("id").map_err(storage)?;
                let plan: String = row.try_get("plan").map_err(storage)?;
                Ok((
                    LessonId::new(parse_id(&id)?),
                    serde_json::from_str(&plan).map_err(|_| corrupt("lesson plan", &id))?,
                ))
            })
            .collect()
    }

    async fn complete(
        &self,
        id: LessonId,
        at: OffsetDateTime,
        job: &Job,
    ) -> Result<Lesson, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let changed = sqlx::query(
            "UPDATE lessons SET status = 'completed', completed_at = ? \
             WHERE id = ? AND status != 'completed'",
        )
        .bind(format_ts(at))
        .bind(id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(storage)?
        .rows_affected();
        if changed == 0 {
            return Err(match fetch_lesson(&mut tx, id).await? {
                Some(_) => RepositoryError::Conflict,
                None => RepositoryError::NotFound,
            });
        }
        insert_job(&mut tx, job).await?;
        let lesson = fetch_lesson(&mut tx, id)
            .await?
            .ok_or(RepositoryError::NotFound)?;
        tx.commit().await.map_err(storage)?;
        Ok(lesson)
    }

    async fn component_requests(
        &self,
        lesson_id: LessonId,
    ) -> Result<Vec<ComponentRequest>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, topic_id, lesson_id, block_id, wanted, purpose, fallback, created_at \
             FROM component_requests WHERE lesson_id = ? ORDER BY created_at, id",
        )
        .bind(lesson_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|row| {
                let id: String = row.try_get("id").map_err(storage)?;
                let topic: String = row.try_get("topic_id").map_err(storage)?;
                let lesson: String = row.try_get("lesson_id").map_err(storage)?;
                let fallback: String = row.try_get("fallback").map_err(storage)?;
                let created: String = row.try_get("created_at").map_err(storage)?;
                Ok(ComponentRequest {
                    id: ComponentRequestId::new(parse_id(&id)?),
                    topic_id: TopicId::new(parse_id(&topic)?),
                    lesson_id: LessonId::new(parse_id(&lesson)?),
                    block_id: row.try_get("block_id").map_err(storage)?,
                    wanted: row.try_get("wanted").map_err(storage)?,
                    purpose: row.try_get("purpose").map_err(storage)?,
                    fallback: json_parse(&fallback, "component fallback", &id)?,
                    created_at: parse_ts(&created)?,
                })
            })
            .collect()
    }

    async fn component_request_groups(
        &self,
        user: UserId,
        after: Option<ComponentRequestCursor>,
        limit: u32,
    ) -> Result<Vec<ComponentRequestGroup>, RepositoryError> {
        // Timestamps are fixed-width UTC text, so text order is time order.
        // The cursor columns are NULL on the first page.
        let (cursor_count, cursor_last, cursor_wanted) = match &after {
            Some(c) => (
                Some(to_i64(c.count)),
                Some(format_ts(c.last_requested_at)),
                Some(c.wanted.clone()),
            ),
            None => (None, None, None),
        };
        let rows = sqlx::query(
            "SELECT r.wanted AS wanted, COUNT(*) AS cnt, MIN(r.created_at) AS first_at, \
                    MAX(r.created_at) AS last_at, MAX(l.catalog_version) AS catalog_version \
             FROM component_requests r JOIN lessons l ON l.id = r.lesson_id \
             JOIN topics ut ON ut.id = r.topic_id AND ut.user_id = ?5 \
             GROUP BY r.wanted \
             HAVING ?1 IS NULL OR cnt < ?1 \
                 OR (cnt = ?1 AND last_at < ?2) \
                 OR (cnt = ?1 AND last_at = ?2 AND wanted > ?3) \
             ORDER BY cnt DESC, last_at DESC, wanted ASC LIMIT ?4",
        )
        .bind(cursor_count)
        .bind(cursor_last)
        .bind(cursor_wanted)
        .bind(i64::from(limit))
        .bind(user.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let mut groups = Vec::with_capacity(rows.len());
        for row in &rows {
            let wanted: String = row.try_get("wanted").map_err(storage)?;
            let n: i64 = row.try_get("cnt").map_err(storage)?;
            let first: String = row.try_get("first_at").map_err(storage)?;
            let last: String = row.try_get("last_at").map_err(storage)?;
            let version: i64 = row.try_get("catalog_version").map_err(storage)?;
            let examples = sqlx::query(
                "SELECT r.purpose AS purpose, r.lesson_id AS lesson_id, l.title AS lesson_title, \
                        t.title AS topic_title, r.created_at AS created_at \
                 FROM component_requests r \
                 JOIN lessons l ON l.id = r.lesson_id JOIN topics t ON t.id = r.topic_id \
                 WHERE r.wanted = ? AND t.user_id = ? \
                 ORDER BY r.created_at DESC, r.id DESC LIMIT ?",
            )
            .bind(&wanted)
            .bind(user.to_string())
            .bind(i64::try_from(COMPONENT_REQUEST_EXAMPLES).unwrap_or(3))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?
            .iter()
            .map(|e| {
                let lesson: String = e.try_get("lesson_id").map_err(storage)?;
                let at: String = e.try_get("created_at").map_err(storage)?;
                Ok(ComponentRequestExample {
                    purpose: e.try_get("purpose").map_err(storage)?,
                    lesson_id: LessonId::new(parse_id(&lesson)?),
                    lesson_title: e.try_get("lesson_title").map_err(storage)?,
                    topic_title: e.try_get("topic_title").map_err(storage)?,
                    requested_at: parse_ts(&at)?,
                })
            })
            .collect::<Result<Vec<_>, RepositoryError>>()?;
            groups.push(ComponentRequestGroup {
                count: count(n, "component request count", &wanted)?,
                first_requested_at: parse_ts(&first)?,
                last_requested_at: parse_ts(&last)?,
                catalog_version: count(version, "catalog version", &wanted)?,
                wanted,
                examples,
            });
        }
        Ok(groups)
    }
}

// --------------------------------------------------------------- attempts
