use application::AppError;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use domain::{SessionId, UserId};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Request id stored in request extensions for spans and handlers.
#[derive(Debug, Clone, Copy)]
pub struct RequestId(pub Uuid);

/// Outermost layer: assigns the request id, then guarantees every error
/// response is in the contract format and every response carries the id.
pub async fn request_context(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v).ok())
        .unwrap_or_else(|| state.ids.new_id());
    req.extensions_mut().insert(RequestId(id));
    let lang = crate::error::message_language(
        req.headers()
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok()),
    );

    let mut response = next.run(req).await;

    if response.status().is_client_error() || response.status().is_server_error() {
        let status = response.status();
        let error = response
            .extensions_mut()
            .remove::<ApiError>()
            .unwrap_or_else(|| ApiError::from_status(status));
        let mut rendered = error.render(status, id, lang);
        // Keep headers set upstream (WWW-Authenticate, Allow) except body-describing ones.
        for (name, value) in response.headers() {
            if name != header::CONTENT_LENGTH
                && name != header::CONTENT_TYPE
                && name != header::TRANSFER_ENCODING
            {
                rendered.headers_mut().append(name.clone(), value.clone());
            }
        }
        response = rendered;
    }

    if let Ok(value) = HeaderValue::from_str(&id.to_string()) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}

/// Who the request is for, put in the request extensions once its session
/// is checked. The handlers read it through [`crate::auth::AuthUser`].
#[derive(Debug, Clone, Copy)]
pub struct CurrentUser {
    pub user_id: UserId,
    pub session_id: SessionId,
}

/// Session check for everything under `/api/v1` except the login: the bearer
/// token must belong to a live session of an active user.
pub async fn require_session(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(bearer_token)
        .filter(|t| !t.is_empty());
    let Some(token) = token else {
        return ApiError::Unauthorized.into_response();
    };
    match state.auth.authenticate(token).await {
        Ok(who) => {
            tracing::Span::current().record("user_id", tracing::field::display(who.user_id));
            req.extensions_mut().insert(CurrentUser {
                user_id: who.user_id,
                session_id: who.session_id,
            });
            next.run(req).await
        }
        Err(AppError::Unauthorized) => ApiError::Unauthorized.into_response(),
        Err(other) => ApiError::from(other).into_response(),
    }
}

pub(crate) fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, rest) = header.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then_some(rest.trim())
}

#[cfg(test)]
mod tests {
    use super::bearer_token;

    #[test]
    fn parses_bearer_header() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bearer abc"), Some("abc"));
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("Bearer"), None);
    }
}
