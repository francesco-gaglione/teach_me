//! Background jobs (`docs/mvp.md` §3 and §7): long operations that outlive a
//! request and report their progress. The runner is generic over `kind`.

use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;

use crate::ContentLanguage;
use crate::ids::{DocumentId, JobId, LessonId, TopicId};

/// What a job does. The string forms are part of the API contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobKind {
    /// Find and annotate sources for the topic's mission.
    Research,
    /// Plan, write and validate one lesson.
    LessonGeneration,
    /// Grade what is pending and record what the learner showed.
    LessonCompletion,
    /// Extract the text of an uploaded PDF.
    DocumentImport,
}

impl JobKind {
    /// Whether running the job calls the model (and so needs the owner's key).
    #[must_use]
    pub const fn needs_llm(self) -> bool {
        !matches!(self, Self::DocumentImport)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Research => "research",
            Self::LessonGeneration => "lesson_generation",
            Self::LessonCompletion => "lesson_completion",
            Self::DocumentImport => "document_import",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown job value `{0}`")]
pub struct UnknownJobValue(pub String);

impl FromStr for JobKind {
    type Err = UnknownJobValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "research" => Ok(Self::Research),
            "lesson_generation" => Ok(Self::LessonGeneration),
            "lesson_completion" => Ok(Self::LessonCompletion),
            "document_import" => Ok(Self::DocumentImport),
            other => Err(UnknownJobValue(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl JobStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    /// Queued and running jobs count against the one-active-job-per-(topic,
    /// kind) rule.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

impl FromStr for JobStatus {
    type Err = UnknownJobValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => Err(UnknownJobValue(other.to_owned())),
        }
    }
}

/// Longest `message` a progress report may carry (contract: `jobs.md` §2).
pub const PROGRESS_MESSAGE_MAX_CHARS: usize = 80;

/// Where a running job is. `step` is a stable machine-readable label chosen by
/// the handler; `message` is an optional short sentence about something that
/// already happened; `done`/`total` are optional counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobProgress {
    pub step: String,
    pub message: Option<String>,
    pub done: Option<u32>,
    pub total: Option<u32>,
}

impl JobProgress {
    #[must_use]
    pub fn step(step: &str) -> Self {
        Self {
            step: step.to_owned(),
            message: None,
            done: None,
            total: None,
        }
    }

    /// Cut to [`PROGRESS_MESSAGE_MAX_CHARS`] (with an ellipsis) so a long
    /// search query can never break the contract.
    #[must_use]
    pub fn with_message(mut self, message: &str) -> Self {
        let message = message.split_whitespace().collect::<Vec<_>>().join(" ");
        self.message = Some(if message.chars().count() > PROGRESS_MESSAGE_MAX_CHARS {
            let kept: String = message
                .chars()
                .take(PROGRESS_MESSAGE_MAX_CHARS - 1)
                .collect();
            format!("{}…", kept.trim_end())
        } else {
            message
        });
        self
    }

    #[must_use]
    pub fn with_done(mut self, done: u32) -> Self {
        self.done = Some(done);
        self
    }
}

/// What a succeeded job produced. The shape depends on the job's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobResult {
    Research {
        resource_count: u32,
        gap_count: u32,
    },
    LessonGeneration {
        lesson_id: LessonId,
        lesson_number: u32,
    },
    LessonCompletion {
        lesson_id: LessonId,
        record_count: u32,
        term_count: u32,
        review_card_count: u32,
    },
    DocumentImport {
        document_id: DocumentId,
        page_count: u32,
        section_count: u32,
    },
}

/// Why a job failed. The string forms are part of the API contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobErrorCode {
    /// The job exceeded its time budget.
    Timeout,
    /// The backend stopped while the job was running, too many times.
    Interrupted,
    UpstreamFailed,
    RateLimited,
    /// The model's output did not pass validation, even after a repair.
    InvalidOutput,
    Internal,
    /// `document_import`: the file does not open as a PDF.
    InvalidPdf,
    /// `document_import`: the PDF is protected by a password.
    EncryptedPdf,
    /// `document_import`: more pages than the limit.
    TooManyPages,
    /// `document_import`: a scan, or nearly (no selectable text).
    NoText,
    /// The topic's owner has no LLM key (never set, or removed).
    LlmKeyMissing,
    /// The provider refused the owner's key (401/403).
    LlmKeyRejected,
    /// The provider accepts the key but the account has no credit left (402).
    LlmCreditExhausted,
    /// The owner spent the day's token budget before the job started.
    LlmBudgetExceeded,
}

