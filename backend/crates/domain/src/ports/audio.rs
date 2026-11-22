//! Ports of the read-aloud feature (ADR 0013): the speech engine, the clip
//! metadata and the clip files.

use std::collections::HashMap;

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::audio::{
    AudioRequest, BlockAudioState, Clip, EngineInfo, QueuedAudio, SpeechError, SynthesizedClip,
    Voice,
};
use crate::ids::{ClipId, LessonId, UserId};
use crate::language::ContentLanguage;
use crate::ports::RepositoryError;

/// A text-to-speech engine. Voices are opaque ids; nothing outside the adapter
/// and the configuration that lists them may depend on what they are.
#[async_trait]
pub trait SpeechSynthesizer: Send + Sync {
    /// Identity of the engine, model and parameters: part of every cache key.
    fn engine(&self) -> &EngineInfo;
    /// The curated voices, in the order to show them.
    fn voices(&self) -> &[Voice];
    /// One of [`Self::voices`].
    fn default_voice(&self) -> &str;
    /// Whether the engine answers right now (a cheap probe, for readiness).
    /// Adapters without a way to ask assume it does.
    async fn is_up(&self) -> bool {
        true
    }
    /// Speaks `text` in `language` with `voice`: Ogg Opus mono and its
    /// duration. [`SpeechError::UnknownVoice`] if `voice` is not in the list.
    async fn synthesize(
        &self,
        text: &str,
        voice: &str,
        language: ContentLanguage,
    ) -> Result<SynthesizedClip, SpeechError>;
}

#[async_trait]
pub trait ClipRepository: Send + Sync {
    async fn get(&self, id: ClipId) -> Result<Option<Clip>, RepositoryError>;
    async fn find_by_key(&self, key: &str) -> Result<Option<Clip>, RepositoryError>;
    /// A clip of the same spoken text (`text_hash`) in a voice other than
    /// `voice`; the most recent one. The fallback of `audio.md` §3.
    async fn find_other_voice(
        &self,
        text_hash: &str,
        voice: &str,
    ) -> Result<Option<Clip>, RepositoryError>;
    /// [`RepositoryError::Duplicate`] if a clip with the same key or id exists.
    async fn insert(&self, clip: &Clip) -> Result<(), RepositoryError>;
    /// Records that `clips` were shown to `user` in a manifest, which is what
    /// lets them read the files (`audio.md` §5). Idempotent. The time kept is
    /// the last one: it moves forward only when the stored one is more than
    /// [`crate::GRANT_REFRESH`] older than `at`, so a manifest is not a write per clip.
    async fn grant(
        &self,
        user: UserId,
        clips: &[ClipId],
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError>;
    async fn is_granted(&self, user: UserId, clip: ClipId) -> Result<bool, RepositoryError>;
    /// Every clip, with when it was last shown to anybody (the newest grant,
    /// or `created_at` when it never was).
    async fn list_with_last_use(&self) -> Result<Vec<(Clip, OffsetDateTime)>, RepositoryError>;
    /// Deletes the row (and its grants) if the clip was last used before
    /// `cutoff`, checked in the same statement: a manifest that granted it a
    /// moment ago wins. Whether it was deleted.
    async fn delete_if_unused_since(
        &self,
        id: ClipId,
        cutoff: OffsetDateTime,
    ) -> Result<bool, RepositoryError>;
}

/// A clip file found in the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredFile {
    pub id: ClipId,
    pub size: u64,
    pub modified: OffsetDateTime,
}

/// Where the clip files live (a rebuildable cache, outside the backup).
#[async_trait]
pub trait ClipStore: Send + Sync {
    /// Stores the file whole or not at all (no partial file is ever visible).
    async fn put(&self, id: ClipId, audio: &[u8]) -> std::io::Result<()>;
    /// Whether the file of `id` is there (the volume can be emptied while the
    /// database keeps the rows).
    async fn exists(&self, id: ClipId) -> std::io::Result<bool>;
    /// `None` if there is no file for `id`.
    async fn read(&self, id: ClipId) -> std::io::Result<Option<Vec<u8>>>;
    /// Size of the file in bytes, `None` if there is no file for `id`.
    async fn size(&self, id: ClipId) -> std::io::Result<Option<u64>>;
    /// Deletes the file of `id`; its size, or `None` if there was none.
    async fn remove(&self, id: ClipId) -> std::io::Result<Option<u64>>;
    /// Every clip file (temporary files and foreign names are not listed).
    async fn list(&self) -> std::io::Result<Vec<StoredFile>>;
    /// Up to `len` bytes from `offset` (fewer at the end of the file), so a
    /// response can be streamed in chunks. `None` if there is no file.
    async fn read_range(
        &self,
        id: ClipId,
        offset: u64,
        len: usize,
    ) -> std::io::Result<Option<Vec<u8>>>;
}

/// The persistent queue of blocks to speak. One worker takes one entry at a
/// time, oldest request first, except that the lesson opened last goes first.
/// An entry exists only while its clip is missing: finishing deletes it.
#[async_trait]
pub trait AudioQueue: Send + Sync {
    /// Adds the requests that are not queued yet; one that is already queued
    /// or running is left alone, one that is `Failed` is left alone unless
    /// `front`, which starts it over (new place in line, no attempts used).
    /// With `front` (the lesson was opened), every queued entry of `lesson`
    /// in `voice` (the new ones too) is moved ahead of all others, keeping
    /// their order. All `requests` are for that `lesson` and `voice`.
    async fn enqueue(
        &self,
        lesson: LessonId,
        voice: &str,
        requests: &[AudioRequest],
        front: bool,
    ) -> Result<(), RepositoryError>;
    /// Takes the next queued entry and marks it running.
    async fn claim_next(&self) -> Result<Option<QueuedAudio>, RepositoryError>;
    /// The clip exists: the entry is done.
    async fn complete(&self, entry: i64) -> Result<(), RepositoryError>;
    /// Puts a running entry back without using an attempt (the engine was
    /// down).
    async fn release(&self, entry: i64) -> Result<(), RepositoryError>;
    /// Uses one attempt; the entry becomes `Failed` once `max_attempts` are
    /// used, else it goes back to the queue. Returns where it ended up.
    async fn record_failure(
        &self,
        entry: i64,
        max_attempts: u32,
    ) -> Result<BlockAudioState, RepositoryError>;
    /// Startup: entries left running by a crash are queued again.
    async fn recover(&self) -> Result<u64, RepositoryError>;
    /// The blocks of a lesson with an entry for `voice`, by block id. A block
    /// with no entry is either done or was never requested.
    async fn states(
        &self,
        lesson: LessonId,
        voice: &str,
    ) -> Result<HashMap<String, BlockAudioState>, RepositoryError>;
}
