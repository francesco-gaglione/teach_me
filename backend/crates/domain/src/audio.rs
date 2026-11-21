//! Spoken audio of lessons: voices, clips and the content-addressed cache key
//! (`docs/contracts/audio.md`, ADR 0013).

use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::ids::{ClipId, LessonId};
use crate::language::ContentLanguage;

/// A voice the server offers. The id is opaque: only the engine adapter (and
/// the configuration that lists the voices) knows what it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voice {
    pub id: String,
    /// 1-40 characters, shown to the user.
    pub label: String,
}

/// What a clip depends on besides the text and the voice (ADR 0013 §9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInfo {
    /// Adapter family, e.g. `qwen3-tts`.
    pub engine: String,
    pub model: String,
    /// Everything else that changes the audio (language, fixed seed, output
    /// encoding) as one canonical string: changing any of it needs new clips.
    pub params: String,
}

/// Encoded audio the engine produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthesizedClip {
    /// Ogg Opus, mono.
    pub audio: Vec<u8>,
    /// Always > 0.
    pub duration_ms: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
    #[error("unknown voice")]
    UnknownVoice,
    /// The engine could not be reached or timed out: worth retrying.
    #[error("speech engine unavailable: {0}")]
    Unavailable(String),
    /// The engine answered but the request or the answer is unusable
    /// (rejected text, audio in an unexpected format).
    #[error("speech synthesis failed: {0}")]
    Failed(String),
}

/// A clip is unused once nobody was shown it for this long (the last use is
/// the newest manifest that named it): the cleanup then deletes it.
pub const CLIP_MAX_IDLE: time::Duration = time::Duration::days(90);

/// A grant already recorded is rewritten only when it is this much older than
/// the new one: "last used" is exact to a day, which is all 90 days needs.
pub const GRANT_REFRESH: time::Duration = time::Duration::days(1);

/// Stored metadata of one clip; the bytes live in a `ClipStore`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    pub id: ClipId,
    /// [`clip_key`] of what it was made from.
    pub key: String,
    /// [`text_hash`] of the spoken text, to find the same text in other voices.
    pub text_hash: String,
    pub voice: String,
    pub duration_ms: u32,
    pub size_bytes: u64,
    pub created_at: OffsetDateTime,
}

/// One block of one lesson waiting to be spoken in one voice (the internal
/// audio queue of ADR 0013 §11, not a `Job`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioRequest {
    pub lesson_id: LessonId,
    pub voice: String,
    /// Language of the lesson's topic: the voice speaks it.
    pub language: ContentLanguage,
    pub block_id: String,
    /// The spoken text ([`crate::readable_blocks`]), kept so the worker does
    /// not need the lesson.
    pub text: String,
}

/// An [`AudioRequest`] the worker took from the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedAudio {
    /// Queue-private handle for `complete`, `release` and `record_failure`.
    pub entry: i64,
    pub request: AudioRequest,
}

/// Where a block that has no clip yet stands (`audio.md` §3: `queued` and
/// `running` are `pending`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockAudioState {
    Queued,
    Running,
    /// Out of attempts; back to `Queued` when the lesson is opened again.
    Failed,
}

fn digest(tag: &str, fields: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tag.as_bytes());
    for field in fields {
        // Length prefix: ("ab", "c") and ("a", "bc") must not collide.
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// The cache key of a clip: spoken text, engine, model, voice and parameters.
/// The tag is versioned so the key scheme itself can change.
#[must_use]
pub fn clip_key(text: &str, engine: &EngineInfo, voice: &str, language: ContentLanguage) -> String {
    let mut fields = vec![text, &engine.engine, &engine.model, voice, &engine.params];
    // Italian adds nothing, so the clips made before topics had a language
    // keep their keys (`engine.params` already says Italian).
    if language != ContentLanguage::It {
        fields.push(language.english_name());
    }
    digest("teach_me.clip.v1", &fields)
}

/// Hash of the spoken text and its language, without voice or engine: what
/// the clips that can stand in for each other have in common. Italian adds
/// nothing, so the clips stored before topics had a language keep their hash;
/// without the language an identical short text could stand in across
/// languages.
#[must_use]
pub fn text_hash(text: &str, language: ContentLanguage) -> String {
    if language == ContentLanguage::It {
        digest("teach_me.text.v1", &[text])
    } else {
        digest("teach_me.text.v1", &[text, language.english_name()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> EngineInfo {
        EngineInfo {
            engine: "qwen3-tts".into(),
            model: "0.6b-customvoice".into(),
            params: "language=Italian;seed=42".into(),
        }
    }

    #[test]
    fn the_key_is_stable() {
        // Pinned: changing the scheme orphans every stored clip.
        assert_eq!(
            clip_key("Ciao mondo.", &engine(), "voice_a", ContentLanguage::It),
            "1bd88c8ca02edad0ada1d032626254b7a75817362baf99a75e522d53e1c7aa64"
        );
        assert_eq!(
            text_hash("Ciao mondo.", ContentLanguage::It),
            "a0243abc015ff77a273933633926a65b3eb4669f916eba0cbbc0f06eae2abaf8"
        );
    }

    #[test]
    fn every_input_changes_the_key() {
        let base = clip_key("t", &engine(), "v", ContentLanguage::It);
        assert_eq!(base, clip_key("t", &engine(), "v", ContentLanguage::It));
        assert_ne!(base, clip_key("t2", &engine(), "v", ContentLanguage::It));
        assert_ne!(base, clip_key("t", &engine(), "v2", ContentLanguage::It));
        let mut e = engine();
        e.model.push('x');
        assert_ne!(base, clip_key("t", &e, "v", ContentLanguage::It));
        let mut e = engine();
        e.params = "language=Italian;seed=43".into();
        assert_ne!(base, clip_key("t", &e, "v", ContentLanguage::It));
        let mut e = engine();
        e.engine = "other".into();
        assert_ne!(base, clip_key("t", &e, "v", ContentLanguage::It));
    }

    #[test]
    fn only_a_language_other_than_italian_changes_the_key() {
        let it = clip_key("t", &engine(), "v", ContentLanguage::It);
        let en = clip_key("t", &engine(), "v", ContentLanguage::En);
        assert_ne!(it, en);
        assert_eq!(en, clip_key("t", &engine(), "v", ContentLanguage::En));
    }

    #[test]
    fn fields_do_not_bleed_into_each_other() {
        let a = EngineInfo {
            engine: "ab".into(),
            model: "c".into(),
            params: String::new(),
        };
        let b = EngineInfo {
            engine: "a".into(),
            model: "bc".into(),
            params: String::new(),
        };
        assert_ne!(
            clip_key("t", &a, "v", ContentLanguage::It),
            clip_key("t", &b, "v", ContentLanguage::It)
        );
    }

    #[test]
    fn the_text_hash_ignores_voice_and_engine() {
        assert_eq!(text_hash("t", ContentLanguage::It).len(), 64);
        assert_ne!(
            text_hash("t", ContentLanguage::It),
            text_hash("u", ContentLanguage::It)
        );
    }

    #[test]
    fn the_text_hash_tells_languages_apart() {
        let it = text_hash("No.", ContentLanguage::It);
        let en = text_hash("No.", ContentLanguage::En);
        assert_ne!(it, en);
        assert_eq!(en, text_hash("No.", ContentLanguage::En));
    }
}
