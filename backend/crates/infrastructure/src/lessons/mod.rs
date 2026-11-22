//! SQLite adapters for lessons, attempts and what a completion leaves behind
//! (learning records, glossary, review cards).

use domain::RepositoryError;
use serde_json::Value;

use crate::repositories::{constraint, corrupt};

// Shared SQL fragments: defined before the submodules so they are in textual scope there.
macro_rules! lesson_select {
    () => {
        "SELECT id, topic_id, number, title, objective, estimated_minutes, status, \
         catalog_version, mission_version, created_at, completed_at, quality_warnings FROM lessons"
    };
}

macro_rules! attempt_columns {
    () => {
        "id, lesson_id, block_id, kind, attempt_number, answer, outcome, \
     grade_verdict, grade_feedback, grade_misconception, answered_at, created_at"
    };
}

macro_rules! record_columns {
    () => {
        "id, topic_id, number, kind, title, body, evidence, status, \
     superseded_by, lesson_id, created_at"
    };
}

mod annotations;
mod attempt;
mod learning;
mod lesson;

#[cfg(test)]
use annotations::unpack_points;
use lesson::fetch_lesson;

pub use annotations::{SqliteHighlightRepository, SqliteStrokeRepository};
pub use attempt::SqliteAttemptRepository;
pub use learning::SqliteLearningRepository;
pub(crate) use learning::{card_from_row, consolidation_ratings};
pub use lesson::SqliteLessonRepository;

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

#[cfg(test)]
mod tests;
