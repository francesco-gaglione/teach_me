//! Review endpoints (contract: `reviews.md` §3).

use application::{DueReviews, ReviewView, SuspendedReviews, Vote};
use axum::Json;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use domain::{IssueKind, PageRequest, Rating, ReviewCardId, TopicId, ValidationIssue, parse_uuid};
use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::auth::AuthUser;
use crate::cursor;
use crate::error::{ApiError, ApiJson, ApiQuery};
use crate::folders::{ListQuery, path_id};
use crate::health::format_instant;
use crate::state::AppState;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/reviews/due", get(due))
        .route("/reviews/suspended", get(suspended))
        .route("/reviews/{card_id}", post(vote))
        .route("/reviews/{card_id}/undo", post(undo))
        .route("/reviews/{card_id}/suspend", post(suspend))
        .route("/reviews/{card_id}/unsuspend", post(unsuspend))
}

#[derive(Serialize)]
struct IntervalsDto {
    again: i64,
    hard: i64,
    good: i64,
    easy: i64,
}

#[derive(Serialize)]
struct ReviewCardDto {
    id: String,
    topic_id: String,
    topic_title: String,
    lesson_id: String,
    lesson_number: u32,
    lesson_title: String,
    kind: &'static str,
    to_consolidate: bool,
    front: String,
    back: String,
    status: &'static str,
    suspension: Option<&'static str>,
    suspended_at: Option<String>,
    state: &'static str,
    due: String,
    last_reviewed_at: Option<String>,
    review_count: u32,
    intervals_preview: Option<IntervalsDto>,
}

impl From<&ReviewView> for ReviewCardDto {
    fn from(v: &ReviewView) -> Self {
        let card = &v.item.card;
        Self {
            id: card.id.to_string(),
            topic_id: card.topic_id.to_string(),
            topic_title: v.item.topic_title.clone(),
            lesson_id: card.lesson_id.to_string(),
            lesson_number: v.item.lesson_number,
            lesson_title: v.item.lesson_title.clone(),
            kind: v.item.kind().as_str(),
            to_consolidate: card.to_consolidate,
            front: card.front.clone(),
            back: card.back.clone(),
            status: card.status.as_str(),
            suspension: card.suspension.map(|s| s.reason.as_str()),
            suspended_at: card.suspension.map(|s| format_instant(s.at)),
            state: card.schedule.state.as_str(),
            due: format_instant(card.schedule.due),
            last_reviewed_at: card.schedule.last_reviewed_at.map(format_instant),
            review_count: card.schedule.review_count,
            intervals_preview: v.intervals_preview.map(|i| IntervalsDto {
                again: i.again.whole_seconds(),
                hard: i.hard.whole_seconds(),
                good: i.good.whole_seconds(),
                easy: i.easy.whole_seconds(),
            }),
        }
    }
}

#[derive(Serialize)]
struct DueDto {
    items: Vec<ReviewCardDto>,
    next_cursor: Option<String>,
    due_count: u32,
    estimated_minutes: u32,
    next_due_at: Option<String>,
    topics: Vec<DueTopicDto>,
}

#[derive(Serialize)]
struct DueTopicDto {
    topic_id: String,
    topic_title: String,
    due_count: u32,
}

#[derive(serde::Deserialize)]
struct DueQuery {
    limit: Option<u32>,
    cursor: Option<String>,
    topic_id: Option<String>,
}

impl DueDto {
    fn new(d: &DueReviews, topic: Option<TopicId>) -> Self {
        Self {
            items: d.page.items.iter().map(ReviewCardDto::from).collect(),
            next_cursor: d.page.next.as_ref().map(|c| cursor::encode_due(c, topic)),
            due_count: d.due_count,
            estimated_minutes: d.estimated_minutes,
            next_due_at: d.next_due_at.map(format_instant),
            topics: d
                .topics
                .iter()
                .map(|t| DueTopicDto {
                    topic_id: t.topic_id.to_string(),
                    topic_title: t.topic_title.clone(),
                    due_count: t.due_count,
                })
                .collect(),
        }
    }
}

