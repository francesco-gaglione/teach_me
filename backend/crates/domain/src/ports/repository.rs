use async_trait::async_trait;

use crate::ContentLanguage;
use crate::chat::{ChatCursor, ChatMessage, MessageError};
use crate::document::{Document, DocumentView, ImportedDocument};
use crate::folder::{Folder, FolderCursor, FolderSummary, NewFolder};
use crate::ids::{
    AttemptId, DocumentId, FolderId, JobId, LessonId, MessageId, PreferenceId, ReviewCardId,
    ReviewLogId, SessionId, TopicId, UserId,
};
use crate::job::{Job, JobCursor, JobError, JobKind, JobStatus};
use crate::learning::{
    Attempt, CardSchedule, CardVotes, CompletionSave, CompletionSummary, ComponentRequest,
    ComponentRequestCursor, ComponentRequestGroup, DueCursor, DueStats, DueTopic, GeneratedLesson,
    GlossaryTerm, Grade, InProgressCursor, InProgressLesson, LearningRecord, Lesson, LessonCursor,
    LessonOutputs, MemoryValues, OptimizationAttempt, RecordCursor, ReviewCard, ReviewEvent,
    ReviewItem, ReviewLog, SchedulerParameters, StoredLesson, SuspendedCursor, TermCursor,
    UserWeights,
};
use crate::lesson::{Highlight, Stroke};
use crate::mission::{Mission, MissionContent, MissionCursor};
use crate::preference::{Preference, PreferenceCursor, PreferenceScope};
use crate::resource::{ResearchOutcome, TopicResources};
use crate::topic::{StateChange, Topic, TopicCursor, TopicState};
use crate::user::{
    NewUser, SealedSecret, Session, StoredPasswordHash, TokenDigest, User, UserCredentials,
    Username,
};
use time::OffsetDateTime;

/// Failures a repository can report, independent of the storage engine.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// A uniqueness rule (case-insensitive name/title) was violated.
    #[error("duplicate")]
    Duplicate,
    /// The row to update no longer exists.
    #[error("not found")]
    NotFound,
    /// A referenced parent (folder) does not exist.
    #[error("missing parent")]
    MissingParent,
    /// A state precondition no longer holds (e.g. a turn is already streaming).
    #[error("conflict")]
    Conflict,
    /// A rule about the stored state does not hold (a second attempt whose
    /// first attempt is missing or already correct), checked where the write
    /// happens so a concurrent write cannot slip past it.
    #[error("rule violated")]
    RuleViolated,
    #[error("storage failure: {0}")]
    Storage(#[source] Box<dyn std::error::Error + Send + Sync>),
}

#[async_trait]
pub trait FolderRepository: Send + Sync {
    /// Stores the folder with `position` = the user's current max + 1 (0 when
    /// the user has none). Names are unique per user.
    async fn insert(&self, folder: NewFolder) -> Result<FolderSummary, RepositoryError>;
    /// `None` when the folder does not exist or belongs to another user.
    async fn get(
        &self,
        user: UserId,
        id: FolderId,
    ) -> Result<Option<FolderSummary>, RepositoryError>;
    /// At most `limit` folders of the user strictly after `after`, by
    /// (`position`, `id`).
    async fn list(
        &self,
        user: UserId,
        after: Option<FolderCursor>,
        limit: u32,
    ) -> Result<Vec<FolderSummary>, RepositoryError>;
    /// Writes the folder's name; [`RepositoryError::NotFound`] when it is not
    /// the folder's owner's (`folder.user_id`) or is gone.
    async fn update(&self, folder: &Folder) -> Result<(), RepositoryError>;
    /// Deletes the folder and, by cascade, everything it owns. `false` if
    /// absent or not the user's.
    async fn delete(&self, user: UserId, id: FolderId) -> Result<bool, RepositoryError>;
}