impl JobErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Interrupted => "interrupted",
            Self::UpstreamFailed => "upstream_failed",
            Self::RateLimited => "rate_limited",
            Self::InvalidOutput => "invalid_output",
            Self::Internal => "internal",
            Self::InvalidPdf => "invalid_pdf",
            Self::EncryptedPdf => "encrypted_pdf",
            Self::TooManyPages => "too_many_pages",
            Self::NoText => "no_text",
            Self::LlmKeyMissing => "llm_key_missing",
            Self::LlmKeyRejected => "llm_key_rejected",
            Self::LlmCreditExhausted => "llm_credit_exhausted",
            Self::LlmBudgetExceeded => "llm_budget_exceeded",
        }
    }
}

impl FromStr for JobErrorCode {
    type Err = UnknownJobValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "timeout" => Ok(Self::Timeout),
            "interrupted" => Ok(Self::Interrupted),
            "upstream_failed" => Ok(Self::UpstreamFailed),
            "rate_limited" => Ok(Self::RateLimited),
            "invalid_output" => Ok(Self::InvalidOutput),
            "internal" => Ok(Self::Internal),
            "invalid_pdf" => Ok(Self::InvalidPdf),
            "encrypted_pdf" => Ok(Self::EncryptedPdf),
            "too_many_pages" => Ok(Self::TooManyPages),
            "no_text" => Ok(Self::NoText),
            "llm_key_missing" => Ok(Self::LlmKeyMissing),
            "llm_key_rejected" => Ok(Self::LlmKeyRejected),
            "llm_credit_exhausted" => Ok(Self::LlmCreditExhausted),
            "llm_budget_exceeded" => Ok(Self::LlmBudgetExceeded),
            other => Err(UnknownJobValue(other.to_owned())),
        }
    }
}

/// Error attached to a failed job. The text is fixed per code so nothing
/// internal can reach the client (same rule as chat errors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    pub code: JobErrorCode,
    pub message: String,
}

impl JobError {
    #[must_use]
    /// The text is in the language of the job's topic (ADR 0019).
    pub fn from_code(code: JobErrorCode, language: ContentLanguage) -> Self {
        let (it, en) = match code {
            JobErrorCode::Timeout => (
                "L'operazione ha impiegato troppo tempo, riprova.",
                "The operation took too long, try again.",
            ),
            JobErrorCode::Interrupted => (
                "L'operazione è stata interrotta, riprova.",
                "The operation was interrupted, try again.",
            ),
            JobErrorCode::UpstreamFailed => (
                "Il servizio esterno non risponde, riprova.",
                "The external service is not responding, try again.",
            ),
            JobErrorCode::RateLimited => (
                "Troppe richieste al servizio esterno, riprova tra poco.",
                "Too many requests to the external service, try again shortly.",
            ),
            JobErrorCode::InvalidOutput => (
                "Non sono riuscito a ottenere un risultato valido, riprova.",
                "I could not get a valid result, try again.",
            ),
            JobErrorCode::Internal => ("Errore interno.", "Internal error."),
            JobErrorCode::InvalidPdf => (
                "Il file non è un PDF valido.",
                "The file is not a valid PDF.",
            ),
            JobErrorCode::EncryptedPdf => (
                "Il PDF è protetto da password.",
                "The PDF is password protected.",
            ),
            JobErrorCode::TooManyPages => (
                "Il PDF ha troppe pagine (massimo 1.500).",
                "The PDF has too many pages (1,500 at most).",
            ),
            JobErrorCode::NoText => (
                "Il PDF non contiene testo selezionabile (forse è una scansione).",
                "The PDF has no selectable text (it may be a scan).",
            ),
            JobErrorCode::LlmKeyMissing => (
                "Manca la chiave del servizio LLM: inseriscila nelle Impostazioni e riprova.",
                "The LLM service key is missing: enter it in Settings and try again.",
            ),
            JobErrorCode::LlmKeyRejected => (
                "Il servizio LLM ha rifiutato la chiave: controllala nelle Impostazioni.",
                "The LLM service rejected the key: check it in Settings.",
            ),
            JobErrorCode::LlmCreditExhausted => (
                "Il credito del servizio LLM è esaurito: ricaricalo presso il provider e riprova.",
                "The LLM service credit is exhausted: top it up with the provider and try again.",
            ),
            JobErrorCode::LlmBudgetExceeded => (
                "Hai esaurito il budget giornaliero di token: riprova dopo mezzanotte (UTC).",
                "You have used up the daily token budget: try again after midnight (UTC).",
            ),
        };
        Self {
            code,
            message: language.pick(it, en).to_owned(),
        }
    }
}

