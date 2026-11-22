//! `/topics/{id}/documents` and `/documents/{id}` (contract: documents §2-§3).

use std::path::{Path as FsPath, PathBuf};
use std::time::Duration;

use application::NewUpload;
use axum::Json;
use axum::body::Body;
use axum::extract::multipart::{Field, Multipart, MultipartError};
use axum::extract::{DefaultBodyLimit, FromRequest, Path, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use domain::document::MAX_UPLOAD_BYTES;
use domain::{Document, DocumentId, DocumentView, TopicId};
use http_body_util::BodyExt;
use serde::Serialize;
use tokio::io::AsyncWriteExt;

use crate::auth::AuthUser;
use crate::chat::ErrorDto;
use crate::error::ApiError;
use crate::folders::path_id;
use crate::health::format_instant;
use crate::jobs::JobDto;
use crate::state::AppState;

/// A PDF may start with a few bytes of junk; readers accept the signature
/// anywhere in the first kilobyte.
const SIGNATURE_WINDOW: usize = 1024;
const SIGNATURE: &[u8] = b"%PDF-";

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/topics/{id}/documents", get(list))
        .route("/documents/{id}", delete(remove))
}

/// The upload route is kept apart: it has its own size limit and no request
/// timeout (a 50 MB transfer on a slow link outlives the REST timeout), both
/// applied by `app::assemble`.
pub fn upload_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/topics/{id}/documents", post(upload))
        // The size is limited by the layer `assemble` adds and by the counter
        // in `receive`; axum's own 2 MB default would stop every real PDF.
        .layer(DefaultBodyLimit::disable())
}

#[derive(Serialize)]
struct SectionDto {
    id: String,
    title: String,
    level: u8,
    page_start: u32,
    page_end: u32,
}

#[derive(Serialize)]
struct DocumentDto {
    id: String,
    topic_id: String,
    resource_id: Option<String>,
    filename: String,
    title: String,
    status: &'static str,
    size_bytes: u64,
    page_count: Option<u32>,
    language: Option<String>,
    error: Option<ErrorDto>,
    sections: Vec<SectionDto>,
    created_at: String,
}

impl DocumentDto {
    fn new(d: &Document, sections: &[domain::DocumentSection]) -> Self {
        Self {
            id: d.id.to_string(),
            topic_id: d.topic_id.to_string(),
            resource_id: d.resource_id.map(|r| r.to_string()),
            filename: d.filename.clone(),
            title: d.title.clone(),
            status: d.status.as_str(),
            size_bytes: d.size_bytes,
            page_count: d.page_count,
            language: d.language.clone(),
            error: d
                .error
                .as_ref()
                .map(|e| ErrorDto::new(e.code.as_str(), &e.message)),
            sections: sections
                .iter()
                .map(|s| SectionDto {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    level: s.level,
                    page_start: s.page_start,
                    page_end: s.page_end,
                })
                .collect(),
            created_at: format_instant(d.created_at),
        }
    }
}

impl From<&DocumentView> for DocumentDto {
    fn from(v: &DocumentView) -> Self {
        Self::new(&v.document, &v.sections)
    }
}

#[derive(Serialize)]
struct DocumentsDto {
    documents: Vec<DocumentDto>,
}

async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
) -> Result<Json<DocumentsDto>, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    let views = state.documents.list(user, topic_id).await?;
    Ok(Json(DocumentsDto {
        documents: views.iter().map(DocumentDto::from).collect(),
    }))
}

