//! `SpeechSynthesizer` over the HTTP server of the C engine for Qwen3-TTS
//! (`POST /v1/tts`, API doc in the engine repo: `docs/serving/api.md`).
//!
//! The engine answers a whole WAV (24 kHz, 16-bit, mono); it is encoded here to
//! Ogg Opus so the rest of the system only ever sees the stored format.

use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use domain::{ContentLanguage, EngineInfo, SpeechError, SpeechSynthesizer, SynthesizedClip, Voice};
use serde_json::json;

use super::opus::{BITRATE, SAMPLE_RATE, encode_ogg_opus};
use super::wav::decode_pcm16_mono;

const ENGINE: &str = "qwen3-tts";
/// Longest error body kept in a message.
const ERROR_BODY_CHARS: usize = 300;

#[derive(Debug, Clone)]
pub struct Qwen3TtsConfig {
    /// Base URL of the engine, without `/v1/tts`.
    pub base_url: String,
    /// Model name, only to tell clips of different models apart in the cache.
    pub model: String,
    pub voices: Vec<Voice>,
    pub default_voice: String,
    /// Fixed so the same text and voice give the same audio (ADR 0013 §14).
    pub seed: u32,
    pub connect_timeout: Duration,
    /// Whole request. A 1200-character paragraph takes about 6 minutes on the
    /// home server and requests wait their turn there: be generous.
    pub request_timeout: Duration,
}

impl Qwen3TtsConfig {
    pub const DEFAULT_MODEL: &'static str = "1.7b-customvoice";
    pub const DEFAULT_SEED: u32 = 42;

    #[must_use]
    pub fn new(
        base_url: impl Into<String>,
        voices: Vec<Voice>,
        default_voice: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            model: Self::DEFAULT_MODEL.to_owned(),
            voices,
            default_voice: default_voice.into(),
            seed: Self::DEFAULT_SEED,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_mins(15),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Qwen3TtsBuildError {
    #[error("the TTS URL is not a valid http(s) URL")]
    InvalidUrl,
    #[error("at least one voice is required")]
    NoVoices,
    #[error("voice id `{0}` is empty, too long or listed twice")]
    InvalidVoiceId(String),
    #[error("the label of voice `{0}` must be 1-40 characters")]
    InvalidLabel(String),
    #[error("the default voice `{0}` is not in the list of voices")]
    UnknownDefaultVoice(String),
    #[error("cannot build the HTTP client: {0}")]
    Client(String),
}

/// Parses `id:Label,id:Label` (a bare `id` uses itself as the label).
#[must_use]
pub fn parse_voices(raw: &str) -> Vec<Voice> {
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            let (id, label) = item.split_once(':').unwrap_or((item, item));
            Voice {
                id: id.trim().to_owned(),
                label: label.trim().to_owned(),
            }
        })
        .collect()
}

impl Qwen3TtsConfig {
    /// Checks voices, default voice and URL, so a mistake in the configuration
    /// stops the start instead of failing a clip hours later.
    pub fn validate(&self) -> Result<(), Qwen3TtsBuildError> {
        validate(self)
    }
}

fn base_url(raw: &str) -> Result<url::Url, Qwen3TtsBuildError> {
    url::Url::parse(raw)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https") && u.host().is_some())
        .ok_or(Qwen3TtsBuildError::InvalidUrl)
}

fn validate(config: &Qwen3TtsConfig) -> Result<(), Qwen3TtsBuildError> {
    if config.voices.is_empty() {
        return Err(Qwen3TtsBuildError::NoVoices);
    }
    let mut seen = HashSet::new();
    for voice in &config.voices {
        let id_ok = !voice.id.is_empty()
            && voice.id.chars().count() <= 40
            && voice.id.chars().all(|c| c.is_ascii_graphic());
        if !id_ok || !seen.insert(voice.id.as_str()) {
            return Err(Qwen3TtsBuildError::InvalidVoiceId(voice.id.clone()));
        }
        if !(1..=40).contains(&voice.label.chars().count()) {
            return Err(Qwen3TtsBuildError::InvalidLabel(voice.id.clone()));
        }
    }
    if !seen.contains(config.default_voice.as_str()) {
        return Err(Qwen3TtsBuildError::UnknownDefaultVoice(
            config.default_voice.clone(),
        ));
    }
    base_url(&config.base_url)?;
    Ok(())
}

pub struct Qwen3TtsSynthesizer {
    http: reqwest::Client,
    url: String,
    health_url: String,
    info: EngineInfo,
    voices: Vec<Voice>,
    default_voice: String,
    seed: u32,
}

impl Qwen3TtsSynthesizer {
    pub fn new(config: Qwen3TtsConfig) -> Result<Self, Qwen3TtsBuildError> {
        validate(&config)?;
        let base = base_url(&config.base_url)?;
        let http = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| Qwen3TtsBuildError::Client(e.without_url().to_string()))?;
        // Everything that changes the audio, so a change makes new cache keys.
        let params = format!(
            // Fixed to Italian: this string is part of the cache key of every
            // clip made before topics had a language. Other languages are told
            // apart by `clip_key` itself.
            "language=Italian;seed={};format=ogg-opus-mono-{}k",
            config.seed,
            BITRATE / 1000
        );
        Ok(Self {
            http,
            url: format!("{}/v1/tts", base.as_str().trim_end_matches('/')),
            health_url: format!("{}/v1/health", base.as_str().trim_end_matches('/')),
            info: EngineInfo {
                engine: ENGINE.to_owned(),
                model: config.model,
                params,
            },
            voices: config.voices,
            default_voice: config.default_voice,
            seed: config.seed,
        })
    }
}

