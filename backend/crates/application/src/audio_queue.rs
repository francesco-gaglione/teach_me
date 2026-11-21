//! Preparing the audio of lessons in the background (ADR 0013 §5, §11,
//! `docs/contracts/audio.md` §1).
//!
//! [`AudioPreparer`] puts the blocks that have no clip on the persistent queue;
//! [`AudioWorker`] is the single worker that makes them, one at a time. The
//! queue is internal: it is not a `Job` and never uses the LLM key.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use domain::{
    AudioQueue, AudioRequest, BlockAudioState, ContentLanguage, LessonId, RepositoryError,
    SpeechError, UserId, UserRepository, readable_blocks,
};
use serde_json::Value;
use tokio::sync::Notify;

use crate::audio::{AudioError, ClipCache, SAMPLE_LANGUAGE, SAMPLE_TEXT};

#[derive(Debug, Clone, Copy)]
pub struct AudioSettings {
    /// Failed tries after which a block is `failed` (ADR 0013 §11).
    pub max_attempts: u32,
    /// How long the worker waits when the engine is down before it tries again.
    pub unavailable_backoff: Duration,
    /// How long it sleeps with an empty queue if nobody wakes it.
    pub idle_poll: Duration,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            unavailable_backoff: Duration::from_secs(60),
            idle_poll: Duration::from_mins(5),
        }
    }
}

/// A voice sample still to make. Samples are not in the persistent queue: they
/// are cheap to work out again at every start, and the worker only takes one
/// when no lesson is waiting.
#[derive(Debug)]
struct SampleTask {
    voice: String,
    attempts: u32,
}

type Samples = Arc<Mutex<VecDeque<SampleTask>>>;

/// The side that asks for audio: lesson generation and the opening of a lesson.
pub struct AudioPreparer {
    cache: Arc<ClipCache>,
    queue: Arc<dyn AudioQueue>,
    users: Arc<dyn UserRepository>,
    wake: Arc<Notify>,
    samples: Samples,
}

impl AudioPreparer {
    /// The preparer and the worker that serves it share the queue and the
    /// wake-up signal.
    #[must_use]
    pub fn new(
        cache: Arc<ClipCache>,
        queue: Arc<dyn AudioQueue>,
        users: Arc<dyn UserRepository>,
        settings: AudioSettings,
    ) -> (Arc<Self>, Arc<AudioWorker>) {
        let wake = Arc::new(Notify::new());
        let samples = Samples::default();
        let worker = Arc::new(AudioWorker {
            cache: cache.clone(),
            queue: queue.clone(),
            wake: wake.clone(),
            samples: samples.clone(),
            settings,
        });
        (
            Arc::new(Self {
                cache,
                queue,
                users,
                wake,
                samples,
            }),
            worker,
        )
    }

    #[must_use]
    pub fn cache(&self) -> &Arc<ClipCache> {
        &self.cache
    }

    /// The voice a saved choice stands for: itself while the server still
    /// lists it, else the default (`audio.md` §4.2).
    #[must_use]
    pub fn effective_voice(&self, saved: Option<&str>) -> &str {
        saved
            .and_then(|s| self.cache.voices().iter().find(|v| v.id == s))
            .map_or_else(|| self.cache.default_voice(), |v| v.id.as_str())
    }

    /// The voice `user` listens with.
    pub async fn voice_of(&self, user: UserId) -> Result<String, AudioError> {
        let user = self
            .users
            .get(user)
            .await?
            .ok_or(AudioError::Repository(RepositoryError::NotFound))?;
        Ok(self.effective_voice(user.tts_voice.as_deref()).to_owned())
    }

    /// Queues the sample of every listed voice that has none, to be made
    /// after all the lesson work. Run at startup; asking twice is harmless.
    pub async fn prepare_samples(&self) -> Result<(), AudioError> {
        for voice in self.cache.voices() {
            if self
                .cache
                .find(SAMPLE_TEXT, &voice.id, SAMPLE_LANGUAGE)
                .await?
                .is_some()
            {
                continue;
            }
            let mut tasks = self
                .samples
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !tasks.iter().any(|t| t.voice == voice.id) {
                tasks.push_back(SampleTask {
                    voice: voice.id.clone(),
                    attempts: 0,
                });
            }
        }
        self.wake.notify_one();
        Ok(())
    }

