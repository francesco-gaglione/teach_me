//! Read-aloud endpoints (contract: `audio.md` §3, §4.1, §5). The choice of
//! voice is `PUT /me/tts-voice`, with the other account routes in `auth.rs`.

use std::io;
use std::sync::Arc;

use application::{AudioService, Manifest, ManifestStatus, VoiceList};
use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use axum::routing::get;
use domain::{ClipId, LessonId};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::folders::path_id;
use crate::state::AppState;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/lessons/{id}/audio", get(manifest))
        .route("/audio/clips/{clip_id}", get(clip))
        .route("/tts/voices", get(voices))
}

fn clip_url(id: ClipId) -> String {
    format!("/audio/clips/{id}")
}

#[derive(Serialize)]
struct ClipRefDto {
    url: String,
    voice: String,
    duration_ms: u32,
}

#[derive(Serialize)]
struct BlockDto {
    block_id: String,
    status: &'static str,
    clip: Option<ClipRefDto>,
}

#[derive(Serialize)]
struct ManifestDto {
    voice: Option<String>,
    blocks: Vec<BlockDto>,
}

impl From<Manifest> for ManifestDto {
    fn from(m: Manifest) -> Self {
        Self {
            voice: m.voice,
            blocks: m
                .blocks
                .into_iter()
                .map(|b| BlockDto {
                    block_id: b.block_id,
                    status: match b.status {
                        ManifestStatus::Ready => "ready",
                        ManifestStatus::Pending => "pending",
                        ManifestStatus::Failed => "failed",
                    },
                    clip: b.clip.map(|c| ClipRefDto {
                        url: clip_url(c.id),
                        voice: c.voice,
                        duration_ms: c.duration_ms,
                    }),
                })
                .collect(),
        }
    }
}

async fn manifest(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(lesson_id): Path<String>,
) -> Result<Json<ManifestDto>, ApiError> {
    let lesson_id = LessonId::new(path_id(&lesson_id)?);
    let manifest = state.read_aloud.manifest(user, lesson_id).await?;
    Ok(Json(manifest.into()))
}

#[derive(Serialize)]
struct VoiceDto {
    id: String,
    label: String,
    sample_url: Option<String>,
}

#[derive(Serialize)]
struct VoicesDto {
    available: bool,
    default_voice: Option<String>,
    voices: Vec<VoiceDto>,
}

impl From<VoiceList> for VoicesDto {
    fn from(l: VoiceList) -> Self {
        Self {
            available: l.available,
            default_voice: l.default_voice,
            voices: l
                .voices
                .into_iter()
                .map(|e| VoiceDto {
                    id: e.voice.id,
                    label: e.voice.label,
                    sample_url: e.sample.map(clip_url),
                })
                .collect(),
        }
    }
}

async fn voices(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<VoicesDto>, ApiError> {
    Ok(Json(state.read_aloud.voices().await?.into()))
}

/// The part of a clip to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Span {
    /// The whole file (no `Range`, or one that is to be ignored).
    Whole,
    /// Bytes `start..=end`.
    Part { start: u64, end: u64 },
    /// A valid range that starts past the end of the file.
    Unsatisfiable,
}

/// One `bytes=` range (RFC 9110 §14). A malformed header or several ranges are
/// ignored and the whole file is sent, which the RFC allows.
fn parse_range(value: Option<&HeaderValue>, size: u64) -> Span {
    let Some(spec) = value
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().strip_prefix("bytes="))
    else {
        return Span::Whole;
    };
    if spec.contains(',') {
        return Span::Whole;
    }
    let Some((first, last)) = spec.trim().split_once('-') else {
        return Span::Whole;
    };
    let (first, last) = (first.trim(), last.trim());
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let last_byte = size.saturating_sub(1);
    match (digits(first), digits(last)) {
        // `-n`: the last n bytes.
        (false, true) if first.is_empty() => {
            let Ok(n) = last.parse::<u64>() else {
                return Span::Whole;
            };
            if n == 0 || size == 0 {
                return Span::Unsatisfiable;
            }
            Span::Part {
                start: size.saturating_sub(n),
                end: last_byte,
            }
        }
        // `a-` and `a-b`.
        (true, bounded) if bounded || last.is_empty() => {
            let Ok(start) = first.parse::<u64>() else {
                return Span::Whole;
            };
            let end = if bounded {
                match last.parse::<u64>() {
                    Ok(end) if end >= start => end,
                    _ => return Span::Whole,
                }
            } else {
                last_byte
            };
            if start >= size {
                return Span::Unsatisfiable;
            }
            Span::Part {
                start,
                end: end.min(last_byte),
            }
        }
        _ => Span::Whole,
    }
}

