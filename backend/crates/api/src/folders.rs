//! `/folders` endpoints (contract: folders-topics §2).

use application::{MAX_PAGE_SIZE, clamp_limit};
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use domain::{FolderId, FolderSummary, PageRequest, parse_uuid};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::cursor;
use crate::error::{ApiError, ApiJson, ApiQuery};
use crate::health::format_instant;
use crate::state::AppState;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/folders", get(list).post(create))
        .route("/folders/{id}", get(show).patch(update).delete(remove))
}

#[derive(Serialize)]
pub struct FolderDto {
    id: String,
    name: String,
    position: u32,
    topic_count: u64,
    created_at: String,
    updated_at: String,
}

impl From<&FolderSummary> for FolderDto {
    fn from(s: &FolderSummary) -> Self {
        Self {
            id: s.folder.id.to_string(),
            name: s.folder.name.as_str().to_owned(),
            position: s.folder.position,
            topic_count: s.topic_count,
            created_at: format_instant(s.folder.created_at),
            updated_at: format_instant(s.folder.updated_at),
        }
    }
}

#[derive(Serialize)]
struct ListDto<T> {
    items: Vec<T>,
    next_cursor: Option<String>,
}

/// Request-body fields read from a JSON object. Deserializing through a map
/// (rather than a derived struct) rejects non-objects such as `["x"]`, which
/// serde's derive would otherwise accept in its sequence form, and keeps an
/// absent field (`None`) distinct from an explicit `null` (`Some(None)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TextField {
    Absent,
    Null,
    Value(String),
}

impl TextField {
    /// Create bodies: absent and `null` both mean "nothing provided".
    pub(crate) fn into_value(self) -> Option<String> {
        match self {
            Self::Value(s) => Some(s),
            Self::Absent | Self::Null => None,
        }
    }
}

pub(crate) fn text_field(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<TextField, String> {
    match map.get(key) {
        None => Ok(TextField::Absent),
        Some(serde_json::Value::Null) => Ok(TextField::Null),
        Some(serde_json::Value::String(s)) => Ok(TextField::Value(s.clone())),
        Some(_) => Err(format!("`{key}` must be a string or null")),
    }
}

/// Contract: an explicit `null` is reported like an empty value.
pub(crate) fn null_as_empty(v: TextField) -> Option<String> {
    match v {
        TextField::Absent => None,
        TextField::Null => Some(String::new()),
        TextField::Value(s) => Some(s),
    }
}

#[derive(Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
struct FolderBody {
    name: TextField,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for FolderBody {
    type Error = String;

    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, String> {
        Ok(Self {
            name: text_field(&map, "name")?,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    pub(crate) limit: Option<u32>,
    pub(crate) cursor: Option<String>,
}

impl ListQuery {
    /// Validates `limit` against the contract range; returns the cursor text too.
    pub(crate) fn parse(self) -> Result<(u32, Option<String>), ApiError> {
        let limit = match self.limit {
            None => clamp_limit(None),
            Some(l) if (1..=MAX_PAGE_SIZE).contains(&l) => l,
            Some(_) => {
                return Err(ApiError::bad_request(
                    format!("limit deve essere tra 1 e {MAX_PAGE_SIZE}."),
                    format!("limit must be between 1 and {MAX_PAGE_SIZE}."),
                ));
            }
        };
        Ok((limit, self.cursor))
    }
}

pub(crate) fn path_id(raw: &str) -> Result<uuid::Uuid, ApiError> {
    parse_uuid(raw).ok_or(ApiError::InvalidId)
}

async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<impl Serialize>, ApiError> {
    let (limit, cursor_text) = query.parse()?;
    let after = cursor_text
        .as_deref()
        .map(cursor::decode_folder)
        .transpose()?;
    let page = state
        .folders
        .list(user, PageRequest { after, limit })
        .await?;
    Ok(Json(ListDto {
        items: page.items.iter().map(FolderDto::from).collect(),
        next_cursor: page.next.as_ref().map(cursor::encode_folder),
    }))
}

async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    ApiJson(body): ApiJson<FolderBody>,
) -> Result<Response, ApiError> {
    let created = state
        .folders
        .create(user, body.name.into_value().as_deref())
        .await?;
    let location = format!("/api/v1/folders/{}", created.folder.id);
    let mut response = (StatusCode::CREATED, Json(FolderDto::from(&created))).into_response();
    if let Ok(value) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    Ok(response)
}

async fn show(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<FolderDto>, ApiError> {
    let id = FolderId::new(path_id(&id)?);
    Ok(Json(FolderDto::from(&state.folders.get(user, id).await?)))
}

async fn update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
    body: Result<ApiJson<FolderBody>, ApiError>,
) -> Result<Json<FolderDto>, ApiError> {
    // Order: path id, existence, then the body (whose rejection was deferred).
    let id = FolderId::new(path_id(&id)?);
    state.folders.get(user, id).await?;
    let ApiJson(body) = body?;
    let name = null_as_empty(body.name);
    let updated = state.folders.rename(user, id, name.as_deref()).await?;
    Ok(Json(FolderDto::from(&updated)))
}

async fn remove(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = FolderId::new(path_id(&id)?);
    state.folders.delete(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
