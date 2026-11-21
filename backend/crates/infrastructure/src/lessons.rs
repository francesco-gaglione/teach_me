//! SQLite adapters for lessons, attempts and what a completion leaves behind
//! (learning records, glossary, review cards).

use async_trait::async_trait;
use domain::{
    Answer, Attempt, AttemptId, AttemptRepository, COMPONENT_REQUEST_EXAMPLES, CardOrigin,
    CardSchedule, CardStatus, CardTarget, CompletionSave, CompletionSummary, ComponentRequest,
    ComponentRequestCursor, ComponentRequestExample, ComponentRequestGroup, ComponentRequestId,
    ExerciseKind, GeneratedLesson, GlossaryTerm, GlossaryTermId, Grade, Highlight, HighlightColor,
    HighlightRepository, InProgressCursor, InProgressLesson, Job, JobId, LearningRecord,
    LearningRecordId, LearningRepository, Lesson, LessonCursor, LessonId, LessonOutputs,
    LessonRepository, LessonStatus, Outcome, Rating, RecordCursor, RecordKind, RecordStatus,
    RepositoryError, ReviewCard, ReviewCardId, StoredLesson, Stroke, StrokeColor, StrokeRepository,
    StrokeWidth, Suspension, SuspensionReason, TermCursor, TopicId, UserId, Verdict,
    is_to_consolidate, term_sort_key,
};
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use crate::jobs::insert_job;
use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};

fn json_text(value: &Value) -> String {
    value.to_string()
}

fn json_parse(raw: &str, what: &str, row_id: &str) -> Result<Value, RepositoryError> {
    serde_json::from_str(raw).map_err(|_| corrupt(what, row_id))
}

fn warnings_json(warnings: &[domain::LessonQualityWarning]) -> String {
    Value::Array(
        warnings
            .iter()
            .map(|w| serde_json::json!({"code": w.code, "block_id": w.block_id, "message": w.message}))
            .collect(),
    )
    .to_string()
}

fn warnings_parse(
    raw: &str,
    row_id: &str,
) -> Result<Vec<domain::LessonQualityWarning>, RepositoryError> {
    let value = json_parse(raw, "lesson quality warnings", row_id)?;
    let items = value
        .as_array()
        .ok_or_else(|| corrupt("lesson quality warnings", row_id))?;
    items
        .iter()
        .map(|w| {
            (|| {
                Some(domain::LessonQualityWarning {
                    code: w.get("code")?.as_str()?.to_owned(),
                    block_id: w.get("block_id").and_then(Value::as_str).map(str::to_owned),
                    message: w.get("message")?.as_str()?.to_owned(),
                })
            })()
            .ok_or_else(|| corrupt("lesson quality warnings", row_id))
        })
        .collect()
}

fn count(n: i64, what: &str, row_id: &str) -> Result<u32, RepositoryError> {
    u32::try_from(n).map_err(|_| corrupt(what, row_id))
}

fn to_i64(n: u32) -> i64 {
    i64::from(n)
}

/// A unique violation where the port says `Conflict`.
fn as_conflict(e: sqlx::Error) -> RepositoryError {
    match constraint(e) {
        RepositoryError::Duplicate => RepositoryError::Conflict,
        other => other,
    }
}

// ---------------------------------------------------------------- lessons

macro_rules! lesson_select {
    () => {
        "SELECT id, topic_id, number, title, objective, estimated_minutes, status, \
         catalog_version, mission_version, created_at, completed_at, quality_warnings FROM lessons"
    };
}

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

async fn fetch_lesson(
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

macro_rules! attempt_columns {
    () => {
        "id, lesson_id, block_id, kind, attempt_number, answer, outcome, \
     grade_verdict, grade_feedback, grade_misconception, answered_at, created_at"
    };
}

fn attempt_from_row(row: &SqliteRow) -> Result<Attempt, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let lesson: String = row.try_get("lesson_id").map_err(storage)?;
    let kind: String = row.try_get("kind").map_err(storage)?;
    let kind = kind
        .parse::<ExerciseKind>()
        .map_err(|_| corrupt("attempt kind", &id))?;
    let number: i64 = row.try_get("attempt_number").map_err(storage)?;
    let answer: String = row.try_get("answer").map_err(storage)?;
    let outcome: String = row.try_get("outcome").map_err(storage)?;
    let verdict: Option<String> = row.try_get("grade_verdict").map_err(storage)?;
    let feedback: Option<String> = row.try_get("grade_feedback").map_err(storage)?;
    let misconception: Option<String> = row.try_get("grade_misconception").map_err(storage)?;
    let answered: String = row.try_get("answered_at").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let grade = match (verdict, feedback) {
        (Some(v), Some(feedback)) => Some(Grade {
            verdict: v
                .parse::<Verdict>()
                .map_err(|_| corrupt("attempt verdict", &id))?,
            feedback,
            misconception,
        }),
        _ => None,
    };
    Ok(Attempt {
        lesson_id: LessonId::new(parse_id(&lesson)?),
        block_id: row.try_get("block_id").map_err(storage)?,
        kind,
        attempt_number: u8::try_from(number).map_err(|_| corrupt("attempt number", &id))?,
        answer: Answer::from_json(kind, &json_parse(&answer, "attempt answer", &id)?)
            .ok_or_else(|| corrupt("attempt answer", &id))?,
        outcome: outcome
            .parse::<Outcome>()
            .map_err(|_| corrupt("attempt outcome", &id))?,
        grade,
        answered_at: parse_ts(&answered)?,
        created_at: parse_ts(&created)?,
        id: AttemptId::new(parse_id(&id)?),
    })
}

#[derive(Clone)]
pub struct SqliteAttemptRepository {
    pool: SqlitePool,
}