/// The requested transition is not allowed from the job's current status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JobRuleError {
    #[error("job is not queued")]
    NotQueued,
    #[error("job is not running")]
    NotRunning,
    #[error("only a failed job can be retried")]
    NotFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: JobId,
    pub topic_id: TopicId,
    /// Only `lesson_completion` jobs: the lesson being closed. Their
    /// one-active-job rule is per lesson instead of per topic.
    pub lesson_id: Option<LessonId>,
    /// Only `document_import` jobs: the document being imported.
    pub document_id: Option<DocumentId>,
    pub kind: JobKind,
    pub status: JobStatus,
    pub progress: Option<JobProgress>,
    pub error: Option<JobError>,
    /// `Some` once the job succeeded.
    pub result: Option<JobResult>,
    /// How many times the current run was started (0 while never started).
    /// A manual retry begins a new run.
    pub attempt: u32,
    pub created_at: OffsetDateTime,
    pub started_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
}

impl Job {
    #[must_use]
    pub fn queued(id: JobId, topic_id: TopicId, kind: JobKind, now: OffsetDateTime) -> Self {
        Self {
            id,
            topic_id,
            lesson_id: None,
            document_id: None,
            kind,
            status: JobStatus::Queued,
            progress: None,
            error: None,
            result: None,
            attempt: 0,
            created_at: now,
            started_at: None,
            finished_at: None,
        }
    }

    /// A queued job about one lesson (`lesson_completion`).
    #[must_use]
    pub fn queued_for_lesson(
        id: JobId,
        topic_id: TopicId,
        lesson_id: LessonId,
        kind: JobKind,
        now: OffsetDateTime,
    ) -> Self {
        let mut job = Self::queued(id, topic_id, kind, now);
        job.lesson_id = Some(lesson_id);
        job
    }

    /// A queued job about one uploaded document (`document_import`).
    #[must_use]
    pub fn queued_for_document(
        id: JobId,
        topic_id: TopicId,
        document_id: DocumentId,
        now: OffsetDateTime,
    ) -> Self {
        let mut job = Self::queued(id, topic_id, JobKind::DocumentImport, now);
        job.document_id = Some(document_id);
        job
    }

    /// A runner picked the job up.
    pub fn start(&mut self, now: OffsetDateTime) -> Result<(), JobRuleError> {
        if self.status != JobStatus::Queued {
            return Err(JobRuleError::NotQueued);
        }
        self.status = JobStatus::Running;
        self.attempt += 1;
        self.started_at = Some(now);
        self.progress = None;
        Ok(())
    }

    pub fn set_progress(&mut self, progress: JobProgress) -> Result<(), JobRuleError> {
        self.require_running()?;
        self.progress = Some(progress);
        Ok(())
    }

    pub fn succeed(&mut self, result: JobResult, now: OffsetDateTime) -> Result<(), JobRuleError> {
        self.require_running()?;
        self.status = JobStatus::Succeeded;
        self.result = Some(result);
        self.finished_at = Some(now);
        Ok(())
    }

    pub fn fail(
        &mut self,
        code: JobErrorCode,
        language: ContentLanguage,
        now: OffsetDateTime,
    ) -> Result<(), JobRuleError> {
        self.require_running()?;
        self.status = JobStatus::Failed;
        self.error = Some(JobError::from_code(code, language));
        self.finished_at = Some(now);
        Ok(())
    }

    /// The backend stopped mid-run: back to the queue, keeping `attempt` so
    /// repeated crashes are counted.
    pub fn requeue_interrupted(&mut self) -> Result<(), JobRuleError> {
        self.require_running()?;
        self.status = JobStatus::Queued;
        self.progress = None;
        self.started_at = None;
        Ok(())
    }

    /// Manual retry of a failed job: same row, a fresh run.
    pub fn retry(&mut self) -> Result<(), JobRuleError> {
        if self.status != JobStatus::Failed {
            return Err(JobRuleError::NotFailed);
        }
        self.status = JobStatus::Queued;
        self.attempt = 0;
        self.progress = None;
        self.error = None;
        self.result = None;
        self.started_at = None;
        self.finished_at = None;
        Ok(())
    }

    fn require_running(&self) -> Result<(), JobRuleError> {
        if self.status == JobStatus::Running {
            Ok(())
        } else {
            Err(JobRuleError::NotRunning)
        }
    }
}

