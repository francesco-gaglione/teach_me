//! The learner's handwritten strokes on a lesson (`docs/contracts/lessons.md` §11).

use std::sync::Arc;

use domain::{
    IssueKind, LessonId, LessonRepository, Stroke, StrokeDraft, StrokeRepository, StrokeTargets,
    UserId,
};
use serde_json::Value;

use crate::error::{AppError, FieldIssue};

pub struct StrokeService {
    lessons: Arc<dyn LessonRepository>,
    strokes: Arc<dyn StrokeRepository>,
}

impl StrokeService {
    #[must_use]
    pub fn new(lessons: Arc<dyn LessonRepository>, strokes: Arc<dyn StrokeRepository>) -> Self {
        Self { lessons, strokes }
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
    /// and annotating it is the typical use.
    pub async fn replace(
        &self,
        user: UserId,
        lesson_id: LessonId,
        body: &Value,
    ) -> Result<Vec<Stroke>, AppError> {
        let stored = self
            .lessons
            .get(user, lesson_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let drafts = parse_drafts(body)?;
        let valid = StrokeTargets::from_document(&stored.document)
            .validate(&drafts)
            .map_err(|issues| {
                AppError::InvalidFields(
                    issues
                        .into_iter()
                        .map(|i| FieldIssue::new(i.field, i.issue))
                        .collect(),
                )
            })?;
        self.strokes.replace(lesson_id, &valid).await?;
        Ok(valid)
    }
}

/// Reads the shape of the body; what each value means is up to the domain.
fn parse_drafts(body: &Value) -> Result<Vec<StrokeDraft>, AppError> {
    let list = body
        .get("strokes")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::InvalidFields(vec![FieldIssue::new("strokes", IssueKind::Invalid)])
        })?;
    let text = |raw: &Value, key: &str| raw.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(list
        .iter()
        .map(|raw| StrokeDraft {
            id: text(raw, "id"),
            block_id: text(raw, "block_id"),
            color: text(raw, "color"),
            width: text(raw, "width"),
            points: raw
                .get("points")
                .and_then(Value::as_array)
                // One element that is not an integer makes the whole array
                // `None`, which the domain reports as `invalid`.
                .and_then(|p| p.iter().map(|v| integer(Some(v))).collect()),
        })
        .collect())
}

/// A JSON integer as `i64`. One too large for `i64` is clamped to the
/// nearest extreme: still far outside any range, so the domain reports
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
