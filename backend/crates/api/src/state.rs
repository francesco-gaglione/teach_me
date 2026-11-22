use std::sync::Arc;
use std::time::Duration;

use crate::stream_limit::StreamLimiter;
use application::{
    AccountService, AttemptService, AudioPreparer, AudioService, AudioSettings, AudioWorker,
    AuthService, ChatService, ChatSettings, ClipCache, ComponentRequestService, DocumentService,
    FolderService, GenerateLesson, Grader, HighlightService, ImportDocument, JobHandler, JobRunner,
    JobService, JobSettings, KeyScopedLlm, LessonPrompts, LessonService, LessonSettings, LlmBudget,
    LlmKeys, MissionService, PreferenceService, ResearchSettings, ResearcherPrompts,
    ResourceService, ReviewService, RunLessonCompletion, RunResearch, StrokeService, TopicService,
    TutorPrompts,
};
use domain::llm::LlmProvider;
use domain::{
    ClipStore, Clock, DocumentFiles, IdGenerator, JobKind, PasswordHasher, PdfError,
    PdfTextExtractor, SecretCipher, SpeechSynthesizer, TokenIssuer,
};
use infrastructure::sqlite::Pool;
use infrastructure::{
    Argon2idHasher, EventHub, EventHubConfig, FsrsScheduler, LocalDocumentFiles, OsTokenIssuer,
    ServerKey, SqliteAttemptRepository, SqliteAudioQueue, SqliteChatRepository,
    SqliteClipRepository, SqliteDocumentRepository, SqliteFolderRepository,
    SqliteHighlightRepository, SqliteJobRepository, SqliteLearningRepository,
    SqliteLessonRepository, SqliteLlmUsageRepository, SqliteMissionRepository,
    SqlitePreferenceRepository, SqliteResourceRepository, SqliteReviewRepository,
    SqliteSessionRepository, SqliteStrokeRepository, SqliteTopicRepository, SqliteUserRepository,
    XChaChaSecretCipher,
};

/// Knobs that have contract-mandated defaults but that tests need to turn.
#[derive(Debug, Clone, Copy)]
pub struct AppOptions {
    pub chat: ChatSettings,
    pub jobs: JobSettings,
    pub research: ResearchSettings,
    pub lessons: LessonSettings,
    pub event_hub: EventHubConfig,
    /// Interval of the SSE `: ping` comment (contract: 15 s).
    pub sse_ping: Duration,
    /// Bytes of documents per user (`TEACH_ME_USER_STORAGE_QUOTA_BYTES`); 0 = unlimited.
    pub storage_quota_bytes: u64,
    /// Input + output tokens per user per UTC day (`TEACH_ME_USER_DAILY_LLM_TOKENS`); 0 = off.
    pub daily_llm_tokens: u64,
    /// Concurrent SSE streams per user (`TEACH_ME_MAX_SSE_PER_USER`).
    pub max_sse_per_user: usize,
}

/// Contract defaults of the resource limits (`api-conventions.md` section 12).
pub const DEFAULT_STORAGE_QUOTA_BYTES: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_MAX_SSE_PER_USER: usize = 8;

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            chat: ChatSettings::default(),
            jobs: JobSettings::default(),
            research: ResearchSettings::default(),
            lessons: LessonSettings::default(),
            event_hub: EventHubConfig::default(),
            sse_ping: Duration::from_secs(15),
            storage_quota_bytes: DEFAULT_STORAGE_QUOTA_BYTES,
            daily_llm_tokens: 0,
            max_sse_per_user: DEFAULT_MAX_SSE_PER_USER,
        }
    }
}

/// What the documents feature needs from the outside: the PDF engine and the
/// place uploads wait in.
#[derive(Clone)]
pub struct DocumentSetup {
    pub extractor: Arc<dyn PdfTextExtractor>,
    pub files: Arc<dyn DocumentFiles>,
}