#[async_trait]
pub trait TopicRepository: Send + Sync {
    /// [`RepositoryError::MissingParent`] when the folder is not the topic's
    /// owner's.
    async fn insert(&self, topic: &Topic) -> Result<(), RepositoryError>;
    /// `None` when the topic does not exist or belongs to another user.
    async fn get(&self, user: UserId, id: TopicId) -> Result<Option<Topic>, RepositoryError>;
    /// The topic whoever owns it, for work that runs without a request (job
    /// handlers, the reply being generated): it then acts for `topic.user_id`.
    /// Never call this from a use case that starts with a request.
    async fn get_system(&self, id: TopicId) -> Result<Option<Topic>, RepositoryError>;
    /// At most `limit` topics of the user's folder strictly after `after`, by
    /// (`created_at`, `id`).
    async fn list_by_folder(
        &self,
        user: UserId,
        folder_id: FolderId,
        after: Option<TopicCursor>,
        limit: u32,
    ) -> Result<Vec<Topic>, RepositoryError>;
    /// [`RepositoryError::NotFound`] when the topic is not `topic.user_id`'s;
    /// [`RepositoryError::MissingParent`] when the destination folder is not
    /// the owner's.
    async fn update(&self, topic: &Topic) -> Result<(), RepositoryError>;
    /// Moves the state from `from` to `to` only if it is still `from`, so a
    /// stale entity cannot overwrite a concurrent change. Returns whether the
    /// transition was applied.
    async fn transition_state(
        &self,
        id: TopicId,
        from: TopicState,
        to: TopicState,
        at: OffsetDateTime,
    ) -> Result<bool, RepositoryError>;
    /// Ids of every topic in the user's folder (read before a folder cascade).
    async fn ids_in_folder(
        &self,
        user: UserId,
        folder_id: FolderId,
    ) -> Result<Vec<TopicId>, RepositoryError>;
    /// `false` if absent or not the user's.
    async fn delete(&self, user: UserId, id: TopicId) -> Result<bool, RepositoryError>;
}

#[async_trait]
pub trait ChatRepository: Send + Sync {
    /// Stores a user message and the assistant message that answers it, in one
    /// transaction. [`RepositoryError::Conflict`] when the topic already has a
    /// `streaming` message (at most one per topic); [`RepositoryError::MissingParent`]
    /// when the topic does not exist.
    async fn insert_turn(
        &self,
        user: &ChatMessage,
        assistant: &ChatMessage,
    ) -> Result<(), RepositoryError>;
    async fn get(
        &self,
        topic_id: TopicId,
        id: MessageId,
    ) -> Result<Option<ChatMessage>, RepositoryError>;
    /// The most recent message of the topic, by (`created_at`, `id`).
    async fn latest(&self, topic_id: TopicId) -> Result<Option<ChatMessage>, RepositoryError>;
    /// At most `limit` messages strictly older than `after`, newest first.
    async fn list(
        &self,
        topic_id: TopicId,
        after: Option<ChatCursor>,
        limit: u32,
    ) -> Result<Vec<ChatMessage>, RepositoryError>;
    /// Overwrites parts, status, error and `updated_at`. [`RepositoryError::NotFound`]
    /// when the message is gone (its topic was deleted).
    async fn save(&self, message: &ChatMessage) -> Result<(), RepositoryError>;
    /// Like `save`, but only applies if the stored message is still `failed`
    /// (retry); [`RepositoryError::Conflict`] otherwise, or when another message
    /// of the topic is streaming.
    async fn restart_failed(&self, message: &ChatMessage) -> Result<(), RepositoryError>;
    /// Marks every `streaming` message `failed` with `error`, keeping the
    /// partial text. `error` gives the error in the language of the message's
    /// topic. Returns how many were changed. Used at startup.
    async fn fail_streaming(
        &self,
        error: &(dyn Fn(ContentLanguage) -> MessageError + Sync),
        at: OffsetDateTime,
    ) -> Result<u64, RepositoryError>;
}

