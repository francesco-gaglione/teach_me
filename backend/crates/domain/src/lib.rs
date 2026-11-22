//! Domain layer: entities, rules and ports. No framework dependencies.

pub mod audio;
pub mod chat;
pub mod document;
pub mod error;
pub mod event;
pub mod folder;
pub mod ids;
pub mod job;
pub mod language;
pub mod learning;
pub mod lesson;
pub mod mission;
pub mod pagination;
pub mod ports;
pub mod preference;
pub mod resource;
pub mod secret;
pub mod topic;
pub mod user;
pub mod validation;

pub use audio::{
    AudioRequest, BlockAudioState, CLIP_MAX_IDLE, Clip, EngineInfo, GRANT_REFRESH, QueuedAudio,
    SpeechError, SynthesizedClip, Voice, clip_key, text_hash,
};
pub use chat::{
    ChatCursor, ChatErrorCode, ChatMessage, ChatRuleError, MessageError, MessagePart, MessageRole,
    MessageStatus, MessageText, has_active_turn,
};
pub use document::{
    Document, DocumentFiles, DocumentSection, DocumentStatus, DocumentView, ExtractedPdf,
    ImportedDocument, OutlineEntry, PdfError, PdfTextExtractor,
};
pub use error::DomainError;
pub use event::{TopicEvent, TopicEventPayload};
pub use folder::{Folder, FolderCursor, FolderName, FolderSummary, NewFolder};
pub use ids::{
    AttemptId, ClipId, ComponentRequestId, DocumentId, FolderId, GlossaryTermId, JobId,
    LearningRecordId, LessonId, MessageId, PreferenceId, ResourceId, ReviewCardId, ReviewLogId,
    SessionId, TopicId, UserId, parse_uuid,
};
pub use job::{
    Job, JobCursor, JobError, JobErrorCode, JobKind, JobProgress, JobResult, JobRuleError,
    JobStatus, UnknownJobValue,
};
pub use language::{ContentLanguage, UnknownContentLanguage};
pub use learning::{
    Answer, Attempt, AttemptRuleError, COMPONENT_REQUEST_EXAMPLES, CardDraft, CardKind, CardOrigin,
    CardSchedule, CardStatus, CardTarget, CardVotes, CompletionSave, CompletionSummary,
    ComponentRequest, ComponentRequestCursor, ComponentRequestDraft, ComponentRequestExample,
    ComponentRequestGroup, DueCursor, DueStats, DueTopic, Exercise, ExerciseKind,
    FEEDBACK_MAX_CHARS, FIRST_REVIEW_DELAY, GeneratedLesson, GlossaryTerm, Grade, InProgressCursor,
    InProgressLesson, LearningRecord, Lesson, LessonCursor, LessonExercises, LessonMeta,
    LessonOutputs, LessonQualityWarning, LessonStatus, MISCONCEPTION_MAX_CHARS, MemoryBasis,
    MemoryValues, NewCard, NewRecord, OptimizationAttempt, OptimizationOutcome, Outcome, Rating,
    RatingIntervals, RecordCursor, RecordDraft, RecordKind, RecordStatus, RecorderContext,
    RecorderDraft, ReviewCard, ReviewEvent, ReviewItem, ReviewLog, ReviewSample, ReviewState,
    SECONDS_PER_CARD, SchedulerParameters, StoredLesson, SuspendedCursor, Suspension,
    SuspensionReason, TermCursor, TermDraft, TermUpsert, UNDO_WINDOW, UnknownLearningValue,
    UserWeights, Verdict, VoteRecord, component_request_drafts, derive_flashcard_outcome,
    estimated_minutes, has_passed_attempt, is_to_consolidate, term_sort_key, validate_recorder,
};
pub use lesson::{
    BLOCK_TYPES, Highlight, HighlightColor, HighlightDraft, HighlightIssue, HighlightTargets,
    LessonDocument, LessonErrorCode, LessonIssue, LessonValidator, MAX_HIGHLIGHTS,
    MAX_LESSON_MINUTES, MAX_POINTS_PER_STROKE, MAX_POINTS_TOTAL, MAX_STROKES, MIN_LESSON_MINUTES,
    QualityWarning, ReadableBlock, Stroke, StrokeColor, StrokeDraft, StrokeIssue, StrokeTargets,
    StrokeWidth, ValidationContext, ValidatorConfig, client_view, document_minutes, lesson_minutes,
    readable_blocks, set_computed_minutes, warning_score,
};
pub use mission::{
    Mission, MissionContent, MissionCursor, MissionRuleError, MissionStatus, TopicMission,
    UnknownMissionStatus,
};
pub use pagination::{Page, PageRequest};
pub use ports::llm;
pub use ports::{
    AttemptRepository, AudioQueue, ChatRepository, ClipRepository, ClipStore, Clock, CryptoError,
    DocumentRepository, FolderRepository, HighlightRepository, IdGenerator, JobRepository,
    LearningRepository, LessonRepository, LlmUsageRepository, MissionRepository, OptimizerError,
    ParameterOptimizer, PasswordHasher, PreferenceRepository, RepositoryError, ResourceRepository,
    ReviewMigrationRepository, ReviewOptimizationRepository, ReviewRepository, ReviewScheduler,
    SchedulerError, SecretCipher, SessionRepository, SpeechSynthesizer, StoredFile,
    StrokeRepository, TokenIssuer, TopicEventPublisher, TopicRepository, UserRepository,
};
pub use preference::{Preference, PreferenceCursor, PreferenceScope, PreferenceText, ScopeKind};
pub use resource::{
    Gap, ResearchInfo, ResearchOutcome, Resource, ResourceDraft, ResourceKind, SourceType,
    TopicResources, https_host, normalize_url,
};
pub use secret::Secret;
pub use topic::{StateChange, Topic, TopicCursor, TopicRuleError, TopicState, TopicTitle};
pub use user::{
    DisplayName, NewUser, Password, SealedSecret, Session, StoredPasswordHash, TokenDigest, User,
    UserCredentials, Username, parse_api_key,
};
pub use validation::{IssueKind, ValidationIssue};
