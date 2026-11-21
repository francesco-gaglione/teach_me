//! `/topics/{id}/mission...` endpoints (contract: mission §2-§4).

use axum::Json;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use domain::{Mission, PageRequest, TopicId, TopicMission};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::auth::AuthUser;
use crate::cursor;
use crate::error::{ApiError, ApiJson, ApiQuery};
use crate::folders::{ListQuery, path_id};
use crate::health::format_instant;
use crate::state::AppState;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/topics/{id}/mission", get(show))
        .route("/topics/{id}/mission/versions", get(versions))
        .route("/topics/{id}/mission/confirm", post(confirm))
}

#[derive(Serialize)]
pub struct MissionDto {
    topic_id: String,
    version: u32,
    status: &'static str,
    why: String,
    /// Always present; `null` for versions saved before contract v1.2.
    starting_point: Option<String>,
    success_criteria: Vec<String>,
    constraints: Vec<String>,
    out_of_scope: Vec<String>,
    created_at: String,
    confirmed_at: Option<String>,
    superseded_at: Option<String>,
}

impl From<&Mission> for MissionDto {
    fn from(m: &Mission) -> Self {
        Self {
            topic_id: m.topic_id.to_string(),
            version: m.version,
            status: m.status.as_str(),
            why: m.content.why().to_owned(),
            starting_point: m.content.starting_point().map(str::to_owned),
            success_criteria: m.content.success_criteria().to_vec(),
            constraints: m.content.constraints().to_vec(),
            out_of_scope: m.content.out_of_scope().to_vec(),
            created_at: format_instant(m.created_at),
            confirmed_at: m.confirmed_at.map(format_instant),
            superseded_at: m.superseded_at.map(format_instant),
        }
    }
}

#[derive(Serialize)]
struct TopicMissionDto {
    current: Option<MissionDto>,
    draft: Option<MissionDto>,
}

impl From<&TopicMission> for TopicMissionDto {
    fn from(t: &TopicMission) -> Self {
        Self {
            current: t.current.as_ref().map(MissionDto::from),
            draft: t.draft.as_ref().map(MissionDto::from),
        }
    }
}

#[derive(Serialize)]
struct VersionsDto {
    items: Vec<MissionDto>,
    next_cursor: Option<String>,
}

async fn show(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<TopicMissionDto>, ApiError> {
    let id = TopicId::new(path_id(&id)?);
    Ok(Json(TopicMissionDto::from(
        &state.missions.get(user, id).await?,
    )))
}

async fn versions(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<VersionsDto>, ApiError> {
    let id = TopicId::new(path_id(&id)?);
    let (limit, cursor_text) = query.parse()?;
    let after = cursor_text
        .as_deref()
        .map(cursor::decode_mission)
        .transpose()?;
    let page = state
        .missions
        .versions(user, id, PageRequest { after, limit })
        .await?;
    Ok(Json(VersionsDto {
        items: page.items.iter().map(MissionDto::from).collect(),
        next_cursor: page.next.as_ref().map(cursor::encode_mission),
    }))
}

/// `{ "version": <integer >= 1> }`. Anything else in `version` (absent, null,
/// a string, a fraction, zero) reaches the use case as `None` and is reported
/// as `{field: "version", issue: "invalid"}`.
fn version_of(body: &Map<String, Value>) -> Option<u32> {
    body.get("version")?
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v >= 1)
}

async fn confirm(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    body: Result<ApiJson<Map<String, Value>>, ApiError>,
) -> Result<Json<TopicMissionDto>, ApiError> {
    // Same order as the other endpoints: path id, existence, then the body.
    let id = TopicId::new(path_id(&id)?);
    state.topics.get(user, id).await?;
    let ApiJson(body) = body?;
    let confirmed = state.missions.confirm(user, id, version_of(&body)).await?;
    Ok(Json(TopicMissionDto::from(&confirmed)))
}
