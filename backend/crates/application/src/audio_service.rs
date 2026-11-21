//! What the user can ask of read-aloud: the manifest of a lesson, the voices,
//! the choice of voice and the permission to read a clip
//! (`docs/contracts/audio.md` §3, §4, §5).
//!
//! Nothing here queues work: that is the opening of a lesson
//! ([`crate::AudioPreparer`]). With no engine configured the service is built
//! without a preparer and every answer is the "feature off" one.

use std::sync::Arc;

use domain::{
    BlockAudioState, Clip, ClipId, IssueKind, LessonId, LessonRepository, TopicRepository, User,
    UserId, UserRepository, ValidationIssue, Voice, readable_blocks,
};

use crate::audio::{AudioError, SAMPLE_LANGUAGE, SAMPLE_TEXT};
use crate::audio_queue::AudioPreparer;
use crate::error::{AppError, ConflictReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestStatus {
    Ready,
    Pending,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestClip {
    pub id: ClipId,
    pub voice: String,
    pub duration_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestBlock {
    pub block_id: String,
    pub status: ManifestStatus,
    pub clip: Option<ManifestClip>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The user's current voice; `None` with the feature off.
    pub voice: Option<String>,
    pub blocks: Vec<ManifestBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceEntry {
    pub voice: Voice,
    /// The clip of the fixed sample sentence, once it is made.
    pub sample: Option<ClipId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceList {
    pub available: bool,
    pub default_voice: Option<String>,
    pub voices: Vec<VoiceEntry>,
}

/// A clip the user may read, with the size of its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipAccess {
    pub clip: Clip,
    pub size: u64,
}

fn storage(e: AudioError) -> AppError {
    match e {
        AudioError::Repository(domain::RepositoryError::NotFound) => AppError::NotFound,
        other => AppError::Storage(Box::new(other)),
    }
}

pub struct AudioService {
    audio: Option<Arc<AudioPreparer>>,
    lessons: Arc<dyn LessonRepository>,
    topics: Arc<dyn TopicRepository>,
    users: Arc<dyn UserRepository>,
}

impl AudioService {
    #[must_use]
    pub fn new(
        audio: Option<Arc<AudioPreparer>>,
        lessons: Arc<dyn LessonRepository>,
        topics: Arc<dyn TopicRepository>,
        users: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            audio,
            lessons,
            topics,
            users,
        }
    }

    /// The `tts_voice` of a `User` response: the saved voice if the server
    /// still lists it, else the default; `None` with the feature off.
    #[must_use]
    pub fn effective_voice(&self, user: &User) -> Option<String> {
        self.audio
            .as_ref()
            .map(|a| a.effective_voice(user.tts_voice.as_deref()).to_owned())
    }

    pub async fn voices(&self) -> Result<VoiceList, AppError> {
        let Some(audio) = &self.audio else {
            return Ok(VoiceList {
                available: false,
                default_voice: None,
                voices: Vec::new(),
            });
        };
        let cache = audio.cache();
        let mut voices = Vec::new();
        for voice in cache.voices() {
            let sample = cache
                .find(SAMPLE_TEXT, &voice.id, SAMPLE_LANGUAGE)
                .await
                .map_err(storage)?
                .map(|c| c.id);
            voices.push(VoiceEntry {
                voice: voice.clone(),
                sample,
            });
        }
        Ok(VoiceList {
            available: true,
            default_voice: Some(cache.default_voice().to_owned()),
            voices,
        })
    }

    /// Saves the user's voice. Choosing does not queue anything: lessons are
    /// made when they are opened.
    pub async fn set_voice(&self, user: UserId, voice: &str) -> Result<User, AppError> {
        // Off is checked before the value: there is no list to check against.
        let Some(audio) = &self.audio else {
            return Err(AppError::Conflict(ConflictReason::ReadAloudOff));
        };
        if !audio.cache().voices().iter().any(|v| v.id == voice) {
            return Err(AppError::validation(ValidationIssue::new(
                "voice",
                IssueKind::UnknownVoice,
            )));
        }
        self.users.set_tts_voice(user, voice).await?;
        self.users.get(user).await?.ok_or(AppError::NotFound)
    }

    /// The audio state of every readable block of a lesson. Records that the
    /// user may read each clip it names. Never queues.
    pub async fn manifest(&self, user: UserId, lesson: LessonId) -> Result<Manifest, AppError> {
        // Ownership first, so a lesson of someone else is `404` on any server.
        let stored = self
            .lessons
            .get(user, lesson)
            .await?
            .ok_or(AppError::NotFound)?;
        let Some(audio) = &self.audio else {
            return Ok(Manifest {
                voice: None,
                blocks: Vec::new(),
            });
        };
        let cache = audio.cache();
        // The voice speaks the language of the lesson's topic.
        let language = self
            .topics
            .get(user, stored.lesson.topic_id)
            .await?
            .ok_or(AppError::NotFound)?
            .language;
        let voice = audio.voice_of(user).await.map_err(storage)?;
        let queued = audio.states(lesson, &voice).await.map_err(storage)?;

        // From the first `find` to the grant: the cleanup must not delete a
        // clip this manifest is about to promise (`ClipCache::clean_up`).
        let _held = cache.hold().await;
        let mut blocks = Vec::new();
        let mut shown = Vec::new();
        for block in readable_blocks(&stored.document) {
            let (status, clip) = if let Some(clip) = cache
                .find(&block.text, &voice, language)
                .await
                .map_err(storage)?
            {
                (ManifestStatus::Ready, Some(clip))
            } else {
                let state = match queued.get(&block.block_id) {
                    Some(BlockAudioState::Failed) => ManifestStatus::Failed,
                    // Queued, running, or never asked for yet.
                    _ => ManifestStatus::Pending,
                };
                let other = cache
                    .find_other_voice(&block.text, &voice, language)
                    .await
                    .map_err(storage)?;
                (state, other)
            };
            if let Some(clip) = &clip {
                shown.push(clip.id);
            }
            blocks.push(ManifestBlock {
                block_id: block.block_id,
                status,
                clip: clip.map(|c| ManifestClip {
                    id: c.id,
                    voice: c.voice,
                    duration_ms: c.duration_ms,
                }),
            });
        }
        cache.grant(user, &shown).await.map_err(storage)?;
        Ok(Manifest {
            voice: Some(voice),
            blocks,
        })
    }

    /// The clip, if `user` may read it: a voice sample, or one a manifest of
    /// theirs named. Anything else, and a clip whose file is gone, is `NotFound`.
    pub async fn clip(&self, user: UserId, id: ClipId) -> Result<ClipAccess, AppError> {
        let audio = self.audio.as_ref().ok_or(AppError::NotFound)?;
        let cache = audio.cache();
        let clip = cache
            .get(id)
            .await
            .map_err(storage)?
            .ok_or(AppError::NotFound)?;
        let allowed =
            cache.is_sample(&clip) || cache.is_granted(user, id).await.map_err(storage)?;
        if !allowed {
            return Err(AppError::NotFound);
        }
        let size = cache
            .file_size(id)
            .await
            .map_err(storage)?
            .ok_or(AppError::NotFound)?;
        Ok(ClipAccess { clip, size })
    }

    /// A slice of a clip's file; call [`Self::clip`] first for the permission.
    pub async fn read_range(
        &self,
        id: ClipId,
        offset: u64,
        len: usize,
    ) -> Result<Vec<u8>, AppError> {
        let audio = self.audio.as_ref().ok_or(AppError::NotFound)?;
        audio
            .cache()
            .read_range(id, offset, len)
            .await
            .map_err(storage)?
            .ok_or(AppError::NotFound)
    }
}
