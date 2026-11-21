//! Lessons, attempts, learning records, glossary and review cards
//! (`docs/contracts/lessons.md`, phases 6 and 7).
//!
//! The lesson *document* (SDUI) lives in [`crate::lesson`]; this module holds
//! what surrounds it: the lesson as a numbered unit of the topic, what the
//! learner answered, and what the Recorder concluded from it.

mod attempt;
mod exercise;
mod lesson;
mod optimization;
mod record;
mod recorder;
mod review;

pub use attempt::{
    Answer, Attempt, AttemptRuleError, FEEDBACK_MAX_CHARS, Grade, MISCONCEPTION_MAX_CHARS, Outcome,
    Rating, Verdict, derive_flashcard_outcome,
};
pub use exercise::{Exercise, ExerciseKind, LessonExercises};
pub use lesson::{
    COMPONENT_REQUEST_EXAMPLES, ComponentRequest, ComponentRequestCursor, ComponentRequestDraft,
    ComponentRequestExample, ComponentRequestGroup, GeneratedLesson, InProgressCursor,
    InProgressLesson, Lesson, LessonCursor, LessonMeta, LessonQualityWarning, LessonStatus,
    StoredLesson, component_request_drafts,
};
pub use optimization::{
    CardVotes, OptimizationAttempt, OptimizationOutcome, ReviewSample, VoteRecord,
};
pub use record::{
    CardOrigin, CardStatus, CardTarget, CompletionSave, CompletionSummary, GlossaryTerm,
    LearningRecord, LessonOutputs, NewCard, NewRecord, RecordCursor, RecordKind, RecordStatus,
    ReviewCard, Suspension, SuspensionReason, TermCursor, TermUpsert, term_sort_key,
};
pub use recorder::{
    CardDraft, RecordDraft, RecorderContext, RecorderDraft, TermDraft, has_passed_attempt,
    validate_recorder,
};
pub use review::{
    CardKind, CardSchedule, DueCursor, DueStats, DueTopic, FIRST_REVIEW_DELAY, MemoryBasis,
    MemoryValues, RatingIntervals, ReviewEvent, ReviewItem, ReviewLog, ReviewState,
    SECONDS_PER_CARD, SchedulerParameters, SuspendedCursor, UNDO_WINDOW, UserWeights,
    estimated_minutes, is_to_consolidate,
};

/// A stored string that is not one of the known values.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown learning value `{0}`")]
pub struct UnknownLearningValue(pub String);
