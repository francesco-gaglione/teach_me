use domain::{IssueKind, RepositoryError, ValidationIssue};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// One or more input problems, each attributed to a field.
    #[error("validation failed ({} issue(s))", .0.len())]
    Validation(Vec<ValidationIssue>),
    /// Input problems whose field names are built at run time (an item of a
    /// list: `attempts[2].answer`).
    #[error("validation failed ({} issue(s))", .0.len())]
    InvalidFields(Vec<FieldIssue>),
    #[error("not found")]
    NotFound,
    /// The topic's owner has no LLM key: nothing that needs the model starts.
    #[error("the user has no LLM key")]
    LlmKeyMissing,
    /// The provider refused the owner's key.
    #[error("the LLM key was rejected")]
    LlmKeyRejected,
    /// The key is fine but the provider account has no credit left.
    #[error("the LLM credit is exhausted")]
    LlmCreditExhausted,
    /// The user spent the day's LLM tokens (`TEACH_ME_USER_DAILY_LLM_TOKENS`);
    /// the budget renews at 00:00 UTC.
    #[error("the daily LLM budget is exhausted")]
    LlmBudgetExceeded { retry_after_secs: u64 },
    /// The upload would take the user's documents over their storage quota.
    #[error("the storage quota is exceeded")]
    StorageQuotaExceeded,
    /// No valid session behind the request.
    #[error("unauthorized")]
    Unauthorized,
    /// Wrong name or password, or a disabled account: the three are one answer.
    #[error("invalid credentials")]
    InvalidCredentials,
    /// Too many failed logins for the name; retry after this long.
    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },
    /// The resource is in a state that does not allow the operation.
    #[error("conflict: {0:?}")]
    Conflict(ConflictReason),
    /// Unexpected storage failure; never shown to the client.
    #[error("storage failure: {0}")]
    Storage(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// The review scheduler rejected its inputs; a server-side defect, never
    /// shown to the client.
    #[error("scheduler failure: {0}")]
    Scheduler(#[from] domain::SchedulerError),
}

/// A problem attributed to a field named at run time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldIssue {
    pub field: String,
    pub issue: IssueKind,
}

impl FieldIssue {
    #[must_use]
    pub fn new(field: impl Into<String>, issue: IssueKind) -> Self {
        Self {
            field: field.into(),
            issue,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictReason {
    /// The topic already has a reply being generated.
    TurnInProgress,
    /// Only the last assistant message, if failed, can be retried.
    NotRetryable,
    /// The version to confirm is not the topic's open draft (already
    /// confirmed, superseded, or never existed).
    MissionNotOpen,
    /// The topic's state does not allow confirming the mission.
    MissionStateMismatch,
    /// Only a failed job can be retried, and only while no other job of its
    /// kind is active for the topic.
    JobNotRetryable,
    /// The topic already has an active job of that kind.
    JobActive,
    /// The operation needs a confirmed mission and the topic has none.
    NoConfirmedMission,
    /// Lessons are generated only for a topic whose sources are ready.
    TopicNotReady,
    /// The lesson is completed: it takes no more attempts and no second
    /// completion.
    LessonCompleted,
    /// A suspended card is not voted on.
    CardSuspended,
    /// The vote is on an old version of the card: it was already applied, or
    /// the card changed since the app read it.
    StaleReview,
    /// The card was suspended because its record was superseded: it is not
    /// reactivated.
    CardSuperseded,
    /// The card has no vote to undo (none, or the last one is undone already).
    NothingToUndo,
    /// The last vote is older than the undo window.
    UndoExpired,
    /// The topic already has the maximum number of documents.
    DocumentLimit,
    /// The document's import is queued or running.
    ImportInProgress,
    /// Read-aloud is off on this server (no speech engine configured).
    ReadAloudOff,
}

impl AppError {
    pub(crate) fn validation(issue: ValidationIssue) -> Self {
        Self::Validation(vec![issue])
    }
}

/// Repository failures that a use case did not translate explicitly are
/// treated as unexpected.
impl From<RepositoryError> for AppError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::NotFound => Self::NotFound,
            other => Self::Storage(Box::new(other)),
        }
    }
}
