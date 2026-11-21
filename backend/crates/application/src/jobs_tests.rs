//! Job service and runner, over in-memory fakes.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use domain::{
    Job, JobCursor, JobErrorCode, JobKind, JobProgress, JobRepository, JobResult, JobStatus,
    PageRequest, TopicId,
};
use time::macros::datetime;
use tokio::sync::Notify;
use uuid::Uuid;

use crate::chat_tests::{Fixture, SLOW, fixture_with, wait_for};
use crate::error::{AppError, ConflictReason};
use crate::{JobContext, JobFailure, JobHandler, JobRunner, JobSettings};

async fn fixture() -> Fixture {
    fixture_with(vec![], SLOW, 4, |llm| llm).await
}

const DONE: JobResult = JobResult::Research {
    resource_count: 1,
    gap_count: 0,
};

fn settings() -> JobSettings {
    JobSettings {
        max_concurrent: 2,
        job_timeout: Duration::from_secs(30),
        max_attempts: 3,
        store_retry_delay: Duration::from_millis(1),
    }
}

impl Fixture {
    fn runner(&self, settings: JobSettings, handler: Arc<dyn JobHandler>) -> JobRunner {
        JobRunner::new(self.jobs.clone(), settings).with_handler(JobKind::Research, handler)
    }

    /// Stores and announces a queued research job for `topic`.
    pub(crate) async fn enqueue(&self, topic: TopicId) -> Job {
        let job = self.jobs.new_job(topic, JobKind::Research);
        self.repo.insert(&job).await.unwrap();
        self.jobs.announce(&job);
        job
    }

    pub(crate) fn stored_job(&self, topic: TopicId) -> Job {
        let s = self.repo.0.lock().unwrap();
        let mut jobs: Vec<_> = s.jobs.iter().filter(|j| j.topic_id == topic).collect();
        jobs.sort_by_key(|j| (j.created_at, j.id));
        jobs.last().copied().unwrap().clone()
    }

    async fn english_topic(&self) -> TopicId {
        let folder = self.repo.0.lock().unwrap().folders[0].id;
        self.topics
            .create_with_language(domain::UserId::OWNER, folder, Some("Topic"), Some("en"))
            .await
            .unwrap()
            .id
    }

    async fn another_topic(&self, title: &str) -> TopicId {
        let folder = self.repo.0.lock().unwrap().folders[0].id;
        self.topics
            .create(domain::UserId::OWNER, folder, Some(title))
            .await
            .unwrap()
            .id
    }
}

/// Reports two steps and succeeds.
struct Steps;

#[async_trait]
impl JobHandler for Steps {
    async fn run(&self, ctx: &JobContext) -> Result<JobResult, JobFailure> {
        ctx.progress(JobProgress::step("searching").with_done(1))
            .await;
        ctx.progress(JobProgress::step("annotating").with_done(2))
            .await;
        Ok(DONE)
    }
}

struct Failing(JobErrorCode);

#[async_trait]
impl JobHandler for Failing {
    async fn run(&self, _: &JobContext) -> Result<JobResult, JobFailure> {
        Err(JobFailure(self.0))
    }
}

struct Panicking;

#[async_trait]
impl JobHandler for Panicking {
    async fn run(&self, _: &JobContext) -> Result<JobResult, JobFailure> {
        panic!("boom");
    }
}

struct Hanging;