impl SqliteAttemptRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AttemptRepository for SqliteAttemptRepository {
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Attempt>, RepositoryError> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            attempt_columns!(),
            " FROM attempts WHERE lesson_id = ? ORDER BY created_at, id"
        ))
        .bind(lesson_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(attempt_from_row).collect()
    }

    async fn record(
        &self,
        lesson_id: LessonId,
        attempts: &[Attempt],
    ) -> Result<Vec<Attempt>, RepositoryError> {
        let lesson = lesson_id.to_string();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Write first (lock), and refuse a completed lesson in the same step.
        let moved = sqlx::query(
            "UPDATE lessons SET status = CASE WHEN status = 'not_started' THEN 'in_progress' \
             ELSE status END WHERE id = ? AND status != 'completed'",
        )
        .bind(&lesson)
        .execute(&mut *tx)
        .await
        .map_err(storage)?
        .rows_affected();
        if moved == 0 {
            return Err(match fetch_lesson(&mut tx, lesson_id).await? {
                Some(_) => RepositoryError::Conflict,
                None => RepositoryError::NotFound,
            });
        }
        let mut stored = Vec::with_capacity(attempts.len());
        for a in attempts {
            if a.attempt_number == 2 {
                // Read in this transaction: the first attempt may have been
                // stored, or answered correctly, since the caller looked.
                let taken: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM attempts WHERE lesson_id = ? AND block_id = ? AND attempt_number = 2",
                )
                .bind(&lesson)
                .bind(&a.block_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
                if taken.is_none() {
                    let first: Option<String> = sqlx::query_scalar(
                        "SELECT outcome FROM attempts WHERE lesson_id = ? AND block_id = ? AND attempt_number = 1",
                    )
                    .bind(&lesson)
                    .bind(&a.block_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(storage)?;
                    if first.is_none_or(|o| o == "correct") {
                        return Err(RepositoryError::RuleViolated);
                    }
                }
            }
            sqlx::query(
                "INSERT INTO attempts (id, lesson_id, block_id, kind, attempt_number, answer, \
                 outcome, grade_verdict, grade_feedback, grade_misconception, answered_at, \
                 created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT (lesson_id, block_id, attempt_number) DO NOTHING",
            )
            .bind(a.id.to_string())
            .bind(&lesson)
            .bind(&a.block_id)
            .bind(a.kind.as_str())
            .bind(i64::from(a.attempt_number))
            .bind(json_text(&a.answer.to_json()))
            .bind(a.outcome.as_str())
            .bind(a.grade.as_ref().map(|g| g.verdict.as_str()))
            .bind(a.grade.as_ref().map(|g| g.feedback.as_str()))
            .bind(a.grade.as_ref().and_then(|g| g.misconception.as_deref()))
            .bind(format_ts(a.answered_at))
            .bind(format_ts(a.created_at))
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
            let row = sqlx::query(concat!(
                "SELECT ",
                attempt_columns!(),
                " FROM attempts \
                 WHERE lesson_id = ? AND block_id = ? AND attempt_number = ?"
            ))
            .bind(&lesson)
            .bind(&a.block_id)
            .bind(i64::from(a.attempt_number))
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
            stored.push(attempt_from_row(&row)?);
        }
        tx.commit().await.map_err(storage)?;
        Ok(stored)
    }

    async fn set_grade(&self, id: AttemptId, grade: &Grade) -> Result<Attempt, RepositoryError> {
        sqlx::query(
            "UPDATE attempts SET outcome = ?, grade_verdict = ?, grade_feedback = ?, \
             grade_misconception = ? WHERE id = ? AND outcome = 'pending'",
        )
        .bind(grade.verdict.outcome().as_str())
        .bind(grade.verdict.as_str())
        .bind(&grade.feedback)
        .bind(grade.misconception.as_deref())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        let row = sqlx::query(concat!(
            "SELECT ",
            attempt_columns!(),
            " FROM attempts WHERE id = ?"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?
        .ok_or(RepositoryError::NotFound)?;
        attempt_from_row(&row)
    }
}

// ------------------------------------------------------------ highlights

#[derive(Clone)]
pub struct SqliteHighlightRepository {
    pool: SqlitePool,
}

impl SqliteHighlightRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn highlight_from_row(row: &SqliteRow, lesson: &str) -> Result<Highlight, RepositoryError> {
    let block_id: String = row.try_get("block_id").map_err(storage)?;
    let start: i64 = row.try_get("range_start").map_err(storage)?;
    let end: i64 = row.try_get("range_end").map_err(storage)?;
    let color: String = row.try_get("color").map_err(storage)?;
    Ok(Highlight {
        block_id,
        start: count(start, "highlight start", lesson)?,
        end: count(end, "highlight end", lesson)?,
        color: color
            .parse::<HighlightColor>()
            .map_err(|_| corrupt("highlight color", lesson))?,
    })
}

#[async_trait]
impl HighlightRepository for SqliteHighlightRepository {
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Highlight>, RepositoryError> {
        let lesson = lesson_id.to_string();
        let rows = sqlx::query(
            "SELECT block_id, range_start, range_end, color FROM highlights \
             WHERE lesson_id = ? ORDER BY position",
        )
        .bind(&lesson)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|r| highlight_from_row(r, &lesson))
            .collect()
    }

    async fn replace(
        &self,
        lesson_id: LessonId,
        highlights: &[Highlight],
    ) -> Result<(), RepositoryError> {
        let lesson = lesson_id.to_string();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // The first statement is a write, so the transaction takes the write
        // lock up front: a read first would fail with SQLITE_BUSY_SNAPSHOT
        // when two replaces race, instead of "the last one wins".
        let touched = sqlx::query("UPDATE lessons SET id = id WHERE id = ?")
            .bind(&lesson)
            .execute(&mut *tx)
            .await
            .map_err(storage)?
            .rows_affected();
        if touched == 0 {
            return Err(RepositoryError::NotFound);
        }
        sqlx::query("DELETE FROM highlights WHERE lesson_id = ?")
            .bind(&lesson)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        for (position, h) in highlights.iter().enumerate() {
            sqlx::query(
                "INSERT INTO highlights (lesson_id, position, block_id, range_start, range_end, color) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&lesson)
            .bind(i64::try_from(position).map_err(|_| corrupt("highlight position", &lesson))?)
            .bind(&h.block_id)
            .bind(to_i64(h.start))
            .bind(to_i64(h.end))
            .bind(h.color.as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        }
        tx.commit().await.map_err(storage)
    }
}

// --------------------------------------------------------------- strokes

#[derive(Clone)]
pub struct SqliteStrokeRepository {
    pool: SqlitePool,
}

impl SqliteStrokeRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// The points as the packed little-endian `i32` the `points` column holds.
fn pack_points(points: &[i32]) -> Vec<u8> {
    points.iter().flat_map(|p| p.to_le_bytes()).collect()
}

fn unpack_points(blob: &[u8], lesson: &str) -> Result<Vec<i32>, RepositoryError> {
    if blob.is_empty() || !blob.len().is_multiple_of(8) {
        return Err(corrupt("stroke points", lesson));
    }
    Ok(blob
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| i32::from_le_bytes(*c))
        .collect())
}

fn stroke_from_row(row: &SqliteRow, lesson: &str) -> Result<Stroke, RepositoryError> {
    let color: String = row.try_get("color").map_err(storage)?;
    let width: String = row.try_get("width").map_err(storage)?;
    let points: Vec<u8> = row.try_get("points").map_err(storage)?;
    Ok(Stroke {
        id: row.try_get("id").map_err(storage)?,
        block_id: row.try_get("block_id").map_err(storage)?,
        color: color
            .parse::<StrokeColor>()
            .map_err(|_| corrupt("stroke color", lesson))?,
        width: width
            .parse::<StrokeWidth>()
            .map_err(|_| corrupt("stroke width", lesson))?,
        points: unpack_points(&points, lesson)?,
    })
}

#[async_trait]
impl StrokeRepository for SqliteStrokeRepository {
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Stroke>, RepositoryError> {
        let lesson = lesson_id.to_string();
        let rows = sqlx::query(
            "SELECT id, block_id, color, width, points FROM strokes \
             WHERE lesson_id = ? ORDER BY position",
        )
        .bind(&lesson)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(|r| stroke_from_row(r, &lesson)).collect()
    }

    async fn replace(
        &self,
        lesson_id: LessonId,
        strokes: &[Stroke],
    ) -> Result<(), RepositoryError> {
        let lesson = lesson_id.to_string();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Write first, as in `SqliteHighlightRepository::replace`: racing
        // replaces then serialize ("the last one wins") instead of failing.
        let touched = sqlx::query("UPDATE lessons SET id = id WHERE id = ?")
            .bind(&lesson)
            .execute(&mut *tx)
            .await
            .map_err(storage)?
            .rows_affected();
        if touched == 0 {
            return Err(RepositoryError::NotFound);
        }
        sqlx::query("DELETE FROM strokes WHERE lesson_id = ?")
            .bind(&lesson)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        for (position, s) in strokes.iter().enumerate() {
            sqlx::query(
                "INSERT INTO strokes (lesson_id, position, id, block_id, color, width, points) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&lesson)
            .bind(i64::try_from(position).map_err(|_| corrupt("stroke position", &lesson))?)
            .bind(&s.id)
            .bind(&s.block_id)
            .bind(s.color.as_str())
            .bind(s.width.as_str())
            .bind(pack_points(&s.points))
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        }
        tx.commit().await.map_err(storage)
    }
}

// -------------------------------------------------------------- learning

macro_rules! record_columns {
    () => {
        "id, topic_id, number, kind, title, body, evidence, status, \
     superseded_by, lesson_id, created_at"
    };
}

fn record_from_row(row: &SqliteRow) -> Result<LearningRecord, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic: String = row.try_get("topic_id").map_err(storage)?;
    let number: i64 = row.try_get("number").map_err(storage)?;
    let kind: String = row.try_get("kind").map_err(storage)?;
    let status: String = row.try_get("status").map_err(storage)?;
    let by: Option<String> = row.try_get("superseded_by").map_err(storage)?;
    let lesson: String = row.try_get("lesson_id").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    Ok(LearningRecord {
        number: count(number, "record number", &id)?,
        kind: kind
            .parse::<RecordKind>()
            .map_err(|_| corrupt("record kind", &id))?,
        title: row.try_get("title").map_err(storage)?,
        body: row.try_get("body").map_err(storage)?,
        evidence: row.try_get("evidence").map_err(storage)?,
        status: status
            .parse::<RecordStatus>()
            .map_err(|_| corrupt("record status", &id))?,
        superseded_by: by
            .as_deref()
            .map(|b| parse_id(b).map(LearningRecordId::new))
            .transpose()?,
        lesson_id: LessonId::new(parse_id(&lesson)?),
        created_at: parse_ts(&created)?,
        topic_id: TopicId::new(parse_id(&topic)?),
        id: LearningRecordId::new(parse_id(&id)?),
    })
}

macro_rules! term_columns {
    () => {
        "id, topic_id, key, term, definition, avoid, lesson_id, first_lesson_id, created_at, \
        updated_at"
    };
}

fn term_from_row(row: &SqliteRow) -> Result<GlossaryTerm, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic: String = row.try_get("topic_id").map_err(storage)?;
    let avoid: String = row.try_get("avoid").map_err(storage)?;
    let lesson: String = row.try_get("lesson_id").map_err(storage)?;
    let first: Option<String> = row.try_get("first_lesson_id").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let updated: String = row.try_get("updated_at").map_err(storage)?;
    let avoid: Vec<String> =
        serde_json::from_str(&avoid).map_err(|_| corrupt("term avoid", &id))?;
    Ok(GlossaryTerm {
        key: row.try_get("key").map_err(storage)?,
        term: row.try_get("term").map_err(storage)?,
        definition: row.try_get("definition").map_err(storage)?,
        avoid,
        lesson_id: LessonId::new(parse_id(&lesson)?),
        first_lesson_id: first.map(|f| parse_id(&f).map(LessonId::new)).transpose()?,
        created_at: parse_ts(&created)?,
        updated_at: parse_ts(&updated)?,
        topic_id: TopicId::new(parse_id(&topic)?),
        id: GlossaryTermId::new(parse_id(&id)?),
    })
}

/// The ratings `card_from_row` needs to tell whether the card is still "to
/// consolidate": the votes not undone, in application order, as one
/// comma-separated column, fetched with the card to avoid a query per card.
/// Only for the cards born unpassed: the others never read it. `$card` is the
/// table or alias the card is selected from.
macro_rules! consolidation_ratings {
    ($card:literal) => {
        concat!(
            "CASE WHEN ",
            $card,
            ".unpassed_block IS NULL THEN NULL ELSE (SELECT group_concat(rating, ',' \
             ORDER BY created_at, rowid) FROM review_logs WHERE card_id = ",
            $card,
            ".id AND undone_at IS NULL) END AS ratings"
        )
    };
}
pub(crate) use consolidation_ratings;

/// Columns `card_from_row` reads.
macro_rules! card_columns {
    () => {
        concat!(
        "id, topic_id, lesson_id, record_id, term_id, front, back, status, suspension, \
         suspended_at, created_at, state, stability, difficulty, stability_fast, learning_step, due, last_reviewed_at, \
         review_count, lapses, unpassed_block, ",
        consolidation_ratings!("review_cards")
        )
    };
}

/// The suspension columns, checked against `status`, which is the source of
/// truth: an active card has neither, a suspended one has both. Suspended
/// cards without a reason are repaired by `backfill_suspensions` (at startup
/// and before the suspension use cases), so here they are plain corruption.
fn suspension_from_row(
    row: &SqliteRow,
    id: &str,
    status: CardStatus,
) -> Result<Option<Suspension>, RepositoryError> {
    let reason: Option<String> = row.try_get("suspension").map_err(storage)?;
    let at: Option<String> = row.try_get("suspended_at").map_err(storage)?;
    match (status, reason, at) {
        (CardStatus::Active, None, None) => Ok(None),
        (CardStatus::Suspended, Some(reason), Some(at)) => Ok(Some(Suspension {
            reason: reason
                .parse::<SuspensionReason>()
                .map_err(|_| corrupt("card suspension", id))?,
            at: parse_ts(&at)?,
        })),
        _ => Err(corrupt("card suspension", id)),
    }
}

pub(crate) fn card_from_row(row: &SqliteRow) -> Result<ReviewCard, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic: String = row.try_get("topic_id").map_err(storage)?;
    let lesson: String = row.try_get("lesson_id").map_err(storage)?;
    let record: Option<String> = row.try_get("record_id").map_err(storage)?;
    let term: Option<String> = row.try_get("term_id").map_err(storage)?;
    let status: String = row.try_get("status").map_err(storage)?;
    let status = status
        .parse::<CardStatus>()
        .map_err(|_| corrupt("card status", &id))?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let origin = match (record, term) {
        (Some(r), None) => CardOrigin::Record(LearningRecordId::new(parse_id(&r)?)),
        (None, Some(t)) => CardOrigin::Term(GlossaryTermId::new(parse_id(&t)?)),
        _ => return Err(corrupt("card origin", &id)),
    };
    let unpassed: Option<String> = row.try_get("unpassed_block").map_err(storage)?;
    let ratings: Option<String> = row.try_get("ratings").map_err(storage)?;
    let to_consolidate = is_to_consolidate(
        unpassed.is_some(),
        &ratings
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .filter(|r| !r.is_empty())
            .map(|r| {
                r.parse::<Rating>()
                    .map_err(|_| corrupt("review rating", &id))
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    Ok(ReviewCard {
        to_consolidate,
        topic_id: TopicId::new(parse_id(&topic)?),
        lesson_id: LessonId::new(parse_id(&lesson)?),
        origin,
        front: row.try_get("front").map_err(storage)?,
        back: row.try_get("back").map_err(storage)?,
        status,
        suspension: suspension_from_row(row, &id, status)?,
        schedule: crate::reviews::schedule_from_row(row, &id)?,
        created_at: parse_ts(&created)?,
        id: ReviewCardId::new(parse_id(&id)?),
    })
}

#[derive(Clone)]
pub struct SqliteLearningRepository {
    pool: SqlitePool,
}

impl SqliteLearningRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LearningRepository for SqliteLearningRepository {
    async fn list_records(
        &self,
        topic_id: TopicId,
        active_only: bool,
        after: Option<RecordCursor>,
        limit: u32,
    ) -> Result<Vec<LearningRecord>, RepositoryError> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            record_columns!(),
            " FROM learning_records WHERE topic_id = ? \
             AND (? = 0 OR status = 'active') AND (? IS NULL OR number < ?) \
             ORDER BY number DESC LIMIT ?"
        ))
        .bind(topic_id.to_string())
        .bind(i64::from(active_only))
        .bind(after.map(|c| to_i64(c.number)))
        .bind(after.map(|c| to_i64(c.number)))
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(record_from_row).collect()
    }

    async fn list_terms(
        &self,
        topic_id: TopicId,
        after: Option<TermCursor>,
        limit: u32,
    ) -> Result<Vec<GlossaryTerm>, RepositoryError> {
        let sort = after.as_ref().map(|c| c.sort_key.clone());
        let key = after.as_ref().map(|c| c.key.clone());
        let rows = sqlx::query(concat!(
            "SELECT ",
            term_columns!(),
            " FROM glossary_terms WHERE topic_id = ? \
             AND (? IS NULL OR term_key > ? OR (term_key = ? AND key > ?)) \
             ORDER BY term_key, key LIMIT ?"
        ))
        .bind(topic_id.to_string())
        .bind(&sort)
        .bind(&sort)
        .bind(&sort)
        .bind(&key)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(term_from_row).collect()
    }

    async fn lesson_outputs(&self, lesson_id: LessonId) -> Result<LessonOutputs, RepositoryError> {
        let lesson = lesson_id.to_string();
        let records = sqlx::query(concat!(
            "SELECT ",
            record_columns!(),
            " FROM learning_records WHERE lesson_id = ? ORDER BY number"
        ))
        .bind(&lesson)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let terms = sqlx::query(concat!(
            "SELECT ",
            term_columns!(),
            " FROM glossary_terms WHERE lesson_id = ? ORDER BY term_key, key"
        ))
        .bind(&lesson)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let cards: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM review_cards WHERE lesson_id = ?")
                .bind(&lesson)
                .fetch_one(&self.pool)
                .await
                .map_err(storage)?;
        Ok(LessonOutputs {
            records: records
                .iter()
                .map(record_from_row)
                .collect::<Result<_, _>>()?,
            terms: terms.iter().map(term_from_row).collect::<Result<_, _>>()?,
            review_card_count: count(cards, "card count", &lesson)?,
        })
    }

    async fn review_cards(&self, topic_id: TopicId) -> Result<Vec<ReviewCard>, RepositoryError> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            card_columns!(),
            " FROM review_cards WHERE topic_id = ? ORDER BY created_at, id"
        ))
        .bind(topic_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(card_from_row).collect()
    }

    #[allow(clippy::too_many_lines)] // one transaction, in the order the rules need
    async fn save_completion(
        &self,
        save: &CompletionSave,
    ) -> Result<CompletionSummary, RepositoryError> {
        let topic = save.topic_id.to_string();
        let lesson = save.lesson_id.to_string();
        let at = format_ts(save.at);
        let summary = CompletionSummary {
            record_count: u32::try_from(save.records.len()).unwrap_or(u32::MAX),
            term_count: u32::try_from(save.terms.len()).unwrap_or(u32::MAX),
            review_card_count: u32::try_from(save.cards.len()).unwrap_or(u32::MAX),
        };
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // First statement: a write that also settles "already recorded".
        sqlx::query(
            "INSERT INTO lesson_completions (lesson_id, record_count, term_count, \
             review_card_count, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&lesson)
        .bind(to_i64(summary.record_count))
        .bind(to_i64(summary.term_count))
        .bind(to_i64(summary.review_card_count))
        .bind(&at)
        .execute(&mut *tx)
        .await
        .map_err(as_conflict)?;

        let mut record_ids = Vec::new();
        for new in &save.records {
            sqlx::query(
                "INSERT INTO learning_records (id, topic_id, number, kind, title, body, evidence, \
                 lesson_id, created_at) VALUES (?, ?, \
                 (SELECT COALESCE(MAX(number), 0) + 1 FROM learning_records WHERE topic_id = ?), \
                 ?, ?, ?, ?, ?, ?)",
            )
            .bind(new.id.to_string())
            .bind(&topic)
            .bind(&topic)
            .bind(new.kind.as_str())
            .bind(&new.title)
            .bind(&new.body)
            .bind(&new.evidence)
            .bind(&lesson)
            .bind(&at)
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
            record_ids.push(new.id);
            for number in &new.supersedes {
                let old: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM learning_records \
                     WHERE topic_id = ? AND number = ? AND status = 'active' AND id != ?",
                )
                .bind(&topic)
                .bind(to_i64(*number))
                .bind(new.id.to_string())
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
                // A replaced record that is not active any more: the whole
                // save is refused (dropping `tx` rolls it back).
                let old = old.ok_or(RepositoryError::Conflict)?;
                sqlx::query(
                    "UPDATE learning_records SET status = 'superseded', superseded_by = ? \
                     WHERE id = ?",
                )
                .bind(new.id.to_string())
                .bind(&old)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
                // Also a card the learner had suspended: from now on it is
                // superseded and cannot be reactivated.
                sqlx::query(
                    "UPDATE review_cards SET status = 'suspended', suspension = 'superseded', \
                     suspended_at = ? WHERE record_id = ? AND suspension IS NOT 'superseded'",
                )
                .bind(&at)
                .bind(&old)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            }
        }

        let mut term_ids = Vec::new();
        for up in &save.terms {
            let existing: Option<String> =
                sqlx::query_scalar("SELECT id FROM glossary_terms WHERE topic_id = ? AND key = ?")
                    .bind(&topic)
                    .bind(&up.key)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(storage)?;
            let avoid = serde_json::to_string(&up.avoid).unwrap_or_else(|_| "[]".to_owned());
            if let Some(id) = existing {
                // Revised in place: same row, same id, so its cards stay linked.
                sqlx::query(
                    "UPDATE glossary_terms SET term = ?, term_key = ?, definition = ?, avoid = ?, \
                     lesson_id = ?, updated_at = ? WHERE id = ?",
                )
                .bind(&up.term)
                .bind(term_sort_key(&up.term))
                .bind(&up.definition)
                .bind(&avoid)
                .bind(&lesson)
                .bind(&at)
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
                term_ids.push(GlossaryTermId::new(parse_id(&id)?));
            } else {
                sqlx::query(
                    "INSERT INTO glossary_terms (id, topic_id, key, term, term_key, definition, \
                     avoid, lesson_id, first_lesson_id, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(up.id.to_string())
                .bind(&topic)
                .bind(&up.key)
                .bind(&up.term)
                .bind(term_sort_key(&up.term))
                .bind(&up.definition)
                .bind(&avoid)
                .bind(&lesson)
                .bind(&lesson)
                .bind(&at)
                .bind(&at)
                .execute(&mut *tx)
                .await
                .map_err(constraint)?;
                term_ids.push(up.id);
            }
        }

        for card in &save.cards {
            let (record, term) = match card.target {
                CardTarget::Record(i) => (record_ids.get(i).map(ToString::to_string), None),
                CardTarget::Term(i) => (None, term_ids.get(i).map(ToString::to_string)),
            };
            sqlx::query(
                "INSERT INTO review_cards (id, topic_id, lesson_id, record_id, term_id, front, \
                 back, created_at, due, model, unpassed_block) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'fsrs7', ?)",
            )
            .bind(card.id.to_string())
            .bind(&topic)
            .bind(&lesson)
            .bind(record)
            .bind(term)
            .bind(&card.front)
            .bind(&card.back)
            .bind(&at)
            .bind(format_ts(CardSchedule::initial(save.at).due))
            .bind(card.unpassed_block.as_deref())
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
        }
        tx.commit().await.map_err(storage)?;
        Ok(summary)
    }

    async fn completion_summary(
        &self,
        lesson_id: LessonId,
    ) -> Result<Option<CompletionSummary>, RepositoryError> {
        let row = sqlx::query(
            "SELECT record_count, term_count, review_card_count FROM lesson_completions \
             WHERE lesson_id = ?",
        )
        .bind(lesson_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let id = lesson_id.to_string();
        row.map(|row| {
            let n = |col: &str| -> Result<u32, RepositoryError> {
                let v: i64 = row.try_get(col).map_err(storage)?;
                count(v, "completion count", &id)
            };
            Ok(CompletionSummary {
                record_count: n("record_count")?,
                term_count: n("term_count")?,
                review_card_count: n("review_card_count")?,
            })
        })
        .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::SqliteJobRepository;
    use crate::repositories::{SqliteFolderRepository, SqliteTopicRepository};
    use crate::sqlite::{connect, migrate};
    use domain::{
        CardTarget, FolderId, FolderName, FolderRepository, JobKind, JobRepository, JobResult,
        JobStatus, LessonMeta, NewCard, NewFolder, NewRecord, Rating, RecordStatus, TermUpsert,
        Topic, TopicRepository, TopicTitle,
    };
    use serde_json::json;
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-09 10:00:00.5 UTC);

    struct Env {
        lessons: SqliteLessonRepository,
        attempts: SqliteAttemptRepository,
        learning: SqliteLearningRepository,
        jobs: SqliteJobRepository,
        topic: TopicId,
        pool: SqlitePool,
    }

    fn uid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    async fn env() -> Env {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let folders = SqliteFolderRepository::new(pool.clone());
        let topics = SqliteTopicRepository::new(pool.clone());
        let folder = FolderId::new(uid(1));
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
            TopicId::new(uid(2)),
            domain::UserId::OWNER,
            folder,
            TopicTitle::parse("T").unwrap(),
            T0,
        );
        topics.insert(&topic).await.unwrap();
        Env {
            lessons: SqliteLessonRepository::new(pool.clone()),
            attempts: SqliteAttemptRepository::new(pool.clone()),
            learning: SqliteLearningRepository::new(pool.clone()),
            jobs: SqliteJobRepository::new(pool.clone()),
            topic: topic.id,
            pool,
        }
    }

    fn document() -> Value {
        json!({"catalog_version": 1, "title": "Stop", "objective": "Saper compensare.",
        "estimated_minutes": 8, "blocks": [
            {"id": "q1", "type": "quiz_single", "question": "Q?", "options": [{"id": "a"}, {"id": "b"}], "correct": "a"},
            {"id": "oa", "type": "open_answer", "question": "Perché?", "rubric": "R"}
        ]})
    }

    fn generated(env: &Env, n: u128) -> GeneratedLesson {
        let id = LessonId::new(uid(0x100 + n));
        GeneratedLesson {
            quality_warnings: vec![],
            id,
            topic_id: env.topic,
            job_id: JobId::new(uid(0x200 + n)),
            meta: LessonMeta::from_document(&document()).unwrap(),
            mission_version: 2,
            document: document(),
            plan: json!({"objective": "Saper compensare."}),
            component_requests: vec![ComponentRequest {
                id: ComponentRequestId::new(uid(0x300 + n)),
                topic_id: env.topic,
                lesson_id: id,
                block_id: "mc".into(),
                wanted: "slider".into(),
                purpose: "p".into(),
                fallback: json!([{"id": "f"}]),
                created_at: T0,
            }],
            created_at: T0 + time::Duration::seconds(i64::try_from(n).unwrap()),
        }
    }

    async fn lesson(env: &Env, n: u128) -> Lesson {
        env.lessons
            .insert_generated(&generated(env, n))
            .await
            .unwrap()
    }

    /// A lesson of catalog `version` whose requests are `(wanted, purpose,
    /// seconds after T0)`.
    async fn lesson_asking(env: &Env, n: u128, version: u32, asks: &[(&str, &str, i64)]) {
        let mut new = generated(env, n);
        new.meta.catalog_version = version;
        new.component_requests = asks
            .iter()
            .enumerate()
            .map(|(i, (wanted, purpose, secs))| ComponentRequest {
                id: ComponentRequestId::new(uid(0x500 + n * 10 + i as u128)),
                topic_id: env.topic,
                lesson_id: new.id,
                block_id: format!("m{i}"),
                wanted: (*wanted).into(),
                purpose: (*purpose).into(),
                fallback: json!([]),
                created_at: T0 + time::Duration::seconds(*secs),
            })
            .collect();
        env.lessons.insert_generated(&new).await.unwrap();
    }

    #[tokio::test]
    async fn in_progress_pages_one_by_one_through_equal_activity_times() {
        let env = env().await;
        let mut ids = Vec::new();
        for n in 1..=3 {
            ids.push(lesson(&env, n).await.id);
        }
        // Same activity time for all three (no attempts: the creation time).
        sqlx::query("UPDATE lessons SET status = 'in_progress', created_at = ?")
            .bind(format_ts(T0))
            .execute(&env.pool)
            .await
            .unwrap();
        let mut seen = Vec::new();
        let mut after = None;
        loop {
            let page = env
                .lessons
                .list_in_progress(domain::UserId::OWNER, after, 1)
                .await
                .unwrap();
            let Some(item) = page.first() else { break };
            assert_eq!(page.len(), 1);
            seen.push(item.lesson.id);
            after = Some(InProgressCursor {
                last_activity_at: item.last_activity_at,
                id: item.lesson.id,
            });
        }
        ids.sort_by_key(ToString::to_string);
        assert_eq!(seen, ids, "every lesson once, ties broken by id");
    }

    #[tokio::test]
    async fn quality_warnings_are_saved_read_back_and_default_to_none() {
        let env = env().await;
        let mut new = generated(&env, 1);
        new.quality_warnings = vec![domain::LessonQualityWarning {
            code: "too_many_callouts".into(),
            block_id: Some("c1".into()),
            message: "Troppi callout.".into(),
        }];
        let saved = env.lessons.insert_generated(&new).await.unwrap();
        assert_eq!(saved.quality_warnings, new.quality_warnings);
        let plain = lesson(&env, 2).await;
        assert_eq!(plain.quality_warnings.len(), 0);
        sqlx::query("UPDATE lessons SET quality_warnings = 'oops' WHERE id = ?")
            .bind(plain.id.to_string())
            .execute(&env.pool)
            .await
            .unwrap();
        let again = fetch_lesson(&mut env.pool.acquire().await.unwrap(), plain.id).await;
        assert!(
            matches!(again, Err(RepositoryError::Storage(e)) if e.to_string().contains("corrupt lesson quality warnings"))
        );
    }

    #[tokio::test]
    async fn component_requests_are_ranked_by_count_then_recency_then_name() {
        let env = env().await;
        let none = env
            .lessons
            .component_request_groups(domain::UserId::OWNER, None, 10)
            .await
            .unwrap();
        assert_eq!(none, []);

        lesson_asking(&env, 1, 1, &[("slider", "s1", 1), ("timeline", "t1", 2)]).await;
        lesson_asking(&env, 2, 2, &[("slider", "s2", 5), ("timeline", "t2", 3)]).await;
        lesson_asking(&env, 3, 1, &[("slider", "s3", 4), ("slider", "s4", 6)]).await;
        lesson_asking(&env, 4, 1, &[("map", "m1", 3), ("globe", "g1", 3)]).await;

        let groups = env
            .lessons
            .component_request_groups(domain::UserId::OWNER, None, 10)
            .await
            .unwrap();
        let names: Vec<_> = groups.iter().map(|g| g.wanted.as_str()).collect();
        // slider 4, timeline 2, then the two single asks tied on time: by name.
        assert_eq!(names, ["slider", "timeline", "globe", "map"]);
        let slider = &groups[0];
        assert_eq!(slider.count, 4);
        assert_eq!(slider.catalog_version, 2);
        assert_eq!(slider.first_requested_at, T0 + time::Duration::seconds(1));
        assert_eq!(slider.last_requested_at, T0 + time::Duration::seconds(6));
        // Three most recent, newest first.
        let purposes: Vec<_> = slider.examples.iter().map(|e| e.purpose.as_str()).collect();
        assert_eq!(purposes, ["s4", "s2", "s3"]);
        assert_eq!(slider.examples[0].topic_title, "T");
        assert_eq!(slider.examples[0].lesson_title, "Stop");
        assert_eq!(slider.examples[0].lesson_id, LessonId::new(uid(0x103)));
    }

    #[tokio::test]
    async fn component_request_groups_page_by_cursor() {
        let env = env().await;
        lesson_asking(&env, 1, 1, &[("a", "x", 1), ("b", "x", 1), ("c", "x", 1)]).await;
        lesson_asking(&env, 2, 1, &[("c", "x", 2)]).await;

        let first = env
            .lessons
            .component_request_groups(domain::UserId::OWNER, None, 2)
            .await
            .unwrap();
        assert_eq!(first.len(), 2);
        let cursor = ComponentRequestCursor::after(&first[1]);
        let rest = env
            .lessons
            .component_request_groups(domain::UserId::OWNER, Some(cursor), 2)
            .await
            .unwrap();
        let all: Vec<_> = first
            .iter()
            .chain(&rest)
            .map(|g| g.wanted.as_str())
            .collect();
        assert_eq!(all, ["c", "a", "b"]);
    }

    #[tokio::test]
    async fn deleting_a_lesson_removes_its_requests_from_the_ranking() {
        let env = env().await;
        lesson_asking(&env, 1, 1, &[("slider", "x", 1)]).await;
        sqlx::query("DELETE FROM lessons")
            .execute(&env.pool)
            .await
            .unwrap();
        assert_eq!(
            env.lessons
                .component_request_groups(domain::UserId::OWNER, None, 10)
                .await
                .unwrap(),
            []
        );
    }

    fn attempt(lesson: &Lesson, n: u128, block: &str, number: u8, outcome: Outcome) -> Attempt {
        Attempt {
            id: AttemptId::new(uid(0x400 + n)),
            lesson_id: lesson.id,
            block_id: block.into(),
            kind: if block == "oa" {
                ExerciseKind::OpenAnswer
            } else {
                ExerciseKind::QuizSingle
            },
            attempt_number: number,
            answer: if block == "oa" {
                Answer::Text("perché".into())
            } else {
                Answer::Option("a".into())
            },
            outcome,
            grade: None,
            answered_at: T0,
            created_at: T0 + time::Duration::milliseconds(i64::try_from(n).unwrap()),
        }
    }

    // ------------------------------------------------------------ lessons

    #[tokio::test]
    async fn lessons_are_numbered_per_topic_and_round_trip() {
        let env = env().await;
        let one = lesson(&env, 1).await;
        let two = lesson(&env, 2).await;
        assert_eq!((one.number, two.number), (1, 2));
        assert_eq!(one.status, LessonStatus::NotStarted);
        assert_eq!(
            (
                one.mission_version,
                one.catalog_version,
                one.estimated_minutes
            ),
            (2, 1, 8)
        );

        let stored = env
            .lessons
            .get(domain::UserId::OWNER, two.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.lesson, two);
        assert_eq!(stored.document, document());
        assert_eq!(stored.plan, json!({"objective": "Saper compensare."}));
        assert!(
            env.lessons
                .get(domain::UserId::OWNER, LessonId::new(uid(9)))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            env.lessons
                .by_generation_job(JobId::new(uid(0x201)))
                .await
                .unwrap(),
            Some(one.clone())
        );
        let requests = env.lessons.component_requests(one.id).await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            (requests[0].wanted.as_str(), &requests[0].fallback),
            ("slider", &json!([{"id": "f"}]))
        );
    }

    #[tokio::test]
    async fn lessons_list_newest_first_with_a_keyset_cursor() {
        let env = env().await;
        for n in 1..=3 {
            lesson(&env, n).await;
        }
        let first = env.lessons.list(env.topic, None, 2).await.unwrap();
        assert_eq!(first.iter().map(|l| l.number).collect::<Vec<_>>(), [3, 2]);
        let next = env
            .lessons
            .list(env.topic, Some(LessonCursor { number: 2 }), 2)
            .await
            .unwrap();
        assert_eq!(next.iter().map(|l| l.number).collect::<Vec<_>>(), [1]);
        let all = env.lessons.all_in_topic(env.topic).await.unwrap();
        assert_eq!(all.iter().map(|l| l.number).collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[tokio::test]
    async fn saving_a_lesson_is_atomic_and_idempotent_per_job() {
        let env = env().await;
        lesson(&env, 1).await;
        // The same generation job cannot save a second lesson.
        let mut again = generated(&env, 2);
        again.job_id = JobId::new(uid(0x201));
        assert!(matches!(
            env.lessons.insert_generated(&again).await,
            Err(RepositoryError::Conflict)
        ));
        // A request that cannot be stored takes the lesson down with it.
        let mut broken = generated(&env, 3);
        broken.component_requests[0].topic_id = TopicId::new(uid(0x999));
        assert!(env.lessons.insert_generated(&broken).await.is_err());
        let all = env.lessons.all_in_topic(env.topic).await.unwrap();
        assert_eq!(all.len(), 1, "no half-saved lesson");
        assert!(
            env.lessons
                .by_generation_job(JobId::new(uid(0x203)))
                .await
                .unwrap()
                .is_none()
        );
        // The numbering has no hole after the failure.
        assert_eq!(lesson(&env, 4).await.number, 2);
        // A missing topic is a missing parent.
        let mut orphan = generated(&env, 5);
        orphan.topic_id = TopicId::new(uid(0x998));
        orphan.component_requests.clear();
        assert!(matches!(
            env.lessons.insert_generated(&orphan).await,
            Err(RepositoryError::MissingParent)
        ));
    }

    #[tokio::test]
    async fn completing_stores_the_lesson_state_and_the_job_together() {
        let env = env().await;
        let one = lesson(&env, 1).await;
        let two = lesson(&env, 2).await;
        let job = |n: u128, l: &Lesson| {
            Job::queued_for_lesson(
                JobId::new(uid(0x500 + n)),
                env.topic,
                l.id,
                JobKind::LessonCompletion,
                T0,
            )
        };

        // A job that cannot be stored (its id is taken) leaves the lesson open.
        env.jobs
            .insert(&Job::queued(
                JobId::new(uid(0x501)),
                env.topic,
                JobKind::Research,
                T0,
            ))
            .await
            .unwrap();
        assert!(
            env.lessons
                .complete(one.id, T0, &job(1, &one))
                .await
                .is_err()
        );
        assert_eq!(
            env.lessons
                .get(domain::UserId::OWNER, one.id)
                .await
                .unwrap()
                .unwrap()
                .lesson
                .status,
            LessonStatus::NotStarted
        );

        let done = env
            .lessons
            .complete(one.id, T0, &job(2, &one))
            .await
            .unwrap();
        assert_eq!(
            (done.status, done.completed_at),
            (LessonStatus::Completed, Some(T0))
        );
        let stored = env
            .jobs
            .get(domain::UserId::OWNER, env.topic, JobId::new(uid(0x502)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (stored.lesson_id, stored.kind, stored.status),
            (Some(one.id), JobKind::LessonCompletion, JobStatus::Queued)
        );

        assert!(matches!(
            env.lessons.complete(one.id, T0, &job(3, &one)).await,
            Err(RepositoryError::Conflict)
        ));
        // Another lesson may be completing at the same time.
        env.lessons
            .complete(two.id, T0, &job(4, &two))
            .await
            .unwrap();
        assert!(matches!(
            env.lessons
                .complete(LessonId::new(uid(0x9)), T0, &job(5, &one))
                .await,
            Err(RepositoryError::NotFound)
        ));
    }

    #[tokio::test]
    async fn the_one_active_job_rule_is_per_topic_except_for_completions() {
        let env = env().await;
        let one = lesson(&env, 1).await;
        let queued = |n: u128, kind: JobKind, lesson: Option<LessonId>| match lesson {
            Some(l) => Job::queued_for_lesson(JobId::new(uid(0x600 + n)), env.topic, l, kind, T0),
            None => Job::queued(JobId::new(uid(0x600 + n)), env.topic, kind, T0),
        };
        env.jobs
            .insert(&queued(1, JobKind::LessonGeneration, None))
            .await
            .unwrap();
        assert!(matches!(
            env.jobs
                .insert(&queued(2, JobKind::LessonGeneration, None))
                .await,
            Err(RepositoryError::Conflict)
        ));
        env.jobs
            .insert(&queued(3, JobKind::Research, None))
            .await
            .unwrap();
        env.jobs
            .insert(&queued(4, JobKind::LessonCompletion, Some(one.id)))
            .await
            .unwrap();
        assert!(matches!(
            env.jobs
                .insert(&queued(5, JobKind::LessonCompletion, Some(one.id)))
                .await,
            Err(RepositoryError::Conflict)
        ));
        let two = lesson(&env, 2).await;
        env.jobs
            .insert(&queued(6, JobKind::LessonCompletion, Some(two.id)))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn lesson_job_results_survive_the_round_trip() {
        let env = env().await;
        let one = lesson(&env, 1).await;
        let mut generation = Job::queued(
            JobId::new(uid(0x700)),
            env.topic,
            JobKind::LessonGeneration,
            T0,
        );
        env.jobs.insert(&generation).await.unwrap();
        generation.start(T0).unwrap();
        env.jobs
            .update(&generation, JobStatus::Queued)
            .await
            .unwrap();
        generation
            .succeed(
                JobResult::LessonGeneration {
                    lesson_id: one.id,
                    lesson_number: 1,
                },
                T0,
            )
            .unwrap();
        env.jobs
            .update(&generation, JobStatus::Running)
            .await
            .unwrap();
        assert_eq!(
            env.jobs
                .get(domain::UserId::OWNER, env.topic, generation.id)
                .await
                .unwrap(),
            Some(generation)
        );

        let mut comp = Job::queued_for_lesson(
            JobId::new(uid(0x701)),
            env.topic,
            one.id,
            JobKind::LessonCompletion,
            T0,
        );
        env.jobs.insert(&comp).await.unwrap();
        comp.start(T0).unwrap();
        env.jobs.update(&comp, JobStatus::Queued).await.unwrap();
        comp.succeed(
            JobResult::LessonCompletion {
                lesson_id: one.id,
                record_count: 2,
                term_count: 3,
                review_card_count: 4,
            },
            T0,
        )
        .unwrap();
        env.jobs.update(&comp, JobStatus::Running).await.unwrap();
        assert_eq!(
            env.jobs
                .get(domain::UserId::OWNER, env.topic, comp.id)
                .await
                .unwrap(),
            Some(comp)
        );
    }

    #[tokio::test]
    async fn the_latest_job_is_found_however_many_jobs_the_topic_has() {
        let env = env().await;
        let one = lesson(&env, 1).await;
        let fail = |mut j: Job| {
            j.start(T0).unwrap();
            j.fail(
                domain::JobErrorCode::Timeout,
                domain::ContentLanguage::It,
                T0,
            )
            .unwrap();
            j
        };
        let completion = Job::queued_for_lesson(
            JobId::new(uid(0x800)),
            env.topic,
            one.id,
            JobKind::LessonCompletion,
            T0,
        );
        env.jobs.insert(&completion).await.unwrap();
        env.jobs
            .update(&fail(completion.clone()), JobStatus::Queued)
            .await
            .unwrap();
        let mut newest = None;
        for n in 1..=120_u32 {
            let job = Job::queued(
                JobId::new(uid(0x900 + u128::from(n))),
                env.topic,
                JobKind::Research,
                T0 + time::Duration::seconds(i64::from(n)),
            );
            env.jobs.insert(&job).await.unwrap();
            let failed = fail(job);
            env.jobs.update(&failed, JobStatus::Queued).await.unwrap();
            newest = Some(failed);
        }
        let found = env
            .jobs
            .latest(env.topic, JobKind::LessonCompletion, Some(one.id))
            .await
            .unwrap();
        assert_eq!(found.map(|j| j.id), Some(completion.id));
        let found = env
            .jobs
            .latest(env.topic, JobKind::Research, None)
            .await
            .unwrap();
        assert_eq!(found.map(|j| j.id), newest.map(|j| j.id));
        // A topic-level query never returns a lesson's job, nor the reverse.
        assert!(
            env.jobs
                .latest(env.topic, JobKind::LessonCompletion, None)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            env.jobs
                .latest(env.topic, JobKind::Research, Some(one.id))
                .await
                .unwrap()
                .is_none()
        );
    }

    // ----------------------------------------------------------- attempts

    #[tokio::test]
    async fn the_first_attempt_stored_wins_and_moves_the_lesson_forward() {
        let env = env().await;
        let l = lesson(&env, 1).await;
        let first = attempt(&l, 1, "q1", 1, Outcome::Incorrect);
        let stored = env
            .attempts
            .record(l.id, std::slice::from_ref(&first))
            .await
            .unwrap();
        assert_eq!(stored, vec![first.clone()]);
        assert_eq!(
            env.lessons
                .get(domain::UserId::OWNER, l.id)
                .await
                .unwrap()
                .unwrap()
                .lesson
                .status,
            LessonStatus::InProgress
        );

        // Same pair, different content and id: the stored one is returned.
        let mut other = attempt(&l, 2, "q1", 1, Outcome::Incorrect);
        other.answer = Answer::Option("b".into());
        let mixed = env
            .attempts
            .record(l.id, &[other, attempt(&l, 3, "q1", 2, Outcome::Correct)])
            .await
            .unwrap();
        assert_eq!(mixed[0], first);
        assert_eq!(mixed[1].attempt_number, 2);
        assert_eq!(env.attempts.list(l.id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_second_attempt_is_checked_against_the_first_inside_the_write() {
        let env = env().await;
        let l = lesson(&env, 1).await;
        let second = attempt(&l, 2, "q1", 2, Outcome::Correct);
        // No first attempt stored.
        assert!(matches!(
            env.attempts
                .record(l.id, std::slice::from_ref(&second))
                .await,
            Err(RepositoryError::RuleViolated)
        ));
        // A first attempt that was already correct.
        env.attempts
            .record(l.id, &[attempt(&l, 1, "q1", 1, Outcome::Correct)])
            .await
            .unwrap();
        let batch = [attempt(&l, 3, "fc", 1, Outcome::Correct), second.clone()];
        assert!(matches!(
            env.attempts.record(l.id, &batch).await,
            Err(RepositoryError::RuleViolated)
        ));
        assert_eq!(
            env.attempts.list(l.id).await.unwrap().len(),
            1,
            "the batch stored nothing"
        );
        // A wrong first attempt, even one sent in the same batch, allows it.
        let wrong = [
            attempt(&l, 4, "qm", 1, Outcome::Incorrect),
            attempt(&l, 5, "qm", 2, Outcome::Correct),
        ];
        assert_eq!(env.attempts.record(l.id, &wrong).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn attempts_round_trip_every_answer_shape_and_a_grade() {
        let env = env().await;
        let l = lesson(&env, 1).await;
        let mut all = Vec::new();
        for (i, (kind, answer)) in [
            (ExerciseKind::QuizSingle, Answer::Option("b".into())),
            (
                ExerciseKind::QuizMulti,
                Answer::Options(vec!["a".into(), "c".into()]),
            ),
            (
                ExerciseKind::FillBlank,
                Answer::Blanks([("b1".to_owned(), "borrow".to_owned())].into()),
            ),
            (
                ExerciseKind::OrderSteps,
                Answer::Steps(vec!["s2".into(), "s1".into()]),
            ),
            (ExerciseKind::Flashcard, Answer::Rating(Rating::Easy)),
        ]
        .into_iter()
        .enumerate()
        {
            let mut a = attempt(&l, i as u128 + 10, &format!("b{i}"), 1, Outcome::Correct);
            a.kind = kind;
            a.answer = answer;
            all.push(a);
        }
        let mut open = attempt(&l, 20, "oa", 1, Outcome::Pending);
        all.push(open.clone());
        env.attempts.record(l.id, &all).await.unwrap();
        assert_eq!(env.attempts.list(l.id).await.unwrap(), all);

        let grade = Grade {
            verdict: Verdict::Partial,
            feedback: "Manca il perché.".into(),
            misconception: Some("Pensa che...".into()),
        };
        let graded = env.attempts.set_grade(open.id, &grade).await.unwrap();
        open.outcome = Outcome::Partial;
        open.grade = Some(grade.clone());
        assert_eq!(graded, open);
        // Already graded: a second grade does not overwrite it.
        let other = Grade {
            verdict: Verdict::Correct,
            feedback: "ok".into(),
            misconception: None,
        };
        assert_eq!(env.attempts.set_grade(open.id, &other).await.unwrap(), open);
        assert!(matches!(
            env.attempts
                .set_grade(AttemptId::new(uid(0x1)), &grade)
                .await,
            Err(RepositoryError::NotFound)
        ));
    }

    #[tokio::test]
    async fn a_completed_lesson_takes_no_attempts() {
        let env = env().await;
        let l = lesson(&env, 1).await;
        let job = Job::queued_for_lesson(
            JobId::new(uid(0x500)),
            env.topic,
            l.id,
            JobKind::LessonCompletion,
            T0,
        );
        env.lessons.complete(l.id, T0, &job).await.unwrap();
        assert!(matches!(
            env.attempts
                .record(l.id, &[attempt(&l, 1, "q1", 1, Outcome::Correct)])
                .await,
            Err(RepositoryError::Conflict)
        ));
        assert!(matches!(
            env.attempts
                .record(
                    LessonId::new(uid(0x9)),
                    &[attempt(&l, 2, "q1", 1, Outcome::Correct)]
                )
                .await,
            Err(RepositoryError::NotFound)
        ));
        assert_eq!(
            env.attempts.list(l.id).await.unwrap(),
            Vec::<Attempt>::new()
        );
    }

    // ----------------------------------------------------------- learning

    fn completion(env: &Env, l: &Lesson) -> CompletionSave {
        CompletionSave {
            topic_id: env.topic,
            lesson_id: l.id,
            at: T0,
            records: vec![NewRecord {
                id: LearningRecordId::new(uid(0x10)),
                kind: RecordKind::Demonstrated,
                title: "Compensare".into(),
                body: "Hai compensato.".into(),
                evidence: "Hai risposto bene.".into(),
                supersedes: vec![],
            }],
            terms: vec![TermUpsert {
                id: GlossaryTermId::new(uid(0x20)),
                key: "stop".into(),
                term: "Stop".into(),
                definition: "Unità di luce.".into(),
                avoid: vec!["step".into()],
            }],
            cards: vec![
                NewCard {
                    id: ReviewCardId::new(uid(0x30)),
                    target: CardTarget::Record(0),
                    front: "f1".into(),
                    back: "b1".into(),
                    unpassed_block: None,
                },
                NewCard {
                    id: ReviewCardId::new(uid(0x31)),
                    target: CardTarget::Term(0),
                    front: "f2".into(),
                    back: "b2".into(),
                    unpassed_block: None,
                },
            ],
        }
    }

    async fn counts(env: &Env) -> (i64, i64, i64, i64) {
        let n = |table: &'static str| {
            let pool = env.pool.clone();
            async move {
                let q = match table {
                    "records" => "SELECT COUNT(*) FROM learning_records",
                    "terms" => "SELECT COUNT(*) FROM glossary_terms",
                    "cards" => "SELECT COUNT(*) FROM review_cards",
                    _ => "SELECT COUNT(*) FROM lesson_completions",
                };
                sqlx::query_scalar::<_, i64>(q)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
            }
        };
        (
            n("records").await,
            n("terms").await,
            n("cards").await,
            n("completions").await,
        )
    }

    #[tokio::test]
    async fn a_completion_stores_records_terms_and_cards_and_is_idempotent() {
        let env = env().await;
        let l = lesson(&env, 1).await;
        let summary = env
            .learning
            .save_completion(&completion(&env, &l))
            .await
            .unwrap();
        assert_eq!(
            (
                summary.record_count,
                summary.term_count,
                summary.review_card_count
            ),
            (1, 1, 2)
        );
        assert_eq!(
            env.learning.completion_summary(l.id).await.unwrap(),
            Some(summary)
        );
        assert_eq!(counts(&env).await, (1, 1, 2, 1));

        let records = env
            .learning
            .list_records(env.topic, true, None, 10)
            .await
            .unwrap();
        assert_eq!(
            (records[0].number, records[0].status, records[0].lesson_id),
            (1, RecordStatus::Active, l.id)
        );
        let cards = env.learning.review_cards(env.topic).await.unwrap();
        assert_eq!(cards.len(), 2);
        assert!(
            matches!(cards[0].origin, CardOrigin::Record(_))
                && matches!(cards[1].origin, CardOrigin::Term(_))
        );
        // Born new, due a day after the lesson was closed.
        for card in &cards {
            assert_eq!(card.schedule, CardSchedule::initial(T0));
            assert_eq!(card.schedule.due, T0 + time::Duration::hours(24));
        }
        // New cards are born on the current model, not the column's default.
        let models: Vec<String> = sqlx::query_scalar("SELECT model FROM review_cards")
            .fetch_all(&env.pool)
            .await
            .unwrap();
        assert_eq!(models, ["fsrs7", "fsrs7"]);
        let outputs = env.learning.lesson_outputs(l.id).await.unwrap();
        assert_eq!(
            (
                outputs.records.len(),
                outputs.terms.len(),
                outputs.review_card_count
            ),
            (1, 1, 2)
        );
        assert_eq!(outputs.terms[0].avoid, ["step"]);

        // Recorded once: the same lesson again is refused and changes nothing.
        assert!(matches!(
            env.learning.save_completion(&completion(&env, &l)).await,
            Err(RepositoryError::Conflict)
        ));
        assert_eq!(counts(&env).await, (1, 1, 2, 1));
    }

    #[tokio::test]
    async fn the_unpassed_block_is_saved_with_the_card_and_read_back_as_to_consolidate() {
        let env = env().await;
        let l = lesson(&env, 1).await;
        let mut save = completion(&env, &l);
        save.cards[0].unpassed_block = Some("q1".into());
        env.learning.save_completion(&save).await.unwrap();

        let stored: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT front, unpassed_block FROM review_cards ORDER BY front")
                .fetch_all(&env.pool)
                .await
                .unwrap();
        assert_eq!(
            stored,
            [
                ("f1".to_owned(), Some("q1".to_owned())),
                ("f2".to_owned(), None)
            ]
        );
        let cards = env.learning.review_cards(l.topic_id).await.unwrap();
        let flag = |front: &str| {
            cards
                .iter()
                .find(|c| c.front == front)
                .unwrap()
                .to_consolidate
        };
        assert!(flag("f1"));
        assert!(!flag("f2"));
    }

    #[tokio::test]
    async fn votes_are_read_in_application_order_even_with_the_same_created_at() {
        let env = env().await;
        let l = lesson(&env, 1).await;
        let mut save = completion(&env, &l);
        save.cards[0].unpassed_block = Some("q1".into());
        env.learning.save_completion(&save).await.unwrap();
        let card = save.cards[0].id.to_string();
        // good, again, good: the series is broken, so the card stays to
        // consolidate. Ids are descending so only `rowid` gives the order.
        for same_instant in [false, true] {
            sqlx::query("DELETE FROM review_logs")
                .execute(&env.pool)
                .await
                .unwrap();
            for (n, rating) in ["good", "again", "good"].into_iter().enumerate() {
                let at = if same_instant {
                    "2026-11-09T10:00:00.000Z".to_owned()
                } else {
                    format!("2026-11-09T10:0{n}:00.000Z")
                };
                sqlx::query(
                    "INSERT INTO review_logs (id, card_id, rating, reviewed_at, state_before, \
                     state_after, created_at) VALUES (?, ?, ?, ?, '{}', '{}', ?)",
                )
                .bind(uid(0x900 - n as u128).to_string())
                .bind(&card)
                .bind(rating)
                .bind(&at)
                .bind(&at)
                .execute(&env.pool)
                .await
                .unwrap();
            }
            let cards = env.learning.review_cards(l.topic_id).await.unwrap();
            assert!(
                cards
                    .iter()
                    .find(|c| c.front == "f1")
                    .unwrap()
                    .to_consolidate,
                "same_instant = {same_instant}"
            );
        }
    }

    #[tokio::test]
    async fn an_empty_completion_is_still_recorded() {
        let env = env().await;
        let l = lesson(&env, 1).await;
        let mut save = completion(&env, &l);
        (save.records, save.terms, save.cards) = (vec![], vec![], vec![]);
        env.learning.save_completion(&save).await.unwrap();
        assert_eq!(
            env.learning.completion_summary(l.id).await.unwrap(),
            Some(CompletionSummary::default())
        );
        assert_eq!(
            env.learning
                .completion_summary(LessonId::new(uid(9)))
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn supersession_keeps_the_instant_of_an_already_superseded_card() {
        let env = env().await;
        let first = lesson(&env, 1).await;
        let second = lesson(&env, 2).await;
        env.learning
            .save_completion(&completion(&env, &first))
            .await
            .unwrap();
        sqlx::query(
            "UPDATE review_cards SET status = 'suspended', suspension = 'superseded', \
             suspended_at = '2026-11-09T10:30:00.000Z' WHERE front = 'f1'",
        )
        .execute(&env.pool)
        .await
        .unwrap();
        let mut save = completion(&env, &second);
        save.at = T0 + time::Duration::hours(1);
        save.records[0].id = LearningRecordId::new(uid(0x11));
        save.records[0].supersedes = vec![1];
        save.terms[0].id = GlossaryTermId::new(uid(0x21));
        save.cards = vec![];
        env.learning.save_completion(&save).await.unwrap();

        let cards = env.learning.review_cards(env.topic).await.unwrap();
        let f1 = cards.iter().find(|c| c.front == "f1").unwrap();
        assert_eq!(
            f1.suspension.map(|s| s.at),
            Some(datetime!(2026-11-09 10:30 UTC))
        );
    }

    #[tokio::test]
    async fn supersession_suspends_cards_and_a_term_is_revised_in_place() {
        let env = env().await;
        let first = lesson(&env, 1).await;
        let second = lesson(&env, 2).await;
        env.learning
            .save_completion(&completion(&env, &first))
            .await
            .unwrap();

        let mut save = completion(&env, &second);
        save.at = T0 + time::Duration::hours(1);
        save.records[0].id = LearningRecordId::new(uid(0x11));
        save.records[0].supersedes = vec![1];
        save.terms[0].id = GlossaryTermId::new(uid(0x21));
        save.terms[0].definition = "Raddoppio o dimezzamento della luce.".into();
        save.terms[0].avoid = vec![];
        save.cards = vec![NewCard {
            id: ReviewCardId::new(uid(0x32)),
            target: CardTarget::Term(0),
            front: "f3".into(),
            back: "b3".into(),
            unpassed_block: None,
        }];
        // The learner had suspended that card already: superseding wins.
        sqlx::query(
            "UPDATE review_cards SET status = 'suspended', suspension = 'user', \
             suspended_at = '2026-11-09T10:30:00.000Z' WHERE front = 'f1'",
        )
        .execute(&env.pool)
        .await
        .unwrap();
        env.learning.save_completion(&save).await.unwrap();

        let all = env
            .learning
            .list_records(env.topic, false, None, 10)
            .await
            .unwrap();
        assert_eq!(
            all.iter().map(|r| r.number).collect::<Vec<_>>(),
            [2, 1],
            "newest first"
        );
        assert_eq!(
            (all[1].status, all[1].superseded_by),
            (RecordStatus::Superseded, Some(all[0].id))
        );
        let active = env
            .learning
            .list_records(env.topic, true, None, 10)
            .await
            .unwrap();
        assert_eq!(active.iter().map(|r| r.number).collect::<Vec<_>>(), [2]);
        let before = env
            .learning
            .list_records(env.topic, false, Some(RecordCursor { number: 2 }), 10)
            .await
            .unwrap();
        assert_eq!(before.len(), 1);

        let cards = env.learning.review_cards(env.topic).await.unwrap();
        let status = |front: &str| cards.iter().find(|c| c.front == front).unwrap().status;
        assert_eq!(
            status("f1"),
            CardStatus::Suspended,
            "the replaced record's card"
        );
        let f1 = cards.iter().find(|c| c.front == "f1").unwrap();
        assert_eq!(
            f1.suspension,
            Some(Suspension {
                reason: SuspensionReason::Superseded,
                at: T0 + time::Duration::hours(1)
            })
        );
        assert_eq!(status("f2"), CardStatus::Active, "the term's card stays");
        assert_eq!(status("f3"), CardStatus::Active);

        let terms = env.learning.list_terms(env.topic, None, 10).await.unwrap();
        assert_eq!(terms.len(), 1, "revised, not duplicated");
        assert_eq!(
            (terms[0].id, terms[0].lesson_id),
            (GlossaryTermId::new(uid(0x20)), second.id)
        );
        assert_eq!(terms[0].definition, "Raddoppio o dimezzamento della luce.");
        // `lesson_id` follows the reviser, `first_lesson_id` stays with the creator.
        assert_ne!(terms[0].first_lesson_id, Some(second.id));
        assert_eq!(terms[0].first_lesson_id, Some(first.id));
        assert_eq!(
            (terms[0].created_at, terms[0].updated_at),
            (T0, T0 + time::Duration::hours(1))
        );
        // The card of the new revision points to the same, original, term row.
        let c3 = cards.iter().find(|c| c.front == "f3").unwrap();
        assert_eq!(c3.origin, CardOrigin::Term(terms[0].id));
    }

    #[tokio::test]
    async fn a_failing_completion_stores_nothing() {
        let env = env().await;
        let first = lesson(&env, 1).await;
        let second = lesson(&env, 2).await;
        env.learning
            .save_completion(&completion(&env, &first))
            .await
            .unwrap();

        // Replaces a record that does not exist: refused after the new record
        // and the completion marker were already written inside the transaction.
        let mut save = completion(&env, &second);
        save.records[0].id = LearningRecordId::new(uid(0x11));
        save.records[0].supersedes = vec![42];
        assert!(matches!(
            env.learning.save_completion(&save).await,
            Err(RepositoryError::Conflict)
        ));
        assert_eq!(
            counts(&env).await,
            (1, 1, 2, 1),
            "everything of the failed save rolled back"
        );
        assert_eq!(
            env.learning.completion_summary(second.id).await.unwrap(),
            None
        );

        // A card that points past the end fails midway too.
        let mut save = completion(&env, &second);
        save.cards[1].id = save.cards[0].id;
        assert!(env.learning.save_completion(&save).await.is_err());
        assert_eq!(counts(&env).await, (1, 1, 2, 1));
        // A save that would replace an already replaced record is refused as well.
        let mut again = completion(&env, &second);
        again.records[0].id = LearningRecordId::new(uid(0x12));
        again.records[0].supersedes = vec![1];
        again.cards.clear();
        again.terms.clear();
        env.learning.save_completion(&again).await.unwrap();
        let third = lesson(&env, 3).await;
        let mut stale = completion(&env, &third);
        stale.records[0].id = LearningRecordId::new(uid(0x13));
        stale.records[0].supersedes = vec![1];
        assert!(matches!(
            env.learning.save_completion(&stale).await,
            Err(RepositoryError::Conflict)
        ));
    }

    #[tokio::test]
    async fn the_glossary_is_alphabetical_without_case_with_a_keyset_cursor() {
        let env = env().await;
        let l = lesson(&env, 1).await;
        let mut save = completion(&env, &l);
        save.records.clear();
        save.cards.clear();
        save.terms = [("b", "beta"), ("a", "Zeta"), ("c", "Alfa"), ("d", "alfa")]
            .into_iter()
            .enumerate()
            .map(|(i, (key, term))| TermUpsert {
                id: GlossaryTermId::new(uid(0x40 + i as u128)),
                key: key.into(),
                term: term.into(),
                definition: "d".into(),
                avoid: vec![],
            })
            .collect();
        env.learning.save_completion(&save).await.unwrap();

        let first = env.learning.list_terms(env.topic, None, 3).await.unwrap();
        assert_eq!(
            first.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
            ["c", "d", "b"]
        );
        let next = env
            .learning
            .list_terms(env.topic, Some(TermCursor::after(&first[2])), 3)
            .await
            .unwrap();
        assert_eq!(
            next.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
            ["a"]
        );
    }

    #[tokio::test]
    async fn deleting_a_topic_deletes_what_its_lessons_left() {
        let env = env().await;
        let l = lesson(&env, 1).await;
        env.attempts
            .record(l.id, &[attempt(&l, 1, "q1", 1, Outcome::Correct)])
            .await
            .unwrap();
        env.learning
            .save_completion(&completion(&env, &l))
            .await
            .unwrap();
        SqliteTopicRepository::new(env.pool.clone())
            .delete(domain::UserId::OWNER, env.topic)
            .await
            .unwrap();
        assert_eq!(counts(&env).await, (0, 0, 0, 0));
        for table in ["lessons", "attempts", "component_requests", "jobs"] {
            let q = format!("SELECT COUNT(*) FROM {table}");
            let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(q))
                .fetch_one(&env.pool)
                .await
                .unwrap();
            assert_eq!(n, 0, "{table}");
        }
    }

    fn hl(block: &str, start: u32, end: u32, color: HighlightColor) -> Highlight {
        Highlight {
            block_id: block.into(),
            start,
            end,
            color,
        }
    }

    #[tokio::test]
    async fn highlights_are_replaced_whole_and_kept_in_the_order_given() {
        let env = env().await;
        let one = lesson(&env, 1).await;
        let other = lesson(&env, 2).await;
        let repo = SqliteHighlightRepository::new(env.pool.clone());
        assert_eq!(repo.list(one.id).await.unwrap(), Vec::new());

        let set = vec![
            hl("p2", 5, 9, HighlightColor::Pink),
            hl("p1", 0, 3, HighlightColor::Yellow),
            hl("p1", 3, 4, HighlightColor::Blue),
        ];
        repo.replace(one.id, &set).await.unwrap();
        repo.replace(other.id, &[hl("p1", 0, 1, HighlightColor::Green)])
            .await
            .unwrap();
        assert_eq!(repo.list(one.id).await.unwrap(), set);

        // Sending the same set again, then a smaller one, then none.
        repo.replace(one.id, &set).await.unwrap();
        assert_eq!(repo.list(one.id).await.unwrap(), set);
        repo.replace(one.id, &set[..1]).await.unwrap();
        assert_eq!(repo.list(one.id).await.unwrap(), &set[..1]);
        repo.replace(one.id, &[]).await.unwrap();
        assert_eq!(repo.list(one.id).await.unwrap(), Vec::new());
        assert_eq!(
            repo.list(other.id).await.unwrap().len(),
            1,
            "other lessons are left alone"
        );
    }

    fn st(id: u128, block: &str, points: &[i32]) -> Stroke {
        Stroke {
            id: Uuid::from_u128(id).to_string(),
            block_id: block.into(),
            color: StrokeColor::Red,
            width: StrokeWidth::Thick,
            points: points.to_vec(),
        }
    }

    #[tokio::test]
    async fn strokes_are_replaced_whole_and_kept_in_the_order_given() {
        let env = env().await;
        let one = lesson(&env, 1).await;
        let other = lesson(&env, 2).await;
        let repo = SqliteStrokeRepository::new(env.pool.clone());
        assert_eq!(repo.list(one.id).await.unwrap(), Vec::new());

        // Negative values and the extremes of the contract's ranges survive.
        let set = vec![
            st(3, "p2", &[-40_000, 200_000, 50_000, -40_000]),
            st(1, "p1", &[0, 0]),
            st(2, "p1", &[7, 8, 9, 10, 11, 12]),
        ];
        repo.replace(one.id, &set).await.unwrap();
        repo.replace(other.id, &[st(1, "p1", &[1, 1])])
            .await
            .unwrap();
        assert_eq!(repo.list(one.id).await.unwrap(), set);

        repo.replace(one.id, &set).await.unwrap();
        assert_eq!(repo.list(one.id).await.unwrap(), set);
        repo.replace(one.id, &set[..1]).await.unwrap();
        assert_eq!(repo.list(one.id).await.unwrap(), &set[..1]);
        repo.replace(one.id, &[]).await.unwrap();
        assert_eq!(repo.list(one.id).await.unwrap(), Vec::new());
        assert_eq!(repo.list(other.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn strokes_of_a_missing_lesson_are_not_found_and_go_with_their_lesson() {
        let env = env().await;
        let repo = SqliteStrokeRepository::new(env.pool.clone());
        let r = repo.replace(LessonId::new(uid(0x999)), &[]).await;
        assert!(matches!(r, Err(RepositoryError::NotFound)));

        let one = lesson(&env, 1).await;
        repo.replace(one.id, &[st(1, "p1", &[1, 2])]).await.unwrap();
        sqlx::query("DELETE FROM lessons WHERE id = ?")
            .bind(one.id.to_string())
            .execute(&env.pool)
            .await
            .unwrap();
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM strokes")
            .fetch_one(&env.pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn a_damaged_points_blob_is_reported_not_served() {
        let env = env().await;
        let one = lesson(&env, 1).await;
        let repo = SqliteStrokeRepository::new(env.pool.clone());
        repo.replace(one.id, &[st(1, "p1", &[1, 2])]).await.unwrap();
        // The CHECK refuses a length that is not a whole number of points.
        let r = sqlx::query("UPDATE strokes SET points = x'0102030405'")
            .execute(&env.pool)
            .await;
        assert!(r.is_err());
        // No blob that passes the CHECK is refused by `unpack_points` (they
        // accept the same lengths), so the guard is tested directly.
        for len in [0usize, 1, 4, 7, 9, 12, 15] {
            let r = unpack_points(&vec![0u8; len], "l");
            assert!(
                matches!(&r, Err(RepositoryError::Storage(e)) if e.to_string().contains("corrupt stroke points")),
                "{len}"
            );
        }
        assert_eq!(
            unpack_points(&[1, 0, 0, 0, 2, 0, 0, 0], "l").unwrap(),
            [1, 2]
        );
    }

    #[tokio::test]
    async fn replacing_the_highlights_of_a_missing_lesson_is_not_found() {
        let env = env().await;
        let repo = SqliteHighlightRepository::new(env.pool.clone());
        let r = repo.replace(LessonId::new(uid(0x999)), &[]).await;
        assert!(matches!(r, Err(RepositoryError::NotFound)));
    }

    #[tokio::test]
    async fn highlights_go_with_their_lesson_and_with_their_topic() {
        let env = env().await;
        let one = lesson(&env, 1).await;
        let repo = SqliteHighlightRepository::new(env.pool.clone());
        repo.replace(one.id, &[hl("p1", 0, 3, HighlightColor::Yellow)])
            .await
            .unwrap();
        sqlx::query("DELETE FROM lessons WHERE id = ?")
            .bind(one.id.to_string())
            .execute(&env.pool)
            .await
            .unwrap();
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM highlights")
            .fetch_one(&env.pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "deleting the lesson");

        let two = lesson(&env, 2).await;
        repo.replace(two.id, &[hl("p1", 0, 3, HighlightColor::Yellow)])
            .await
            .unwrap();
        sqlx::query("DELETE FROM topics WHERE id = ?")
            .bind(env.topic.to_string())
            .execute(&env.pool)
            .await
            .unwrap();
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM highlights")
            .fetch_one(&env.pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "deleting the topic");
    }

    #[tokio::test]
    async fn two_replaces_at_once_end_with_one_of_the_two_sets_whole() {
        // A file database with several connections: in memory there is one.
        let path = std::env::temp_dir().join(format!("teach_me_hl_{}.db", uid(0).simple()));
        let _ = std::fs::remove_file(&path);
        let pool = connect(&format!("sqlite://{}", path.display()))
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let lessons = SqliteLessonRepository::new(pool.clone());
        let repo = SqliteHighlightRepository::new(pool.clone());
        let folder = FolderId::new(uid(1));
        SqliteFolderRepository::new(pool.clone())
            .insert(NewFolder {
                user_id: domain::UserId::OWNER,
                id: folder,
                name: FolderName::parse("F").unwrap(),
                created_at: T0,
            })
            .await
            .unwrap();
        let topic = Topic::new(
            TopicId::new(uid(2)),
            domain::UserId::OWNER,
            folder,
            TopicTitle::parse("T").unwrap(),
            T0,
        );
        SqliteTopicRepository::new(pool.clone())
            .insert(&topic)
            .await
            .unwrap();
        let new = GeneratedLesson {
            topic_id: topic.id,
            ..generated(
                &Env {
                    lessons: lessons.clone(),
                    attempts: SqliteAttemptRepository::new(pool.clone()),
                    learning: SqliteLearningRepository::new(pool.clone()),
                    jobs: SqliteJobRepository::new(pool.clone()),
                    topic: topic.id,
                    pool: pool.clone(),
                },
                1,
            )
        };
        let lesson = lessons.insert_generated(&new).await.unwrap();

        let a: Vec<Highlight> = (0..40)
            .map(|i| hl("p1", i, i + 1, HighlightColor::Yellow))
            .collect();
        let b: Vec<Highlight> = (0..30)
            .map(|i| hl("p2", i, i + 1, HighlightColor::Pink))
            .collect();
        for _ in 0..10 {
            let (ra, rb) = tokio::join!(repo.replace(lesson.id, &a), repo.replace(lesson.id, &b));
            ra.unwrap();
            rb.unwrap();
            let stored = repo.list(lesson.id).await.unwrap();
            assert!(stored == a || stored == b, "a mix of the two sets");
        }
        pool.close().await;
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }
}
