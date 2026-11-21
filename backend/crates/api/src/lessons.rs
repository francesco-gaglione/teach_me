//! Lesson endpoints (contract: `lessons.md` §3).

use application::{CompletionView, DocumentSource, InProgressItem, LessonItem, LessonView};
use axum::Json;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use domain::{
    Attempt, GlossaryTerm, Highlight, LearningRecord, Lesson, LessonId, PageRequest, Stroke,
    TopicId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::AuthUser;
use crate::cursor;
use crate::error::{ApiError, ApiJson, ApiQuery};
use crate::folders::{ListQuery, path_id};
use crate::health::format_instant;
use crate::jobs::JobDto;
use crate::state::AppState;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/topics/{id}/lessons", get(list).post(generate))
        .route("/lessons", get(list_in_progress))
        .route("/lessons/{id}", get(show))
        .route("/lessons/{id}/attempts", post(record_attempts))
        .route("/lessons/{id}/highlights", put(replace_highlights))
        .route("/lessons/{id}/complete", post(complete))
        .route("/topics/{id}/glossary", get(glossary))
        .route("/topics/{id}/learning-records", get(records))
}

/// Kept apart from `routes` because of its own body limit (4 MiB, set by
/// `app::assemble`); axum's 2 MiB default would stop it first.
pub fn strokes_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/lessons/{id}/strokes", put(replace_strokes))
        .layer(DefaultBodyLimit::disable())
}

#[derive(Serialize)]
struct QualityWarningDto {
    code: String,
    block_id: Option<String>,
    message: String,
}

fn warnings_dto(l: &Lesson) -> Vec<QualityWarningDto> {
    l.quality_warnings
        .iter()
        .map(|w| QualityWarningDto {
            code: w.code.clone(),
            block_id: w.block_id.clone(),
            message: w.message.clone(),
        })
        .collect()
}

#[derive(Serialize)]
struct DocumentSourceDto {
    document_id: String,
    title: String,
    page_start: u32,
    page_end: u32,
}

impl From<&DocumentSource> for DocumentSourceDto {
    fn from(s: &DocumentSource) -> Self {
        Self {
            document_id: s.document_id.to_string(),
            title: s.title.clone(),
            page_start: s.page_start,
            page_end: s.page_end,
        }
    }
}

fn sources_dto(sources: &[DocumentSource]) -> Vec<DocumentSourceDto> {
    sources.iter().map(DocumentSourceDto::from).collect()
}

#[derive(Serialize)]
struct LessonDto {
    document_sources: Vec<DocumentSourceDto>,
    quality_warnings: Vec<QualityWarningDto>,
    id: String,
    topic_id: String,
    number: u32,
    title: String,
    objective: String,
    estimated_minutes: u32,
    status: &'static str,
    catalog_version: u32,
    mission_version: u32,
    created_at: String,
    completed_at: Option<String>,
}

impl LessonDto {
    fn new(l: &Lesson, sources: &[DocumentSource]) -> Self {
        Self {
            document_sources: sources_dto(sources),
            quality_warnings: warnings_dto(l),
            id: l.id.to_string(),
            topic_id: l.topic_id.to_string(),
            number: l.number,
            title: l.title.clone(),
            objective: l.objective.clone(),
            estimated_minutes: l.estimated_minutes,
            status: match l.status {
                domain::LessonStatus::NotStarted => "not_started",
                domain::LessonStatus::InProgress => "in_progress",
                domain::LessonStatus::Completed => "completed",
            },
            catalog_version: l.catalog_version,
            mission_version: l.mission_version,
            created_at: format_instant(l.created_at),
            completed_at: l.completed_at.map(format_instant),
        }
    }
}

impl From<&LessonItem> for LessonDto {
    fn from(i: &LessonItem) -> Self {
        Self::new(&i.lesson, &i.document_sources)
    }
}