/// Stands in when no PDF engine is installed: imports fail with `internal`
/// and the reason is in the log, everything else keeps working.
pub struct NoPdfEngine;

impl PdfTextExtractor for NoPdfEngine {
    fn extract(
        &self,
        _path: &std::path::Path,
        _max_pages: u32,
        _on_page: &mut dyn FnMut(u32, u32),
    ) -> Result<domain::ExtractedPdf, PdfError> {
        Err(PdfError::Unavailable(
            "no PDF engine (libpdfium) was found at startup".into(),
        ))
    }
}

impl DocumentSetup {
    /// No engine, uploads under the system temp dir: for tests that do not
    /// import documents. The binary builds its own setup.
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            extractor: Arc::new(NoPdfEngine),
            files: Arc::new(LocalDocumentFiles::new(
                std::env::temp_dir().join("teach_me_uploads"),
            )),
        }
    }
}

/// The speech engine and the clip files of the read-aloud feature. The binary
/// passes one only when `TEACH_ME_TTS_URL` is set: without it nothing is
/// queued and no worker exists.
#[derive(Clone)]
pub struct AudioSetup {
    pub synthesizer: Arc<dyn SpeechSynthesizer>,
    pub store: Arc<dyn ClipStore>,
    pub settings: AudioSettings,
}

/// The credential adapters and what the API says about the LLM service.
#[derive(Clone)]
pub struct AuthSetup {
    pub hasher: Arc<dyn PasswordHasher>,
    pub cipher: Arc<dyn SecretCipher>,
    pub tokens: Arc<dyn TokenIssuer>,
    /// `anthropic` or `openrouter`, as `User.llm_provider` reports it.
    pub llm_provider: String,
}

impl AuthSetup {
    /// The production adapters: Argon2id, XChaCha20-Poly1305 under the server
    /// key, OS randomness.
    pub fn production(key: &ServerKey, llm_provider: &str) -> Result<Self, domain::CryptoError> {
        Ok(Self {
            hasher: Arc::new(Argon2idHasher::new()),
            cipher: Arc::new(XChaChaSecretCipher::new(key)?),
            tokens: Arc::new(OsTokenIssuer),
            llm_provider: llm_provider.to_owned(),
        })
    }
}

/// Shared dependencies injected into handlers and middleware.
#[derive(Clone)]
pub struct AppState {
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGenerator>,
    pub auth: Arc<AuthService>,
    pub account: Arc<AccountService>,
    pub keys: Arc<LlmKeys>,
    pub llm_provider: String,
    pub folders: Arc<FolderService>,
    pub topics: Arc<TopicService>,
    pub chat: Arc<ChatService>,
    pub missions: Arc<MissionService>,
    pub preferences: Arc<PreferenceService>,
    pub jobs: Arc<JobService>,
    pub resources: Arc<ResourceService>,
    pub lessons: Arc<LessonService>,
    pub attempts: Arc<AttemptService>,
    pub highlights: Arc<HighlightService>,
    pub strokes: Arc<StrokeService>,
    pub reviews: Arc<ReviewService>,
    pub component_requests: Arc<ComponentRequestService>,
    pub documents: Arc<DocumentService>,
    /// `None` when read-aloud is off.
    pub audio: Option<Arc<AudioPreparer>>,
    /// The read-aloud endpoints' use cases (answers "off" without an engine).
    pub read_aloud: Arc<AudioService>,
    audio_worker: Option<Arc<AudioWorker>>,
    /// What `GET /ready` asks; also owns the job runner's heartbeat.
    pub ready: Arc<crate::ready::Readiness>,
    import: Arc<ImportDocument>,
    research: Arc<RunResearch>,
    generation: Arc<GenerateLesson>,
    completion: Arc<RunLessonCompletion>,
    job_settings: JobSettings,
    /// The job runner's proof of life (`GET /ready`); the binary's runner
    /// beats it, tests beat it by hand.
    pub heartbeat: Arc<application::Heartbeat>,
    pub events: Arc<EventHub>,
    pub sse_ping: Duration,
    /// Open SSE streams per user.
    pub streams: StreamLimiter,
}

