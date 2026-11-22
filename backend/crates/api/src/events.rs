//! `GET /topics/{id}/events`: the SSE stream of a topic (contract: chat §3-§4,
//! api-conventions §10).

use std::collections::VecDeque;
use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use domain::{TopicEvent, TopicEventPayload, TopicId};
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use crate::auth::AuthUser;
use crate::chat::MessageDto;
use crate::error::ApiError;
use crate::folders::path_id;
use crate::health::format_instant;
use crate::jobs::JobDto;
use crate::mission::MissionDto;
use crate::preferences::PreferenceDto;
use crate::state::AppState;
use crate::topics::TopicDto;

const LAST_EVENT_ID: HeaderName = HeaderName::from_static("last-event-id");

/// Mounted outside the REST timeout layer (see `app::assemble`).
pub fn routes() -> axum::Router<AppState> {
    axum::Router::new().route("/topics/{id}/events", get(stream))
}

/// Absent or empty header: only future events. Unparsable: treated as an id
/// from the future, which the hub answers with `resync_required`.
fn last_event_id(headers: &HeaderMap) -> Option<u64> {
    let raw = headers.get(LAST_EVENT_ID)?.to_str().ok().map(str::trim);
    match raw {
        Some("") => None,
        Some(text) => Some(text.parse().unwrap_or(u64::MAX)),
        None => Some(u64::MAX),
    }
}

fn frame(id: Option<u64>, event: &TopicEvent) -> Event {
    let (name, payload): (&str, Value) = match &event.payload {
        TopicEventPayload::MessageCreated(m) => {
            ("message_created", json!({ "message": MessageDto::from(m) }))
        }
        TopicEventPayload::MessageDelta {
            message_id,
            part_index,
            text,
        } => (
            "message_delta",
            json!({ "message_id": message_id.to_string(), "part_index": part_index, "text": text }),
        ),
        TopicEventPayload::MessageCompleted(m) => (
            "message_completed",
            json!({ "message": MessageDto::from(m) }),
        ),
        TopicEventPayload::MessageFailed(m) => {
            ("message_failed", json!({ "message": MessageDto::from(m) }))
        }
        TopicEventPayload::TopicUpdated(t) => {
            ("topic_updated", json!({ "topic": TopicDto::from(t) }))
        }
        TopicEventPayload::MissionProposed(m) => (
            "mission_proposed",
            json!({ "mission": MissionDto::from(m) }),
        ),
        TopicEventPayload::MissionConfirmed(m) => (
            "mission_confirmed",
            json!({ "mission": MissionDto::from(m) }),
        ),
        TopicEventPayload::JobUpdated(j) => ("job_updated", json!({ "job": JobDto::from(j) })),
        TopicEventPayload::PreferenceSaved(p) => (
            "preference_saved",
            json!({ "preference": PreferenceDto::from(p) }),
        ),
        TopicEventPayload::PreferenceDeleted { id, scope } => (
            "preference_deleted",
            json!({
                "preference_id": id.to_string(),
                "scope": scope.as_str(),
                "topic_id": scope.topic_id().map(|t| t.to_string()),
            }),
        ),
    };
    envelope(
        id,
        name,
        event.topic_id,
        &format_instant(event.at),
        &payload,
    )
}

fn envelope(id: Option<u64>, name: &str, topic_id: TopicId, at: &str, payload: &Value) -> Event {
    let data = json!({
        "type": name,
        "topic_id": topic_id.to_string(),
        "at": at,
        "payload": payload,
    });
    let mut event = Event::default().event(name).data(data.to_string());
    if let Some(id) = id {
        event = event.id(id.to_string());
    }
    event
}

async fn stream(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    state.topics.get(user, topic_id).await?;
    // The slot lives in the stream's state: it is freed when the response
    // body is dropped, however the stream ends.
    let slot = state
        .streams
        .acquire(user)
        .ok_or(ApiError::TooManyStreams)?;

    // Subscribing after the existence check can race with a deletion; the
    // stream then simply never receives anything and ends with the topic.
    let subscription = state
        .events
        .subscribe(user, topic_id, last_event_id(&headers));

    let mut first: VecDeque<Event> = VecDeque::new();
    if let Some(latest) = subscription.resync_at {
        // Carries the latest id so the client adopts it as its position and a
        // later reconnect does not resync again.
        first.push_back(envelope(
            Some(latest),
            "resync_required",
            topic_id,
            &format_instant(state.clock.now()),
            &json!({}),
        ));
    }
    first.extend(
        subscription
            .backlog
            .iter()
            .map(|stored| frame(Some(stored.id), &stored.event)),
    );

    // The session is checked again on every ping interval: a stream does not
    // outlive a logout, a password change, a disabled or a deleted user (the
    // last two may come from the CLI, another process, so no event tells).
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(crate::middleware::bearer_token)
        .unwrap_or_default()
        .to_owned();
    let auth = state.auth.clone();
    let recheck =
        tokio::time::interval_at(tokio::time::Instant::now() + state.sse_ping, state.sse_ping);
    let events = futures_util::stream::unfold(
        (first, subscription.live, recheck, slot),
        move |(mut first, mut live, mut recheck, slot)| {
            let (auth, token) = (auth.clone(), token.clone());
            async move {
                if let Some(event) = first.pop_front() {
                    return Some((Ok::<_, Infallible>(event), (first, live, recheck, slot)));
                }
                loop {
                    tokio::select! {
                        received = live.recv() => return match received {
                            Ok(stored) => {
                                Some((Ok(frame(Some(stored.id), &stored.event)), (first, live, recheck, slot)))
                            }
                            // Fell too far behind: end the stream; the client reconnects
                            // with its last id and replays from the buffer (or resyncs).
                            Err(RecvError::Lagged(_)) => {
                                tracing::warn!("SSE subscriber lagged; closing its stream");
                                None
                            }
                            // The topic was deleted or the server is shutting down.
                            Err(RecvError::Closed) => None,
                        },
                        _ = recheck.tick() => {
                            if auth.authenticate(&token).await.is_err() {
                                return None;
                            }
                        }
                    }
                }
            }
        },
    );

    Ok(Sse::new(events).keep_alive(KeepAlive::new().interval(state.sse_ping).text("ping")))
}
