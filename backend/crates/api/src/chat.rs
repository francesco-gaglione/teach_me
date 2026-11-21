//! `/topics/{id}/messages` endpoints (contract: chat §1-§2).

use application::SentTurn;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use domain::{
    ChatMessage, LessonId, MessageId, MessagePart, MessageRole, MessageStatus, PageRequest, TopicId,
};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::cursor;
use crate::error::{ApiError, ApiJson, ApiQuery, ErrorDetail};
use crate::folders::{ListQuery, TextField, path_id, text_field};
use crate::health::format_instant;
use crate::mission::MissionDto;
use crate::state::AppState;

/// Position of the event stream the listed messages correspond to (chat.md §2).
const LAST_EVENT_ID_HEADER: &str = "x-last-event-id";

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/topics/{id}/messages", get(list).post(send))
        .route("/topics/{id}/messages/{message_id}/retry", post(retry))
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PartDto {
    Text {
        text: String,
    },
    MissionProposal {
        mission: Box<MissionDto>,
        replaces_version: Option<u32>,
    },
    LessonRequest {
        job_id: String,
    },
}

#[derive(Serialize)]
pub struct ErrorDto {
    code: &'static str,
    message: String,
}

impl ErrorDto {
    pub(crate) fn new(code: &'static str, message: &str) -> Self {
        Self {
            code,
            message: message.to_owned(),
        }
    }
}

#[derive(Serialize)]
pub struct MessageDto {
    id: String,
    topic_id: String,
    role: &'static str,
    status: &'static str,
    parts: Vec<PartDto>,
    error: Option<ErrorDto>,
    created_at: String,
    last_event_id: Option<u64>,
    updated_at: String,
}

impl From<&ChatMessage> for MessageDto {
    fn from(m: &ChatMessage) -> Self {
        Self {
            id: m.id.to_string(),
            topic_id: m.topic_id.to_string(),
            role: match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
            },
            status: match m.status {
                MessageStatus::Streaming => "streaming",
                MessageStatus::Complete => "complete",
                MessageStatus::Failed => "failed",
            },
            parts: m
                .parts
                .iter()
                .map(|part| match part {
                    MessagePart::Text(text) => PartDto::Text { text: text.clone() },
                    MessagePart::MissionProposal {
                        mission,
                        replaces_version,
                    } => PartDto::MissionProposal {
                        mission: Box::new(MissionDto::from(mission)),
                        replaces_version: *replaces_version,
                    },
                    MessagePart::LessonRequest { job_id } => PartDto::LessonRequest {
                        job_id: job_id.to_string(),
                    },
                })
                .collect(),
            error: m.error.as_ref().map(|e| ErrorDto {
                code: e.code.as_str(),
                message: e.message.clone(),
            }),
            created_at: format_instant(m.created_at),
            last_event_id: m.last_event_id,
            updated_at: format_instant(m.updated_at),
        }
    }
}

#[derive(Serialize)]
struct ListDto {
    items: Vec<MessageDto>,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
struct SendResponse {
    user_message: MessageDto,
    assistant_message: MessageDto,
}

/// `{ "text": ... }`; anything else in the object is ignored.
#[derive(Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
struct SendBody {
    text: TextField,
    /// Set when the chat was opened from a lesson (`lessons.md` §7).
    lesson_id: TextField,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for SendBody {
    type Error = String;

    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, String> {
        Ok(Self {
            text: text_field(&map, "text")?,
            lesson_id: text_field(&map, "lesson_id")?,
        })
    }
}

async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Response, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let (limit, cursor_text) = query.parse()?;
    let after = cursor_text
        .as_deref()
        .map(cursor::decode_message)
        .transpose()?;
    let listed = state
        .chat
        .list(user, topic_id, PageRequest { after, limit })
        .await?;
    let body = Json(ListDto {
        items: listed.page.items.iter().map(MessageDto::from).collect(),
        next_cursor: listed.page.next.as_ref().map(cursor::encode_message),
    });
    Ok((
        [(LAST_EVENT_ID_HEADER, listed.last_event_id.to_string())],
        body,
    )
        .into_response())
}

async fn send(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
    body: Result<ApiJson<SendBody>, ApiError>,
) -> Result<Response, ApiError> {
    // Same order as the other endpoints: path id, existence, then the body.
    let topic_id = TopicId::new(path_id(&topic_id)?);
    state.topics.get(user, topic_id).await?;
    let ApiJson(body) = body?;
    let lesson_id = match body.lesson_id.into_value() {
        None => None,
        Some(raw) => Some(LessonId::new(domain::parse_uuid(&raw).ok_or_else(
            || {
                ApiError::Validation(vec![ErrorDetail {
                    field: "lesson_id".to_owned(),
                    issue: "invalid".to_owned(),
                }])
            },
        )?)),
    };
    let SentTurn { user, assistant } = state
        .chat
        .send_about(user, topic_id, body.text.into_value().as_deref(), lesson_id)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(SendResponse {
            user_message: MessageDto::from(&user),
            assistant_message: MessageDto::from(&assistant),
        }),
    )
        .into_response())
}

async fn retry(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((topic_id, message_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let message_id = MessageId::new(path_id(&message_id)?);
    let message = state.chat.retry(user, topic_id, message_id).await?;
    Ok((StatusCode::ACCEPTED, Json(MessageDto::from(&message))).into_response())
}