    /// Queues the readable blocks of `document` that have no clip in the
    /// user's voice. With `front` (a lesson opened by the user) its queued
    /// blocks also move ahead of every other lesson's, and blocks that had
    /// `failed` are queued again.
    pub async fn prepare(
        &self,
        user: UserId,
        lesson: LessonId,
        document: &Value,
        language: ContentLanguage,
        front: bool,
    ) -> Result<(), AudioError> {
        let voice = self.voice_of(user).await?;
        let mut missing = Vec::new();
        let mut found = Vec::new();
        // Opening a lesson is a use of its clips: a clip found here is not
        // queued again, so it must not be deleted before the manifest that
        // follows names it (`ClipCache::clean_up`).
        let held = self.cache.hold().await;
        for block in readable_blocks(document) {
            if let Some(clip) = self.cache.find(&block.text, &voice, language).await? {
                found.push(clip.id);
            } else {
                missing.push(AudioRequest {
                    lesson_id: lesson,
                    voice: voice.clone(),
                    language,
                    block_id: block.block_id,
                    text: block.text,
                });
            }
        }
        self.cache.grant(user, &found).await?;
        drop(held);
        if missing.is_empty() && !front {
            return Ok(());
        }
        self.queue.enqueue(lesson, &voice, &missing, front).await?;
        if !missing.is_empty() {
            self.wake.notify_one();
        }
        Ok(())
    }

    /// Which blocks of the lesson are waiting, being made or out of attempts
    /// for `voice`. A block that is not in the map has its clip or was never
    /// requested: the manifest tells them apart by looking for the clip.
    pub async fn states(
        &self,
        lesson: LessonId,
        voice: &str,
    ) -> Result<HashMap<String, BlockAudioState>, AudioError> {
        Ok(self.queue.states(lesson, voice).await?)
    }
}

/// What one turn of the worker did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing queued.
    Idle,
    /// The clip was made.
    Made,
    /// The clip was already there: nothing to make.
    Cached,
    /// The attempt failed; the block is in this state now.
    Failed(BlockAudioState),
    /// The engine (or the metadata store) could not be reached: the block is
    /// back in the queue with no attempt used and the worker should back off.
    Unavailable,
}

/// Makes the queued clips, one at a time, in queue order.
pub struct AudioWorker {
    cache: Arc<ClipCache>,
    queue: Arc<dyn AudioQueue>,
    wake: Arc<Notify>,
    samples: Samples,
    settings: AudioSettings,
}

impl AudioWorker {
    /// Entries that were running when the process stopped are queued again.
    pub async fn recover(&self) -> Result<u64, AudioError> {
        Ok(self.queue.recover().await?)
    }

    /// Takes the next entry and makes its clip.
    pub async fn step(&self) -> Result<Step, AudioError> {
        let Some(next) = self.queue.claim_next().await? else {
            return self.sample_step().await;
        };
        let AudioRequest {
            lesson_id,
            voice,
            language,
            block_id,
            text,
        } = &next.request;
        // A clip made meanwhile (another lesson shares the text, or the entry
        // was queued twice around a restart) needs no engine.
        match self.cache.find(text, voice, *language).await {
            Ok(Some(_)) => {
                self.queue.complete(next.entry).await?;
                return Ok(Step::Cached);
            }
            Ok(None) => {}
            Err(e) => return self.give_back(next.entry, &e).await,
        }
        match self.cache.ensure(text, voice, *language).await {
            Ok(_) => {
                self.queue.complete(next.entry).await?;
                Ok(Step::Made)
            }
            Err(
                e @ (AudioError::Speech(SpeechError::Unavailable(_)) | AudioError::Repository(_)),
            ) => self.give_back(next.entry, &e).await,
            Err(e) => {
                let state = self
                    .queue
                    .record_failure(next.entry, self.settings.max_attempts)
                    .await?;
                tracing::warn!(
                    lesson = %lesson_id, block = %block_id, voice = %voice, error = %e,
                    state = ?state, "audio of a block failed"
                );
                Ok(Step::Failed(state))
            }
        }
    }