#[async_trait]
pub trait MissionRepository: Send + Sync {
    /// Stores a new draft as version `max + 1` (1 for the first) and, in the
    /// same transaction, supersedes the topic's open draft if there is one, so
    /// at most one draft is ever open. With `topic_change` the topic moves
    /// from `from` to `to` in that same transaction: if it is no longer in
    /// `from` ([`RepositoryError::Conflict`]) or the move fails, nothing is
    /// stored. [`RepositoryError::MissingParent`] when the topic does not exist.
    async fn insert_draft(
        &self,
        topic_id: TopicId,
        content: &MissionContent,
        at: OffsetDateTime,
        topic_change: Option<StateChange>,
    ) -> Result<Mission, RepositoryError>;
    /// The `confirmed` mission, if any.
    async fn current(&self, topic_id: TopicId) -> Result<Option<Mission>, RepositoryError>;
    /// The open `draft`, if any.
    async fn open_draft(&self, topic_id: TopicId) -> Result<Option<Mission>, RepositoryError>;
    /// Stores the confirmation of `mission` (already confirmed by the domain)
    /// and supersedes the previously confirmed version at `superseded_at`, in
    /// one transaction, together with `topic_change` (see `insert_draft`).
    /// [`RepositoryError::Conflict`] when the stored version is no longer an
    /// open draft (confirmed twice, superseded meanwhile), the topic is not
    /// in the expected state; nothing is stored then. [`RepositoryError::Duplicate`]
    /// when `follow_up` cannot be stored because a job of that kind is already
    /// active (nothing is stored then either). `follow_up` is a
    /// queued job inserted in the same transaction, so a confirmed mission
    /// is never left without its research.
    async fn confirm(
        &self,
        mission: &Mission,
        superseded_at: OffsetDateTime,
        topic_change: Option<StateChange>,
        follow_up: Option<&Job>,
    ) -> Result<(), RepositoryError>;
    /// At most `limit` versions strictly older than `after`, newest first.
    async fn list_versions(
        &self,
        topic_id: TopicId,
        after: Option<MissionCursor>,
        limit: u32,
    ) -> Result<Vec<Mission>, RepositoryError>;
}

#[async_trait]
pub trait PreferenceRepository: Send + Sync {
    /// [`RepositoryError::Duplicate`] when the same scope already has a
    /// preference with the same text key; [`RepositoryError::MissingParent`]
    /// when its topic does not exist.
    async fn insert(&self, preference: &Preference) -> Result<(), RepositoryError>;
    /// How many preferences the user's scope holds (to cap them).
    async fn count(&self, user: UserId, scope: PreferenceScope) -> Result<u64, RepositoryError>;
    /// `None` when absent or another user's.
    async fn get(
        &self,
        user: UserId,
        id: PreferenceId,
    ) -> Result<Option<Preference>, RepositoryError>;
    /// At most `limit` of the user's preferences strictly after `after`, by
    /// (`created_at`, `id`). `None` lists all of the user's; `Some(topic)` the
    /// global ones and that topic's (a topic of another user adds nothing).
    async fn list(
        &self,
        user: UserId,
        topic: Option<TopicId>,
        after: Option<PreferenceCursor>,
        limit: u32,
    ) -> Result<Vec<Preference>, RepositoryError>;
    /// The newest `limit` preferences that apply to the topic (the user's
    /// global ones and the topic's own), newest first. `user` is the topic's
    /// owner: another user's global preferences never apply.
    async fn latest_for_topic(
        &self,
        user: UserId,
        topic_id: TopicId,
        limit: u32,
    ) -> Result<Vec<Preference>, RepositoryError>;
    /// `false` if absent or not the user's.
    async fn delete(&self, user: UserId, id: PreferenceId) -> Result<bool, RepositoryError>;
}

#[async_trait]
pub trait JobRepository: Send + Sync {
    /// [`RepositoryError::Conflict`] when the topic already has an active
    /// (queued or running) job of the same kind; [`RepositoryError::MissingParent`]
    /// when the topic does not exist.
    async fn insert(&self, job: &Job) -> Result<(), RepositoryError>;
    /// `None` when the job is not in the topic or the topic is not the user's.
    async fn get(
        &self,
        user: UserId,
        topic_id: TopicId,
        id: JobId,
    ) -> Result<Option<Job>, RepositoryError>;
    /// At most `limit` jobs of the topic strictly older than `after`, newest first.
    async fn list(
        &self,
        topic_id: TopicId,
        after: Option<JobCursor>,
        limit: u32,
    ) -> Result<Vec<Job>, RepositoryError>;
    /// The newest job of `kind`: of the lesson when `lesson_id` is given, else
    /// of the topic among the jobs that belong to no lesson.
    async fn latest(
        &self,
        topic_id: TopicId,
        kind: JobKind,
        lesson_id: Option<LessonId>,
    ) -> Result<Option<Job>, RepositoryError>;
    /// The oldest queued job whose kind is in `kinds`, across topics.
    async fn next_queued(&self, kinds: &[JobKind]) -> Result<Option<Job>, RepositoryError>;
    /// Every job currently `running` (to settle them at startup).
    async fn running(&self) -> Result<Vec<Job>, RepositoryError>;
    /// Overwrites the mutable fields (status, progress, error, attempt,
    /// timestamps) only if the stored status is still `expected`, so a stale
    /// copy cannot overwrite a newer one. [`RepositoryError::Conflict`] when
    /// the status differs or the new status would break the one-active-job
    /// rule; [`RepositoryError::NotFound`] when the job is gone.
    async fn update(&self, job: &Job, expected: JobStatus) -> Result<(), RepositoryError>;
}

