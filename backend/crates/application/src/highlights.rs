//! The learner's highlights on a lesson (`docs/contracts/lessons.md` §10).

use std::sync::Arc;

use domain::{
    Highlight, HighlightDraft, HighlightRepository, HighlightTargets, IssueKind, LessonId,
    LessonRepository, UserId,
};
use serde_json::Value;

use crate::error::{AppError, FieldIssue};

pub struct HighlightService {
    lessons: Arc<dyn LessonRepository>,
    highlights: Arc<dyn HighlightRepository>,
}

impl HighlightService {
    #[must_use]
    pub fn new(
        lessons: Arc<dyn LessonRepository>,
        highlights: Arc<dyn HighlightRepository>,
    ) -> Self {
        Self {
            lessons,
            highlights,
        }
    }

    /// `NotFound` unless the lesson is in a topic of `user`. Cheap: reads no
    /// document, so a handler can check it before looking at the body.
    pub async fn ensure_lesson(&self, user: UserId, lesson_id: LessonId) -> Result<(), AppError> {
        if self.lessons.exists(user, lesson_id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound)
        }
    }

    /// Replaces the lesson's set with the one in `body` and returns it as
    /// stored. Allowed in every lesson status: rereading a completed lesson
    /// and marking it is the typical use.
    pub async fn replace(
        &self,
        user: UserId,
        lesson_id: LessonId,
        body: &Value,
    ) -> Result<Vec<Highlight>, AppError> {
        let stored = self
            .lessons
            .get(user, lesson_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let drafts = parse_drafts(body)?;
        let valid = HighlightTargets::from_document(&stored.document)
            .validate(&drafts)
            .map_err(|issues| {
                AppError::InvalidFields(
                    issues
                        .into_iter()
                        .map(|i| FieldIssue::new(i.field, i.issue))
                        .collect(),
                )
            })?;
        self.highlights.replace(lesson_id, &valid).await?;
        Ok(valid)
    }
}

/// Reads the shape of the body; what each value means is up to the domain.
fn parse_drafts(body: &Value) -> Result<Vec<HighlightDraft>, AppError> {
    let list = body
        .get("highlights")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::InvalidFields(vec![FieldIssue::new("highlights", IssueKind::Invalid)])
        })?;
    Ok(list
        .iter()
        .map(|raw| HighlightDraft {
            block_id: raw
                .get("block_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            start: integer(raw.get("start")),
            end: integer(raw.get("end")),
            color: raw.get("color").and_then(Value::as_str).map(str::to_owned),
        })
        .collect())
}

/// A JSON integer as `i64`. One too large for `i64` is clamped to the
/// nearest extreme: still far outside any text, so the domain reports
/// `out_of_range` for it, not `invalid`. Anything else is `None`.
fn integer(value: Option<&Value>) -> Option<i64> {
    let n = value?.as_number()?;
    if let Some(i) = n.as_i64() {
        return Some(i);
    }
    if n.is_u64() {
        return Some(i64::MAX);
    }
    // Beyond u64/i64 serde keeps a float: whole and huge is still an integer.
    let f = n.as_f64()?;
    (f.is_finite() && f.abs() >= 9.0e18).then_some(if f < 0.0 { i64::MIN } else { i64::MAX })
}
