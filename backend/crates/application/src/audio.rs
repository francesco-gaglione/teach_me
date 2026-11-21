//! The clip cache: a clip exists for a (spoken text, voice) of the current
//! engine, or it is made (`docs/contracts/audio.md`, ADR 0013 §9).
//!
//! No special case for a changed voice, a changed model or a restart: the key
//! covers all of them, so a missing key just means "generate it".

use std::collections::HashSet;
use std::sync::Arc;

use domain::{
    CLIP_MAX_IDLE, Clip, ClipId, ClipRepository, ClipStore, Clock, ContentLanguage, IdGenerator,
    RepositoryError, SpeechError, SpeechSynthesizer, UserId, Voice, clip_key, text_hash,
};
use time::Duration;
use tokio::sync::{RwLock, RwLockReadGuard};

/// A file without a row is swept only when it is older than this: the worker
/// writes the file first and the row right after, so a young file may be a
/// clip about to be recorded.
pub const ORPHAN_MIN_AGE: Duration = Duration::hours(24);

/// What one cleanup pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CleanupReport {
    /// Idle clips deleted (row and file).
    pub clips_deleted: u32,
    pub bytes_freed: u64,
    /// Files with no row, deleted.
    pub orphans_deleted: u32,
    pub orphan_bytes_freed: u64,
    /// Deletions that failed; they are tried again at the next pass.
    pub failed: u32,
}

