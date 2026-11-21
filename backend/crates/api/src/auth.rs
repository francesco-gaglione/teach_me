//! Accounts and sessions over HTTP (`docs/contracts/users.md` §2).

use application::{AppError, LoggedIn};
use axum::Json;
use axum::extract::{FromRequestParts, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::routing::{get, post, put};
use domain::{SessionId, User, UserId};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiJson};
use crate::health::format_instant;
use crate::middleware::CurrentUser;
use crate::state::AppState;

/// The user of the request, put there by the session middleware. A handler
/// that takes it cannot run without a valid session, and passes the id on to
/// every use case (ADR 0008 §2).
#[derive(Debug, Clone, Copy)]
pub struct AuthUser(pub UserId);

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl std::future::Future<Output = Result<Self, ApiError>> + Send {
        std::future::ready(
            parts
                .extensions
                .get::<CurrentUser>()
                .map(|c| Self(c.user_id))
                .ok_or(ApiError::Unauthorized),
        )
    }
}

/// The session behind the request, for logout and the password change.
#[derive(Debug, Clone, Copy)]
pub struct AuthSession(pub SessionId);

impl<S: Send + Sync> FromRequestParts<S> for AuthSession {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl std::future::Future<Output = Result<Self, ApiError>> + Send {
        std::future::ready(
            parts
                .extensions
                .get::<CurrentUser>()
                .map(|c| Self(c.session_id))
                .ok_or(ApiError::Unauthorized),
        )
    }
}

/// The one route under `/api/v1` that needs no session.
pub fn public_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/auth/login", post(login))
}

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/auth/logout", post(logout))
        .route("/me", get(me))
        .route("/me/password", put(change_password))
        .route("/me/llm-key", put(set_llm_key).delete(clear_llm_key))
        .route("/me/tts-voice", put(set_tts_voice))
}

#[derive(Serialize)]
pub struct UserDto {
    id: String,
    username: String,
    display_name: String,
    has_llm_key: bool,
    llm_provider: String,
    tts_voice: Option<String>,
    created_at: String,
}

impl UserDto {
    /// `tts_voice` is the effective voice (`AudioService::effective_voice`).
    fn new(user: &User, provider: &str, tts_voice: Option<String>) -> Self {
        Self {
            id: user.id.to_string(),
            username: user.username.as_str().to_owned(),
            display_name: user.display_name.as_str().to_owned(),
            has_llm_key: user.has_llm_key,
            llm_provider: provider.to_owned(),
            tts_voice,
            created_at: format_instant(user.created_at),
        }
    }
}

#[derive(Deserialize)]
struct LoginBody {
    username: String,
    password: String,
    device_name: Option<String>,
}

#[derive(Serialize)]
struct SessionDto {
    token: String,
    expires_at: String,
    user: UserDto,
}

async fn login(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<LoginBody>,
) -> Result<Json<SessionDto>, ApiError> {
    let LoggedIn {
        token,
        expires_at,
        user,
    } = state
        .auth
        .login(&body.username, &body.password, body.device_name.as_deref())
        .await
        .map_err(login_error)?;
    Ok(Json(SessionDto {
        token: token.expose().to_owned(),
        expires_at: format_instant(expires_at),
        user: user_dto(&state, &user),
    }))
}

fn login_error(e: AppError) -> ApiError {
    match e {
        AppError::InvalidCredentials => ApiError::InvalidCredentials,
        AppError::RateLimited { retry_after_secs } => ApiError::LoginBlocked(retry_after_secs),
        other => other.into(),
    }
}

async fn logout(
    State(state): State<AppState>,
    AuthSession(session): AuthSession,
) -> Result<StatusCode, ApiError> {
    state.auth.logout(session).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<UserDto>, ApiError> {
    let user = state.account.me(user).await?;
    Ok(Json(user_dto(&state, &user)))
}

#[derive(Deserialize)]
struct PasswordBody {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    AuthSession(session): AuthSession,
    ApiJson(body): ApiJson<PasswordBody>,
) -> Result<StatusCode, ApiError> {
    state
        .auth
        .change_password(user, session, &body.current_password, &body.new_password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct KeyBody {
    api_key: String,
}

async fn set_llm_key(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    ApiJson(body): ApiJson<KeyBody>,
) -> Result<Json<UserDto>, ApiError> {
    let user = state.account.set_llm_key(user, &body.api_key).await?;
    Ok(Json(user_dto(&state, &user)))
}

async fn clear_llm_key(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<StatusCode, ApiError> {
    state.account.clear_llm_key(user).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn user_dto(state: &AppState, user: &User) -> UserDto {
    UserDto::new(
        user,
        &state.llm_provider,
        state.read_aloud.effective_voice(user),
    )
}

#[derive(Deserialize)]
struct VoiceBody {
    voice: String,
}

async fn set_tts_voice(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    ApiJson(body): ApiJson<VoiceBody>,
) -> Result<Json<UserDto>, ApiError> {
    let user = state.read_aloud.set_voice(user, &body.voice).await?;
    Ok(Json(user_dto(&state, &user)))
}