impl AppState {
    /// Composition root: wires the SQLite adapters and the event hub into the
    /// use cases. `llm` is the (already decorated) model provider.
    #[must_use]
    pub fn new(
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        auth: AuthSetup,
        pool: &Pool,
        llm: Arc<dyn LlmProvider>,
        options: AppOptions,
    ) -> Self {
        Self::with_documents(
            clock,
            ids,
            auth,
            pool,
            llm,
            options,
            DocumentSetup::unavailable(),
        )
    }

    /// Like [`Self::new`], with the PDF engine and the upload directory.
    #[must_use]
    pub fn with_documents(
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        auth_setup: AuthSetup,
        pool: &Pool,
        llm: Arc<dyn LlmProvider>,
        options: AppOptions,
        documents_setup: DocumentSetup,
    ) -> Self {
        Self::with_audio(
            clock,
            ids,
            auth_setup,
            pool,
            llm,
            options,
            documents_setup,
            None,
        )
    }

    /// Like [`Self::with_documents`], with the read-aloud feature when `audio`
    /// is given.
    #[must_use]
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)] // the composition root: one line per wire
    pub fn with_audio(
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        auth_setup: AuthSetup,
        pool: &Pool,
        llm: Arc<dyn LlmProvider>,
        options: AppOptions,
        documents_setup: DocumentSetup,
        audio_setup: Option<AudioSetup>,
    ) -> Self {
        let budget = Arc::new(LlmBudget::new(
            Arc::new(SqliteLlmUsageRepository::new(pool.clone())),
            clock.clone(),
            options.daily_llm_tokens,
        ));
        // Every model call carries the key of the owner of its work, and is
        // counted against that owner's daily token budget.
        let llm: Arc<dyn LlmProvider> =
            Arc::new(KeyScopedLlm::new(llm).with_budget(budget.clone()));
        let user_repo = Arc::new(SqliteUserRepository::new(pool.clone()));
        let session_repo = Arc::new(SqliteSessionRepository::new(pool.clone()));
        let keys = Arc::new(
            LlmKeys::new(user_repo.clone(), auth_setup.cipher.clone()).with_budget(budget),
        );
        let auth = Arc::new(AuthService::new(
            user_repo.clone(),
            session_repo,
            auth_setup.hasher.clone(),
            auth_setup.tokens.clone(),
            clock.clone(),
            ids.clone(),
        ));
        let account = Arc::new(AccountService::new(
            user_repo.clone(),
            auth_setup.cipher.clone(),
        ));
        let folder_repo = Arc::new(SqliteFolderRepository::new(pool.clone()));
        let topic_repo = Arc::new(SqliteTopicRepository::new(pool.clone()));
        let chat_repo = Arc::new(SqliteChatRepository::new(pool.clone()));
        let mission_repo = Arc::new(SqliteMissionRepository::new(pool.clone()));
        let job_repo = Arc::new(SqliteJobRepository::new(pool.clone()));
        let preference_repo = Arc::new(SqlitePreferenceRepository::new(pool.clone()));
        let resource_repo = Arc::new(SqliteResourceRepository::new(pool.clone()));
        let lesson_repo = Arc::new(SqliteLessonRepository::new(pool.clone()));
        let attempt_repo = Arc::new(SqliteAttemptRepository::new(pool.clone()));
        let highlight_repo = Arc::new(SqliteHighlightRepository::new(pool.clone()));
        let stroke_repo = Arc::new(SqliteStrokeRepository::new(pool.clone()));
        let learning_repo = Arc::new(SqliteLearningRepository::new(pool.clone()));
        let review_repo = Arc::new(SqliteReviewRepository::new(pool.clone()));
        let document_repo = Arc::new(SqliteDocumentRepository::new(pool.clone()));
        let events = Arc::new(EventHub::new(clock.clone(), options.event_hub));
        let heartbeat = Arc::new(application::Heartbeat::default());
        let ready = Arc::new(crate::ready::Readiness::new(
            pool.clone(),
            heartbeat.clone(),
            audio_setup.as_ref().map(|s| s.synthesizer.clone()),
            clock.clone(),
        ));
        let (audio, audio_worker) = match audio_setup {
            Some(setup) => {
                let cache = Arc::new(ClipCache::new(
                    setup.synthesizer,
                    Arc::new(SqliteClipRepository::new(pool.clone())),
                    setup.store,
                    clock.clone(),
                    ids.clone(),
                ));
                let (preparer, worker) = AudioPreparer::new(
                    cache,
                    Arc::new(SqliteAudioQueue::new(pool.clone())),
                    user_repo.clone(),
                    setup.settings,
                );
                (Some(preparer), Some(worker))
            }
            None => (None, None),
        };
        let read_aloud = Arc::new(AudioService::new(
            audio.clone(),
            lesson_repo.clone(),
            topic_repo.clone(),
            user_repo.clone(),
        ));
        let folders = Arc::new(FolderService::new(
            folder_repo.clone(),
            topic_repo.clone(),
            events.clone(),
            clock.clone(),
            ids.clone(),
        ));
        let topics = Arc::new(TopicService::new(
            topic_repo.clone(),
            folder_repo,
            clock.clone(),
            ids.clone(),
            events.clone(),
        ));
        let jobs = Arc::new(JobService::new(
            job_repo,
            topic_repo.clone(),
            events.clone(),
            clock.clone(),
            ids.clone(),
            keys.clone(),
        ));
        let documents = Arc::new(
            DocumentService::new(
                topic_repo.clone(),
                document_repo.clone(),
                documents_setup.files.clone(),
                jobs.clone(),
                ids.clone(),
                clock.clone(),
            )
            .with_storage_quota(options.storage_quota_bytes),
        );
        let import = Arc::new(ImportDocument::new(
            topic_repo.clone(),
            document_repo.clone(),
            documents_setup.files,
            documents_setup.extractor,
            clock.clone(),
            ids.clone(),
        ));
        let missions = Arc::new(MissionService::new(
            topic_repo.clone(),
            mission_repo.clone(),
            events.clone(),
            clock.clone(),
            jobs.clone(),
        ));
        let resources = Arc::new(ResourceService::new(
            topic_repo.clone(),
            mission_repo.clone(),
            resource_repo.clone(),
            jobs.clone(),
        ));
        let research = Arc::new(
            RunResearch::new(
                topic_repo.clone(),
                mission_repo.clone(),
                preference_repo.clone(),
                resource_repo.clone(),
                events.clone(),
                llm.clone(),
                ResearcherPrompts::embedded(),
                clock.clone(),
                ids.clone(),
                options.research,
            )
            .with_documents(document_repo.clone()),
        );
        let lesson_prompts = LessonPrompts::embedded();
        let grader = Arc::new(Grader::new(
            llm.clone(),
            lesson_prompts.grader,
            options.lessons.grader_max_output_tokens,
        ));
        let lessons = LessonService::new(
            topic_repo.clone(),
            lesson_repo.clone(),
            attempt_repo.clone(),
            highlight_repo.clone(),
            stroke_repo.clone(),
            learning_repo.clone(),
            jobs.clone(),
            keys.clone(),
            clock.clone(),
        )
        .with_documents(document_repo.clone());
        let lessons = Arc::new(match &audio {
            Some(audio) => lessons.with_audio(audio.clone()),
            None => lessons,
        });
        let highlights = Arc::new(HighlightService::new(lesson_repo.clone(), highlight_repo));
        let strokes = Arc::new(StrokeService::new(lesson_repo.clone(), stroke_repo));
        let attempts = Arc::new(AttemptService::new(
            lesson_repo.clone(),
            topic_repo.clone(),
            attempt_repo.clone(),
            grader.clone(),
            keys.clone(),
            clock.clone(),
            ids.clone(),
            options.lessons.grader_timeout,
        ));
        let component_requests = Arc::new(ComponentRequestService::new(lesson_repo.clone()));
        let reviews = Arc::new(ReviewService::new(
            review_repo,
            Arc::new(FsrsScheduler::new()),
            clock.clone(),
            ids.clone(),
        ));
        let generation = GenerateLesson::new(
            topic_repo.clone(),
            mission_repo.clone(),
            preference_repo.clone(),
            resource_repo.clone(),
            lesson_repo.clone(),
            learning_repo.clone(),
            llm.clone(),
            lesson_prompts,
            clock.clone(),
            ids.clone(),
            options.lessons,
        )
        .with_documents(document_repo.clone());
        let generation = Arc::new(match &audio {
            Some(audio) => generation.with_audio(audio.clone()),
            None => generation,
        });
        let completion = Arc::new(RunLessonCompletion::new(
            topic_repo.clone(),
            mission_repo.clone(),
            lesson_repo.clone(),
            attempt_repo,
            learning_repo,
            grader,
            llm.clone(),
            lesson_prompts.recorder,
            clock.clone(),
            ids.clone(),
            options.lessons,
        ));
        let preferences = Arc::new(PreferenceService::new(
            preference_repo.clone(),
            events.clone(),
            clock.clone(),
        ));
        let chat = Arc::new(
            ChatService::new(
                topic_repo,
                chat_repo,
                mission_repo,
                preference_repo,
                events.clone(),
                llm,
                TutorPrompts::embedded(),
                clock.clone(),
                ids.clone(),
                keys.clone(),
                options.chat,
            )
            .with_lessons(lesson_repo)
            .with_lesson_requests(lessons.clone())
            .with_documents(document_repo),
        );
        Self {
            clock,
            ids,
            auth,
            account,
            keys,
            llm_provider: auth_setup.llm_provider,
            folders,
            topics,
            chat,
            missions,
            preferences,
            jobs,
            resources,
            lessons,
            attempts,
            highlights,
            strokes,
            reviews,
            component_requests,
            documents,
            read_aloud,
            audio,
            audio_worker,
            ready,
            heartbeat,
            import,
            research,
            generation,
            completion,
            job_settings: options.jobs,
            events,
            sse_ping: options.sse_ping,
            streams: StreamLimiter::new(options.max_sse_per_user),
        }
    }

    /// The worker that makes the queued clips, to spawn once; `None` when
    /// read-aloud is off.
    #[must_use]
    pub fn audio_worker(&self) -> Option<Arc<AudioWorker>> {
        self.audio_worker.clone()
    }

    /// The handler of `research` jobs, to register with [`Self::job_runner`].
    #[must_use]
    pub fn research_handler(&self) -> Arc<dyn JobHandler> {
        self.research.clone()
    }

    /// The handler of `document_import` jobs.
    #[must_use]
    pub fn import_handler(&self) -> Arc<dyn JobHandler> {
        self.import.clone()
    }

    /// The handler of `lesson_generation` jobs.
    #[must_use]
    pub fn generation_handler(&self) -> Arc<dyn JobHandler> {
        self.generation.clone()
    }

    /// The handler of `lesson_completion` jobs.
    #[must_use]
    pub fn completion_handler(&self) -> Arc<dyn JobHandler> {
        self.completion.clone()
    }

    /// The runner for this state's jobs, with the handlers the binary has.
    /// Kinds without a handler are not executed: their jobs wait in the queue.
    #[must_use]
    pub fn job_runner(&self, handlers: Vec<(JobKind, Arc<dyn JobHandler>)>) -> JobRunner {
        handlers.into_iter().fold(
            JobRunner::new(self.jobs.clone(), self.job_settings)
                .with_heartbeat(self.heartbeat.clone()),
            |runner, (kind, handler)| runner.with_handler(kind, handler),
        )
    }
}
