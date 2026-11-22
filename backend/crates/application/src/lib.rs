//! Application layer: use cases orchestrating domain ports.

pub mod attempts;
pub mod audio;
pub mod audio_cleanup;
pub mod audio_queue;
pub mod audio_service;
pub mod budget;
pub mod chat;
pub mod completion;
pub mod component_requests;
pub mod context;
pub mod document_context;
pub mod documents;
pub mod error;
pub mod folders;
pub mod generation;
pub mod grader;
pub mod highlights;
pub mod jobs;
pub mod lesson_context;
pub mod lesson_plan;
pub mod lessons;
pub mod llm_keys;
pub mod mission;
pub mod paging;
pub mod planned_visual;
pub mod preferences;
pub mod prompts;
pub mod research;
pub mod research_output;
pub mod retry;
pub mod review_migration;
pub mod review_optimization;
pub mod reviews;
pub mod strokes;
pub mod structured;
pub mod topics;
pub mod tutor_tools;
pub mod users;
pub mod web_fetch;

mod time_util;

#[cfg(test)]
mod chat_tests;
#[cfg(test)]
pub(crate) mod document_tests;
#[cfg(test)]
mod fakes;
#[cfg(test)]
mod jobs_tests;
#[cfg(test)]
mod lesson_request_tests;
#[cfg(test)]
mod lesson_tests;
#[cfg(test)]
mod llm_tests;
#[cfg(test)]
mod mission_tests;
#[cfg(test)]
mod research_tests;
#[cfg(test)]
mod retry_tests;
#[cfg(test)]
mod review_optimization_tests;
#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod users_tests;

pub use attempts::AttemptService;
pub use audio::{AudioError, CleanupReport, ClipCache};
pub use audio_cleanup::AudioCleaner;
pub use audio_queue::{AudioPreparer, AudioSettings, AudioWorker, Step as AudioStep};
pub use audio_service::{
    AudioService, ClipAccess, Manifest, ManifestBlock, ManifestClip, ManifestStatus, VoiceEntry,
    VoiceList,
};
pub use budget::LlmBudget;
pub use chat::{ChatService, ChatSettings, MessagePage, SentTurn};
pub use completion::RunLessonCompletion;
pub use component_requests::ComponentRequestService;
pub use document_context::DocumentSource;
pub use documents::{DocumentService, ImportDocument, NewUpload};
pub use error::{AppError, ConflictReason};
pub use folders::FolderService;
pub use generation::{GenerateLesson, LessonSettings};
pub use grader::Grader;
pub use highlights::HighlightService;
pub use jobs::{
    Heartbeat, JobContext, JobFailure, JobHandler, JobRunner, JobService, JobSettings,
    RUNNER_HEARTBEAT, Recovery,
};
pub use lesson_plan::{PlanDocument, PlanFacts, ValidPlan, validate_plan};
pub use lessons::{CompletionView, InProgressItem, LessonItem, LessonService, LessonView};
pub use llm_keys::{KeyError, KeyScopedLlm, LlmKeys, key_context, with_key};
pub use mission::MissionService;
pub use paging::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, clamp_limit};
pub use preferences::PreferenceService;
pub use prompts::{LessonPrompts, ResearcherPrompts, RolePrompts, TutorPrompts};
pub use research::{ResearchSettings, ResourceService, ResourcesView, RunResearch};
pub use retry::{RetryPolicy, RetryingLlm, SystemRetryEnv};
pub use review_migration::{MigrateReviewModel, MigrationReport};
pub use review_optimization::{OptimizationSchedule, OptimizeReviewParameters, UserRun};
pub use reviews::{DueReviews, ReviewService, ReviewView, SuspendedReviews, Vote};
pub use strokes::StrokeService;
pub use topics::{TopicService, UpdateTopic};
pub use users::{
    AccountService, AuthService, Authenticated, CreateUser, CreatedUser, LoggedIn, LoginLimiter,
    NewAccount, PasswordSet, UserAdmin,
};
