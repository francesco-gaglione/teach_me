//! The single place where errors become HTTP responses (contract §7).
//!
//! Handlers and extractors return [`ApiError`]; its `IntoResponse` only tags the
//! response. The outermost middleware (`middleware::request_context`) renders the
//! JSON body, adding the `request_id`. Responses produced by layers we do not
//! control (404 fallback, 413, timeout, panic) have no tag and are converted by
//! status class with [`ApiError::from_status`], so nothing leaves in another format.

use application::{AppError, ConflictReason};
use axum::body::Body;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use domain::{ContentLanguage, DomainError, ValidationIssue};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ErrorDetail {
    pub field: String,
    pub issue: String,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(Msg),
    #[error("invalid id")]
    InvalidId,
    #[error("unauthorized")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error("method not allowed")]
    MethodNotAllowed,
    /// A 409 with its own `code` (`conflict` unless the contract names one).
    #[error("conflict ({0}): {1}")]
    Conflict(&'static str, Msg),
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("unsupported media type")]
    UnsupportedMediaType,
    #[error("validation failed")]
    Validation(Vec<ErrorDetail>),
    #[error("rate limited")]
    RateLimited,
    #[error("invalid credentials")]
    InvalidCredentials,
    /// Too many failed logins for the name; seconds until the next try.
    #[error("login blocked")]
    LoginBlocked(u64),
    #[error("llm key missing")]
    LlmKeyMissing,
    #[error("llm key rejected")]
    LlmKeyRejected,
    #[error("llm credit exhausted")]
    LlmCreditExhausted,
    /// A `Range` that starts past the end of the file; its size, for
    /// `Content-Range: bytes */size`.
    #[error("range not satisfiable")]
    RangeNotSatisfiable(u64),
    #[error("internal error")]
    Internal,
    #[error("upstream failed")]
    UpstreamFailed,
    #[error("unavailable")]
    Unavailable,
}

/// A fixed user-facing sentence in both supported languages; the one sent is
/// picked per request from `Accept-Language` (ADR 0019).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    it: String,
    en: String,
}

impl std::fmt::Display for Msg {
    /// Italian text, only for logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.it)
    }
}

impl Msg {
    fn pick(&self, lang: ContentLanguage) -> String {
        match lang {
            ContentLanguage::It => self.it.clone(),
            ContentLanguage::En => self.en.clone(),
        }
    }
}

/// Language of the error `message` for a request: the first supported language
/// of `Accept-Language` by `q` order (ties keep header order), Italian when the
/// header is absent, malformed or lists nothing supported. `q=0` means "not
/// acceptable" and is skipped.
#[must_use]
pub fn message_language(header: Option<&str>) -> ContentLanguage {
    let Some(header) = header else {
        return ContentLanguage::default();
    };
    let mut ranked: Vec<(f32, ContentLanguage)> = Vec::new();
    for part in header.split(',') {
        let mut pieces = part.split(';');
        let tag = pieces.next().unwrap_or("").trim();
        let primary = tag.split('-').next().unwrap_or("").to_ascii_lowercase();
        let lang = match primary.as_str() {
            "en" => ContentLanguage::En,
            "it" => ContentLanguage::It,
            _ => continue,
        };
        let mut q = 1.0_f32;
        for param in pieces {
            if let Some((name, value)) = param.split_once('=')
                && name.trim().eq_ignore_ascii_case("q")
            {
                // A malformed weight makes the entry unusable, not the header.
                match value.trim().parse::<f32>() {
                    Ok(v) if (0.0..=1.0).contains(&v) => q = v,
                    _ => q = -1.0,
                }
            }
        }
        if q > 0.0 {
            ranked.push((q, lang));
        }
    }
    // Stable sort: equal weights keep the order of the header.
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    ranked
        .first()
        .map_or_else(ContentLanguage::default, |r| r.1)
}

#[derive(Serialize)]
struct Envelope<'a> {
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: &'a str,
    details: Option<&'a [ErrorDetail]>,
    request_id: String,
}

