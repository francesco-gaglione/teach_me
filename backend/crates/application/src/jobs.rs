//! Background jobs: the use cases around the job table ([`JobService`]) and
//! the runner that executes them ([`JobRunner`]).
//!
//! The runner knows nothing about what a job does: each `kind` has a
//! [`JobHandler`] registered by the composition root. A kind without a handler
//! is never claimed, so its jobs simply wait in the queue (until a build that
//! has the handler starts).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use domain::{
    Clock, ContentLanguage, DocumentId, IdGenerator, Job, JobCursor, JobError, JobErrorCode, JobId,
    JobKind, JobProgress, JobRepository, JobResult, JobStatus, LessonId, Page, PageRequest,
    RepositoryError, TopicEvent, TopicEventPayload, TopicEventPublisher, TopicId, TopicRepository,
    UserId,
};
use tokio::sync::{Mutex, Notify, Semaphore};
use tracing::Instrument as _;

use crate::error::{AppError, ConflictReason};
use crate::llm_keys::{KeyError, LlmKeys, with_key};
use crate::paging::paginate;
use crate::time_util::now_millis;

/// Knobs of the runner. The defaults suit a single user.
#[derive(Debug, Clone, Copy)]
pub struct JobSettings {
    /// Jobs executing at the same time (across topics).
    pub max_concurrent: usize,
    /// Time budget of one run; the job fails with `timeout` when exceeded.
    pub job_timeout: Duration,
    /// A job found `running` at startup is queued again unless it already
    /// started this many times (it probably crashes the backend).
    pub max_attempts: u32,
    /// First wait before storing a job's outcome again after a storage
    /// failure; doubles on each of the (three) attempts.
    pub store_retry_delay: Duration,
}

impl Default for JobSettings {
    fn default() -> Self {
        Self {
            max_concurrent: 2,
            // A lesson job is a Planner call and a Generator call of up to
            // 20k output tokens, plus up to two rewrites of the document.
            job_timeout: Duration::from_mins(15),
            max_attempts: 3,
            store_retry_delay: Duration::from_millis(500),
        }
    }
}

/// Outcome of settling the jobs a previous run left `running`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Recovery {
    pub requeued: u64,
    pub failed: u64,
}

pub struct JobService {
    jobs: Arc<dyn JobRepository>,
    topics: Arc<dyn TopicRepository>,
    events: Arc<dyn TopicEventPublisher>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    keys: Arc<LlmKeys>,
    /// Wakes the runner when there may be something to claim.
    wake: Notify,
}

impl JobService {
    #[must_use]
    pub fn new(
        jobs: Arc<dyn JobRepository>,
        topics: Arc<dyn TopicRepository>,
        events: Arc<dyn TopicEventPublisher>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        keys: Arc<LlmKeys>,
    ) -> Self {
        Self {
            jobs,
            topics,
            events,
            clock,
            ids,
            keys,
            wake: Notify::new(),
        }
    }

    /// A new queued job, not stored yet. Callers that must create it together
    /// with something else (the mission confirmation) store it in their own
    /// transaction and then call [`announce`](Self::announce).
    #[must_use]
    pub fn new_job(&self, topic_id: TopicId, kind: JobKind) -> Job {
        Job::queued(
            JobId::new(self.ids.new_id()),
            topic_id,
            kind,
            now_millis(self.clock.as_ref()),
        )
    }

    /// A new queued job about a lesson, not stored yet (see [`new_job`](Self::new_job)).
    #[must_use]
    pub fn new_lesson_job(&self, topic_id: TopicId, lesson_id: LessonId, kind: JobKind) -> Job {
        Job::queued_for_lesson(
            JobId::new(self.ids.new_id()),
            topic_id,
            lesson_id,
            kind,
            now_millis(self.clock.as_ref()),
        )
    }

    /// A new queued `document_import` job, not stored yet: it is stored with
    /// its document, in one transaction (see [`new_job`](Self::new_job)).
    #[must_use]
    pub fn new_document_job(&self, topic_id: TopicId, document_id: DocumentId) -> Job {
        Job::queued_for_document(
            JobId::new(self.ids.new_id()),
            topic_id,
            document_id,
            now_millis(self.clock.as_ref()),
        )
    }

