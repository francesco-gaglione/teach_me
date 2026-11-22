//! Attempt adapter.

use super::{fetch_lesson, json_parse, json_text};
use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};
use async_trait::async_trait;
use domain::{
    Answer, Attempt, AttemptId, AttemptRepository, ExerciseKind, Grade, LessonId, Outcome,
    RepositoryError, Verdict,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

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