#[async_trait]
impl JobHandler for Hanging {
    async fn run(&self, _: &JobContext) -> Result<JobResult, JobFailure> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn a_job_runs_to_success_publishing_every_change() {
    let f = fixture().await;
    let queued = f.enqueue(f.topic).await;
    f.clock.set(datetime!(2026-11-08 10:05:00 UTC));
    let runner = f.runner(settings(), Arc::new(Steps));

    assert!(runner.run_next().await.unwrap());
    assert!(!runner.run_next().await.unwrap(), "queue is empty");

    let job = f.stored_job(f.topic);
    assert_eq!(job.id, queued.id);
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(job.attempt, 1);
    assert_eq!(job.started_at, Some(datetime!(2026-11-08 10:05:00 UTC)));
    assert_eq!(job.finished_at, Some(datetime!(2026-11-08 10:05:00 UTC)));
    assert_eq!(job.progress.unwrap().step, "annotating");
    assert_eq!(job.result, Some(DONE));
    assert_eq!(
        f.kinds(),
        [
            "job:research:queued",
            "job:research:running",
            "job:research:running",
            "job:research:running",
            "job:research:succeeded"
        ]
    );
}

#[tokio::test]
async fn a_failing_handler_fails_the_job_and_retry_starts_a_fresh_run() {
    let f = fixture().await;
    let job = f.enqueue(f.topic).await;
    f.runner(settings(), Arc::new(Failing(JobErrorCode::UpstreamFailed)))
        .run_next()
        .await
        .unwrap();
    let failed = f.stored_job(f.topic);
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error.unwrap().code, JobErrorCode::UpstreamFailed);

    let retried = f
        .jobs
        .retry(domain::UserId::OWNER, f.topic, job.id)
        .await
        .unwrap();
    assert_eq!((retried.status, retried.attempt), (JobStatus::Queued, 0));
    assert!(retried.error.is_none());

    f.runner(settings(), Arc::new(Steps))
        .run_next()
        .await
        .unwrap();
    let done = f.stored_job(f.topic);
    assert_eq!((done.status, done.attempt), (JobStatus::Succeeded, 1));
    assert_eq!(done.id, job.id, "same job, new run");
}

#[tokio::test]
async fn only_a_failed_job_can_be_retried() {
    let f = fixture().await;
    let job = f.enqueue(f.topic).await;
    assert!(matches!(
        f.jobs.retry(domain::UserId::OWNER, f.topic, job.id).await,
        Err(AppError::Conflict(ConflictReason::JobNotRetryable))
    ));
    let ghost = domain::JobId::new(Uuid::from_u128(999));
    assert!(matches!(
        f.jobs.retry(domain::UserId::OWNER, f.topic, ghost).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        f.jobs
            .retry(
                domain::UserId::OWNER,
                TopicId::new(Uuid::from_u128(999)),
                job.id
            )
            .await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn retry_is_refused_while_another_job_of_the_kind_is_active() {
    let f = fixture().await;
    let job = f.enqueue(f.topic).await;
    f.runner(settings(), Arc::new(Failing(JobErrorCode::Internal)))
        .run_next()
        .await
        .unwrap();
    f.enqueue(f.topic).await;
    assert!(matches!(
        f.jobs.retry(domain::UserId::OWNER, f.topic, job.id).await,
        Err(AppError::Conflict(ConflictReason::JobNotRetryable))
    ));
}

#[tokio::test]
async fn a_panicking_handler_fails_its_job_not_the_runner() {
    let f = fixture().await;
    f.enqueue(f.topic).await;
    f.runner(settings(), Arc::new(Panicking))
        .run_next()
        .await
        .unwrap();
    let job = f.stored_job(f.topic);
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::Internal);
}

#[tokio::test]
async fn a_job_over_its_time_budget_fails_with_timeout() {
    let f = fixture().await;
    f.enqueue(f.topic).await;
    let tight = JobSettings {
        job_timeout: Duration::from_millis(30),
        ..settings()
    };
    f.runner(tight, Arc::new(Hanging)).run_next().await.unwrap();
    let job = f.stored_job(f.topic);
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::Timeout);
}

#[tokio::test]
async fn a_kind_without_handler_stays_queued_quietly() {
    let f = fixture().await;
    f.enqueue(f.topic).await;
    let runner = JobRunner::new(f.jobs.clone(), settings());
    assert!(!runner.run_next().await.unwrap());
    let job = f.stored_job(f.topic);
    assert_eq!((job.status, job.attempt), (JobStatus::Queued, 0));
    assert_eq!(f.kinds(), ["job:research:queued"], "nothing else happened");
}

#[tokio::test]
async fn recovery_requeues_interrupted_jobs_until_the_attempt_cap() {
    let f = fixture().await;
    let second = f.another_topic("Altro").await;
    for (topic, attempt) in [(f.topic, 1), (second, 3)] {
        let mut job = f.jobs.new_job(topic, JobKind::Research);
        job.status = JobStatus::Running;
        job.attempt = attempt;
        job.started_at = Some(job.created_at);
        f.repo.insert(&job).await.unwrap();
    }

    let report = f.jobs.recover_interrupted(3).await.unwrap();
    assert_eq!((report.requeued, report.failed), (1, 1));

    let requeued = f.stored_job(f.topic);
    assert_eq!((requeued.status, requeued.attempt), (JobStatus::Queued, 1));
    assert!(requeued.started_at.is_none());
    let failed = f.stored_job(second);
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error.unwrap().code, JobErrorCode::Interrupted);

    // The requeued one runs again and counts a second attempt.
    f.runner(settings(), Arc::new(Steps))
        .run_next()
        .await
        .unwrap();
    let rerun = f.stored_job(f.topic);
    assert_eq!((rerun.status, rerun.attempt), (JobStatus::Succeeded, 2));
}