#[async_trait]
pub trait ResourceRepository: Send + Sync {
    /// The topic's current sources, gaps and last-research facts (empty if
    /// it was never researched).
    async fn get(&self, topic_id: TopicId) -> Result<TopicResources, RepositoryError>;
    /// Replaces the topic's whole set with `outcome` in one transaction and,
    /// in the same one, moves the topic from `researching` to `ready` if (and
    /// only if) it is `researching`; a `ready` topic stays as it is. Returns
    /// whether the topic moved. [`RepositoryError::NotFound`] when the topic
    /// is gone; on any error the previous set is untouched.
    async fn replace(
        &self,
        outcome: &ResearchOutcome,
        at: OffsetDateTime,
    ) -> Result<bool, RepositoryError>;
}

#[async_trait]
pub trait LessonRepository: Send + Sync {
    /// Stores the lesson with `number` = the topic's highest + 1, its document,
    /// plan and component requests, in one transaction.
    /// [`RepositoryError::MissingParent`] when the topic is gone;
    /// [`RepositoryError::Conflict`] when a lesson was already stored for the
    /// same generation job.
    async fn insert_generated(&self, new: &GeneratedLesson) -> Result<Lesson, RepositoryError>;
    /// The lesson stored by a generation job, if it got that far (a job that
    /// is run again after a crash does not write a second lesson).
    async fn by_generation_job(&self, job_id: JobId) -> Result<Option<Lesson>, RepositoryError>;
    /// Whether the lesson exists and is in a topic of `user`; reads no document.
    async fn exists(&self, user: UserId, id: LessonId) -> Result<bool, RepositoryError>;
    /// `None` when absent or in another user's topic.
    async fn get(
        &self,
        user: UserId,
        id: LessonId,
    ) -> Result<Option<StoredLesson>, RepositoryError>;
    /// At most `limit` lessons of the topic strictly after `after`, by
    /// `number` descending.
    async fn list(
        &self,
        topic_id: TopicId,
        after: Option<LessonCursor>,
        limit: u32,
    ) -> Result<Vec<Lesson>, RepositoryError>;
    /// At most `limit` `in_progress` lessons of the user's topics after
    /// `after`, by last attempt descending then `id`.
    async fn list_in_progress(
        &self,
        user: UserId,
        after: Option<InProgressCursor>,
        limit: u32,
    ) -> Result<Vec<InProgressLesson>, RepositoryError>;
    /// Every lesson of the topic, oldest first (small: a topic has a handful).
    async fn all_in_topic(&self, topic_id: TopicId) -> Result<Vec<Lesson>, RepositoryError>;
    /// The Planner's plan of every lesson of the topic, to know what each one
    /// covered (the sections of uploaded documents it followed).
    async fn plans(
        &self,
        topic_id: TopicId,
    ) -> Result<Vec<(LessonId, serde_json::Value)>, RepositoryError>;
    /// Marks the lesson `completed` at `at` and stores the queued `job` that
    /// records it, in one transaction. [`RepositoryError::Conflict`] when the
    /// lesson is already completed or already has an active job;
    /// [`RepositoryError::NotFound`] when it is gone.
    async fn complete(
        &self,
        id: LessonId,
        at: time::OffsetDateTime,
        job: &Job,
    ) -> Result<Lesson, RepositoryError>;
    async fn component_requests(
        &self,
        lesson_id: LessonId,
    ) -> Result<Vec<ComponentRequest>, RepositoryError>;
    /// At most `limit` groups of the user's requests by `wanted`, strictly
    /// after `after` in the ranking order (see [`ComponentRequestCursor`]).
    async fn component_request_groups(
        &self,
        user: UserId,
        after: Option<ComponentRequestCursor>,
        limit: u32,
    ) -> Result<Vec<ComponentRequestGroup>, RepositoryError>;
}

