//! `/folders/{id}/topics` and `/topics` endpoints (contract: folders-topics §2).

use application::UpdateTopic;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use domain::{FolderId, PageRequest, Topic, TopicId};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::cursor;
use crate::error::{ApiError, ApiJson, ApiQuery};
use crate::folders::{ListQuery, TextField, null_as_empty, path_id, text_field};
use crate::health::format_instant;
use crate::state::AppState;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/folders/{id}/topics", get(list).post(create))
        .route("/topics/{id}", get(show).patch(update).delete(remove))
}

#[derive(Serialize)]
pub struct TopicDto {
    id: String,
    folder_id: String,
    title: String,
    language: &'static str,
    state: &'static str,
    created_at: String,
    updated_at: String,
}

impl From<&Topic> for TopicDto {
    fn from(t: &Topic) -> Self {
        Self {
            id: t.id.to_string(),
            folder_id: t.folder_id.to_string(),
            title: t.title.as_str().to_owned(),
            language: t.language.code(),
            state: t.state.as_str(),
            created_at: format_instant(t.created_at),
            updated_at: format_instant(t.updated_at),
        }
    }
}

#[derive(Serialize)]
struct ListDto {
    items: Vec<TopicDto>,
    next_cursor: Option<String>,
}

/// Unknown fields (`state`, `id`, dates...) are ignored by serde's default.
#[derive(Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
struct TopicBody {
    title: TextField,
    folder_id: TextField,
    language: LanguageField,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for TopicBody {
    type Error = String;

    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, String> {
        Ok(Self {
            title: text_field(&map, "title")?,
            folder_id: text_field(&map, "folder_id")?,
            language: language_field(&map)?,
        })
    }
}

/// `language` of a create body: absent and `null` both mean the default. Any
/// string goes to the use case, which reports `invalid`; PATCH never reads it.
struct LanguageField(Option<String>);

fn language_field(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<LanguageField, String> {
    Ok(LanguageField(text_field(map, "language")?.into_value()))
}

async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(folder_id): Path<String>,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<ListDto>, ApiError> {
    let folder_id = FolderId::new(path_id(&folder_id)?);
    let (limit, cursor_text) = query.parse()?;
    let after = cursor_text
        .as_deref()
        .map(cursor::decode_topic)
        .transpose()?;
    let page = state
        .topics
        .list(user, folder_id, PageRequest { after, limit })
        .await?;
    Ok(Json(ListDto {
        items: page.items.iter().map(TopicDto::from).collect(),
        next_cursor: page.next.as_ref().map(cursor::encode_topic),
    }))
}

async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(folder_id): Path<String>,
    body: Result<ApiJson<TopicBody>, ApiError>,
) -> Result<Response, ApiError> {
    // Order: path id, existence, then the body (whose rejection was deferred).
    let folder_id = FolderId::new(path_id(&folder_id)?);
    state.folders.get(user, folder_id).await?;
    let ApiJson(body) = body?;
    let topic = state
        .topics
        .create_with_language(
            user,
            folder_id,
            body.title.into_value().as_deref(),
            body.language.0.as_deref(),
        )
        .await?;
    let location = format!("/api/v1/topics/{}", topic.id);
    let mut response = (StatusCode::CREATED, Json(TopicDto::from(&topic))).into_response();
    if let Ok(value) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    Ok(response)
}

async fn show(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<TopicDto>, ApiError> {
    let id = TopicId::new(path_id(&id)?);
    Ok(Json(TopicDto::from(&state.topics.get(user, id).await?)))
}

async fn update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    body: Result<ApiJson<TopicBody>, ApiError>,
) -> Result<Json<TopicDto>, ApiError> {
    let id = TopicId::new(path_id(&id)?);
    state.topics.get(user, id).await?;
    let ApiJson(body) = body?;
    let patch = UpdateTopic {
        title: null_as_empty(body.title),
        // `null` becomes "" which the use case reports as `invalid`.
        folder_id: null_as_empty(body.folder_id),
    };
    Ok(Json(TopicDto::from(
        &state.topics.update(user, id, patch).await?,
    )))
}

async fn remove(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = TopicId::new(path_id(&id)?);
    state.topics.delete(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