/// Parks until released, tracking how many run at once.
struct Gate {
    running: AtomicUsize,
    peak: AtomicUsize,
    release: Notify,
}

#[async_trait]
impl JobHandler for Gate {
    async fn run(&self, _: &JobContext) -> Result<JobResult, JobFailure> {
        let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        self.release.notified().await;
        self.running.fetch_sub(1, Ordering::SeqCst);
        Ok(DONE)
    }
}

#[tokio::test]
async fn the_loop_runs_at_most_max_concurrent_jobs_and_picks_up_new_ones() {
    let f = fixture().await;
    let topics = [
        f.topic,
        f.another_topic("B").await,
        f.another_topic("C").await,
    ];
    let gate = Arc::new(Gate {
        running: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
        release: Notify::new(),
    });
    let runner = Arc::new(f.runner(settings(), gate.clone()));
    let task = tokio::spawn(runner.run());
    for topic in topics {
        f.enqueue(topic).await;
    }

    wait_for(|| gate.running.load(Ordering::SeqCst) == 2).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(gate.running.load(Ordering::SeqCst), 2, "third job waits");

    gate.release.notify_waiters();
    wait_for(|| gate.running.load(Ordering::SeqCst) == 1).await;
    gate.release.notify_waiters();
    wait_for(|| {
        topics
            .iter()
            .all(|t| f.stored_job(*t).status == JobStatus::Succeeded)
    })
    .await;
    assert_eq!(gate.peak.load(Ordering::SeqCst), 2);
    task.abort();
}