async fn due(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    ApiQuery(query): ApiQuery<DueQuery>,
) -> Result<Json<DueDto>, ApiError> {
    let topic = query
        .topic_id
        .as_deref()
        .map(|raw| {
            parse_uuid(raw)
                .map(TopicId::new)
                .ok_or_else(|| ApiError::bad_request("topic_id non valido.", "Invalid topic_id."))
        })
        .transpose()?;
    let (limit, cursor_text) = ListQuery {
        limit: query.limit,
        cursor: query.cursor,
    }
    .parse()?;
    let after = cursor_text
        .as_deref()
        .map(|c| cursor::decode_due(c, topic))
        .transpose()?;
    let due = state
        .reviews
        .due(user, topic, PageRequest { after, limit })
        .await?;
    Ok(Json(DueDto::new(&due, topic)))
}

#[derive(Serialize)]
struct SuspendedDto {
    items: Vec<ReviewCardDto>,
    next_cursor: Option<String>,
    suspended_count: u32,
}

impl From<&SuspendedReviews> for SuspendedDto {
    fn from(d: &SuspendedReviews) -> Self {
        Self {
            items: d.page.items.iter().map(ReviewCardDto::from).collect(),
            next_cursor: d.page.next.as_ref().map(cursor::encode_suspended),
            suspended_count: d.suspended_count,
        }
    }
}

async fn suspended(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<SuspendedDto>, ApiError> {
    let (limit, cursor_text) = query.parse()?;
    let after = cursor_text
        .as_deref()
        .map(cursor::decode_suspended)
        .transpose()?;
    let list = state
        .reviews
        .suspended(user, PageRequest { after, limit })
        .await?;
    Ok(Json(SuspendedDto::from(&list)))
}

fn invalid(field: &'static str) -> ApiError {
    ApiError::from(application::AppError::Validation(vec![
        ValidationIssue::new(field, IssueKind::Invalid),
    ]))
}

/// `{rating, review_count, reviewed_at?}` (`reviews.md` §4). `reviewed_at`
/// may be omitted by a client without an offline queue: it means now.
fn parse_vote(body: &Value) -> Result<Vote, ApiError> {
    let rating = body
        .get("rating")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<Rating>().ok())
        .ok_or_else(|| invalid("rating"))?;
    let review_count = body
        .get("review_count")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| invalid("review_count"))?;
    let reviewed_at = match body.get("reviewed_at") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(
            OffsetDateTime::parse(s, &Rfc3339)
                .map_err(|_| invalid("reviewed_at"))?
                .to_offset(time::UtcOffset::UTC),
        ),
        Some(_) => return Err(invalid("reviewed_at")),
    };
    Ok(Vote {
        rating,
        reviewed_at,
        review_count,
    })
}

async fn vote(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(card_id): Path<String>,
    body: Result<ApiJson<Value>, ApiError>,
) -> Result<Json<ReviewCardDto>, ApiError> {
    let id = ReviewCardId::new(path_id(&card_id)?);
    let ApiJson(body) = body?;
    let vote = parse_vote(&body)?;
    let view = state.reviews.rate(user, id, vote).await?;
    Ok(Json(ReviewCardDto::from(&view)))
}

async fn undo(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(card_id): Path<String>,
) -> Result<Json<ReviewCardDto>, ApiError> {
    let id = ReviewCardId::new(path_id(&card_id)?);
    let view = state.reviews.undo(user, id).await?;
    Ok(Json(ReviewCardDto::from(&view)))
}

async fn suspend(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(card_id): Path<String>,
) -> Result<Json<ReviewCardDto>, ApiError> {
    let id = ReviewCardId::new(path_id(&card_id)?);
    let view = state.reviews.suspend(user, id).await?;
    Ok(Json(ReviewCardDto::from(&view)))
}

async fn unsuspend(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(card_id): Path<String>,
) -> Result<Json<ReviewCardDto>, ApiError> {
    let id = ReviewCardId::new(path_id(&card_id)?);
    let view = state.reviews.unsuspend(user, id).await?;
    Ok(Json(ReviewCardDto::from(&view)))
}