#[derive(Serialize)]
struct InProgressDto {
    #[serde(flatten)]
    lesson: LessonDto,
    topic_title: String,
    last_activity_at: String,
}

impl From<&InProgressItem> for InProgressDto {
    fn from(i: &InProgressItem) -> Self {
        Self {
            lesson: LessonDto::from(&i.item),
            topic_title: i.topic_title.clone(),
            last_activity_at: format_instant(i.last_activity_at),
        }
    }
}

#[derive(Serialize)]
struct GradeDto {
    verdict: &'static str,
    feedback: String,
    misconception: Option<String>,
}

#[derive(Serialize)]
struct AttemptDto {
    id: String,
    block_id: String,
    block_type: &'static str,
    attempt_number: u8,
    answer: Value,
    outcome: &'static str,
    grade: Option<GradeDto>,
    answered_at: String,
    created_at: String,
}

impl From<&Attempt> for AttemptDto {
    fn from(a: &Attempt) -> Self {
        Self {
            id: a.id.to_string(),
            block_id: a.block_id.clone(),
            block_type: a.kind.as_str(),
            attempt_number: a.attempt_number,
            answer: a.answer.to_json(),
            outcome: a.outcome.as_str(),
            grade: a.grade.as_ref().map(|g| GradeDto {
                verdict: g.verdict.as_str(),
                feedback: g.feedback.clone(),
                misconception: g.misconception.clone(),
            }),
            answered_at: format_instant(a.answered_at),
            created_at: format_instant(a.created_at),
        }
    }
}

#[derive(Serialize)]
struct HighlightDto {
    block_id: String,
    start: u32,
    end: u32,
    color: &'static str,
}

impl From<&Highlight> for HighlightDto {
    fn from(h: &Highlight) -> Self {
        Self {
            block_id: h.block_id.clone(),
            start: h.start,
            end: h.end,
            color: h.color.as_str(),
        }
    }
}

#[derive(Serialize)]
struct StrokeDto {
    id: String,
    block_id: String,
    color: &'static str,
    width: &'static str,
    points: Vec<i32>,
}

impl From<&Stroke> for StrokeDto {
    fn from(s: &Stroke) -> Self {
        Self {
            id: s.id.clone(),
            block_id: s.block_id.clone(),
            color: s.color.as_str(),
            width: s.width.as_str(),
            points: s.points.clone(),
        }
    }
}

#[derive(Serialize)]
struct RecordDto {
    id: String,
    number: u32,
    kind: &'static str,
    title: String,
    body: String,
    evidence: String,
    status: &'static str,
    superseded_by: Option<String>,
    lesson_id: String,
    created_at: String,
}

impl From<&LearningRecord> for RecordDto {
    fn from(r: &LearningRecord) -> Self {
        Self {
            id: r.id.to_string(),
            number: r.number,
            kind: r.kind.as_str(),
            title: r.title.clone(),
            body: r.body.clone(),
            evidence: r.evidence.clone(),
            status: r.status.as_str(),
            superseded_by: r.superseded_by.map(|b| b.to_string()),
            lesson_id: r.lesson_id.to_string(),
            created_at: format_instant(r.created_at),
        }
    }
}

