//! `/topics/{id}/resources` endpoints (contract: resources §2-§3).

use application::ResourcesView;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use domain::{Gap, ResearchInfo, Resource, TopicId};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::folders::path_id;
use crate::health::format_instant;
use crate::jobs::JobDto;
use crate::state::AppState;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/topics/{id}/resources", get(show))
        .route("/topics/{id}/resources/research", post(research))
}

#[derive(Serialize)]
struct ResourceDto {
    id: String,
    kind: &'static str,
    /// Only for `kind: document`.
    #[serde(skip_serializing_if = "Option::is_none")]
    document_id: Option<String>,
    source_type: &'static str,
    title: String,
    url: Option<String>,
    domain: Option<String>,
    language: String,
    annotation: String,
    use_for: String,
    created_at: String,
}

impl From<&Resource> for ResourceDto {
    fn from(r: &Resource) -> Self {
        Self {
            id: r.id.to_string(),
            kind: r.kind.as_str(),
            document_id: r.document_id.map(|d| d.to_string()),
            source_type: r.source_type.as_str(),
            title: r.title.clone(),
            url: r.url.clone(),
            domain: r.domain(),
            language: r.language.clone(),
            annotation: r.annotation.clone(),
            use_for: r.use_for.clone(),
            created_at: format_instant(r.created_at),
        }
    }
}

#[derive(Serialize)]
struct GapDto {
    area: String,
    note: String,
}

impl From<&Gap> for GapDto {
    fn from(g: &Gap) -> Self {
        Self {
            area: g.area.clone(),
            note: g.note.clone(),
        }
    }
}

#[derive(Serialize)]
struct ResearchDto {
    job_id: String,
    mission_version: u32,
    finished_at: String,
}

impl From<&ResearchInfo> for ResearchDto {
    fn from(r: &ResearchInfo) -> Self {
        Self {
            job_id: r.job_id.to_string(),
            mission_version: r.mission_version,
            finished_at: format_instant(r.finished_at),
        }
    }
}

#[derive(Serialize)]
struct ResourcesViewDto {
    resources: Vec<ResourceDto>,
    gaps: Vec<GapDto>,
    communities_skipped: bool,
    research: Option<ResearchDto>,
    job: Option<JobDto>,
}

impl From<&ResourcesView> for ResourcesViewDto {
    fn from(v: &ResourcesView) -> Self {
        Self {
            resources: v.resources.iter().map(ResourceDto::from).collect(),
            gaps: v.gaps.iter().map(GapDto::from).collect(),
            communities_skipped: v.communities_skipped,
            research: v.research.as_ref().map(ResearchDto::from),
            job: v.job.as_ref().map(JobDto::from),
        }
    }
}

async fn show(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
) -> Result<Json<ResourcesViewDto>, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let view = state.resources.get(user, topic_id).await?;
    Ok(Json(ResourcesViewDto::from(&view)))
}

async fn research(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
) -> Result<Response, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let job = state.resources.start_research(user, topic_id).await?;
    Ok((StatusCode::ACCEPTED, Json(JobDto::from(&job))).into_response())
}