#[async_trait]
pub trait AttemptRepository: Send + Sync {
    /// Every attempt of the lesson, by (`created_at`, `id`).
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Attempt>, RepositoryError>;
    /// Stores the attempts that are not there yet (the pair `block_id` +
    /// `attempt_number` is unique per lesson: the first stored wins) and
    /// returns what is stored for each, in the order given. In the same
    /// transaction a `not_started` lesson becomes `in_progress`, and a second
    /// attempt is accepted only if the first one is stored and not `correct`
    /// ([`RepositoryError::RuleViolated`] otherwise: nothing is stored).
    /// [`RepositoryError::Conflict`] when the lesson is `completed`;
    /// [`RepositoryError::NotFound`] when it is gone.
    async fn record(
        &self,
        lesson_id: LessonId,
        attempts: &[Attempt],
    ) -> Result<Vec<Attempt>, RepositoryError>;
    /// Stores the grade of an attempt that is still `pending` and returns the
    /// stored attempt; one already graded is returned untouched.
    async fn set_grade(&self, id: AttemptId, grade: &Grade) -> Result<Attempt, RepositoryError>;
}

#[async_trait]
/// The caller has already checked that the lesson belongs to the user (as the
/// services do with `LessonRepository::get`/`exists`): these methods work on
/// the lesson id alone.
pub trait HighlightRepository: Send + Sync {
    /// The highlights of the lesson, in the order they were stored.
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Highlight>, RepositoryError>;
    /// Replaces the whole set, in one transaction, keeping the given order.
    /// [`RepositoryError::NotFound`] when the lesson is gone.
    async fn replace(
        &self,
        lesson_id: LessonId,
        highlights: &[Highlight],
    ) -> Result<(), RepositoryError>;
}

#[async_trait]
/// Same contract as [`HighlightRepository`]: the caller has already checked
/// the lesson belongs to the user.
pub trait StrokeRepository: Send + Sync {
    /// The strokes of the lesson in drawing order (the first is underneath).
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Stroke>, RepositoryError>;
    /// Replaces the whole set, in one transaction, keeping the given order.
    /// [`RepositoryError::NotFound`] when the lesson is gone.
    async fn replace(&self, lesson_id: LessonId, strokes: &[Stroke])
    -> Result<(), RepositoryError>;
}

#[async_trait]
pub trait LearningRepository: Send + Sync {
    /// Records of the topic, `number` descending, strictly after `after`.
    async fn list_records(
        &self,
        topic_id: TopicId,
        active_only: bool,
        after: Option<RecordCursor>,
        limit: u32,
    ) -> Result<Vec<LearningRecord>, RepositoryError>;
    /// Glossary of the topic by term (case-insensitive), then key.
    async fn list_terms(
        &self,
        topic_id: TopicId,
        after: Option<TermCursor>,
        limit: u32,
    ) -> Result<Vec<GlossaryTerm>, RepositoryError>;
    /// What the completion of the lesson stored.
    async fn lesson_outputs(&self, lesson_id: LessonId) -> Result<LessonOutputs, RepositoryError>;
    /// Every review card of the topic, oldest first.
    async fn review_cards(&self, topic_id: TopicId) -> Result<Vec<ReviewCard>, RepositoryError>;
    /// Stores records, terms and cards of a completion, supersedes the
    /// replaced records and suspends their cards, in one transaction. Records
    /// get the next `number`s; a term whose `key` exists is revised in place.
    /// [`RepositoryError::Conflict`] when the lesson was already recorded or a
    /// replaced record is no longer active: nothing is stored then.
    async fn save_completion(
        &self,
        save: &CompletionSave,
    ) -> Result<CompletionSummary, RepositoryError>;
    /// The counts stored for the lesson, `None` if it was never recorded.
    async fn completion_summary(
        &self,
        lesson_id: LessonId,
    ) -> Result<Option<CompletionSummary>, RepositoryError>;
}