#[derive(Serialize)]
struct TermDto {
    id: String,
    key: String,
    term: String,
    definition: String,
    avoid: Vec<String>,
    lesson_id: String,
    first_lesson_id: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<&GlossaryTerm> for TermDto {
    fn from(t: &GlossaryTerm) -> Self {
        Self {
            id: t.id.to_string(),
            key: t.key.clone(),
            term: t.term.clone(),
            definition: t.definition.clone(),
            avoid: t.avoid.clone(),
            lesson_id: t.lesson_id.to_string(),
            first_lesson_id: t.first_lesson_id.map(|l| l.to_string()),
            created_at: format_instant(t.created_at),
            updated_at: format_instant(t.updated_at),
        }
    }
}

#[derive(Serialize)]
struct CompletionDto {
    job: Option<JobDto>,
    records: Vec<RecordDto>,
    terms: Vec<TermDto>,
    review_card_count: u32,
}

impl From<&CompletionView> for CompletionDto {
    fn from(c: &CompletionView) -> Self {
        Self {
            job: c.job.as_ref().map(JobDto::from),
            records: c.outputs.records.iter().map(RecordDto::from).collect(),
            terms: c.outputs.terms.iter().map(TermDto::from).collect(),
            review_card_count: c.outputs.review_card_count,
        }
    }
}

#[derive(Serialize)]
struct LessonViewDto {
    lesson: LessonDto,
    language: &'static str,
    quality_warnings: Vec<QualityWarningDto>,
    document: Value,
    attempts: Vec<AttemptDto>,
    highlights: Vec<HighlightDto>,
    strokes: Vec<StrokeDto>,
    completion: Option<CompletionDto>,
}

impl From<&LessonView> for LessonViewDto {
    fn from(v: &LessonView) -> Self {
        Self {
            lesson: LessonDto::new(&v.lesson, &v.document_sources),
            language: v.language.code(),
            quality_warnings: warnings_dto(&v.lesson),
            document: v.document.clone(),
            attempts: v.attempts.iter().map(AttemptDto::from).collect(),
            highlights: v.highlights.iter().map(HighlightDto::from).collect(),
            strokes: v.strokes.iter().map(StrokeDto::from).collect(),
            completion: v.completion.as_ref().map(CompletionDto::from),
        }
    }
}

#[derive(Serialize)]
struct ListDto<T> {
    items: Vec<T>,
    next_cursor: Option<String>,
}

async fn generate(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
) -> Result<Response, ApiError> {
    // No body is read: an empty one and `{}` are both fine.
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let job = state.lessons.request_generation(user, topic_id).await?;
    Ok((StatusCode::ACCEPTED, Json(JobDto::from(&job))).into_response())
}

async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<ListDto<LessonDto>>, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let (limit, cursor_text) = query.parse()?;
    let after = cursor_text
        .as_deref()
        .map(cursor::decode_lesson)
        .transpose()?;
    let page = state
        .lessons
        .list(user, topic_id, PageRequest { after, limit })
        .await?;
    Ok(Json(ListDto {
        items: page.items.iter().map(LessonDto::from).collect(),
        next_cursor: page.next.as_ref().map(cursor::encode_lesson),
    }))
}

#[derive(Deserialize)]
struct InProgressQuery {
    status: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
}

async fn list_in_progress(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    ApiQuery(query): ApiQuery<InProgressQuery>,
) -> Result<Json<ListDto<InProgressDto>>, ApiError> {
    if query.status.as_deref() != Some("in_progress") {
        return Err(ApiError::bad_request(
            "status è obbligatorio e deve essere in_progress.",
            "status is required and must be in_progress.",
        ));
    }
    let (limit, cursor_text) = ListQuery {
        limit: query.limit,
        cursor: query.cursor,
    }
    .parse()?;
    let after = cursor_text
        .as_deref()
        .map(cursor::decode_in_progress)
        .transpose()?;
    let page = state
        .lessons
        .list_in_progress(user, PageRequest { after, limit })
        .await?;
    Ok(Json(ListDto {
        items: page.items.iter().map(InProgressDto::from).collect(),
        next_cursor: page.next.as_ref().map(cursor::encode_in_progress),
    }))
}

async fn show(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(lesson_id): Path<String>,
) -> Result<Json<LessonViewDto>, ApiError> {
    let lesson_id = LessonId::new(path_id(&lesson_id)?);
    let view = state.lessons.view(user, lesson_id).await?;
    Ok(Json(LessonViewDto::from(&view)))
}

#[derive(Serialize)]
struct AttemptsDto {
    attempts: Vec<AttemptDto>,
}

