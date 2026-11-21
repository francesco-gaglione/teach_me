//! Missing-component ranking (contract: `component-requests.md`).

use axum::Json;
use axum::extract::State;
use axum::routing::get;
use domain::{ComponentRequestExample, ComponentRequestGroup, PageRequest};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::cursor;
use crate::error::{ApiError, ApiQuery};
use crate::folders::ListQuery;
use crate::health::format_instant;
use crate::state::AppState;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new().route("/component-requests", get(list))
}

#[derive(Serialize)]
struct ExampleDto {
    purpose: String,
    lesson_id: String,
    lesson_title: String,
    topic_title: String,
    requested_at: String,
}

impl From<&ComponentRequestExample> for ExampleDto {
    fn from(e: &ComponentRequestExample) -> Self {
        Self {
            purpose: e.purpose.clone(),
            lesson_id: e.lesson_id.to_string(),
            lesson_title: e.lesson_title.clone(),
            topic_title: e.topic_title.clone(),
            requested_at: format_instant(e.requested_at),
        }
    }
}

#[derive(Serialize)]
struct GroupDto {
    wanted: String,
    count: u32,
    first_requested_at: String,
    last_requested_at: String,
    catalog_version: u32,
    examples: Vec<ExampleDto>,
}

impl From<&ComponentRequestGroup> for GroupDto {
    fn from(g: &ComponentRequestGroup) -> Self {
        Self {
            wanted: g.wanted.clone(),
            count: g.count,
            first_requested_at: format_instant(g.first_requested_at),
            last_requested_at: format_instant(g.last_requested_at),
            catalog_version: g.catalog_version,
            examples: g.examples.iter().map(ExampleDto::from).collect(),
        }
    }
}

#[derive(Serialize)]
struct PageDto {
    items: Vec<GroupDto>,
    next_cursor: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<PageDto>, ApiError> {
    let (limit, cursor_text) = query.parse()?;
    let after = cursor_text
        .as_deref()
        .map(cursor::decode_component_request)
        .transpose()?;
    let page = state
        .component_requests
        .ranking(user, PageRequest { after, limit })
        .await?;
    Ok(Json(PageDto {
        items: page.items.iter().map(GroupDto::from).collect(),
        next_cursor: page.next.as_ref().map(cursor::encode_component_request),
    }))
}