fn truncated(body: &[u8]) -> String {
    String::from_utf8_lossy(body)
        .chars()
        .take(ERROR_BODY_CHARS)
        .collect()
}

/// A timeout counts as a failed attempt: a text that makes the engine hang must
/// not stall the queue forever. Anything else at transport level (connection
/// refused, reset) means the engine is down, which uses no attempt.
fn transport(e: reqwest::Error) -> SpeechError {
    let timed_out = e.is_timeout() && !e.is_connect();
    let message = e.without_url().to_string();
    if timed_out {
        SpeechError::Failed(message)
    } else {
        SpeechError::Unavailable(message)
    }
}

#[async_trait]
impl SpeechSynthesizer for Qwen3TtsSynthesizer {
    fn engine(&self) -> &EngineInfo {
        &self.info
    }

    fn voices(&self) -> &[Voice] {
        &self.voices
    }

    fn default_voice(&self) -> &str {
        &self.default_voice
    }

    async fn is_up(&self) -> bool {
        self.http
            .get(&self.health_url)
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
    }

    #[tracing::instrument(name = "tts.synthesize", skip_all, fields(tts.voice = voice, tts.chars = text.chars().count()))]
    async fn synthesize(
        &self,
        text: &str,
        voice: &str,
        language: ContentLanguage,
    ) -> Result<SynthesizedClip, SpeechError> {
        if !self.voices.iter().any(|v| v.id == voice) {
            return Err(SpeechError::UnknownVoice);
        }
        // Exactly these fields: the engine answers 400 to any it does not know.
        let body = json!({
            "text": text,
            "speaker": voice,
            "language": language.english_name(),
            "seed": self.seed,
        });
        let response = self
            .http
            .post(&self.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(transport)?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(transport)?;
        if matches!(status.as_u16(), 502..=504) {
            // 503 is the engine's full queue, 502/504 a proxy in front of it
            // that cannot reach it: retry later, no attempt used.
            return Err(SpeechError::Unavailable(format!(
                "HTTP {status}: {}",
                truncated(&bytes)
            )));
        }
        // Any other error, 500 included, is about this text: it must use an attempt.
        if !status.is_success() {
            return Err(SpeechError::Failed(format!(
                "HTTP {status}: {}",
                truncated(&bytes)
            )));
        }
        // Decoding and encoding are CPU work: keep them off the async threads.
        let encoded = tokio::task::spawn_blocking(move || {
            let pcm = decode_pcm16_mono(&bytes, SAMPLE_RATE).map_err(|e| e.to_string())?;
            encode_ogg_opus(&pcm).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| SpeechError::Failed(e.to_string()))?
        .map_err(SpeechError::Failed)?;
        Ok(SynthesizedClip {
            audio: encoded.audio,
            duration_ms: encoded.duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voices() -> Vec<Voice> {
        parse_voices("a:Alfa, b:Beta ,c")
    }

    #[test]
    fn voices_are_parsed_in_order() {
        let v = voices();
        let pairs: Vec<_> = v
            .iter()
            .map(|v| (v.id.as_str(), v.label.as_str()))
            .collect();
        assert_eq!(pairs, [("a", "Alfa"), ("b", "Beta"), ("c", "c")]);
    }

    #[test]
    fn the_configuration_is_checked() {
        let ok = Qwen3TtsConfig::new("http://tts:8080", voices(), "a");
        assert!(Qwen3TtsSynthesizer::new(ok.clone()).is_ok());

        let mut c = ok.clone();
        c.default_voice = "z".into();
        assert!(matches!(
            Qwen3TtsSynthesizer::new(c),
            Err(Qwen3TtsBuildError::UnknownDefaultVoice(_))
        ));
        let mut c = ok.clone();
        c.voices.push(Voice {
            id: "a".into(),
            label: "Again".into(),
        });
        assert!(matches!(
            Qwen3TtsSynthesizer::new(c),
            Err(Qwen3TtsBuildError::InvalidVoiceId(_))
        ));
        let mut c = ok.clone();
        c.voices[0].label = "x".repeat(41);
        assert!(matches!(
            Qwen3TtsSynthesizer::new(c),
            Err(Qwen3TtsBuildError::InvalidLabel(_))
        ));
        let mut c = ok.clone();
        c.voices.clear();
        assert!(matches!(
            Qwen3TtsSynthesizer::new(c),
            Err(Qwen3TtsBuildError::NoVoices)
        ));
        let mut c = ok;
        c.base_url = "ftp://x".into();
        assert!(matches!(
            Qwen3TtsSynthesizer::new(c),
            Err(Qwen3TtsBuildError::InvalidUrl)
        ));
    }
}