/// The `code` of the 409 for a reason: only the ones the contract names have
/// their own, the rest are plain `conflict`.
fn conflict_code(reason: ConflictReason) -> &'static str {
    match reason {
        ConflictReason::StaleReview => "already_reviewed",
        ConflictReason::CardSuspended => "card_suspended",
        ConflictReason::NothingToUndo => "nothing_to_undo",
        ConflictReason::UndoExpired => "undo_expired",
        ConflictReason::CardSuperseded => "card_superseded",
        ConflictReason::TurnInProgress
        | ConflictReason::NotRetryable
        | ConflictReason::MissionNotOpen
        | ConflictReason::MissionStateMismatch
        | ConflictReason::JobNotRetryable
        | ConflictReason::JobActive
        | ConflictReason::NoConfirmedMission
        | ConflictReason::TopicNotReady
        | ConflictReason::LessonCompleted
        | ConflictReason::DocumentLimit
        | ConflictReason::ImportInProgress
        | ConflictReason::ReadAloudOff => "conflict",
    }
}

/// `(italian, english)` sentence for a 409 reason.
fn conflict_message(reason: ConflictReason) -> (&'static str, &'static str) {
    match reason {
        ConflictReason::TurnInProgress => (
            "C'è già una risposta in corso per questo argomento.",
            "A reply is already in progress for this topic.",
        ),
        ConflictReason::NotRetryable => (
            "Si può riprovare solo l'ultima risposta fallita.",
            "Only the last failed reply can be retried.",
        ),
        ConflictReason::MissionNotOpen => (
            "Questa bozza non è più aperta: ricarica la mission.",
            "This draft is no longer open: reload the mission.",
        ),
        ConflictReason::JobNotRetryable => (
            "Si può riprovare solo un'operazione fallita, e solo se non ce n'è un'altra attiva.",
            "Only a failed operation can be retried, and only if no other one is active.",
        ),
        ConflictReason::JobActive => (
            "C'è già un'operazione dello stesso tipo in corso per questo argomento.",
            "An operation of the same kind is already running for this topic.",
        ),
        ConflictReason::NoConfirmedMission => (
            "L'argomento non ha ancora una mission confermata.",
            "The topic does not have a confirmed mission yet.",
        ),
        ConflictReason::TopicNotReady => (
            "L'argomento non è ancora pronto: aspetta che le fonti siano state cercate.",
            "The topic is not ready yet: wait until the sources have been searched.",
        ),
        ConflictReason::LessonCompleted => (
            "La lezione è già completata.",
            "The lesson is already completed.",
        ),
        ConflictReason::CardSuspended => ("La scheda è sospesa.", "The card is suspended."),
        ConflictReason::CardSuperseded => (
            "La scheda è stata sostituita da un record nuovo: non si riattiva.",
            "The card was replaced by a new record: it cannot be reactivated.",
        ),
        ConflictReason::StaleReview => (
            "Il voto riguarda una versione vecchia della scheda: ricaricala.",
            "The rating refers to an old version of the card: reload it.",
        ),
        ConflictReason::NothingToUndo => (
            "Non c'è un voto da annullare.",
            "There is no rating to undo.",
        ),
        ConflictReason::UndoExpired => (
            "Il voto è troppo vecchio per essere annullato.",
            "The rating is too old to be undone.",
        ),
        ConflictReason::DocumentLimit => (
            "L'argomento ha già 5 documenti: eliminane uno per caricarne un altro.",
            "The topic already has 5 documents: delete one to upload another.",
        ),
        ConflictReason::ImportInProgress => (
            "Il documento è in fase di importazione: riprova quando ha finito.",
            "The document is being imported: try again when it has finished.",
        ),
        ConflictReason::ReadAloudOff => (
            "La lettura ad alta voce non è attiva su questo server.",
            "Read-aloud is not enabled on this server.",
        ),
        ConflictReason::MissionStateMismatch => (
            "L'argomento non è in uno stato che consente di confermare la mission.",
            "The topic is not in a state that allows confirming the mission.",
        ),
    }
}

