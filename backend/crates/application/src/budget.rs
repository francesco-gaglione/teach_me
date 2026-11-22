//! The daily token budget of each user (`TEACH_ME_USER_DAILY_LLM_TOKENS`,
//! `docs/contracts/api-conventions.md` section 12).
//!
//! Spending is counted from the usage the provider reports, per UTC day.
//! The budget gates *starts*: a request or a new job is refused once the day's
//! count has reached the limit, but a call or a job already running finishes
//! (and may overshoot, by one call's worth at most per concurrent job).

use std::sync::Arc;

use domain::llm::Usage;
use domain::{Clock, LlmUsageRepository, UserId};
use time::{Date, OffsetDateTime, Time};

use crate::error::AppError;

pub struct LlmBudget {
    usage: Arc<dyn LlmUsageRepository>,
    clock: Arc<dyn Clock>,
    /// Tokens per user per day; 0 = no limit (the counts are not even kept).
    daily_tokens: u64,
}

impl LlmBudget {
    #[must_use]
    pub fn new(
        usage: Arc<dyn LlmUsageRepository>,
        clock: Arc<dyn Clock>,
        daily_tokens: u64,
    ) -> Self {
        Self {
            usage,
            clock,
            daily_tokens,
        }
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.daily_tokens > 0
    }

    /// `Err(LlmBudgetExceeded)` when the user has already spent the day's
    /// tokens. Always `Ok` when the budget is disabled.
    pub async fn check(&self, user: UserId) -> Result<(), AppError> {
        if !self.is_enabled() {
            return Ok(());
        }
        let now = self.clock.now();
        let spent = self.usage.tokens_on(user, now.date()).await?;
        if spent >= self.daily_tokens {
            return Err(AppError::LlmBudgetExceeded {
                retry_after_secs: secs_until_midnight_utc(now),
            });
        }
        Ok(())
    }

    /// Adds what a finished call used. A storage failure is logged, never
    /// propagated: the user already got the answer.
    pub async fn record(&self, user: UserId, usage: &Usage) {
        if !self.is_enabled() {
            return;
        }
        let tokens = billable_tokens(usage);
        if tokens == 0 {
            return;
        }
        let day = self.clock.now().date();
        if let Err(e) = self.usage.add_tokens(user, day, tokens).await {
            tracing::error!(error = %e, %user, "cannot record the LLM token usage");
        }
    }
}

/// What counts against the budget: input, output and both cache kinds
/// (contract section 12: cache tokens are real spend too).
#[must_use]
pub(crate) fn billable_tokens(usage: &Usage) -> u64 {
    u64::from(usage.input_tokens)
        + u64::from(usage.output_tokens)
        + u64::from(usage.cache_write_tokens)
        + u64::from(usage.cache_read_tokens)
}