#[tokio::test]
async fn lists_newest_first_and_pages() {
    let f = fixture().await;
    let first = f.enqueue(f.topic).await;
    f.runner(settings(), Arc::new(Steps))
        .run_next()
        .await
        .unwrap();
    f.clock.set(datetime!(2026-11-08 11:00:00 UTC));
    let second = f.enqueue(f.topic).await;

    let page = f
        .jobs
        .list(
            domain::UserId::OWNER,
            f.topic,
            PageRequest {
                after: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].id, second.id);
    let next: JobCursor = page.next.unwrap();
    let rest = f
        .jobs
        .list(
            domain::UserId::OWNER,
            f.topic,
            PageRequest {
                after: Some(next),
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(rest.items[0].id, first.id);
    assert!(rest.next.is_none());
    assert!(matches!(
        f.jobs
            .list(
                domain::UserId::OWNER,
                TopicId::new(Uuid::from_u128(999)),
                PageRequest {
                    after: None,
                    limit: 1
                }
            )
            .await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn only_the_newest_job_of_a_kind_can_be_retried() {
    let f = fixture().await;
    let old = f.enqueue(f.topic).await;
    f.runner(settings(), Arc::new(Failing(JobErrorCode::Internal)))
        .run_next()
        .await
        .unwrap();
    // A newer job of the kind, already over: the old failure is history.
    f.clock.set(datetime!(2026-11-08 11:00:00 UTC));
    let newer = f.enqueue(f.topic).await;
    f.runner(settings(), Arc::new(Steps))
        .run_next()
        .await
        .unwrap();
    assert_eq!(f.stored_job(f.topic).id, newer.id);
    assert!(matches!(
        f.jobs.retry(domain::UserId::OWNER, f.topic, old.id).await,
        Err(AppError::Conflict(ConflictReason::JobNotRetryable))
    ));
}

#[tokio::test]
async fn a_failed_store_of_the_outcome_is_retried_and_then_kept_for_later() {
    // One failure: the second attempt stores it.
    let f = fixture().await;
    f.enqueue(f.topic).await;
    f.repo.0.lock().unwrap().fail_finishing_jobs = 1;
    f.runner(settings(), Arc::new(Steps))
        .run_next()
        .await
        .unwrap();
    assert_eq!(f.stored_job(f.topic).status, JobStatus::Succeeded);

    // Three failures: the job stays `running` for now, but the runner owns
    // the outcome and stores it as soon as the storage answers again.
    let f = fixture().await;
    f.enqueue(f.topic).await;
    f.repo.0.lock().unwrap().fail_finishing_jobs = 3;
    let runner = f.runner(settings(), Arc::new(Steps));
    runner.run_next().await.unwrap();
    assert_eq!(f.stored_job(f.topic).status, JobStatus::Running);
    runner.settle_unsettled().await;
    let job = f.stored_job(f.topic);
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(job.result, Some(DONE));
    assert_eq!(
        f.kinds().last().map(String::as_str),
        Some("job:research:succeeded")
    );
}

/// Says one fixed sentence the way the real handlers do.
struct Says;

#[async_trait]
impl JobHandler for Says {
    async fn run(&self, ctx: &JobContext) -> Result<JobResult, JobFailure> {
        let text = ctx
            .language()
            .pick("Ho letto la mission", "I read the mission");
        ctx.progress(JobProgress::step("planning").with_message(text))
            .await;
        Err(JobFailure(JobErrorCode::Timeout))
    }
}

#[tokio::test]
async fn texts_shown_for_a_job_follow_the_language_of_its_topic() {
    let f = fixture().await;
    let english = f.english_topic().await;
    for (topic, progress, error) in [
        (
            f.topic,
            "Ho letto la mission",
            "L'operazione ha impiegato troppo tempo, riprova.",
        ),
        (
            english,
            "I read the mission",
            "The operation took too long, try again.",
        ),
    ] {
        f.enqueue(topic).await;
        f.runner(settings(), Arc::new(Says))
            .run_next()
            .await
            .unwrap();
        let job = f.stored_job(topic);
        assert_eq!(job.progress.unwrap().message.as_deref(), Some(progress));
        assert_eq!(job.error.unwrap().message, error);
    }
}

#[tokio::test]
async fn an_interrupted_job_of_an_english_topic_fails_with_an_english_message() {
    let f = fixture().await;
    let english = f.english_topic().await;
    let mut job = f.jobs.new_job(english, JobKind::Research);
    job.status = JobStatus::Running;
    job.attempt = 3;
    job.started_at = Some(job.created_at);
    f.repo.insert(&job).await.unwrap();

    f.jobs.recover_interrupted(3).await.unwrap();

    let error = f.stored_job(english).error.unwrap();
    assert_eq!(error.code, JobErrorCode::Interrupted);
    assert_eq!(error.message, "The operation was interrupted, try again.");
}