/// What every voice reads as its sample (`audio.md` §4.1). Fixed: a sample is
/// recognised by the cache key of this text, so changing it makes the old
/// samples ordinary clips nobody can read until the new ones exist.
/// The sample sentence is Italian, whatever the topics are.
pub const SAMPLE_LANGUAGE: ContentLanguage = ContentLanguage::It;
pub const SAMPLE_TEXT: &str = "Ciao, sono la voce che leggerà le tue lezioni.";

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error(transparent)]
    Speech(#[from] SpeechError),
    #[error("clip metadata: {0}")]
    Repository(#[from] RepositoryError),
    #[error("clip file: {0}")]
    Files(#[source] std::io::Error),
}

pub struct ClipCache {
    synthesizer: Arc<dyn SpeechSynthesizer>,
    clips: Arc<dyn ClipRepository>,
    store: Arc<dyn ClipStore>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    /// Orders the cleanup against what promises a clip. Whoever finds a clip
    /// and then records its use ([`Self::grant`]) holds the read side across
    /// both; the cleanup deletes one clip at a time under the write side. So
    /// a clip is either gone before it is found, or used before it can be
    /// deleted, and the delete itself re-checks the last use.
    gate: RwLock<()>,
}

impl ClipCache {
    #[must_use]
    pub fn new(
        synthesizer: Arc<dyn SpeechSynthesizer>,
        clips: Arc<dyn ClipRepository>,
        store: Arc<dyn ClipStore>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            synthesizer,
            clips,
            store,
            clock,
            ids,
            gate: RwLock::new(()),
        }
    }

    /// Hold this from finding a clip until its use is recorded with
    /// [`Self::grant`], so the cleanup cannot delete it in between. Never hold
    /// it across a synthesis: a waiting cleanup would block every new holder.
    pub async fn hold(&self) -> RwLockReadGuard<'_, ()> {
        self.gate.read().await
    }

    /// The curated voices, in the order to show them.
    #[must_use]
    pub fn voices(&self) -> &[Voice] {
        self.synthesizer.voices()
    }

    /// Whether `clip` is the sample of one of the listed voices.
    #[must_use]
    pub fn is_sample(&self, clip: &Clip) -> bool {
        self.voices().iter().any(|v| {
            clip.key
                == clip_key(
                    SAMPLE_TEXT,
                    self.synthesizer.engine(),
                    &v.id,
                    SAMPLE_LANGUAGE,
                )
        })
    }

    pub async fn get(&self, id: ClipId) -> Result<Option<Clip>, AudioError> {
        Ok(self.clips.get(id).await?)
    }

    /// Lets `user` read `clips` from now on (they are in their manifest).
    pub async fn grant(&self, user: UserId, clips: &[ClipId]) -> Result<(), AudioError> {
        Ok(self.clips.grant(user, clips, self.clock.now()).await?)
    }

    pub async fn is_granted(&self, user: UserId, clip: ClipId) -> Result<bool, AudioError> {
        Ok(self.clips.is_granted(user, clip).await?)
    }

    /// Size of the clip's file, `None` if the file is gone.
    pub async fn file_size(&self, id: ClipId) -> Result<Option<u64>, AudioError> {
        self.store.size(id).await.map_err(AudioError::Files)
    }

    /// A slice of the clip's file, for streaming.
    pub async fn read_range(
        &self,
        id: ClipId,
        offset: u64,
        len: usize,
    ) -> Result<Option<Vec<u8>>, AudioError> {
        self.store
            .read_range(id, offset, len)
            .await
            .map_err(AudioError::Files)
    }

    /// The voice a user gets when they have not chosen one.
    #[must_use]
    pub fn default_voice(&self) -> &str {
        self.synthesizer.default_voice()
    }

    /// The clip of `text` in `voice` if it is already made and its file is
    /// there. Never synthesizes.
    pub async fn find(
        &self,
        text: &str,
        voice: &str,
        language: ContentLanguage,
    ) -> Result<Option<Clip>, AudioError> {
        let key = clip_key(text, self.synthesizer.engine(), voice, language);
        match self.clips.find_by_key(&key).await? {
            Some(clip)
                if self
                    .store
                    .exists(clip.id)
                    .await
                    .map_err(AudioError::Files)? =>
            {
                Ok(Some(clip))
            }
            _ => Ok(None),
        }
    }

    /// A clip of the same text in another voice and the same language, to
    /// play while `voice` is made.
    pub async fn find_other_voice(
        &self,
        text: &str,
        voice: &str,
        language: ContentLanguage,
    ) -> Result<Option<Clip>, AudioError> {
        let found = self
            .clips
            .find_other_voice(&text_hash(text, language), voice)
            .await?;
        match found {
            Some(clip)
                if self
                    .store
                    .exists(clip.id)
                    .await
                    .map_err(AudioError::Files)? =>
            {
                Ok(Some(clip))
            }
            _ => Ok(None),
        }
    }

    /// One pass of the cleanup (ADR 0013 §Consequences): deletes the clips
    /// nobody was shown for [`CLIP_MAX_IDLE`], never the voice samples, then
    /// the files that have no row and are older than [`ORPHAN_MIN_AGE`].
    ///
    /// A clip deleted here is made again, by the same path as one never made,
    /// the next time a lesson with that text is opened (the key is its content).
    ///
    /// Races: what promises a clip to a user records its use under
    /// [`Self::hold`], and each deletion here takes the write side and
    /// re-checks the last use in the statement that deletes the row, so a
    /// clip just promised is never deleted. The row goes before the file: a
    /// crash in between leaves an orphan file (swept later), never a row
    /// whose file is gone. A row whose file is missing is left alone: the
    /// manifest reports the block as pending and the worker repairs it in
    /// place.
    pub async fn clean_up(&self) -> Result<CleanupReport, AudioError> {
        let now = self.clock.now();
        let cutoff = now - CLIP_MAX_IDLE;
        let mut report = CleanupReport::default();

        for (clip, last_use) in self.clips.list_with_last_use().await? {
            if last_use >= cutoff || self.is_sample(&clip) {
                continue;
            }
            let _gate = self.gate.write().await;
            match self.clips.delete_if_unused_since(clip.id, cutoff).await {
                // Shown again since the listing: it stays.
                Ok(false) => continue,
                Ok(true) => report.clips_deleted += 1,
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(clip = %clip.id, %error, "could not delete an idle clip");
                    continue;
                }
            }
            match self.store.remove(clip.id).await {
                Ok(freed) => report.bytes_freed += freed.unwrap_or(0),
                Err(error) => {
                    // The file stays with no row: the orphan sweep takes it.
                    report.failed += 1;
                    tracing::warn!(clip = %clip.id, %error, "could not delete the file of a clip");
                }
            }
        }

        // Files first, rows after: a clip recorded in between has a young file
        // anyway, and one missing from the files is not touched.
        let files = self.store.list().await.map_err(AudioError::Files)?;
        let known: HashSet<ClipId> = self
            .clips
            .list_with_last_use()
            .await?
            .into_iter()
            .map(|(clip, _)| clip.id)
            .collect();
        for file in files {
            if known.contains(&file.id) || file.modified >= now - ORPHAN_MIN_AGE {
                continue;
            }
            match self.store.remove(file.id).await {
                Ok(freed) => {
                    report.orphans_deleted += 1;
                    report.orphan_bytes_freed += freed.unwrap_or(0);
                }
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(clip = %file.id, %error, "could not delete an orphan clip file");
                }
            }
        }
        Ok(report)
    }

    /// The clip of `text` in `voice`, made now if it does not exist yet.
    ///
    /// Order matters: the file is written before the row, so a row always has
    /// its file; a crash in between leaves an orphan file, never a dangling row.
    pub async fn ensure(
        &self,
        text: &str,
        voice: &str,
        language: ContentLanguage,
    ) -> Result<Clip, AudioError> {
        let engine = self.synthesizer.engine();
        let key = clip_key(text, engine, voice, language);
        let known = {
            let _held = self.hold().await;
            let known = self.clips.find_by_key(&key).await?;
            if let Some(clip) = &known
                && self
                    .store
                    .exists(clip.id)
                    .await
                    .map_err(AudioError::Files)?
            {
                return Ok(clip.clone());
            }
            known
        };

        let made = self.synthesizer.synthesize(text, voice, language).await?;
        let size_bytes = made.audio.len() as u64;
        if let Some(known) = known {
            // A row whose file was lost (an emptied volume) is repaired in
            // place. The file is written under the read side of the gate: the
            // cleanup works from a listing, and without the gate it could
            // delete the row (idle clip) and then the file we are writing.
            // The write takes milliseconds, unlike the synthesis. Never nest
            // `hold()`: tokio's RwLock favours a waiting writer, so a second
            // read taken while the first is held can deadlock.
            let _held = self.hold().await;
            self.store
                .put(known.id, &made.audio)
                .await
                .map_err(AudioError::Files)?;
            if self.clips.get(known.id).await?.is_some() {
                return Ok(known);
            }
            // The cleanup deleted the row during the synthesis (it was idle):
            // record the clip again, as new, with the file just written.
            let renewed = Clip {
                created_at: self.clock.now(),
                ..known
            };
            return match self.clips.insert(&renewed).await {
                Ok(()) => Ok(renewed),
                Err(RepositoryError::Duplicate) => self
                    .clips
                    .find_by_key(&renewed.key)
                    .await?
                    .ok_or(AudioError::Repository(RepositoryError::Duplicate)),
                Err(e) => Err(e.into()),
            };
        }
        let id = ClipId::new(self.ids.new_id());
        self.store
            .put(id, &made.audio)
            .await
            .map_err(AudioError::Files)?;
        let clip = Clip {
            id,
            key,
            text_hash: text_hash(text, language),
            voice: voice.to_owned(),
            duration_ms: made.duration_ms,
            size_bytes,
            created_at: self.clock.now(),
        };
        // `created_at` is now: the clip is not idle, whatever the cleanup does.
        match self.clips.insert(&clip).await {
            Ok(()) => Ok(clip),
            // Another task made the same clip meanwhile: keep theirs.
            Err(RepositoryError::Duplicate) => self
                .clips
                .find_by_key(&clip.key)
                .await?
                .ok_or(AudioError::Repository(RepositoryError::Duplicate)),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use domain::{EngineInfo, SynthesizedClip, UserId, Voice};
    use time::OffsetDateTime;
    use time::macros::datetime;

    use super::*;
    use crate::fakes::{SeqIds, TestClock};

    #[derive(Clone, Copy, PartialEq)]
    pub(crate) enum FailMode {
        Never,
        Unavailable,
        Rejects,
    }

    pub(crate) struct FakeTts {
        info: EngineInfo,
        voices: Vec<Voice>,
        pub(crate) calls: AtomicUsize,
        /// `(voice, text)` of every call, in order.
        pub(crate) spoken: Mutex<Vec<(String, String)>>,
        /// Language of every call, in order.
        pub(crate) languages: Mutex<Vec<ContentLanguage>>,
        pub(crate) fail: Mutex<FailMode>,
    }

    impl FakeTts {
        pub(crate) fn new(model: &str) -> Self {
            Self {
                info: EngineInfo {
                    engine: "fake".into(),
                    model: model.into(),
                    params: "seed=1".into(),
                },
                voices: ["a", "b"]
                    .map(|id| Voice {
                        id: id.into(),
                        label: id.to_uppercase(),
                    })
                    .to_vec(),
                calls: AtomicUsize::new(0),
                spoken: Mutex::default(),
                languages: Mutex::default(),
                fail: Mutex::new(FailMode::Never),
            }
        }
    }

    #[async_trait]
    impl SpeechSynthesizer for FakeTts {
        fn engine(&self) -> &EngineInfo {
            &self.info
        }
        fn voices(&self) -> &[Voice] {
            &self.voices
        }
        #[allow(clippy::unnecessary_literal_bound)]
        fn default_voice(&self) -> &str {
            "a"
        }
        async fn synthesize(
            &self,
            text: &str,
            voice: &str,
            language: ContentLanguage,
        ) -> Result<SynthesizedClip, SpeechError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.languages.lock().unwrap().push(language);
            self.spoken
                .lock()
                .unwrap()
                .push((voice.to_owned(), text.to_owned()));
            match *self.fail.lock().unwrap() {
                FailMode::Never => {}
                FailMode::Unavailable => return Err(SpeechError::Unavailable("down".into())),
                FailMode::Rejects => return Err(SpeechError::Failed("no".into())),
            }
            Ok(SynthesizedClip {
                audio: format!("{voice}:{text}").into_bytes(),
                duration_ms: 1000,
            })
        }
    }

    #[derive(Default)]
    pub(crate) struct MemClips(
        pub(crate) Mutex<Vec<Clip>>,
        pub(crate) Mutex<HashMap<(UserId, ClipId), OffsetDateTime>>,
    );

    #[async_trait]
    impl ClipRepository for MemClips {
        async fn get(&self, id: ClipId) -> Result<Option<Clip>, RepositoryError> {
            Ok(self.0.lock().unwrap().iter().find(|c| c.id == id).cloned())
        }
        async fn find_by_key(&self, key: &str) -> Result<Option<Clip>, RepositoryError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.key == key)
                .cloned())
        }
        async fn find_other_voice(
            &self,
            text_hash: &str,
            voice: &str,
        ) -> Result<Option<Clip>, RepositoryError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|c| c.text_hash == text_hash && c.voice != voice)
                .cloned())
        }
        async fn insert(&self, clip: &Clip) -> Result<(), RepositoryError> {
            let mut all = self.0.lock().unwrap();
            if all.iter().any(|c| c.key == clip.key || c.id == clip.id) {
                return Err(RepositoryError::Duplicate);
            }
            all.push(clip.clone());
            Ok(())
        }
        async fn grant(
            &self,
            user: UserId,
            clips: &[ClipId],
            at: OffsetDateTime,
        ) -> Result<(), RepositoryError> {
            let mut grants = self.1.lock().unwrap();
            for clip in clips {
                // Same rule as the SQLite adapter: move forward past a day.
                let kept = grants.entry((user, *clip)).or_insert(at);
                if *kept < at - domain::GRANT_REFRESH {
                    *kept = at;
                }
            }
            Ok(())
        }
        async fn is_granted(&self, user: UserId, clip: ClipId) -> Result<bool, RepositoryError> {
            Ok(self.1.lock().unwrap().contains_key(&(user, clip)))
        }
        async fn list_with_last_use(&self) -> Result<Vec<(Clip, OffsetDateTime)>, RepositoryError> {
            let grants = self.1.lock().unwrap();
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|c| {
                    let last = grants
                        .iter()
                        .filter(|((_, id), _)| *id == c.id)
                        .map(|(_, at)| *at)
                        .max()
                        .map_or(c.created_at, |at| at.max(c.created_at));
                    (c.clone(), last)
                })
                .collect())
        }
        async fn delete_if_unused_since(
            &self,
            id: ClipId,
            cutoff: OffsetDateTime,
        ) -> Result<bool, RepositoryError> {
            let mut grants = self.1.lock().unwrap();
            let mut clips = self.0.lock().unwrap();
            let Some(pos) = clips.iter().position(|c| c.id == id) else {
                return Ok(false);
            };
            let last = grants
                .iter()
                .filter(|((_, c), _)| *c == id)
                .map(|(_, at)| *at)
                .max()
                .map_or(clips[pos].created_at, |at| at.max(clips[pos].created_at));
            if last >= cutoff {
                return Ok(false);
            }
            clips.remove(pos);
            grants.retain(|(_, c), _| *c != id);
            Ok(true)
        }
    }

    #[derive(Default)]
    pub(crate) struct MemStore(
        pub(crate) Mutex<HashMap<ClipId, Vec<u8>>>,
        /// When a file was written; files not listed here date from the test
        /// clock's start.
        pub(crate) Mutex<HashMap<ClipId, OffsetDateTime>>,
    );

    #[async_trait]
    impl ClipStore for MemStore {
        async fn put(&self, id: ClipId, audio: &[u8]) -> std::io::Result<()> {
            self.0.lock().unwrap().insert(id, audio.to_vec());
            Ok(())
        }
        async fn exists(&self, id: ClipId) -> std::io::Result<bool> {
            Ok(self.0.lock().unwrap().contains_key(&id))
        }
        async fn remove(&self, id: ClipId) -> std::io::Result<Option<u64>> {
            Ok(self.0.lock().unwrap().remove(&id).map(|b| b.len() as u64))
        }
        async fn list(&self) -> std::io::Result<Vec<domain::StoredFile>> {
            let modified = self.1.lock().unwrap();
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|(id, b)| domain::StoredFile {
                    id: *id,
                    size: b.len() as u64,
                    modified: modified
                        .get(id)
                        .copied()
                        .unwrap_or(datetime!(2026-11-13 10:00 UTC)),
                })
                .collect())
        }
        async fn read(&self, id: ClipId) -> std::io::Result<Option<Vec<u8>>> {
            Ok(self.0.lock().unwrap().get(&id).cloned())
        }
        async fn size(&self, id: ClipId) -> std::io::Result<Option<u64>> {
            Ok(self.0.lock().unwrap().get(&id).map(|b| b.len() as u64))
        }
        async fn read_range(
            &self,
            id: ClipId,
            offset: u64,
            len: usize,
        ) -> std::io::Result<Option<Vec<u8>>> {
            Ok(self.0.lock().unwrap().get(&id).map(|b| {
                let start = usize::try_from(offset).unwrap_or(usize::MAX).min(b.len());
                b[start..(start + len).min(b.len())].to_vec()
            }))
        }
    }

    struct Env {
        clock: Arc<TestClock>,
        tts: Arc<FakeTts>,
        clips: Arc<MemClips>,
        store: Arc<MemStore>,
        ids: Arc<SeqIds>,
        cache: ClipCache,
    }

    fn env_with(
        tts: Arc<FakeTts>,
        clips: Arc<MemClips>,
        store: Arc<MemStore>,
        ids: Arc<SeqIds>,
    ) -> Env {
        let clock = Arc::new(TestClock::new(datetime!(2026-11-13 10:00 UTC)));
        let cache = ClipCache::new(
            tts.clone(),
            clips.clone(),
            store.clone(),
            clock.clone(),
            ids.clone(),
        );
        Env {
            clock,
            tts,
            clips,
            store,
            ids,
            cache,
        }
    }

    fn env() -> Env {
        env_with(
            Arc::new(FakeTts::new("m1")),
            Arc::default(),
            Arc::default(),
            Arc::default(),
        )
    }

    #[tokio::test]
    async fn the_same_text_in_another_language_is_another_clip() {
        let e = env();
        let it = e
            .cache
            .ensure("Hello.", "a", ContentLanguage::It)
            .await
            .unwrap();
        let en = e
            .cache
            .ensure("Hello.", "a", ContentLanguage::En)
            .await
            .unwrap();
        assert_ne!(it.id, en.id);
        assert_eq!(
            *e.tts.languages.lock().unwrap(),
            [ContentLanguage::It, ContentLanguage::En]
        );
        assert_eq!(
            e.cache
                .find("Hello.", "a", ContentLanguage::En)
                .await
                .unwrap(),
            Some(en)
        );
    }

    #[tokio::test]
    async fn a_clip_is_made_once() {
        let e = env();
        let first = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        let again = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        assert_eq!(first, again);
        assert_eq!(e.tts.calls.load(Ordering::SeqCst), 1);
        assert_eq!(first.voice, "a");
        assert_eq!(first.size_bytes, 7);
        assert_eq!(e.store.read(first.id).await.unwrap().unwrap(), b"a:Ciao.");
        assert_eq!(
            e.cache
                .find("Ciao.", "a", ContentLanguage::It)
                .await
                .unwrap(),
            Some(first)
        );
    }

    #[tokio::test]
    async fn a_new_voice_or_model_means_a_new_clip_and_the_old_one_stays() {
        let e = env();
        let a = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        let b = e
            .cache
            .ensure("Ciao.", "b", ContentLanguage::It)
            .await
            .unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(e.tts.calls.load(Ordering::SeqCst), 2);
        // Back to the first voice: nothing to generate.
        assert_eq!(
            e.cache
                .ensure("Ciao.", "a", ContentLanguage::It)
                .await
                .unwrap(),
            a
        );
        assert_eq!(e.tts.calls.load(Ordering::SeqCst), 2);

        // Another model shares the stores but not the keys.
        let other = env_with(
            Arc::new(FakeTts::new("m2")),
            e.clips.clone(),
            e.store.clone(),
            e.ids.clone(),
        );
        assert_eq!(
            other
                .cache
                .find("Ciao.", "a", ContentLanguage::It)
                .await
                .unwrap(),
            None
        );
        let c = other
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        assert_ne!(c.id, a.id);
    }

    #[tokio::test]
    async fn another_voice_of_the_same_text_is_the_fallback() {
        let e = env();
        assert_eq!(
            e.cache
                .find_other_voice("Ciao.", "b", ContentLanguage::It)
                .await
                .unwrap(),
            None
        );
        let a = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        assert_eq!(
            e.cache
                .find("Ciao.", "b", ContentLanguage::It)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            e.cache
                .find_other_voice("Ciao.", "b", ContentLanguage::It)
                .await
                .unwrap(),
            Some(a)
        );
        assert_eq!(
            e.cache
                .find_other_voice("Altro.", "b", ContentLanguage::It)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn another_voice_never_comes_from_another_language() {
        let e = env();
        let it = e
            .cache
            .ensure("No.", "a", ContentLanguage::It)
            .await
            .unwrap();
        let en = e
            .cache
            .ensure("No.", "a", ContentLanguage::En)
            .await
            .unwrap();
        assert_ne!(it.id, en.id);
        assert_eq!(
            e.cache
                .find_other_voice("No.", "b", ContentLanguage::It)
                .await
                .unwrap(),
            Some(it)
        );
        assert_eq!(
            e.cache
                .find_other_voice("No.", "b", ContentLanguage::En)
                .await
                .unwrap(),
            Some(en)
        );
    }

    #[tokio::test]
    async fn a_row_whose_file_is_gone_is_repaired_in_place() {
        let e = env();
        let clip = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        e.store.0.lock().unwrap().clear();
        assert_eq!(
            e.cache
                .find("Ciao.", "a", ContentLanguage::It)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            e.cache
                .find_other_voice("Ciao.", "b", ContentLanguage::It)
                .await
                .unwrap(),
            None
        );

        let repaired = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        assert_eq!(repaired.id, clip.id);
        assert_eq!(e.clips.0.lock().unwrap().len(), 1);
        assert!(e.store.exists(clip.id).await.unwrap());
    }

    #[tokio::test]
    async fn a_failed_synthesis_leaves_nothing_behind() {
        let e = env();
        *e.tts.fail.lock().unwrap() = FailMode::Unavailable;
        let err = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            AudioError::Speech(SpeechError::Unavailable(_))
        ));
        assert!(e.clips.0.lock().unwrap().is_empty());
        assert!(e.store.0.lock().unwrap().is_empty());
    }

    fn days(n: i64) -> Duration {
        Duration::days(n)
    }

    fn advance(e: &Env, by: Duration) {
        let mut now = e.clock.0.lock().unwrap();
        *now += by;
    }

    #[tokio::test]
    async fn only_clips_idle_for_ninety_days_are_deleted_and_samples_stay() {
        let e = env();
        let idle = e
            .cache
            .ensure("Vecchia.", "a", ContentLanguage::It)
            .await
            .unwrap();
        let used = e
            .cache
            .ensure("Usata.", "a", ContentLanguage::It)
            .await
            .unwrap();
        let sample = e
            .cache
            .ensure(SAMPLE_TEXT, "a", ContentLanguage::It)
            .await
            .unwrap();
        let unlisted_voice = e
            .cache
            .ensure(SAMPLE_TEXT, "zz", ContentLanguage::It)
            .await
            .unwrap();
        advance(&e, days(60));
        e.cache.grant(UserId::OWNER, &[used.id]).await.unwrap();
        advance(&e, days(31));
        // Day 91: `idle` was never shown since it was made, `used` 31 days ago.
        let report = e.cache.clean_up().await.unwrap();
        assert_eq!(report.clips_deleted, 2);
        assert_eq!(
            report.bytes_freed,
            ("a:Vecchia.".len() + format!("zz:{SAMPLE_TEXT}").len()) as u64
        );
        assert_eq!(e.cache.get(idle.id).await.unwrap(), None);
        assert!(!e.store.exists(idle.id).await.unwrap());
        assert!(e.cache.get(used.id).await.unwrap().is_some());
        assert!(e.store.exists(used.id).await.unwrap());
        // The sample of a listed voice is kept however old; a sentence in a
        // voice that is not listed is an ordinary clip.
        assert!(e.cache.get(sample.id).await.unwrap().is_some());
        assert_eq!(e.cache.get(unlisted_voice.id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_clip_just_inside_ninety_days_stays() {
        let e = env();
        let clip = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        advance(&e, days(90));
        assert_eq!(e.cache.clean_up().await.unwrap().clips_deleted, 0);
        advance(&e, Duration::seconds(1));
        assert_eq!(e.cache.clean_up().await.unwrap().clips_deleted, 1);
        assert_eq!(e.cache.get(clip.id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_deleted_clip_is_made_again_when_it_is_needed() {
        let e = env();
        let old = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        advance(&e, days(91));
        e.cache.clean_up().await.unwrap();
        assert_eq!(
            e.cache
                .find("Ciao.", "a", ContentLanguage::It)
                .await
                .unwrap(),
            None
        );
        assert_eq!(e.tts.calls.load(Ordering::SeqCst), 1);

        let again = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        assert_eq!(e.tts.calls.load(Ordering::SeqCst), 2);
        assert_ne!(again.id, old.id);
        assert_eq!(e.store.read(again.id).await.unwrap().unwrap(), b"a:Ciao.");
        // Fresh: the next pass leaves it.
        assert_eq!(e.cache.clean_up().await.unwrap().clips_deleted, 0);
    }

    #[tokio::test]
    async fn files_without_a_row_go_only_after_a_day() {
        let e = env();
        let kept = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        let young = ClipId::new(uuid::Uuid::from_u128(901));
        let old = ClipId::new(uuid::Uuid::from_u128(902));
        let now = e.clock.now();
        e.store.put(young, b"12345").await.unwrap();
        e.store.put(old, b"123").await.unwrap();
        {
            let mut m = e.store.1.lock().unwrap();
            m.insert(young, now - Duration::hours(23));
            m.insert(old, now - Duration::hours(25));
        }
        let report = e.cache.clean_up().await.unwrap();
        assert_eq!(report.orphans_deleted, 1);
        assert_eq!(report.orphan_bytes_freed, 3);
        assert_eq!(report.clips_deleted, 0);
        assert!(e.store.exists(young).await.unwrap());
        assert!(!e.store.exists(old).await.unwrap());
        // A file with a row is never an orphan, however old.
        e.store.1.lock().unwrap().insert(kept.id, now - days(400));
        assert_eq!(e.cache.clean_up().await.unwrap().orphans_deleted, 0);
        assert!(e.store.exists(kept.id).await.unwrap());
    }

    #[tokio::test]
    async fn a_row_without_a_file_is_not_deleted_before_it_is_idle() {
        let e = env();
        let clip = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        e.store.0.lock().unwrap().clear();
        let report = e.cache.clean_up().await.unwrap();
        assert_eq!(report, CleanupReport::default());
        assert_eq!(e.cache.get(clip.id).await.unwrap(), Some(clip));
        // Idle and missing its file: the row goes, with nothing to free.
        advance(&e, days(91));
        let report = e.cache.clean_up().await.unwrap();
        assert_eq!((report.clips_deleted, report.bytes_freed), (1, 0));
    }

    #[tokio::test]
    async fn a_clip_shown_again_is_not_idle() {
        let e = env();
        let clip = e
            .cache
            .ensure("Ciao.", "a", ContentLanguage::It)
            .await
            .unwrap();
        for _ in 0..4 {
            advance(&e, days(50));
            e.cache.grant(UserId::OWNER, &[clip.id]).await.unwrap();
            assert_eq!(e.cache.clean_up().await.unwrap().clips_deleted, 0);
        }
        assert!(e.cache.get(clip.id).await.unwrap().is_some());
    }
}