impl fmt::Display for JobKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Keyset position in the job ordering (`created_at`, then `id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobCursor {
    pub created_at: OffsetDateTime,
    pub id: JobId,
}

impl JobCursor {
    #[must_use]
    pub fn after(job: &Job) -> Self {
        Self {
            created_at: job.created_at,
            id: job.id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-09 10:00:00 UTC);

    #[test]
    fn credit_exhausted_code_round_trips_and_has_the_fixed_message() {
        let code = JobErrorCode::LlmCreditExhausted;
        assert_eq!(code.as_str(), "llm_credit_exhausted");
        assert_eq!("llm_credit_exhausted".parse::<JobErrorCode>(), Ok(code));
        assert_eq!(
            JobError::from_code(code, ContentLanguage::It).message,
            "Il credito del servizio LLM è esaurito: ricaricalo presso il provider e riprova."
        );
        assert_eq!(
            JobError::from_code(code, ContentLanguage::En).message,
            "The LLM service credit is exhausted: top it up with the provider and try again."
        );
        assert_ne!(code, JobErrorCode::LlmKeyRejected);
    }

    const RESULT: JobResult = JobResult::Research {
        resource_count: 3,
        gap_count: 1,
    };

    fn job() -> Job {
        Job::queued(
            JobId::new(Uuid::from_u128(1)),
            TopicId::new(Uuid::from_u128(2)),
            JobKind::Research,
            T0,
        )
    }

    #[test]
    fn happy_path_counts_the_attempt() {
        let mut j = job();
        j.start(T0).unwrap();
        assert_eq!((j.status, j.attempt), (JobStatus::Running, 1));
        j.set_progress(JobProgress::step("searching").with_done(1))
            .unwrap();
        j.succeed(RESULT, T0).unwrap();
        assert_eq!(j.result, Some(RESULT));
        assert_eq!(j.status, JobStatus::Succeeded);
        assert_eq!(j.finished_at, Some(T0));
        assert_eq!(j.start(T0), Err(JobRuleError::NotQueued));
    }

    #[test]
    fn interrupted_run_is_queued_again_and_keeps_the_count() {
        let mut j = job();
        j.start(T0).unwrap();
        j.requeue_interrupted().unwrap();
        assert_eq!(
            (j.status, j.attempt, j.started_at),
            (JobStatus::Queued, 1, None)
        );
        j.start(T0).unwrap();
        assert_eq!(j.attempt, 2);
    }

    #[test]
    fn only_a_failed_job_can_be_retried_and_it_starts_a_new_run() {
        let mut j = job();
        assert_eq!(j.retry(), Err(JobRuleError::NotFailed));
        j.start(T0).unwrap();
        j.fail(JobErrorCode::Timeout, ContentLanguage::It, T0)
            .unwrap();
        assert_eq!(j.error.as_ref().unwrap().code, JobErrorCode::Timeout);
        j.retry().unwrap();
        assert_eq!((j.status, j.attempt), (JobStatus::Queued, 0));
        assert!(j.error.is_none() && j.finished_at.is_none());
    }

    #[test]
    fn progress_and_finish_need_a_running_job() {
        let mut j = job();
        assert_eq!(j.succeed(RESULT, T0), Err(JobRuleError::NotRunning));
        assert_eq!(
            j.fail(JobErrorCode::Internal, ContentLanguage::It, T0),
            Err(JobRuleError::NotRunning)
        );
    }

    #[test]
    fn string_forms_round_trip() {
        for s in ["queued", "running", "succeeded", "failed"] {
            assert_eq!(s.parse::<JobStatus>().unwrap().as_str(), s);
        }
        for s in [
            "timeout",
            "interrupted",
            "upstream_failed",
            "rate_limited",
            "invalid_output",
            "internal",
            "invalid_pdf",
            "encrypted_pdf",
            "too_many_pages",
            "no_text",
        ] {
            assert_eq!(s.parse::<JobErrorCode>().unwrap().as_str(), s);
        }
        assert!("nope".parse::<JobKind>().is_err());
    }

    #[test]
    fn progress_messages_are_one_line_and_at_most_80_characters() {
        let long = "è ".repeat(60);
        let p = JobProgress::step("searching").with_message(&format!("Ho cercato:\n{long}"));
        let m = p.message.unwrap();
        assert!(m.chars().count() <= PROGRESS_MESSAGE_MAX_CHARS);
        assert!(m.ends_with('…') && !m.contains('\n'));
        let short = JobProgress::step("x").with_message("Ho letto la mission");
        assert_eq!(short.message.as_deref(), Some("Ho letto la mission"));
    }
}
