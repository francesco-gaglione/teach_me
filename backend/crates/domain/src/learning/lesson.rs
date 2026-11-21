use std::fmt;
use std::str::FromStr;

use serde_json::Value;
use time::OffsetDateTime;

use super::UnknownLearningValue;
use crate::ids::{ComponentRequestId, JobId, LessonId, TopicId};

/// A non-blocking quality warning kept with the lesson (`lessons.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonQualityWarning {
    pub code: String,
    pub block_id: Option<String>,
    /// Italian text for the learner.
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LessonStatus {
    NotStarted,
    /// The first attempt was recorded.
    InProgress,
    Completed,
}

impl LessonStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }
}

impl FromStr for LessonStatus {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "not_started" => Ok(Self::NotStarted),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

impl fmt::Display for LessonStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The lesson as the API shows it. The document and the plan are stored next
/// to it ([`StoredLesson`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lesson {
    /// Non-blocking quality warnings of the saved document (issue #14).
    pub quality_warnings: Vec<LessonQualityWarning>,
    pub id: LessonId,
    pub topic_id: TopicId,
    /// 1, 2, 3... in the topic, assigned when the lesson is stored.
    pub number: u32,
    pub title: String,
    pub objective: String,
    pub estimated_minutes: u32,
    pub status: LessonStatus,
    pub catalog_version: u32,
    pub mission_version: u32,
    pub created_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}

impl Lesson {
    /// What the learner is told about the lesson at a given moment; also the
    /// check done before accepting attempts.
    #[must_use]
    pub fn is_completed(&self) -> bool {
        self.status == LessonStatus::Completed
    }
}

/// A lesson with its full document (rubrics included) and the Planner's plan.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredLesson {
    pub lesson: Lesson,
    pub document: Value,
    /// What the Planner decided for it.
    pub plan: Value,
}

/// The facts of a document that the lesson row repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonMeta {
    pub title: String,
    pub objective: String,
    pub estimated_minutes: u32,
    pub catalog_version: u32,
}

impl LessonMeta {
    /// Reads the header of a document that already passed the Validator.
    #[must_use]
    pub fn from_document(document: &Value) -> Option<Self> {
        let text = |key: &str| document.get(key)?.as_str().map(str::to_owned);
        let number = |key: &str| {
            document
                .get(key)?
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
        };
        Some(Self {
            title: text("title")?,
            objective: text("objective")?,
            estimated_minutes: number("estimated_minutes")?,
            catalog_version: number("catalog_version")?,
        })
    }
}

/// A block of the lesson that asks for a component the catalog does not have
/// (`missing_component`): stored so the missing ones can be prioritized.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentRequest {
    pub id: ComponentRequestId,
    pub topic_id: TopicId,
    pub lesson_id: LessonId,
    pub block_id: String,
    pub wanted: String,
    pub purpose: String,
    /// The blocks the app shows instead.
    pub fallback: Value,
    pub created_at: OffsetDateTime,
}

/// How many recent requests a group shows.
pub const COMPONENT_REQUEST_EXAMPLES: usize = 3;

/// The requests for one `wanted` name, across all lessons (the ranking that
/// tells which component to build next).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentRequestGroup {
    pub wanted: String,
    pub count: u32,
    pub first_requested_at: OffsetDateTime,
    pub last_requested_at: OffsetDateTime,
    /// The highest catalog version the component was asked with.
    pub catalog_version: u32,
    /// The most recent requests, newest first, at most
    /// [`COMPONENT_REQUEST_EXAMPLES`].
    pub examples: Vec<ComponentRequestExample>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentRequestExample {
    pub purpose: String,
    pub lesson_id: LessonId,
    pub lesson_title: String,
    pub topic_title: String,
    pub requested_at: OffsetDateTime,
}

/// Keyset position in the ranking: `count` desc, `last_requested_at` desc,
/// `wanted` asc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentRequestCursor {
    pub count: u32,
    pub last_requested_at: OffsetDateTime,
    pub wanted: String,
}

impl ComponentRequestCursor {
    #[must_use]
    pub fn after(group: &ComponentRequestGroup) -> Self {
        Self {
            count: group.count,
            last_requested_at: group.last_requested_at,
            wanted: group.wanted.clone(),
        }
    }
}

/// A request found in a document, before it has an id.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentRequestDraft {
    pub block_id: String,
    pub wanted: String,
    pub purpose: String,
    pub fallback: Value,
}

/// One draft per top-level `missing_component` block (they cannot nest).
#[must_use]
pub fn component_request_drafts(document: &Value) -> Vec<ComponentRequestDraft> {
    let Some(blocks) = document.get("blocks").and_then(Value::as_array) else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("missing_component"))
        .filter_map(|b| {
            let text = |key: &str| b.get(key)?.as_str().map(str::to_owned);
            Some(ComponentRequestDraft {
                block_id: text("id")?,
                wanted: text("wanted")?,
                purpose: text("purpose")?,
                fallback: b.get("fallback").cloned().unwrap_or(Value::Array(vec![])),
            })
        })
        .collect()
}

/// Everything saved when a generation succeeds, in one transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedLesson {
    pub quality_warnings: Vec<LessonQualityWarning>,
    pub id: LessonId,
    pub topic_id: TopicId,
    /// The `lesson_generation` job that produced it.
    pub job_id: JobId,
    pub meta: LessonMeta,
    pub mission_version: u32,
    /// The complete document, `rubric` included.
    pub document: Value,
    /// What the Planner decided; kept for later lessons and debugging.
    pub plan: Value,
    pub component_requests: Vec<ComponentRequest>,
    pub created_at: OffsetDateTime,
}

/// Keyset position in the lesson list (`number`, newest first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LessonCursor {
    pub number: u32,
}

impl LessonCursor {
    #[must_use]
    pub fn after(lesson: &Lesson) -> Self {
        Self {
            number: lesson.number,
        }
    }
}

/// A lesson in progress, for the cross-topic "continue" list (issue #13).
#[derive(Debug, Clone, PartialEq)]
pub struct InProgressLesson {
    pub lesson: Lesson,
    pub topic_title: String,
    /// When the most recent attempt was stored.
    pub last_activity_at: OffsetDateTime,
    /// The Planner's plan, to derive the document sources.
    pub plan: Value,
}

/// Keyset position in the in-progress list: `last_activity_at` descending,
/// then `id` ascending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InProgressCursor {
    pub last_activity_at: OffsetDateTime,
    pub id: LessonId,
}

impl InProgressCursor {
    #[must_use]
    pub fn after(item: &InProgressLesson) -> Self {
        Self {
            last_activity_at: item.last_activity_at,
            id: item.lesson.id,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn collects_missing_components_with_their_fallback() {
        let doc = json!({"blocks": [
            {"id": "a", "type": "paragraph", "text": "x"},
            {"id": "m", "type": "missing_component", "wanted": "slider", "purpose": "p",
             "fallback": [{"id": "f", "type": "paragraph", "text": "y"}]}
        ]});
        let drafts = component_request_drafts(&doc);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].block_id, "m");
        assert_eq!(drafts[0].wanted, "slider");
        assert_eq!(drafts[0].fallback[0]["id"], "f");
    }

    #[test]
    fn meta_needs_every_header_field() {
        let doc =
            json!({"title": "t", "objective": "o", "estimated_minutes": 8, "catalog_version": 1});
        assert_eq!(
            LessonMeta::from_document(&doc).unwrap().estimated_minutes,
            8
        );
        assert!(LessonMeta::from_document(&json!({"title": "t"})).is_none());
    }
}