    /// With no lesson waiting, makes one voice sample if any is missing.
    async fn sample_step(&self) -> Result<Step, AudioError> {
        let task = self
            .samples
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front();
        let Some(mut task) = task else {
            return Ok(Step::Idle);
        };
        match self
            .cache
            .find(SAMPLE_TEXT, &task.voice, SAMPLE_LANGUAGE)
            .await
        {
            Ok(Some(_)) => return Ok(Step::Cached),
            Ok(None) => {}
            Err(e) => return Ok(self.sample_back(task, &e, false)),
        }
        match self
            .cache
            .ensure(SAMPLE_TEXT, &task.voice, SAMPLE_LANGUAGE)
            .await
        {
            Ok(_) => Ok(Step::Made),
            Err(
                e @ (AudioError::Speech(SpeechError::Unavailable(_)) | AudioError::Repository(_)),
            ) => Ok(self.sample_back(task, &e, false)),
            Err(e) => {
                task.attempts += 1;
                Ok(self.sample_back(task, &e, true))
            }
        }
    }

    /// Puts a sample back after a failed try. A try that used an attempt is
    /// retried at the end of the line until `max_attempts`; one that did not
    /// (engine down) goes first again and the worker backs off.
    fn sample_back(&self, task: SampleTask, cause: &AudioError, used_attempt: bool) -> Step {
        tracing::warn!(voice = %task.voice, error = %cause, "voice sample not made");
        if !used_attempt {
            self.samples
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push_front(task);
            return Step::Unavailable;
        }
        if task.attempts >= self.settings.max_attempts {
            // Out of attempts: the next start tries again.
            return Step::Failed(BlockAudioState::Failed);
        }
        self.samples
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(task);
        Step::Failed(BlockAudioState::Queued)
    }

    async fn give_back(&self, entry: i64, cause: &AudioError) -> Result<Step, AudioError> {
        tracing::warn!(error = %cause, "audio worker backs off");
        self.queue.release(entry).await?;
        Ok(Step::Unavailable)
    }

