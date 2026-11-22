use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum::response::IntoResponse;
use axum::routing::get;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::sensitive_headers::SetSensitiveHeadersLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use domain::document::MAX_UPLOAD_BYTES;

use crate::error::ApiError;
use crate::health;
use crate::middleware::{RequestId, request_context, require_session};
use crate::state::AppState;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub body_bytes: usize,
    pub request_timeout: Duration,
    /// Largest request body on the document upload route (and only there).
    pub upload_bytes: usize,
    /// Largest request body on `PUT /lessons/{id}/strokes` (and only there).
    pub strokes_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            body_bytes: 1024 * 1024,
            request_timeout: Duration::from_secs(30),
            upload_bytes: usize::try_from(MAX_UPLOAD_BYTES).unwrap_or(usize::MAX),
            strokes_bytes: 4 * 1024 * 1024,
        }
    }
}

/// Room for the multipart boundary and part headers around a file.
const UPLOAD_FRAMING_BYTES: usize = 64 * 1024;

/// Builds the full application router.
pub fn build_router(state: AppState, limits: Limits) -> Router {
    assemble(
        state,
        limits,
        crate::auth::routes()
            .merge(crate::audio::routes())
            .merge(crate::folders::routes())
            .merge(crate::topics::routes())
            .merge(crate::chat::routes())
            .merge(crate::documents::routes())
            .merge(crate::component_requests::routes())
            .merge(crate::jobs::routes())
            .merge(crate::lessons::routes())
            .merge(crate::mission::routes())
            .merge(crate::preferences::routes())
            .merge(crate::resources::routes())
            .merge(crate::reviews::routes()),
    )
}

/// Like [`build_router`], with extra authenticated routes mounted under `/api/v1`.
/// Feature routers plug in here; tests use it to exercise extractors.
pub fn assemble(state: AppState, limits: Limits, api_routes: Router<AppState>) -> Router {
    // The REST timeout and the 1 MiB body limit cover ordinary routes only.
    // An SSE stream lives for as long as the client stays (contract:
    // api-conventions §12) and an upload is large and slow, so both are merged
    // after those layers are applied; the upload has its own limit.
    let rest = api_routes
        .route("/health", get(health::health))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::SERVICE_UNAVAILABLE,
            limits.request_timeout,
        ))
        .layer(RequestBodyLimitLayer::new(limits.body_bytes));
    // The file plus the multipart framing (boundary, headers).
    let upload = crate::documents::upload_routes().layer(RequestBodyLimitLayer::new(
        limits.upload_bytes.saturating_add(UPLOAD_FRAMING_BYTES),
    ));
    // A whole set of strokes is far bigger than any other JSON body: it has a
    // limit of its own, with the REST timeout like the other routes.
    let strokes = crate::lessons::strokes_routes()
        .layer(TimeoutLayer::with_status_code(
            StatusCode::SERVICE_UNAVAILABLE,
            limits.request_timeout,
        ))
        .layer(RequestBodyLimitLayer::new(limits.strokes_bytes));
    let protected = rest
        .merge(strokes)
        .merge(crate::events::routes())
        .merge(upload)
        .fallback(not_found)
        // `layer` (not `route_layer`) so unknown /api/v1 paths also need a session.
        .layer(from_fn_with_state(state.clone(), require_session));
    // The login is the only route without a session; merged after the layer.
    let public = crate::auth::public_routes()
        .layer(TimeoutLayer::with_status_code(
            StatusCode::SERVICE_UNAVAILABLE,
            limits.request_timeout,
        ))
        .layer(RequestBodyLimitLayer::new(limits.body_bytes));
    let api = protected.merge(public);

    Router::new()
        .route("/health", get(health::liveness))
        .route("/ready", get(crate::ready::ready))
        .nest("/api/v1", api)
        .fallback(not_found)
        .with_state(state.clone())
        .layer(CatchPanicLayer::custom(|panic: Box<dyn std::any::Any + Send>| {
            // Payload may contain user data: log only that a panic happened and its kind.
            let kind = if panic.is::<&str>() || panic.is::<String>() {
                "message"
            } else {
                "opaque"
            };
            tracing::error!(payload_kind = kind, "handler panicked");
            ApiError::Internal.into_response()
        }))
        .layer(
            TraceLayer::new_for_http()
                .on_response(
                    |res: &axum::http::Response<_>, latency: Duration, span: &tracing::Span| {
                        span.record("status", res.status().as_u16());
                        tracing::info!(parent: span, latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX), status = res.status().as_u16(), "finished processing request");
                    },
                )
                .make_span_with(|req: &axum::http::Request<_>| {
                let request_id = req
                    .extensions()
                    .get::<RequestId>()
                    .map_or_default(|r| r.0.to_string());
                // Path only: never log the query string or headers.
                // The matched route (`/topics/{id}`) keeps traces groupable by endpoint.
                let route = req
                    .extensions()
                    .get::<axum::extract::MatchedPath>()
                    .map_or("unmatched", |m| m.as_str());
                tracing::info_span!("http", method = %req.method(), path = %req.uri().path(), route, %request_id, status = tracing::field::Empty, user_id = tracing::field::Empty)
            }),
        )
        // Marks these headers sensitive for every layer inside it (TraceLayer
        // and any future debug logging): their values print as `Sensitive`.
        .layer(SetSensitiveHeadersLayer::new(SENSITIVE_HEADERS))
        .layer(from_fn_with_state(state, request_context))
}

/// Headers that carry credentials. Never logged, never in a span.
const SENSITIVE_HEADERS: [axum::http::HeaderName; 4] = [
    axum::http::header::AUTHORIZATION,
    axum::http::header::PROXY_AUTHORIZATION,
    axum::http::header::COOKIE,
    axum::http::header::SET_COOKIE,
];

async fn not_found() -> ApiError {
    ApiError::NotFound
}