/// Bytes read from the store per chunk of the response.
const CHUNK: u64 = 64 * 1024;

fn stream(service: Arc<AudioService>, id: ClipId, start: u64, end_exclusive: u64) -> Body {
    let chunks = futures_util::stream::unfold(start, move |position| {
        let service = service.clone();
        async move {
            if position >= end_exclusive {
                return None;
            }
            let want = usize::try_from((end_exclusive - position).min(CHUNK)).unwrap_or(0);
            match service.read_range(id, position, want).await {
                Ok(bytes) if !bytes.is_empty() => {
                    let next = position + bytes.len() as u64;
                    Some((Ok::<_, io::Error>(Bytes::from(bytes)), next))
                }
                // The file shrank or vanished under us: fail the body rather
                // than send a short one that looks complete.
                _ => Some((
                    Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "clip file changed while it was sent",
                    )),
                    end_exclusive,
                )),
            }
        }
    });
    Body::from_stream(chunks)
}

async fn clip(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(clip_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let id = ClipId::new(path_id(&clip_id)?);
    let access = state.read_aloud.clip(user, id).await?;
    let size = access.size;
    let (status, start, end_exclusive, content_range) =
        match parse_range(headers.get(header::RANGE), size) {
            Span::Whole => (StatusCode::OK, 0, size, None),
            Span::Part { start, end } => (
                StatusCode::PARTIAL_CONTENT,
                start,
                end + 1,
                Some(format!("bytes {start}-{end}/{size}")),
            ),
            Span::Unsatisfiable => return Err(ApiError::RangeNotSatisfiable(size)),
        };
    let mut response = Response::new(stream(state.read_aloud.clone(), id, start, end_exclusive));
    *response.status_mut() = status;
    let h = response.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/ogg"));
    // A clip never changes (content-addressed): cache it for good.
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from(end_exclusive - start),
    );
    if let Some(range) = content_range
        && let Ok(value) = HeaderValue::from_str(&range)
    {
        h.insert(header::CONTENT_RANGE, value);
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(header: &str, size: u64) -> Span {
        parse_range(Some(&HeaderValue::from_str(header).unwrap()), size)
    }

    #[test]
    fn ranges_follow_rfc_9110() {
        assert_eq!(parse_range(None, 100), Span::Whole);
        assert_eq!(span("bytes=0-9", 100), Span::Part { start: 0, end: 9 });
        assert_eq!(span("bytes=90-", 100), Span::Part { start: 90, end: 99 });
        assert_eq!(span("bytes=-10", 100), Span::Part { start: 90, end: 99 });
        // Past the end is cut; a suffix longer than the file is all of it.
        assert_eq!(span("bytes=90-500", 100), Span::Part { start: 90, end: 99 });
        assert_eq!(span("bytes=-500", 100), Span::Part { start: 0, end: 99 });
    }

    #[test]
    fn a_range_past_the_end_is_unsatisfiable() {
        assert_eq!(span("bytes=100-", 100), Span::Unsatisfiable);
        assert_eq!(span("bytes=150-160", 100), Span::Unsatisfiable);
        assert_eq!(span("bytes=-0", 100), Span::Unsatisfiable);
        assert_eq!(span("bytes=0-1", 0), Span::Unsatisfiable);
    }

    #[test]
    fn what_cannot_be_read_as_one_range_is_ignored() {
        for header in [
            "bytes=0-1,5-6",
            "items=0-1",
            "bytes=",
            "bytes=-",
            "bytes=a-b",
            "bytes=9-3",
            "0-9",
        ] {
            assert_eq!(span(header, 100), Span::Whole, "{header}");
        }
    }
}
