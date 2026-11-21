//! `/topics/{id}/jobs` endpoints (contract proposal: jobs).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use domain::{Job, JobId, JobResult, PageRequest, TopicEventPublisher, TopicId};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::chat::ErrorDto;
use crate::cursor;
use crate::error::{ApiError, ApiQuery};
use crate::folders::{ListQuery, path_id};
use crate::health::format_instant;
use crate::state::AppState;

/// Position of the event stream the listed jobs correspond to (same as chat).
const LAST_EVENT_ID_HEADER: &str = "x-last-event-id";

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/topics/{id}/jobs", get(list))
        .route("/topics/{id}/jobs/{job_id}/retry", post(retry))
}

#[derive(Serialize)]
struct ProgressDto {
    step: String,
    message: Option<String>,
    done: Option<u32>,
    total: Option<u32>,
}

/// Shape depends on the job's kind (`resources.md` §4).
#[derive(Serialize)]
#[serde(untagged)]
enum ResultDto {
    Research {
        resource_count: u32,
        gap_count: u32,
    },
    LessonGeneration {
        lesson_id: String,
        lesson_number: u32,
    },
    LessonCompletion {
        lesson_id: String,
        record_count: u32,
        term_count: u32,
        review_card_count: u32,
    },
    DocumentImport {
        document_id: String,
        page_count: u32,
        section_count: u32,
    },
}

impl From<&JobResult> for ResultDto {
    fn from(result: &JobResult) -> Self {
        match *result {
            JobResult::Research {
                resource_count,
                gap_count,
            } => Self::Research {
                resource_count,
                gap_count,
            },
            JobResult::LessonGeneration {
                lesson_id,
                lesson_number,
            } => Self::LessonGeneration {
                lesson_id: lesson_id.to_string(),
                lesson_number,
            },
            JobResult::LessonCompletion {
                lesson_id,
                record_count,
                term_count,
                review_card_count,
            } => Self::LessonCompletion {
                lesson_id: lesson_id.to_string(),
                record_count,
                term_count,
                review_card_count,
            },
            JobResult::DocumentImport {
                document_id,
                page_count,
                section_count,
            } => Self::DocumentImport {
                document_id: document_id.to_string(),
                page_count,
                section_count,
            },
        }
    }
}

#[derive(Serialize)]
pub struct JobDto {
    id: String,
    topic_id: String,
    lesson_id: Option<String>,
    /// Only `document_import` jobs: the document being imported.
    document_id: Option<String>,
    kind: &'static str,
    status: &'static str,
    progress: Option<ProgressDto>,
    error: Option<ErrorDto>,
    result: Option<ResultDto>,
    attempt: u32,
    created_at: String,
    started_at: Option<String>,
    finished_at: Option<String>,
}

impl From<&Job> for JobDto {
    fn from(j: &Job) -> Self {
        Self {
            id: j.id.to_string(),
            topic_id: j.topic_id.to_string(),
            lesson_id: j.lesson_id.map(|l| l.to_string()),
            document_id: j.document_id.map(|d| d.to_string()),
            kind: j.kind.as_str(),
            status: j.status.as_str(),
            progress: j.progress.as_ref().map(|p| ProgressDto {
                step: p.step.clone(),
                message: p.message.clone(),
                done: p.done,
                total: p.total,
            }),
            error: j
                .error
                .as_ref()
                .map(|e| ErrorDto::new(e.code.as_str(), &e.message)),
            result: j.result.as_ref().map(ResultDto::from),
            attempt: j.attempt,
            created_at: format_instant(j.created_at),
            started_at: j.started_at.map(format_instant),
            finished_at: j.finished_at.map(format_instant),
        }
    }
}

#[derive(Serialize)]
struct ListDto {
    items: Vec<JobDto>,
    next_cursor: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Response, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let (limit, cursor_text) = query.parse()?;
    let after = cursor_text.as_deref().map(cursor::decode_job).transpose()?;
    // Read before the jobs, so everything up to this id is reflected in them.
    let last_event_id = state.events.latest_event_id(topic_id);
    let page = state
        .jobs
        .list(user, topic_id, PageRequest { after, limit })
        .await?;
    let body = Json(ListDto {
        items: page.items.iter().map(JobDto::from).collect(),
        next_cursor: page.next.as_ref().map(cursor::encode_job),
    });
    Ok(([(LAST_EVENT_ID_HEADER, last_event_id.to_string())], body).into_response())
}

async fn retry(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((topic_id, job_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let job_id = JobId::new(path_id(&job_id)?);
    let job = state.jobs.retry(user, topic_id, job_id).await?;
    Ok((StatusCode::ACCEPTED, Json(JobDto::from(&job))).into_response())
}