impl ApiError {
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) | Self::InvalidId => StatusCode::BAD_REQUEST,
            Self::Unauthorized | Self::InvalidCredentials => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::Conflict(..) | Self::LlmKeyMissing => StatusCode::CONFLICT,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited | Self::LoginBlocked(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::UpstreamFailed | Self::LlmKeyRejected | Self::LlmCreditExhausted => {
                StatusCode::BAD_GATEWAY
            }
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::RangeNotSatisfiable(_) => StatusCode::RANGE_NOT_SATISFIABLE,
        }
    }

    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::InvalidId => "invalid_id",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not_found",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::Conflict(code, _) => code,
            Self::PayloadTooLarge => "payload_too_large",
            Self::UnsupportedMediaType => "unsupported_media_type",
            Self::Validation(_) => "validation_failed",
            Self::RateLimited | Self::LoginBlocked(_) => "rate_limited",
            Self::InvalidCredentials => "invalid_credentials",
            Self::LlmKeyMissing => "llm_key_missing",
            Self::LlmKeyRejected => "llm_key_rejected",
            Self::LlmCreditExhausted => "llm_credit_exhausted",
            Self::Internal => "internal",
            Self::UpstreamFailed => "upstream_failed",
            Self::Unavailable => "unavailable",
            Self::RangeNotSatisfiable(_) => "range_not_satisfiable",
        }
    }

    /// User-facing text in the requester's language. Never carries internal detail.
    pub(crate) fn message(&self, lang: ContentLanguage) -> String {
        let (it, en) = match self {
            Self::BadRequest(m) | Self::Conflict(_, m) => return m.pick(lang),
            Self::InvalidId => (
                "L'identificatore non è valido.",
                "The identifier is not valid.",
            ),
            Self::Unauthorized => (
                "Sessione mancante, scaduta o non valida.",
                "Your session is missing, expired or invalid.",
            ),
            Self::NotFound => ("Risorsa non trovata.", "Resource not found."),
            Self::MethodNotAllowed => (
                "Metodo non consentito per questa risorsa.",
                "Method not allowed for this resource.",
            ),
            Self::PayloadTooLarge => (
                "Il corpo della richiesta supera il limite consentito.",
                "The request body exceeds the allowed size.",
            ),
            Self::UnsupportedMediaType => ("Il file non è un PDF.", "The file is not a PDF."),
            Self::Validation(_) => (
                "I dati inviati non sono validi.",
                "The submitted data is not valid.",
            ),
            Self::RateLimited => (
                "Troppe richieste, riprova tra poco.",
                "Too many requests, try again shortly.",
            ),
            Self::LoginBlocked(_) => (
                "Troppi tentativi di accesso: riprova più tardi.",
                "Too many sign-in attempts: try again later.",
            ),
            Self::InvalidCredentials => (
                "Nome utente o password non validi.",
                "Invalid username or password.",
            ),
            Self::LlmKeyMissing => (
                "Per questa operazione serve la tua chiave del servizio LLM: inseriscila nelle Impostazioni.",
                "This operation needs your LLM service key: enter it in Settings.",
            ),
            Self::LlmKeyRejected => (
                "Il servizio LLM ha rifiutato la tua chiave: controllala nelle Impostazioni.",
                "The LLM service rejected your key: check it in Settings.",
            ),
            Self::LlmCreditExhausted => (
                "Il credito del servizio LLM è esaurito: ricaricalo presso il provider e riprova.",
                "The LLM service credit is exhausted: top it up with the provider and try again.",
            ),
            Self::Internal => ("Errore interno del server.", "Internal server error."),
            Self::UpstreamFailed => (
                "Un servizio esterno ha restituito un errore.",
                "An external service returned an error.",
            ),
            Self::Unavailable => (
                "Servizio temporaneamente non disponibile.",
                "Service temporarily unavailable.",
            ),
            Self::RangeNotSatisfiable(_) => (
                "L'intervallo richiesto è fuori dal file.",
                "The requested range is outside the file.",
            ),
        };
        match lang {
            ContentLanguage::It => it,
            ContentLanguage::En => en,
        }
        .to_owned()
    }

    /// A 400 with its own sentence in each language.
    #[must_use]
    pub fn bad_request(it: impl Into<String>, en: impl Into<String>) -> Self {
        Self::BadRequest(Msg {
            it: it.into(),
            en: en.into(),
        })
    }

    /// Fallback for responses that did not come from this module.
    #[must_use]
    pub fn from_status(status: StatusCode) -> Self {
        match status {
            StatusCode::UNAUTHORIZED => Self::Unauthorized,
            StatusCode::NOT_FOUND => Self::NotFound,
            StatusCode::METHOD_NOT_ALLOWED => Self::MethodNotAllowed,
            StatusCode::CONFLICT => Self::Conflict(
                "conflict",
                Msg {
                    it: "Stato incompatibile con l'operazione.".into(),
                    en: "The current state does not allow this operation.".into(),
                },
            ),
            StatusCode::PAYLOAD_TOO_LARGE => Self::PayloadTooLarge,
            StatusCode::UNSUPPORTED_MEDIA_TYPE => Self::UnsupportedMediaType,
            StatusCode::UNPROCESSABLE_ENTITY => Self::Validation(Vec::new()),
            StatusCode::TOO_MANY_REQUESTS => Self::RateLimited,
            StatusCode::BAD_GATEWAY => Self::UpstreamFailed,
            // Timeouts from tower-http are configured as 503, but cover 408/504 too.
            StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::REQUEST_TIMEOUT
            | StatusCode::GATEWAY_TIMEOUT => Self::Unavailable,
            s if s.is_client_error() => {
                Self::bad_request("Richiesta non valida.", "Invalid request.")
            }
            _ => Self::Internal,
        }
    }

    /// Renders the contract §7 body, keeping the original status when it is
    /// an unexpected one of the same class.
    pub(crate) fn render(
        &self,
        original: StatusCode,
        request_id: Uuid,
        lang: ContentLanguage,
    ) -> Response {
        let details = match self {
            Self::Validation(d) if !d.is_empty() => Some(d.as_slice()),
            _ => None,
        };
        let body = Envelope {
            error: ErrorBody {
                code: self.code(),
                message: &self.message(lang),
                details,
                request_id: request_id.to_string(),
            },
        };
        let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
        let mut response = Response::new(Body::from(bytes));
        *response.status_mut() = original;
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        response
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if matches!(self, Self::Internal) {
            tracing::error!("internal error response");
        }
        let mut response = Response::new(Body::empty());
        *response.status_mut() = self.status();
        if matches!(self, Self::Unauthorized) {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        if let Self::RangeNotSatisfiable(size) = &self
            && let Ok(value) = HeaderValue::from_str(&format!("bytes */{size}"))
        {
            response.headers_mut().insert(header::CONTENT_RANGE, value);
        }
        if let Self::LoginBlocked(secs) = &self
            && let Ok(value) = HeaderValue::from_str(&secs.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response.extensions_mut().insert(self);
        response
    }
}

fn detail(issue: ValidationIssue) -> ErrorDetail {
    ErrorDetail {
        field: issue.field.to_owned(),
        issue: issue.issue.as_str().to_owned(),
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::Validation(issue) => Self::Validation(vec![detail(issue)]),
        }
    }
}

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::Validation(issues) => {
                Self::Validation(issues.into_iter().map(detail).collect())
            }
            AppError::InvalidFields(fields) => Self::Validation(
                fields
                    .into_iter()
                    .map(|f| ErrorDetail {
                        field: f.field,
                        issue: f.issue.as_str().to_owned(),
                    })
                    .collect(),
            ),
            AppError::NotFound => Self::NotFound,
            AppError::LlmKeyMissing => Self::LlmKeyMissing,
            AppError::LlmKeyRejected => Self::LlmKeyRejected,
            AppError::LlmCreditExhausted => Self::LlmCreditExhausted,
            AppError::InvalidCredentials => Self::InvalidCredentials,
            AppError::Unauthorized => Self::Unauthorized,
            AppError::RateLimited { retry_after_secs } => Self::LoginBlocked(retry_after_secs),
            AppError::Conflict(reason) => {
                let (it, en) = conflict_message(reason);
                Self::Conflict(
                    conflict_code(reason),
                    Msg {
                        it: it.into(),
                        en: en.into(),
                    },
                )
            }
            AppError::Storage(source) => {
                // Details stay in the log; the client only sees `internal`.
                tracing::error!(error = %source, "storage failure");
                Self::Internal
            }
            AppError::Scheduler(source) => {
                tracing::error!(error = %source, "scheduler failure");
                Self::Internal
            }
        }
    }
}