    /// The worker loop; spawn it once. Returns only if the task is aborted.
    pub async fn run(self: Arc<Self>) {
        loop {
            match self.recover().await {
                Ok(0) => break,
                Ok(n) => {
                    tracing::info!(entries = n, "audio queue resumed after a restart");
                    break;
                }
                Err(e) => {
                    tracing::error!(error = %e, "audio queue recovery failed");
                    tokio::time::sleep(self.settings.unavailable_backoff).await;
                }
            }
        }
        loop {
            match self.step().await {
                Ok(Step::Idle) => {
                    // A request wakes it; the timeout covers a lost wake-up.
                    let _ =
                        tokio::time::timeout(self.settings.idle_poll, self.wake.notified()).await;
                }
                Ok(Step::Unavailable) => {
                    tokio::time::sleep(self.settings.unavailable_backoff).await;
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(error = %e, "audio worker error");
                    tokio::time::sleep(self.settings.unavailable_backoff).await;
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::Ordering;

    use async_trait::async_trait;
    use domain::{QueuedAudio, RepositoryError};
    use serde_json::json;
    use time::macros::datetime;
    use uuid::Uuid;

    use super::*;
    use crate::audio::tests::{FailMode, FakeTts, MemClips, MemStore};
    use crate::fakes::{MemUsers, SeqIds, TestClock};

    struct Row {
        id: i64,
        request: AudioRequest,
        state: BlockAudioState,
        attempts: u32,
        bump: i64,
    }

    /// Same rules as `SqliteAudioQueue` (which has its own tests).
    #[derive(Default)]
    pub(crate) struct MemQueue(Mutex<(i64, Vec<Row>)>);

    #[async_trait]
    impl AudioQueue for MemQueue {
        async fn enqueue(
            &self,
            lesson: LessonId,
            voice: &str,
            requests: &[AudioRequest],
            front: bool,
        ) -> Result<(), RepositoryError> {
            let mut guard = self.0.lock().unwrap();
            let (next_id, rows) = &mut *guard;
            rows.retain(|x| {
                !(x.request.lesson_id == lesson
                    && x.request.voice != voice
                    && x.state == BlockAudioState::Queued)
            });
            for r in requests {
                let same = |x: &Row| {
                    x.request.lesson_id == lesson
                        && x.request.voice == voice
                        && x.request.block_id == r.block_id
                };
                if front {
                    rows.retain(|x| !(same(x) && x.state == BlockAudioState::Failed));
                }
                if !rows.iter().any(same) {
                    *next_id += 1;
                    rows.push(Row {
                        id: *next_id,
                        request: r.clone(),
                        state: BlockAudioState::Queued,
                        attempts: 0,
                        bump: 0,
                    });
                }
            }
            if front {
                let top = rows.iter().map(|x| x.bump).max().unwrap_or(0) + 1;
                for x in rows.iter_mut().filter(|x| {
                    x.request.lesson_id == lesson
                        && x.request.voice == voice
                        && x.state == BlockAudioState::Queued
                }) {
                    x.bump = top;
                }
            }
            Ok(())
        }

        async fn claim_next(&self) -> Result<Option<QueuedAudio>, RepositoryError> {
            let mut guard = self.0.lock().unwrap();
            let next = guard
                .1
                .iter_mut()
                .filter(|x| x.state == BlockAudioState::Queued)
                .min_by_key(|x| (-x.bump, x.id));
            Ok(next.map(|x| {
                x.state = BlockAudioState::Running;
                QueuedAudio {
                    entry: x.id,
                    request: x.request.clone(),
                }
            }))
        }

        async fn complete(&self, entry: i64) -> Result<(), RepositoryError> {
            self.0.lock().unwrap().1.retain(|x| x.id != entry);
            Ok(())
        }

        async fn release(&self, entry: i64) -> Result<(), RepositoryError> {
            for x in &mut self.0.lock().unwrap().1 {
                if x.id == entry && x.state == BlockAudioState::Running {
                    x.state = BlockAudioState::Queued;
                }
            }
            Ok(())
        }

        async fn record_failure(
            &self,
            entry: i64,
            max_attempts: u32,
        ) -> Result<BlockAudioState, RepositoryError> {
            let mut guard = self.0.lock().unwrap();
            let Some(x) = guard.1.iter_mut().find(|x| x.id == entry) else {
                return Ok(BlockAudioState::Failed);
            };
            x.attempts += 1;
            x.state = if x.attempts >= max_attempts {
                BlockAudioState::Failed
            } else {
                BlockAudioState::Queued
            };
            Ok(x.state)
        }

        async fn recover(&self) -> Result<u64, RepositoryError> {
            let mut n = 0;
            for x in &mut self.0.lock().unwrap().1 {
                if x.state == BlockAudioState::Running {
                    x.state = BlockAudioState::Queued;
                    n += 1;
                }
            }
            Ok(n)
        }

        async fn states(
            &self,
            lesson: LessonId,
            voice: &str,
        ) -> Result<HashMap<String, BlockAudioState>, RepositoryError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .1
                .iter()
                .filter(|x| x.request.lesson_id == lesson && x.request.voice == voice)
                .map(|x| (x.request.block_id.clone(), x.state))
                .collect())
        }
    }

    pub(crate) struct Env {
        pub tts: Arc<FakeTts>,
        pub store: Arc<MemStore>,
        pub queue: Arc<MemQueue>,
        pub cache: Arc<ClipCache>,
        pub preparer: Arc<AudioPreparer>,
        pub worker: Arc<AudioWorker>,
        pub users: Arc<MemUsers>,
    }

    pub(crate) fn env() -> Env {
        let tts = Arc::new(FakeTts::new("m"));
        let clips = Arc::new(MemClips::default());
        let store = Arc::new(MemStore::default());
        let queue = Arc::new(MemQueue::default());
        let users = MemUsers::with_owner();
        let cache = Arc::new(ClipCache::new(
            tts.clone(),
            clips,
            store.clone(),
            Arc::new(TestClock::new(datetime!(2026-11-13 10:00 UTC))),
            Arc::new(SeqIds::default()),
        ));
        let (preparer, worker) = AudioPreparer::new(
            cache.clone(),
            queue.clone(),
            users.clone(),
            AudioSettings::default(),
        );
        Env {
            tts,
            store,
            queue,
            cache,
            preparer,
            worker,
            users,
        }
    }

    pub(crate) fn lesson(n: u128) -> LessonId {
        LessonId::new(Uuid::from_u128(n))
    }

    /// A document whose paragraphs `<prefix>1..=n` say "<prefix>N".
    pub(crate) fn doc(prefix: &str, n: usize) -> Value {
        let blocks: Vec<Value> = (1..=n)
            .map(|i| json!({"type": "paragraph", "id": format!("{prefix}{i}"), "text": format!("{prefix}{i}")}))
            .collect();
        json!({"blocks": blocks})
    }

    fn spoken(e: &Env) -> Vec<String> {
        e.tts
            .spoken
            .lock()
            .unwrap()
            .iter()
            .map(|(_, t)| t.clone())
            .collect()
    }

    async fn drain(e: &Env) {
        while e.worker.step().await.unwrap() != Step::Idle {}
    }

    const USER: UserId = UserId::OWNER;

    #[tokio::test]
    async fn blocks_are_made_in_the_order_they_were_requested() {
        let e = env();
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 2), ContentLanguage::It, false)
            .await
            .unwrap();
        e.preparer
            .prepare(USER, lesson(2), &doc("b", 2), ContentLanguage::It, false)
            .await
            .unwrap();
        drain(&e).await;
        assert_eq!(spoken(&e), ["a1", "a2", "b1", "b2"]);
    }