/// Seconds from `now` to the next 00:00 UTC (at least 1).
fn secs_until_midnight_utc(now: OffsetDateTime) -> u64 {
    let now = now.to_offset(time::UtcOffset::UTC);
    let next: Date = now.date().next_day().unwrap_or(now.date());
    let midnight = next.with_time(Time::MIDNIGHT).assume_utc();
    // Rounded up: asking again a fraction of a second early would be refused again.
    let left = midnight - now;
    let secs = left.whole_seconds() + i64::from(left.subsec_nanoseconds() > 0);
    u64::try_from(secs).unwrap_or(1).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakes::TestClock;
    use async_trait::async_trait;
    use domain::RepositoryError;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use time::macros::datetime;

    #[derive(Default)]
    struct MemUsage(Mutex<HashMap<(UserId, Date), u64>>);

    #[async_trait]
    impl LlmUsageRepository for MemUsage {
        async fn tokens_on(&self, user: UserId, day: Date) -> Result<u64, RepositoryError> {
            Ok(*self.0.lock().unwrap().get(&(user, day)).unwrap_or(&0))
        }
        async fn add_tokens(
            &self,
            user: UserId,
            day: Date,
            tokens: u64,
        ) -> Result<(), RepositoryError> {
            *self.0.lock().unwrap().entry((user, day)).or_default() += tokens;
            Ok(())
        }
    }

    fn usage(input: u32, output: u32) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            ..Usage::default()
        }
    }

    fn budget(limit: u64, at: OffsetDateTime) -> (LlmBudget, Arc<TestClock>) {
        let clock = Arc::new(TestClock::new(at));
        (
            LlmBudget::new(Arc::new(MemUsage::default()), clock.clone(), limit),
            clock,
        )
    }

    #[tokio::test]
    async fn disabled_budget_never_refuses_nor_counts() {
        let (b, _) = budget(0, datetime!(2026-11-22 10:00 UTC));
        b.record(UserId::OWNER, &usage(1_000_000, 1_000_000)).await;
        assert!(b.check(UserId::OWNER).await.is_ok());
    }

    #[tokio::test]
    async fn refuses_once_input_plus_output_reach_the_limit() {
        let (b, _) = budget(100, datetime!(2026-11-22 10:00 UTC));
        b.record(UserId::OWNER, &usage(40, 50)).await;
        assert!(b.check(UserId::OWNER).await.is_ok());
        b.record(UserId::OWNER, &usage(5, 5)).await;
        let err = b.check(UserId::OWNER).await.unwrap_err();
        assert!(matches!(
            err,
            AppError::LlmBudgetExceeded {
                retry_after_secs: 50_400
            }
        ));
    }

    #[tokio::test]
    async fn the_count_is_per_user_and_resets_at_midnight_utc() {
        let (b, clock) = budget(10, datetime!(2026-11-22 23:59:30 UTC));
        b.record(UserId::OWNER, &usage(10, 0)).await;
        assert!(matches!(
            b.check(UserId::OWNER).await,
            Err(AppError::LlmBudgetExceeded {
                retry_after_secs: 30
            })
        ));
        let other = UserId::new(uuid::Uuid::from_u128(7));
        assert!(b.check(other).await.is_ok());
        clock.set(datetime!(2026-11-23 00:00:00 UTC));
        assert!(b.check(UserId::OWNER).await.is_ok());
    }

    // ---------- wiring: the key gate and the meter ----------

    use crate::fakes::{FakeCipher, FakeLlm, MemUsers, Step};
    use crate::{KeyScopedLlm, LlmKeys, with_key};
    use domain::Secret;
    use domain::llm::{LlmEvent, LlmProvider, LlmRequest, LlmRole};

    fn keyed_user_env(limit: u64) -> (Arc<LlmKeys>, Arc<LlmBudget>, Arc<MemUsage>) {
        let users = MemUsers::with_owner();
        users.give_key(UserId::OWNER, &FakeCipher, "sk-a");
        let clock = Arc::new(TestClock::new(datetime!(2026-11-22 10:00 UTC)));
        let usage = Arc::new(MemUsage::default());
        let budget = Arc::new(LlmBudget::new(usage.clone(), clock, limit));
        let keys = Arc::new(LlmKeys::new(users, Arc::new(FakeCipher)).with_budget(budget.clone()));
        (keys, budget, usage)
    }

    fn spent_script(input: u32, output: u32) -> Vec<Step> {
        vec![
            Step::Delta("hi"),
            Step::Event(LlmEvent::Usage(usage(input, output))),
        ]
    }

    #[tokio::test]
    async fn calls_made_in_a_scope_are_counted_for_its_user_and_then_gate_new_work() {
        let (keys, budget, store) = keyed_user_env(100);
        let llm = FakeLlm::with(vec![spent_script(60, 50)]);
        let scoped = KeyScopedLlm::new(llm).with_budget(budget);
        assert!(keys.require(UserId::OWNER).await.is_ok());

        with_key(UserId::OWNER, Secret::new("sk-a").unwrap(), async {
            let mut stream = scoped
                .stream(LlmRequest::new(LlmRole::Tutor, "t"))
                .await
                .unwrap();
            while stream.next().await.is_some() {}
        })
        .await;

        let day = datetime!(2026-11-22 10:00 UTC).date();
        assert_eq!(store.tokens_on(UserId::OWNER, day).await.unwrap(), 110);
        assert!(matches!(
            keys.require(UserId::OWNER).await,
            Err(AppError::LlmBudgetExceeded { .. })
        ));
        // The grader path asks the gate directly.
        assert!(keys.check_budget(UserId::OWNER).await.is_err());
    }

    #[tokio::test]
    async fn a_stream_dropped_midway_still_counts_what_was_reported() {
        let (_, budget, store) = keyed_user_env(100);
        let llm = FakeLlm::with(vec![spent_script(30, 20)]);
        let scoped = KeyScopedLlm::new(llm).with_budget(budget);
        with_key(UserId::OWNER, Secret::new("sk-a").unwrap(), async {
            let mut stream = scoped
                .stream(LlmRequest::new(LlmRole::Tutor, "t"))
                .await
                .unwrap();
            stream.next().await; // text
            stream.next().await; // usage
        })
        .await;
        // The drop settles on a spawned task.
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        let day = datetime!(2026-11-22 10:00 UTC).date();
        assert_eq!(store.tokens_on(UserId::OWNER, day).await.unwrap(), 50);
    }

    #[tokio::test]
    async fn calls_outside_a_scope_or_with_the_budget_off_are_not_counted() {
        let (_, budget, store) = keyed_user_env(100);
        let llm = FakeLlm::with(vec![spent_script(30, 20)]);
        let scoped = KeyScopedLlm::new(llm).with_budget(budget);
        let _ = scoped.complete(LlmRequest::new(LlmRole::Tutor, "t")).await;
        let day = datetime!(2026-11-22 10:00 UTC).date();
        assert_eq!(store.tokens_on(UserId::OWNER, day).await.unwrap(), 0);

        let (_, off, store) = keyed_user_env(0);
        let scoped = KeyScopedLlm::new(FakeLlm::with(vec![spent_script(30, 20)])).with_budget(off);
        with_key(UserId::OWNER, Secret::new("sk-a").unwrap(), async {
            let _ = scoped.complete(LlmRequest::new(LlmRole::Tutor, "t")).await;
        })
        .await;
        assert_eq!(store.tokens_on(UserId::OWNER, day).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn a_new_job_is_refused_over_the_budget_and_nothing_is_queued() {
        use crate::JobService;
        use crate::fakes::{RecordingEvents, SeqIds};
        use domain::{JobKind, Topic, TopicId, TopicTitle};

        let (keys, budget, _) = keyed_user_env(10);
        let repo = Arc::new(crate::fakes::MemRepo::default());
        let at = datetime!(2026-11-22 10:00 UTC);
        let topic = TopicId::new(uuid::Uuid::from_u128(0xa1));
        {
            let mut s = repo.0.lock().unwrap();
            let folder = domain::FolderId::new(uuid::Uuid::from_u128(0xa2));
            s.folders.push(domain::Folder {
                id: folder,
                user_id: UserId::OWNER,
                name: domain::FolderName::parse("F").unwrap(),
                position: 0,
                created_at: at,
                updated_at: at,
            });
            s.topics.push(Topic::new(
                topic,
                UserId::OWNER,
                folder,
                TopicTitle::parse("T").unwrap(),
                at,
            ));
        }
        let jobs = JobService::new(
            repo.clone(),
            repo.clone(),
            Arc::new(RecordingEvents::default()),
            Arc::new(TestClock::new(at)),
            Arc::new(SeqIds::default()),
            keys,
        );
        budget.record(UserId::OWNER, &usage(6, 4)).await;
        let err = jobs
            .enqueue(UserId::OWNER, topic, JobKind::Research)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::LlmBudgetExceeded { .. }), "{err:?}");
        assert!(repo.0.lock().unwrap().jobs.is_empty(), "no job was created");
    }

    #[tokio::test]
    async fn cache_tokens_count_against_the_budget() {
        let (b, _) = budget(100, datetime!(2026-11-22 10:00 UTC));
        b.record(
            UserId::OWNER,
            &Usage {
                input_tokens: 10,
                output_tokens: 10,
                cache_write_tokens: 30,
                cache_read_tokens: 50,
                ..Usage::default()
            },
        )
        .await;
        assert!(b.check(UserId::OWNER).await.is_err());
    }

    #[tokio::test]
    async fn a_queued_job_whose_owner_is_over_budget_fails_without_running() {
        use crate::fakes::{RecordingEvents, SeqIds};
        use crate::{JobContext, JobFailure, JobHandler, JobRunner, JobService, JobSettings};
        use domain::{
            JobErrorCode, JobKind, JobRepository, JobResult, JobStatus, Topic, TopicId, TopicTitle,
        };
        use std::sync::atomic::{AtomicBool, Ordering};

        struct Probe(Arc<AtomicBool>);
        #[async_trait]
        impl JobHandler for Probe {
            fn needs_llm(&self) -> bool {
                true
            }
            async fn run(&self, _: &JobContext) -> Result<JobResult, JobFailure> {
                self.0.store(true, Ordering::SeqCst);
                Ok(JobResult::Research {
                    resource_count: 0,
                    gap_count: 0,
                })
            }
        }

        let (keys, budget, _) = keyed_user_env(10);
        let repo = Arc::new(crate::fakes::MemRepo::default());
        let at = datetime!(2026-11-22 10:00 UTC);
        let topic = TopicId::new(uuid::Uuid::from_u128(0xa1));
        {
            let mut s = repo.0.lock().unwrap();
            let folder = domain::FolderId::new(uuid::Uuid::from_u128(0xa2));
            s.folders.push(domain::Folder {
                id: folder,
                user_id: UserId::OWNER,
                name: domain::FolderName::parse("F").unwrap(),
                position: 0,
                created_at: at,
                updated_at: at,
            });
            s.topics.push(Topic::new(
                topic,
                UserId::OWNER,
                folder,
                TopicTitle::parse("T").unwrap(),
                at,
            ));
        }
        let service = Arc::new(JobService::new(
            repo.clone(),
            repo.clone(),
            Arc::new(RecordingEvents::default()),
            Arc::new(TestClock::new(at)),
            Arc::new(SeqIds::default()),
            keys,
        ));
        // Queued while the budget was still available.
        let job = service.new_job(topic, JobKind::Research);
        repo.insert(&job).await.unwrap();
        budget.record(UserId::OWNER, &usage(6, 4)).await;

        let ran = Arc::new(AtomicBool::new(false));
        let runner = JobRunner::new(service, JobSettings::default())
            .with_handler(JobKind::Research, Arc::new(Probe(ran.clone())));
        assert!(runner.run_next().await.unwrap());

        assert!(!ran.load(Ordering::SeqCst), "the handler must not run");
        let stored = repo.0.lock().unwrap().jobs[0].clone();
        assert_eq!(stored.status, JobStatus::Failed);
        assert_eq!(stored.error.unwrap().code, JobErrorCode::LlmBudgetExceeded);
        assert_eq!(stored.attempt, 1, "not retried by itself");
    }
}