async fn remove(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = DocumentId::new(path_id(&id)?);
    state.documents.delete(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct UploadedDto {
    document: DocumentDto,
    job: JobDto,
}

async fn upload(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(topic_id): Path<String>,
    request: Request,
) -> Result<Response, ApiError> {
    let topic_id = TopicId::new(path_id(&topic_id)?);
    // Refuse before reading the file when the answer cannot be yes. The rest
    // of the body is read and thrown away first: a server that answers and
    // closes while the client is still sending can make the client see a reset
    // instead of the response.
    // The multipart framing adds a few hundred bytes to the file's own size,
    // so this over-counts a little: it is a hint for refusing early, the real
    // size is checked when the file has been received.
    let declared = request
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(|n| n.saturating_sub(1024));
    if let Err(e) = state.documents.check_upload(user, topic_id, declared).await {
        drain_body(request.into_body()).await;
        return Err(e.into());
    }
    let mut multipart = Multipart::from_request(request, &state)
        .await
        .map_err(|_| {
            ApiError::bad_request(
                "Il corpo deve essere un modulo multipart.",
                "The body must be a multipart form.",
            )
        })?;

    let id = state.documents.new_document_id();
    let (partial, complete) = state.documents.upload_paths(id);
    // If the client disconnects, hyper drops this future: the guard removes
    // the file then too, whichever step it was at.
    let mut guard = FileGuard(Some(partial.clone()));
    let (filename, received) = {
        let mut field = loop {
            match multipart.next_field().await.map_err(multipart_error)? {
                Some(f) if f.name() == Some("file") => break f,
                Some(_) => {}
                None => {
                    return Err(ApiError::bad_request(
                        "Manca il campo `file` del modulo.",
                        "The form is missing the `file` field.",
                    ));
                }
            }
        };
        let filename = field.file_name().unwrap_or("documento.pdf").to_owned();
        (filename, receive(&mut field, &partial).await)
    };
    let size = match received {
        Ok(size) => size,
        Err(e) => {
            drain_multipart(&mut multipart).await;
            return Err(e);
        }
    };
    if let Err(e) = tokio::fs::rename(&partial, &complete).await {
        tracing::error!(error = %e, "cannot move the upload into place");
        return Err(ApiError::Internal);
    }
    guard.0 = Some(complete);
    match state
        .documents
        .create(
            user,
            topic_id,
            NewUpload {
                id,
                filename,
                size_bytes: size,
            },
        )
        .await
    {
        Ok((document, job)) => {
            // From now on the import job owns the file.
            guard.0 = None;
            Ok((
                StatusCode::ACCEPTED,
                Json(UploadedDto {
                    document: DocumentDto::new(&document, &[]),
                    job: JobDto::from(&job),
                }),
            )
                .into_response())
        }
        Err(e) => Err(e.into()),
    }
}

/// How much of a refused upload is read and discarded, and for how long.
const DRAIN_MAX_BYTES: u64 = MAX_UPLOAD_BYTES;
const DRAIN_TIMEOUT: Duration = Duration::from_secs(60);

async fn drain_body(mut body: Body) {
    let drain = async {
        let mut read = 0u64;
        while let Some(Ok(frame)) = body.frame().await {
            read += frame.data_ref().map_or(0, |d| d.len() as u64);
            if read > DRAIN_MAX_BYTES {
                break;
            }
        }
    };
    let _ = tokio::time::timeout(DRAIN_TIMEOUT, drain).await;
}

async fn drain_multipart(multipart: &mut Multipart) {
    let drain = async {
        let mut read = 0u64;
        while let Ok(Some(mut field)) = multipart.next_field().await {
            while let Ok(Some(chunk)) = field.chunk().await {
                read += chunk.len() as u64;
                if read > DRAIN_MAX_BYTES {
                    return;
                }
            }
        }
    };
    let _ = tokio::time::timeout(DRAIN_TIMEOUT, drain).await;
}

/// Deletes its file when dropped, unless the file was handed over.
struct FileGuard(Option<PathBuf>);

impl Drop for FileGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[allow(clippy::needless_pass_by_value)] // used as `map_err(multipart_error)`
fn multipart_error(err: MultipartError) -> ApiError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::PayloadTooLarge
    } else {
        ApiError::bad_request(
            "Il modulo caricato non è valido.",
            "The uploaded form is not valid.",
        )
    }
}

/// Streams the field into `path`, chunk by chunk: the file is never in memory
/// whole. Checks the size while receiving and the `%PDF-` signature at the
/// start. Returns the size in bytes.
async fn receive(field: &mut Field<'_>, path: &FsPath) -> Result<u64, ApiError> {
    let mut file = tokio::fs::File::create(path).await.map_err(|e| {
        tracing::error!(error = %e, "cannot create the upload file");
        ApiError::Internal
    })?;
    let mut size: u64 = 0;
    let mut head: Vec<u8> = Vec::with_capacity(SIGNATURE_WINDOW);
    let mut signature_ok = false;
    while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
        size += chunk.len() as u64;
        if size > MAX_UPLOAD_BYTES {
            return Err(ApiError::PayloadTooLarge);
        }
        if !signature_ok {
            let room = SIGNATURE_WINDOW - head.len();
            head.extend_from_slice(&chunk[..chunk.len().min(room)]);
            signature_ok = has_signature(&head);
            if !signature_ok && head.len() >= SIGNATURE_WINDOW {
                return Err(ApiError::UnsupportedMediaType);
            }
        }
        file.write_all(&chunk).await.map_err(|e| {
            tracing::error!(error = %e, "cannot write the upload file");
            ApiError::Internal
        })?;
    }
    if !signature_ok {
        return Err(ApiError::UnsupportedMediaType);
    }
    file.flush().await.map_err(|e| {
        tracing::error!(error = %e, "cannot write the upload file");
        ApiError::Internal
    })?;
    Ok(size)
}

fn has_signature(head: &[u8]) -> bool {
    head.windows(SIGNATURE.len()).any(|w| w == SIGNATURE)
}

#[cfg(test)]
mod tests {
    use super::has_signature;

    #[test]
    fn the_signature_is_looked_for_in_the_first_kilobyte() {
        assert!(has_signature(b"%PDF-1.7\n"));
        assert!(has_signature(b"\xef\xbb\xbf%PDF-1.4"));
        assert!(!has_signature(b"PK\x03\x04"));
        assert!(!has_signature(b"%PDF"));
        assert!(!has_signature(b""));
    }
}