/// `Query` extractor whose rejections use the contract error format
/// (`400 bad_request`).
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(v)) => Ok(Self(v)),
            Err(rejection) => Err(match rejection {
                QueryRejection::FailedToDeserializeQueryString(_) => ApiError::bad_request(
                    "Parametri di query non validi.",
                    "Invalid query parameters.",
                ),
                _ => ApiError::bad_request("Query string non valida.", "Invalid query string."),
            }),
        }
    }
}

/// `Json` extractor whose rejections use the contract error format.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(v)) => Ok(Self(v)),
            Err(rejection) => Err(match rejection {
                JsonRejection::JsonDataError(_) => ApiError::Validation(Vec::new()),
                JsonRejection::BytesRejection(_)
                    if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE =>
                {
                    ApiError::PayloadTooLarge
                }
                JsonRejection::MissingJsonContentType(_) => ApiError::bad_request(
                    "Content-Type deve essere application/json.",
                    "Content-Type must be application/json.",
                ),
                _ => ApiError::bad_request(
                    "Il corpo non è un JSON valido.",
                    "The body is not valid JSON.",
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_credit_is_a_502_with_its_own_code_and_fixed_message() {
        let error = ApiError::from(AppError::LlmCreditExhausted);
        assert_eq!(error.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(error.code(), "llm_credit_exhausted");
        assert_eq!(
            error.message(ContentLanguage::It),
            "Il credito del servizio LLM è esaurito: ricaricalo presso il provider e riprova."
        );
        assert_eq!(
            error.message(ContentLanguage::En),
            "The LLM service credit is exhausted: top it up with the provider and try again."
        );
        let rejected = ApiError::from(AppError::LlmKeyRejected);
        assert_eq!(rejected.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(rejected.code(), "llm_key_rejected");
    }

    #[test]
    fn accept_language_picks_the_first_supported_language_by_weight() {
        use ContentLanguage::{En, It};
        for (header, expected) in [
            (Some("en-US,en;q=0.9"), En),
            (Some("it-IT"), It),
            (Some("fr, en;q=0.5"), En),
            (Some("fr, de"), It),
            (Some("en;q=0.3, it;q=0.8"), It),
            (Some("it;q=0.4, en"), En),
            (Some("EN"), En),
            (Some("en;q=0, it;q=0.1"), It),
            (Some("*"), It),
            (Some(";;,,q=="), It),
            (Some("en;q=abc"), It),
            (Some(""), It),
            (None, It),
        ] {
            assert_eq!(message_language(header), expected, "{header:?}");
        }
    }

    #[test]
    fn every_conflict_and_fixed_message_has_both_languages() {
        let error = ApiError::from(AppError::Conflict(ConflictReason::UndoExpired));
        assert_eq!(
            error.message(ContentLanguage::En),
            "The rating is too old to be undone."
        );
        assert_eq!(
            error.message(ContentLanguage::It),
            "Il voto è troppo vecchio per essere annullato."
        );
        assert_eq!(
            ApiError::NotFound.message(ContentLanguage::En),
            "Resource not found."
        );
    }
}