    /// A stored job is ready to run: tells the listeners and wakes the runner.
    pub fn announce(&self, job: &Job) {
        self.publish(job);
        self.wake.notify_one();
    }

    /// Stores a new queued job of `kind` and announces it. `Conflict` when the
    /// topic already has an active job of that kind; `llm_key_missing` (and
    /// no job) when the kind needs the model and the owner has no key.
    pub async fn enqueue(
        &self,
        user: UserId,
        topic_id: TopicId,
        kind: JobKind,
    ) -> Result<Job, AppError> {
        self.require_topic(user, topic_id).await?;
        if kind.needs_llm() {
            self.keys.require(user).await?;
        }
        let job = self.new_job(topic_id, kind);
        self.jobs.insert(&job).await.map_err(|e| match e {
            RepositoryError::Conflict => AppError::Conflict(ConflictReason::JobActive),
            RepositoryError::MissingParent | RepositoryError::NotFound => AppError::NotFound,
            other => other.into(),
        })?;
        self.announce(&job);
        Ok(job)
    }

    /// The most recent job of `kind` for the topic, whatever its status.
    /// Language of a topic's texts; Italian when the topic cannot be read
    /// (deleted meanwhile, storage down), because a message must still be
    /// produced and Italian is the default language.
    pub(crate) async fn language_of(&self, topic_id: TopicId) -> ContentLanguage {
        match self.topics.get_system(topic_id).await {
            Ok(Some(topic)) => topic.language,
            Ok(None) => ContentLanguage::default(),
            Err(e) => {
                tracing::warn!(error = %e, "cannot read the topic language of a job");
                ContentLanguage::default()
            }
        }
    }

    pub async fn latest(&self, topic_id: TopicId, kind: JobKind) -> Result<Option<Job>, AppError> {
        self.latest_matching(topic_id, kind, None).await
    }

    /// Like [`latest`](Self::latest) for the jobs of one lesson, when
    /// `lesson_id` is given (`lesson_completion` is per lesson, not per
    /// topic). One targeted query, however many jobs the topic has.
    pub async fn latest_matching(
        &self,
        topic_id: TopicId,
        kind: JobKind,
        lesson_id: Option<LessonId>,
    ) -> Result<Option<Job>, AppError> {
        Ok(self.jobs.latest(topic_id, kind, lesson_id).await?)
    }

    /// Newest job first.
    pub async fn list(
        &self,
        user: UserId,
        topic_id: TopicId,
        request: PageRequest<JobCursor>,
    ) -> Result<Page<Job, JobCursor>, AppError> {
        self.require_topic(user, topic_id).await?;
        paginate(
            request,
            |after, limit| async move { self.jobs.list(topic_id, after, limit).await },
            JobCursor::after,
        )
        .await
        .map_err(AppError::from)
    }