#[async_trait]
pub trait ReviewRepository: Send + Sync {
    /// One card with the names of its topic and lesson; `None` when absent or
    /// in another user's topic.
    async fn get(
        &self,
        user: UserId,
        id: ReviewCardId,
    ) -> Result<Option<ReviewItem>, RepositoryError>;
    /// The user's active cards with `due` <= `now`, by (`due`, `id`), strictly
    /// after `after`; only those of `topic` when given.
    async fn list_due(
        &self,
        user: UserId,
        now: OffsetDateTime,
        topic: Option<TopicId>,
        after: Option<DueCursor>,
        limit: u32,
    ) -> Result<Vec<ReviewItem>, RepositoryError>;
    /// Totals of the user's cards, limited to `topic` when given.
    async fn due_stats(
        &self,
        user: UserId,
        now: OffsetDateTime,
        topic: Option<TopicId>,
    ) -> Result<DueStats, RepositoryError>;
    /// The topics with cards due now, ordered by `due_count` descending, then
    /// title, then id.
    async fn due_topics(
        &self,
        user: UserId,
        now: OffsetDateTime,
    ) -> Result<Vec<DueTopic>, RepositoryError>;
    /// Whether the topic exists and belongs to the user.
    async fn topic_exists(&self, user: UserId, topic: TopicId) -> Result<bool, RepositoryError>;
    /// Stores the new schedule and the log in one transaction, only if the
    /// card is still active and still has `expected_review_count` votes.
    /// `weights` is the adoption instant of the user's weights the memory
    /// values were computed with (`None`: the defaults); it is stored on the
    /// card and in both snapshots of the log, so that an undo puts it back.
    /// [`RepositoryError::Conflict`] otherwise (a concurrent vote, a
    /// suspension): nothing is stored then. `NotFound` if the card is gone.
    async fn apply_review(
        &self,
        expected_review_count: u32,
        after: &CardSchedule,
        log: &ReviewLog,
        weights: Option<OffsetDateTime>,
    ) -> Result<(), RepositoryError>;
    /// The votes of the card that were not undone, in the order they were
    /// given (by `reviewed_at`, ties by insertion).
    async fn card_history(&self, card: ReviewCardId) -> Result<Vec<ReviewEvent>, RepositoryError>;
    /// The latest log of the card, undone or not.
    async fn last_log(&self, card_id: ReviewCardId) -> Result<Option<ReviewLog>, RepositoryError>;
    /// Restores `log.before` on the card and marks the log undone at `at`, in
    /// one transaction. [`RepositoryError::Conflict`] if the log is already
    /// undone or the card moved on since (its `review_count` differs from
    /// `log.after`'s). [`RepositoryError::RuleViolated`] if the vote was given
    /// on a previous scheduling model and cannot be put back.
    async fn undo_review(
        &self,
        log_id: ReviewLogId,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError>;
    /// Suspends the card as the learner's choice, at `at` (a no-op if it is
    /// suspended already, whatever the reason: neither reason nor instant
    /// change). `NotFound` if absent or not the user's.
    async fn suspend(
        &self,
        user: UserId,
        id: ReviewCardId,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError>;
    /// Reactivates a card suspended by the learner; one already active is left
    /// as is. [`RepositoryError::Conflict`] if it was suspended because its
    /// record was superseded. `NotFound` if absent or not the user's.
    async fn unsuspend(&self, user: UserId, id: ReviewCardId) -> Result<(), RepositoryError>;
    /// The user's cards suspended by the learner, by (`suspended_at` descending,
    /// `id`), strictly after `after`.
    async fn list_suspended(
        &self,
        user: UserId,
        after: Option<SuspendedCursor>,
        limit: u32,
    ) -> Result<Vec<ReviewItem>, RepositoryError>;
    /// How many cards the user suspended themselves.
    async fn suspended_count(&self, user: UserId) -> Result<u32, RepositoryError>;
    /// The personal weights of the user, `None` when they have none (or the
    /// stored ones cannot be read: the caller then uses the defaults).
    async fn parameters(&self, user: UserId) -> Result<Option<UserWeights>, RepositoryError>;
}

/// What the personal-weights optimization needs (ADR 0014 points 11 and 12).
#[async_trait]
pub trait ReviewOptimizationRepository: Send + Sync {
    /// Users with at least one vote that was not undone, or with saved weights.
    async fn users_with_review_data(&self) -> Result<Vec<UserId>, RepositoryError>;
    /// Every vote of the user that was not undone, grouped by card (cards in
    /// id order, votes in the order given: by `reviewed_at`, ties by insertion).
    async fn card_votes(&self, user: UserId) -> Result<Vec<CardVotes>, RepositoryError>;
    /// The latest attempt, whatever came of it.
    async fn last_attempt(
        &self,
        user: UserId,
    ) -> Result<Option<OptimizationAttempt>, RepositoryError>;
    /// Stores the attempt and, when it adopted weights, `adopted` as the
    /// user's weights (adopted at `attempt.at`), in one transaction.
    async fn record_attempt(
        &self,
        user: UserId,
        attempt: &OptimizationAttempt,
        adopted: Option<&SchedulerParameters>,
    ) -> Result<(), RepositoryError>;
    /// Cards of the user whose memory was not computed with the weights adopted
    /// at `weights` by the current model: the ones a rebuild still has to do.
    async fn cards_out_of_date(
        &self,
        user: UserId,
        weights: OffsetDateTime,
    ) -> Result<Vec<ReviewCardId>, RepositoryError>;
    /// The card's `review_count` and its votes that were not undone, read
    /// together (one snapshot), so the count says which votes the list has.
    async fn card_snapshot(
        &self,
        card: ReviewCardId,
    ) -> Result<(u32, Vec<ReviewEvent>), RepositoryError>;
    /// Only if the card still has `expected_review_count` votes: stores
    /// `memory` (a card without votes has none: then only the stamp is
    /// written), marks it as computed by the current model with the weights
    /// adopted at `weights`. Leaves `due`, `state`, `learning_step` and the
    /// counts alone. Returns whether it wrote.
    async fn rebuild_memory(
        &self,
        card: ReviewCardId,
        expected_review_count: u32,
        memory: Option<MemoryValues>,
        weights: OffsetDateTime,
    ) -> Result<bool, RepositoryError>;
}

/// What the startup migration to the current memory model needs from the
/// cards (ADR 0014 point 9). Across all users: it runs before any request.
#[async_trait]
pub trait ReviewMigrationRepository: Send + Sync {
    /// Cards whose memory values were not produced by the current model.
    async fn cards_to_migrate(&self) -> Result<Vec<ReviewCardId>, RepositoryError>;
    /// The votes of the card that were not undone, in the order they were
    /// given (by `reviewed_at`, ties by insertion).
    async fn vote_history(&self, card: ReviewCardId) -> Result<Vec<ReviewEvent>, RepositoryError>;
    /// In one transaction: stores `memory` (when there is one) on the card,
    /// resets its short-step index and marks it as on the current model with
    /// the default weights (no weights stamp).
    /// `due`, `state`, `last_reviewed_at` and the counts stay as they are.
    /// A card that is already on the current model or gone is left alone.
    async fn mark_migrated(
        &self,
        card: ReviewCardId,
        memory: Option<MemoryValues>,
    ) -> Result<(), RepositoryError>;
}

#[async_trait]
pub trait DocumentRepository: Send + Sync {
    /// Stores the new `document` and its queued import `job` in one
    /// transaction. [`RepositoryError::Conflict`] when the topic already has
    /// the maximum number of documents or an active `document_import`;
    /// [`RepositoryError::MissingParent`] when the topic is gone.
    async fn insert_with_job(&self, document: &Document, job: &Job) -> Result<(), RepositoryError>;
    /// The document with its status as the import job says: `importing`
    /// while the job is queued or running, `failed` (with the job's error)
    /// when it failed, whatever was stored last.
    /// `None` when absent or in another user's topic.
    async fn get(&self, user: UserId, id: DocumentId) -> Result<Option<Document>, RepositoryError>;
    /// Ids of every document the user has, to remove their files when the
    /// user is deleted.
    async fn ids_for_user(&self, user: UserId) -> Result<Vec<DocumentId>, RepositoryError>;
    /// Sum of `size_bytes` of every document the user has, whatever its
    /// status: what the storage quota counts.
    async fn total_bytes_for_user(&self, user: UserId) -> Result<u64, RepositoryError>;
    /// The topic's documents, newest first, with sections for the ready ones.
    async fn list(&self, topic_id: TopicId) -> Result<Vec<DocumentView>, RepositoryError>;
    /// A run of the import starts (also after a retry): `importing`, no error.
    async fn mark_importing(&self, id: DocumentId) -> Result<(), RepositoryError>;
    /// Replaces pages and sections, creates the `document` resource and marks
    /// the document `ready`, in one transaction (a run repeated after a
    /// crash leaves one copy). [`RepositoryError::NotFound`] when the
    /// document was deleted meanwhile.
    async fn complete(
        &self,
        import: &ImportedDocument,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError>;
    /// Records why the import failed.
    async fn fail(&self, id: DocumentId, error: &JobError) -> Result<(), RepositoryError>;
    /// Deletes the document with its pages, sections and resource.
    /// [`RepositoryError::Conflict`] while its import is queued or running;
    /// [`RepositoryError::NotFound`] when absent or not the user's.
    async fn delete(&self, user: UserId, id: DocumentId) -> Result<(), RepositoryError>;
    /// The text of pages `first..=last` (from 1), in order.
    async fn pages(
        &self,
        id: DocumentId,
        first: u32,
        last: u32,
    ) -> Result<Vec<(u32, String)>, RepositoryError>;
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    /// [`RepositoryError::Duplicate`] when the username is taken.
    async fn insert(&self, user: &NewUser) -> Result<(), RepositoryError>;
    async fn get(&self, id: UserId) -> Result<Option<User>, RepositoryError>;
    /// The user with the stored password hash. `name` is already in lookup
    /// form ([`Username::lookup_key`]).
    async fn credentials(&self, name: &str) -> Result<Option<UserCredentials>, RepositoryError>;
    async fn by_username(&self, name: &Username) -> Result<Option<User>, RepositoryError>;
    /// Every user, by username.
    async fn list(&self) -> Result<Vec<User>, RepositoryError>;
    /// The stored hash, `None` when the account has no password.
    async fn password_hash(
        &self,
        id: UserId,
    ) -> Result<Option<StoredPasswordHash>, RepositoryError>;
    async fn set_password_hash(
        &self,
        id: UserId,
        hash: &StoredPasswordHash,
    ) -> Result<(), RepositoryError>;
    /// [`RepositoryError::Duplicate`] when the new name is taken.
    async fn rename(&self, id: UserId, name: &Username) -> Result<(), RepositoryError>;
    /// `Some(at)` disables, `None` enables.
    async fn set_disabled(
        &self,
        id: UserId,
        at: Option<OffsetDateTime>,
    ) -> Result<(), RepositoryError>;
    async fn llm_key(&self, id: UserId) -> Result<Option<SealedSecret>, RepositoryError>;
    /// `None` removes the key.
    async fn set_llm_key(
        &self,
        id: UserId,
        key: Option<&SealedSecret>,
    ) -> Result<(), RepositoryError>;
    /// Sets the key only if the user has none; whether it was set.
    async fn set_llm_key_if_missing(
        &self,
        id: UserId,
        key: &SealedSecret,
    ) -> Result<bool, RepositoryError>;
    async fn set_last_login(&self, id: UserId, at: OffsetDateTime) -> Result<(), RepositoryError>;
    /// Saves the read-aloud voice the user chose (not checked here).
    async fn set_tts_voice(&self, id: UserId, voice: &str) -> Result<(), RepositoryError>;
    /// Deletes the user and, by cascade, everything they own. `false` if absent.
    async fn delete(&self, id: UserId) -> Result<bool, RepositoryError>;
}

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn insert(&self, session: &Session) -> Result<(), RepositoryError>;
    async fn by_digest(&self, digest: &TokenDigest) -> Result<Option<Session>, RepositoryError>;
    /// Moves `last_seen_at` and `expires_at`.
    async fn renew(
        &self,
        id: SessionId,
        last_seen_at: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<(), RepositoryError>;
    /// `false` if absent.
    async fn revoke(&self, id: SessionId) -> Result<bool, RepositoryError>;
    /// Revokes the user's sessions except `keep`; how many were revoked.
    async fn revoke_all(
        &self,
        user: UserId,
        keep: Option<SessionId>,
    ) -> Result<u64, RepositoryError>;
    /// Removes the sessions that expired before `now`; how many.
    async fn delete_expired(&self, now: OffsetDateTime) -> Result<u64, RepositoryError>;
}

/// Tokens the model providers billed to each user, per UTC day: the numbers
/// behind the daily budget (`TEACH_ME_USER_DAILY_LLM_TOKENS`).
#[async_trait]
pub trait LlmUsageRepository: Send + Sync {
    /// Input + output tokens counted for `user` on `day` (0 when none).
    async fn tokens_on(&self, user: UserId, day: time::Date) -> Result<u64, RepositoryError>;
    /// Adds `tokens` to the user's count of `day`. Atomic: concurrent calls
    /// do not lose increments.
    async fn add_tokens(
        &self,
        user: UserId,
        day: time::Date,
        tokens: u64,
    ) -> Result<(), RepositoryError>;
}