async fn record_attempts(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(lesson_id): Path<String>,
    body: Result<ApiJson<Value>, ApiError>,
) -> Result<Json<AttemptsDto>, ApiError> {
    let lesson_id = LessonId::new(path_id(&lesson_id)?);
    // Same order as the other endpoints: id, existence, then the body.
    state.lessons.get(user, lesson_id).await?;
    let ApiJson(body) = body?;
    let attempts = state.attempts.record(user, lesson_id, &body).await?;
    Ok(Json(AttemptsDto {
        attempts: attempts.iter().map(AttemptDto::from).collect(),
    }))
}

#[derive(Serialize)]
struct HighlightsDto {
    highlights: Vec<HighlightDto>,
}

async fn replace_highlights(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(lesson_id): Path<String>,
    body: Result<ApiJson<Value>, ApiError>,
) -> Result<Json<HighlightsDto>, ApiError> {
    let lesson_id = LessonId::new(path_id(&lesson_id)?);
    // Same order as the other endpoints: id, existence, then the body.
    state.highlights.ensure_lesson(user, lesson_id).await?;
    let ApiJson(body) = body?;
    let highlights = state.highlights.replace(user, lesson_id, &body).await?;
    Ok(Json(HighlightsDto {
        highlights: highlights.iter().map(HighlightDto::from).collect(),
    }))
}

#[derive(Serialize)]
struct StrokesDto {
    strokes: Vec<StrokeDto>,
}

async fn replace_strokes(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(lesson_id): Path<String>,
    body: Result<ApiJson<Value>, ApiError>,
) -> Result<Json<StrokesDto>, ApiError> {
    let lesson_id = LessonId::new(path_id(&lesson_id)?);
    // Same order as the other endpoints: id, existence, then the body.
    state.strokes.ensure_lesson(user, lesson_id).await?;
    let ApiJson(body) = body?;
    let strokes = state.strokes.replace(user, lesson_id, &body).await?;
    Ok(Json(StrokesDto {
        strokes: strokes.iter().map(StrokeDto::from).collect(),
    }))
}

async fn complete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(lesson_id): Path<String>,
) -> Result<Response, ApiError> {
    let lesson_id = LessonId::new(path_id(&lesson_id)?);
    let job = state.lessons.complete(user, lesson_id).await?;
    Ok((StatusCode::ACCEPTED, Json(JobDto::from(&job))).into_response())
}

async fn glossary(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<ListDto<TermDto>>, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let (limit, cursor_text) = query.parse()?;
    let after = cursor_text
        .as_deref()
        .map(cursor::decode_term)
        .transpose()?;
    let page = state
        .lessons
        .glossary(user, topic_id, PageRequest { after, limit })
        .await?;
    Ok(Json(ListDto {
        items: page.items.iter().map(TermDto::from).collect(),
        next_cursor: page.next.as_ref().map(cursor::encode_term),
    }))
}

#[derive(Deserialize)]
struct RecordsQuery {
    limit: Option<u32>,
    cursor: Option<String>,
    status: Option<String>,
}

async fn records(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
    ApiQuery(query): ApiQuery<RecordsQuery>,
) -> Result<Json<ListDto<RecordDto>>, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let active_only = match query.status.as_deref() {
        None | Some("active") => true,
        Some("all") => false,
        Some(_) => {
            return Err(ApiError::bad_request(
                "status deve essere active o all.",
                "status must be active or all.",
            ));
        }
    };
    let (limit, cursor_text) = ListQuery {
        limit: query.limit,
        cursor: query.cursor,
    }
    .parse()?;
    let after = cursor_text
        .as_deref()
        .map(cursor::decode_record)
        .transpose()?;
    let page = state
        .lessons
        .records(user, topic_id, active_only, PageRequest { after, limit })
        .await?;
    Ok(Json(ListDto {
        items: page.items.iter().map(RecordDto::from).collect(),
        next_cursor: page.next.as_ref().map(cursor::encode_record),
    }))
}