    /// Puts a failed job back in the queue for a fresh run.
    pub async fn retry(
        &self,
        user: UserId,
        topic_id: TopicId,
        job_id: JobId,
    ) -> Result<Job, AppError> {
        self.require_topic(user, topic_id).await?;
        let mut job = self
            .jobs
            .get(user, topic_id, job_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if job.kind.needs_llm() {
            self.keys.require(user).await?;
        }
        job.retry()
            .map_err(|_| AppError::Conflict(ConflictReason::JobNotRetryable))?;
        // Only the newest job of a kind can be retried: an older failure was
        // superseded by a later run.
        // A document has exactly one import job, which is also its newest:
        // another document's job says nothing about it. The one-active-import
        // rule is enforced by the update below.
        if job.document_id.is_none()
            && self
                .latest_matching(topic_id, job.kind, job.lesson_id)
                .await?
                .is_some_and(|newest| newest.id != job.id)
        {
            return Err(AppError::Conflict(ConflictReason::JobNotRetryable));
        }
        self.jobs
            .update(&job, JobStatus::Failed)
            .await
            .map_err(|e| match e {
                // Not failed any more, or a newer job of the kind took the slot.
                RepositoryError::Conflict => AppError::Conflict(ConflictReason::JobNotRetryable),
                RepositoryError::NotFound => AppError::NotFound,
                other => other.into(),
            })?;
        self.announce(&job);
        Ok(job)
    }

    /// Startup recovery: a job left `running` was interrupted by a restart.
    ///
    /// It goes back to the queue (a restart is not the job's fault and the
    /// topic would otherwise sit in `researching` until the user notices and
    /// retries), unless it already started `max_attempts` times: then it is
    /// failed with `interrupted`, so a job that crashes the process cannot
    /// loop forever. Handlers must therefore be safe to run again.
    pub async fn recover_interrupted(&self, max_attempts: u32) -> Result<Recovery, AppError> {
        let now = now_millis(self.clock.as_ref());
        let mut report = Recovery::default();
        for mut job in self.jobs.running().await? {
            let outcome = if job.attempt < max_attempts {
                job.requeue_interrupted()
            } else {
                let language = self.language_of(job.topic_id).await;
                job.fail(JobErrorCode::Interrupted, language, now)
            };
            if outcome.is_err() {
                continue;
            }
            match self.jobs.update(&job, JobStatus::Running).await {
                Ok(()) => {}
                // Settled by someone else, or its topic was deleted meanwhile.
                Err(RepositoryError::Conflict | RepositoryError::NotFound) => continue,
                Err(e) => return Err(e.into()),
            }
            if job.status == JobStatus::Queued {
                report.requeued += 1;
            } else {
                report.failed += 1;
            }
            self.publish(&job);
        }
        if report.requeued > 0 {
            self.wake.notify_one();
        }
        Ok(report)
    }

    async fn require_topic(&self, user: UserId, id: TopicId) -> Result<(), AppError> {
        self.topics
            .get(user, id)
            .await?
            .map(|_| ())
            .ok_or(AppError::NotFound)
    }

    fn publish(&self, job: &Job) {
        self.events.publish(TopicEvent {
            topic_id: job.topic_id,
            at: now_millis(self.clock.as_ref()),
            payload: TopicEventPayload::JobUpdated(job.clone()),
        });
    }
}

/// Why a handler gave up. Only the code is kept: the text the client sees is
/// fixed per code, details belong in the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobFailure(pub JobErrorCode);

/// What a handler sees of the job it is running.
pub struct JobContext {
    job: Mutex<Job>,
    service: Arc<JobService>,
    language: ContentLanguage,
}

impl JobContext {
    fn new(job: Job, service: Arc<JobService>, language: ContentLanguage) -> Self {
        Self {
            job: Mutex::new(job),
            service,
            language,
        }
    }

    /// Language of the job's topic: the texts shown to the person (progress
    /// messages, error messages) are written in it (ADR 0019).
    #[must_use]
    pub fn language(&self) -> ContentLanguage {
        self.language
    }

    pub async fn job(&self) -> Job {
        self.job.lock().await.clone()
    }

    /// Stores and publishes the progress. A failure to do so is logged, never
    /// fatal: losing a progress tick must not lose the work.
    pub async fn progress(&self, progress: JobProgress) {
        let mut job = self.job.lock().await;
        if job.set_progress(progress).is_err() {
            return;
        }
        match self.service.jobs.update(&job, JobStatus::Running).await {
            Ok(()) => self.service.publish(&job),
            Err(e) => tracing::warn!(job = %job.id, error = %e, "cannot store job progress"),
        }
    }
}

/// The work of one job kind. May run more than once for the same job (after a
/// restart or a retry), so it must be idempotent.
#[async_trait]
pub trait JobHandler: Send + Sync {
    /// Whether the job calls the model. The runner then resolves the key of
    /// the topic's owner and runs the job with it, or fails the job with
    /// `llm_key_missing` without running it.
    fn needs_llm(&self) -> bool {
        false
    }

    async fn run(&self, ctx: &JobContext) -> Result<JobResult, JobFailure>;