    #[tokio::test]
    async fn the_lesson_opened_last_moves_to_the_front() {
        let e = env();
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 2), ContentLanguage::It, false)
            .await
            .unwrap();
        e.preparer
            .prepare(USER, lesson(2), &doc("b", 2), ContentLanguage::It, false)
            .await
            .unwrap();
        e.preparer
            .prepare(USER, lesson(3), &doc("c", 1), ContentLanguage::It, false)
            .await
            .unwrap();
        // The worker is mid-lesson when the user opens lesson 3, then lesson 2.
        assert_eq!(e.worker.step().await.unwrap(), Step::Made);
        e.preparer
            .prepare(USER, lesson(3), &doc("c", 1), ContentLanguage::It, true)
            .await
            .unwrap();
        e.preparer
            .prepare(USER, lesson(2), &doc("b", 2), ContentLanguage::It, true)
            .await
            .unwrap();
        drain(&e).await;
        assert_eq!(spoken(&e), ["a1", "b1", "b2", "c1", "a2"]);
    }

    #[tokio::test]
    async fn blocks_with_a_clip_are_not_queued_and_a_cached_entry_costs_nothing() {
        let e = env();
        let voice = e.preparer.voice_of(USER).await.unwrap();
        e.cache
            .ensure("a1", &voice, ContentLanguage::It)
            .await
            .unwrap();
        assert_eq!(e.tts.calls.load(Ordering::SeqCst), 1);

        e.preparer
            .prepare(USER, lesson(1), &doc("a", 2), ContentLanguage::It, false)
            .await
            .unwrap();
        assert_eq!(
            e.preparer
                .states(lesson(1), &voice)
                .await
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            ["a2"]
        );
        // An entry whose clip appeared after it was queued is done at once.
        e.queue
            .enqueue(
                lesson(1),
                &voice,
                &[AudioRequest {
                    lesson_id: lesson(1),
                    voice: voice.clone(),
                    language: ContentLanguage::It,
                    block_id: "a1".into(),
                    text: "a1".into(),
                }],
                false,
            )
            .await
            .unwrap();
        let mut steps = Vec::new();
        loop {
            let s = e.worker.step().await.unwrap();
            if s == Step::Idle {
                break;
            }
            steps.push(s);
        }
        assert_eq!(steps, [Step::Made, Step::Cached]);
        assert_eq!(spoken(&e), ["a1", "a2"]);
    }

    #[tokio::test]
    async fn preparing_a_finished_lesson_queues_nothing() {
        let e = env();
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 2), ContentLanguage::It, false)
            .await
            .unwrap();
        drain(&e).await;
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 2), ContentLanguage::It, true)
            .await
            .unwrap();
        assert_eq!(e.worker.step().await.unwrap(), Step::Idle);
        let voice = e.preparer.voice_of(USER).await.unwrap();
        assert!(
            e.preparer
                .states(lesson(1), &voice)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn opening_a_lesson_counts_as_a_use_of_the_clips_it_finds() {
        let e = env();
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 2), ContentLanguage::It, false)
            .await
            .unwrap();
        drain(&e).await;
        let voice = e.preparer.voice_of(USER).await.unwrap();
        let clip = e
            .cache
            .find("a1", &voice, ContentLanguage::It)
            .await
            .unwrap();
        assert!(clip.is_some(), "the fixture's first block text changed");
        let clip = clip.unwrap();
        assert!(!e.cache.is_granted(USER, clip.id).await.unwrap());
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 2), ContentLanguage::It, false)
            .await
            .unwrap();
        assert!(e.cache.is_granted(USER, clip.id).await.unwrap());
    }

    #[tokio::test]
    async fn a_restart_resumes_from_the_first_missing_block() {
        let e = env();
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 3), ContentLanguage::It, false)
            .await
            .unwrap();
        assert_eq!(e.worker.step().await.unwrap(), Step::Made);
        // Killed while making the second one: its entry stays `running`.
        let taken = e.queue.claim_next().await.unwrap().unwrap();
        assert_eq!(taken.request.block_id, "a2");

        // The new process: same queue and clips, a new worker.
        let (_, worker) = AudioPreparer::new(
            e.cache.clone(),
            e.queue.clone(),
            MemUsers::with_owner(),
            AudioSettings::default(),
        );
        assert_eq!(worker.recover().await.unwrap(), 1);
        while worker.step().await.unwrap() != Step::Idle {}
        assert_eq!(spoken(&e), ["a1", "a2", "a3"]);
    }

    #[tokio::test]
    async fn three_failures_leave_the_block_failed_until_the_lesson_is_reopened() {
        let e = env();
        let voice = e.preparer.voice_of(USER).await.unwrap();
        *e.tts.fail.lock().unwrap() = FailMode::Rejects;
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 1), ContentLanguage::It, false)
            .await
            .unwrap();
        assert_eq!(
            e.worker.step().await.unwrap(),
            Step::Failed(BlockAudioState::Queued)
        );
        assert_eq!(
            e.worker.step().await.unwrap(),
            Step::Failed(BlockAudioState::Queued)
        );
        assert_eq!(
            e.worker.step().await.unwrap(),
            Step::Failed(BlockAudioState::Failed)
        );
        assert_eq!(e.worker.step().await.unwrap(), Step::Idle);
        assert_eq!(
            e.preparer.states(lesson(1), &voice).await.unwrap()["a1"],
            BlockAudioState::Failed
        );

        // Not queued again by a trigger that is not an opening.
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 1), ContentLanguage::It, false)
            .await
            .unwrap();
        assert_eq!(e.worker.step().await.unwrap(), Step::Idle);

        // Reopened with a healthy engine.
        *e.tts.fail.lock().unwrap() = FailMode::Never;
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 1), ContentLanguage::It, true)
            .await
            .unwrap();
        assert_eq!(
            e.preparer.states(lesson(1), &voice).await.unwrap()["a1"],
            BlockAudioState::Queued
        );
        assert_eq!(e.worker.step().await.unwrap(), Step::Made);
    }

    #[tokio::test]
    async fn an_engine_that_is_down_burns_no_attempt() {
        let e = env();
        let voice = e.preparer.voice_of(USER).await.unwrap();
        *e.tts.fail.lock().unwrap() = FailMode::Unavailable;
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 1), ContentLanguage::It, false)
            .await
            .unwrap();
        for _ in 0..10 {
            assert_eq!(e.worker.step().await.unwrap(), Step::Unavailable);
        }
        assert_eq!(
            e.preparer.states(lesson(1), &voice).await.unwrap()["a1"],
            BlockAudioState::Queued
        );
        // Back up: the block is made, it was never counted as a failure.
        *e.tts.fail.lock().unwrap() = FailMode::Never;
        assert_eq!(e.worker.step().await.unwrap(), Step::Made);
        assert!(
            e.preparer
                .states(lesson(1), &voice)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(e.store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn the_chosen_voice_counts_and_an_unlisted_one_means_the_default() {
        let e = env();
        assert_eq!(e.preparer.voice_of(USER).await.unwrap(), "a");
        e.users.set_tts_voice(USER, "b").await.unwrap();
        assert_eq!(e.preparer.voice_of(USER).await.unwrap(), "b");
        // The server dropped the voice from its list.
        e.users.set_tts_voice(USER, "gone").await.unwrap();
        assert_eq!(e.preparer.voice_of(USER).await.unwrap(), "a");
    }

    #[tokio::test]
    async fn a_new_voice_drops_the_queued_entries_of_the_old_one_but_not_the_running() {
        let e = env();
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 3), ContentLanguage::It, true)
            .await
            .unwrap();
        // One block is being made in voice "a" when the user changes voice.
        let running = e.queue.claim_next().await.unwrap().unwrap();
        assert_eq!(running.request.block_id, "a1");
        e.users.set_tts_voice(USER, "b").await.unwrap();
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 3), ContentLanguage::It, true)
            .await
            .unwrap();

        let old = e.preparer.states(lesson(1), "a").await.unwrap();
        assert_eq!(old.len(), 1, "{old:?}");
        assert_eq!(old["a1"], BlockAudioState::Running);
        assert_eq!(e.preparer.states(lesson(1), "b").await.unwrap().len(), 3);
        // Another lesson's entries in "a" are not touched.
        e.users.set_tts_voice(USER, "a").await.unwrap();
        e.preparer
            .prepare(USER, lesson(2), &doc("z", 1), ContentLanguage::It, true)
            .await
            .unwrap();
        e.users.set_tts_voice(USER, "b").await.unwrap();
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 3), ContentLanguage::It, true)
            .await
            .unwrap();
        assert_eq!(e.preparer.states(lesson(2), "a").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn samples_wait_for_the_lessons_and_each_voice_is_made_once() {
        let e = env();
        e.preparer.prepare_samples().await.unwrap();
        e.preparer.prepare_samples().await.unwrap();
        e.preparer
            .prepare(USER, lesson(1), &doc("a", 2), ContentLanguage::It, false)
            .await
            .unwrap();
        drain(&e).await;
        let spoken = e.tts.spoken.lock().unwrap().clone();
        let sample = |v: &str| (v.to_owned(), SAMPLE_TEXT.to_owned());
        assert_eq!(spoken.len(), 4, "{spoken:?}");
        assert_eq!(&spoken[2..], [sample("a"), sample("b")]);
        // Already made: nothing is asked again.
        e.preparer.prepare_samples().await.unwrap();
        drain(&e).await;
        assert_eq!(e.tts.spoken.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn a_sample_the_engine_rejects_is_dropped_after_its_attempts() {
        let e = env();
        *e.tts.fail.lock().unwrap() = FailMode::Rejects;
        e.preparer.prepare_samples().await.unwrap();
        let mut steps = 0;
        while e.worker.step().await.unwrap() != Step::Idle {
            steps += 1;
            assert!(steps <= 6, "the idle loop would spin");
        }
        // Two voices, three attempts each.
        assert_eq!(steps, 6);
    }
}
