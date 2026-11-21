//! `/preferences` endpoints (contract: mission §2, §4).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use domain::{PageRequest, Preference, PreferenceId, TopicId, parse_uuid};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::cursor;
use crate::error::{ApiError, ApiQuery};
use crate::folders::{ListQuery, path_id};
use crate::health::format_instant;
use crate::state::AppState;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/preferences", get(list))
        .route("/preferences/{id}", delete(remove))
}

#[derive(Serialize)]
pub struct PreferenceDto {
    id: String,
    scope: &'static str,
    topic_id: Option<String>,
    text: String,
    created_at: String,
}

impl From<&Preference> for PreferenceDto {
    fn from(p: &Preference) -> Self {
        Self {
            id: p.id.to_string(),
            scope: p.scope.as_str(),
            topic_id: p.scope.topic_id().map(|t| t.to_string()),
            text: p.text.as_str().to_owned(),
            created_at: format_instant(p.created_at),
        }
    }
}

#[derive(Serialize)]
struct ListDto {
    items: Vec<PreferenceDto>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct PreferencesQuery {
    limit: Option<u32>,
    cursor: Option<String>,
    topic_id: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    ApiQuery(query): ApiQuery<PreferencesQuery>,
) -> Result<Json<ListDto>, ApiError> {
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
        .map(cursor::decode_preference)
        .transpose()?;
    let page = state
        .preferences
        .list(user, topic, PageRequest { after, limit })
        .await?;
    Ok(Json(ListDto {
        items: page.items.iter().map(PreferenceDto::from).collect(),
        next_cursor: page.next.as_ref().map(cursor::encode_preference),
    }))
}

async fn remove(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = PreferenceId::new(path_id(&id)?);
    state.preferences.delete(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