    /// Called when a run ends in failure (the handler's own error, a timeout,
    /// a panic), just before the failure is stored. Best effort: it cannot
    /// change the outcome. Used to keep what is derived from the job (the
    /// status of an uploaded document) in step with it.
    async fn on_failed(&self, _job: &Job, _error: &JobError) {}
}

pub struct JobRunner {
    service: Arc<JobService>,
    handlers: HashMap<JobKind, Arc<dyn JobHandler>>,
    settings: JobSettings,
    /// Finished jobs whose outcome could not be stored (storage failure):
    /// still `running` in the database although no task owns them. Stored
    /// again by [`JobRunner::settle_unsettled`].
    unsettled: std::sync::Mutex<Vec<Job>>,
}

impl JobRunner {
    #[must_use]
    pub fn new(service: Arc<JobService>, settings: JobSettings) -> Self {
        Self {
            service,
            handlers: HashMap::new(),
            settings,
            unsettled: std::sync::Mutex::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn with_handler(mut self, kind: JobKind, handler: Arc<dyn JobHandler>) -> Self {
        self.handlers.insert(kind, handler);
        self
    }

    pub async fn recover_interrupted(&self) -> Result<Recovery, AppError> {
        self.service
            .recover_interrupted(self.settings.max_attempts)
            .await
    }

    /// Claims and runs jobs until the task is dropped or aborted. Anything
    /// still running then is settled by the next startup's recovery.
    pub async fn run(self: Arc<Self>) {
        let permits = Arc::new(Semaphore::new(self.settings.max_concurrent.max(1)));
        loop {
            self.settle_unsettled().await;
            let Ok(permit) = permits.clone().acquire_owned().await else {
                return;
            };
            match self.claim().await {
                Ok(Some(job)) => {
                    let runner = self.clone();
                    tokio::spawn(async move {
                        runner.execute(job).await;
                        drop(permit);
                    });
                }
                Ok(None) => {
                    drop(permit);
                    if self.has_unsettled() {
                        tokio::select! {
                            () = self.service.wake.notified() => {},
                            () = tokio::time::sleep(Duration::from_secs(5)) => {},
                        }
                    } else {
                        self.service.wake.notified().await;
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "cannot claim a job; retrying shortly");
                    drop(permit);
                    tokio::select! {
                        () = self.service.wake.notified() => {},
                        () = tokio::time::sleep(Duration::from_secs(5)) => {},
                    }
                }
            }
        }
    }

    /// Claims the oldest runnable job and runs it to its end. `false` if the
    /// queue holds nothing this runner can execute. Meant for tests; [`run`]
    /// is the production loop.
    ///
    /// [`run`]: Self::run
    pub async fn run_next(&self) -> Result<bool, AppError> {
        self.settle_unsettled().await;
        match self.claim().await? {
            Some(job) => {
                self.execute(job).await;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Moves the oldest queued job of a handled kind to `running`.
    async fn claim(&self) -> Result<Option<Job>, AppError> {
        let kinds: Vec<JobKind> = self.handlers.keys().copied().collect();
        if kinds.is_empty() {
            return Ok(None);
        }
        loop {
            let Some(mut job) = self.service.jobs.next_queued(&kinds).await? else {
                return Ok(None);
            };
            let now = now_millis(self.service.clock.as_ref());
            if job.start(now).is_err() {
                continue;
            }
            match self.service.jobs.update(&job, JobStatus::Queued).await {
                Ok(()) => {
                    self.service.publish(&job);
                    return Ok(Some(job));
                }
                // Taken meanwhile, or its topic was deleted: look at the next one.
                Err(RepositoryError::Conflict | RepositoryError::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    #[tracing::instrument(name = "job.run", skip_all, fields(job.id = %job.id, job.kind = job.kind.as_str(), job.outcome = tracing::field::Empty))]
    async fn execute(&self, job: Job) {
        let Some(handler) = self.handlers.get(&job.kind).cloned() else {
            return;
        };
        let job_id = job.id;
        let language = self.service.language_of(job.topic_id).await;
        let ctx = Arc::new(JobContext::new(job, self.service.clone(), language));
        // Own task: a panicking handler fails its job instead of the runner,
        // and a timed-out one can be aborted.
        let mut task = {
            let ctx = ctx.clone();
            let handler = handler.clone();
            let keys = self.service.keys.clone();
            let topics = self.service.topics.clone();
            tokio::spawn(async move {
                if !handler.needs_llm() {
                    return handler.run(&ctx).await;
                }
                // The job acts for the topic's owner: their key, nobody else's.
                let (job_id, topic_id) = {
                    let job = ctx.job().await;
                    (job.id, job.topic_id)
                };
                let owner = match topics.get_system(topic_id).await {
                    Ok(Some(topic)) => topic.user_id,
                    Ok(None) => return Err(JobFailure(JobErrorCode::Internal)),
                    Err(e) => {
                        tracing::error!(error = %e, "cannot read the topic of a job");
                        return Err(JobFailure(JobErrorCode::Internal));
                    }
                };
                match keys.for_user(owner).await {
                    // The span carries the owner, so the token counts logged by the
                    // calls inside the job say whose they are.
                    Ok(key) => {
                        let span = tracing::info_span!("job", job = %job_id, user_id = %owner, topic_id = %topic_id);
                        with_key(key, handler.run(&ctx).instrument(span)).await
                    }
                    Err(KeyError::Missing | KeyError::Unreadable) => {
                        Err(JobFailure(JobErrorCode::LlmKeyMissing))
                    }
                    Err(KeyError::Storage(e)) => {
                        tracing::error!(error = %e, "cannot read the LLM key of a job's owner");
                        Err(JobFailure(JobErrorCode::Internal))
                    }
                }
            })
        };
        let outcome = match tokio::time::timeout(self.settings.job_timeout, &mut task).await {
            Ok(Ok(result)) => result.map_err(|JobFailure(code)| code),
            Ok(Err(join)) => {
                tracing::error!(job = %job_id, panicked = join.is_panic(), "job handler died");
                Err(JobErrorCode::Internal)
            }
            Err(_) => {
                task.abort();
                tracing::warn!(job = %job_id, "job timed out");
                Err(JobErrorCode::Timeout)
            }
        };
        let label = match &outcome {
            Ok(_) => "succeeded",
            Err(code) => code.as_str(),
        };
        tracing::Span::current().record("job.outcome", label);
        self.finish(&ctx, outcome, handler.as_ref()).await;
    }

    async fn finish(
        &self,
        ctx: &JobContext,
        outcome: Result<JobResult, JobErrorCode>,
        handler: &dyn JobHandler,
    ) {
        let now = now_millis(self.service.clock.as_ref());
        let mut job = ctx.job.lock().await;
        let applied = match outcome {
            Ok(result) => job.succeed(result, now),
            Err(code) => {
                tracing::warn!(job = %job.id, code = code.as_str(), "job failed");
                job.fail(code, ctx.language, now)
            }
        };
        if applied.is_err() {
            return;
        }
        let finished = job.clone();
        drop(job);
        if let Some(error) = &finished.error {
            handler.on_failed(&finished, error).await;
        }
        self.store_outcome(finished).await;
    }

    /// Stores the final state of a job, retrying a storage failure a few
    /// times. If it still fails the job is kept to be stored later, so it
    /// does not stay `running` (and block its kind) until a restart.
    async fn store_outcome(&self, job: Job) {
        let mut delay = self.settings.store_retry_delay;
        for attempt in 1..=3 {
            match self.service.jobs.update(&job, JobStatus::Running).await {
                Ok(()) => {
                    self.service.publish(&job);
                    return;
                }
                // The topic (and with it the job) was deleted while it ran,
                // or somebody else settled the job.
                Err(RepositoryError::NotFound | RepositoryError::Conflict) => return,
                Err(e) => {
                    tracing::error!(job = %job.id, attempt, error = %e, "cannot store the job outcome");
                    if attempt < 3 {
                        tokio::time::sleep(delay).await;
                        delay = delay.saturating_mul(2);
                    }
                }
            }
        }
        if let Ok(mut pending) = self.unsettled.lock() {
            pending.push(job);
        }
    }

    fn has_unsettled(&self) -> bool {
        self.unsettled.lock().is_ok_and(|p| !p.is_empty())
    }

    /// One more try for every outcome that could not be stored.
    pub async fn settle_unsettled(&self) {
        let pending = match self.unsettled.lock() {
            Ok(mut p) => std::mem::take(&mut *p),
            Err(_) => return,
        };
        for job in pending {
            match self.service.jobs.update(&job, JobStatus::Running).await {
                Ok(()) => self.service.publish(&job),
                Err(RepositoryError::NotFound | RepositoryError::Conflict) => {}
                Err(e) => {
                    tracing::error!(job = %job.id, error = %e, "still cannot store the job outcome");
                    if let Ok(mut p) = self.unsettled.lock() {
                        p.push(job);
                    }
                }
            }
        }
    }
}
