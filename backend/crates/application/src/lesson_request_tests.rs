//! The Tutor's `request_lesson` tool (`lessons.md` §12), over in-memory fakes.

use std::sync::Arc;

use domain::{
    ChatErrorCode, ChatMessage, JobKind, JobStatus, MessagePart, MessageStatus, TopicState,
};
use serde_json::json;

use crate::chat_tests::{Fixture, SLOW, fixture_with};
use crate::fakes::Step;
use crate::mission_tests::{save_draft, tool};
use crate::{ChatService, ChatSettings, JobService, LessonService, TutorPrompts};

fn request() -> Step {
    tool("request_lesson", json!({}))
}

async fn fixture(scripts: Vec<Vec<Step>>) -> Fixture {
    fixture_with(scripts, SLOW, 4, |llm| llm).await
}

impl Fixture {
    fn set_state(&self, state: TopicState) {
        self.repo
            .0
            .lock()
            .unwrap()
            .topics
            .iter_mut()
            .find(|t| t.id == self.topic)
            .unwrap()
            .state = state;
    }

    fn generation_jobs(&self) -> Vec<domain::Job> {
        self.repo
            .0
            .lock()
            .unwrap()
            .jobs
            .iter()
            .filter(|j| j.kind == JobKind::LessonGeneration)
            .cloned()
            .collect()
    }
}

fn lesson_part(m: &ChatMessage) -> Option<domain::JobId> {
    m.parts.iter().find_map(|p| match p {
        MessagePart::LessonRequest { job_id } => Some(*job_id),
        _ => None,
    })
}

#[tokio::test]
async fn a_requested_lesson_queues_the_job_and_ends_the_message_with_its_part() {
    let f = fixture(vec![
        vec![Step::Delta("Preparo la lezione."), request()],
        vec![],
    ])
    .await;
    f.set_state(TopicState::Ready);

    let reply = f.turn("Passiamo alla prossima lezione").await;

    let jobs = f.generation_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, JobStatus::Queued);
    assert_eq!(reply.status, MessageStatus::Complete);
    assert_eq!(reply.parts.len(), 2, "{:?}", reply.parts);
    assert_eq!(
        reply.parts[1],
        MessagePart::LessonRequest { job_id: jobs[0].id }
    );
    let results = f.results_of_request(1);
    assert_eq!(results.len(), 1);
    assert!(!results[0].1);
    assert!(
        results[0].0.starts_with("Generazione avviata"),
        "{results:?}"
    );
    // The job is announced like the endpoint does, before the message closes.
    let kinds = f.kinds();
    let queued = kinds
        .iter()
        .position(|k| k == "job:lesson_generation:queued")
        .expect("job_updated");
    let completed = kinds.iter().position(|k| k == "completed").unwrap();
    assert!(queued < completed, "{kinds:?}");
}

#[tokio::test]
async fn an_active_generation_is_pointed_to_and_no_second_job_is_made() {
    let f = fixture(vec![vec![request()], vec![Step::Delta("Già in corso.")]]).await;
    f.set_state(TopicState::Ready);
    let active = f
        .jobs
        .enqueue(domain::UserId::OWNER, f.topic, JobKind::LessonGeneration)
        .await
        .unwrap();

    let reply = f.turn("La prossima lezione?").await;

    assert_eq!(f.generation_jobs().len(), 1);
    assert_eq!(lesson_part(&reply), Some(active.id));
    let results = f.results_of_request(1);
    assert!(!results[0].1);
    assert!(
        results[0]
            .0
            .starts_with("Una lezione è già in preparazione")
    );
}

#[tokio::test]
async fn a_topic_that_is_not_ready_gets_no_job_and_no_part() {
    let f = fixture(vec![
        vec![request()],
        vec![Step::Delta("Prima la mission.")],
    ])
    .await;
    f.set_state(TopicState::Researching);

    let reply = f.turn("Lezione?").await;

    assert_eq!(f.generation_jobs().len(), 0);
    assert_eq!(lesson_part(&reply), None);
    let results = f.results_of_request(1);
    assert!(results[0].1);
    assert!(results[0].0.starts_with("Non ammesso"), "{results:?}");
}

#[tokio::test]
async fn a_missing_llm_key_gets_no_job_and_no_part() {
    let mut f = fixture(vec![vec![request()], vec![Step::Delta("Serve la chiave.")]]).await;
    f.set_state(TopicState::Ready);
    // Same wiring as the fixture, but an owner without a key.
    let keyless = Arc::new(crate::LlmKeys::new(
        crate::fakes::MemUsers::with_owner(),
        Arc::new(crate::fakes::FakeCipher),
    ));
    let jobs = Arc::new(JobService::new(
        f.repo.clone(),
        f.repo.clone(),
        f.events.clone(),
        f.clock.clone(),
        Arc::new(crate::fakes::SeqIds::default()),
        keyless.clone(),
    ));
    let lessons = Arc::new(LessonService::new(
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        jobs,
        keyless,
        f.clock.clone(),
    ));
    f.chat = ChatService::new(
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.events.clone(),
        f.llm.clone(),
        TutorPrompts::embedded(),
        f.clock.clone(),
        Arc::new(crate::fakes::SeqIds::default()),
        crate::fakes::owner_keys(),
        ChatSettings {
            flush_interval: SLOW,
            history_token_budget: 8000,
            max_tool_iterations: 4,
        },
    )
    .with_lesson_requests(lessons);

    let reply = f.turn("Lezione?").await;

    assert_eq!(f.generation_jobs().len(), 0);
    assert_eq!(lesson_part(&reply), None);
    let results = f.results_of_request(1);
    assert!(results[0].1);
    assert!(
        results[0].0.starts_with("Lezione NON avviata"),
        "{results:?}"
    );
}

#[tokio::test]
async fn a_second_request_in_the_turn_is_answered_already_in_turn_without_a_job() {
    let f = fixture(vec![vec![request(), request()], vec![]]).await;
    f.set_state(TopicState::Ready);

    let reply = f.turn("Lezione!").await;

    let jobs = f.generation_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(lesson_part(&reply), Some(jobs[0].id));
    let results = f.results_of_request(1);
    assert!(results[0].0.starts_with("Generazione avviata"));
    assert!(!results[1].1);
    assert!(
        results[1]
            .0
            .starts_with("Hai già avviato la lezione in questo turno")
    );
}

#[tokio::test]
async fn a_mission_tool_and_request_lesson_exclude_each_other_in_both_orders() {
    // Mission tool first: the lesson is refused, the card is the draft.
    let f = fixture(vec![vec![save_draft("Per lavoro"), request()], vec![]]).await;
    let reply = f.turn("Voglio imparare Rust").await;
    assert_eq!(f.generation_jobs().len(), 0);
    assert_eq!(lesson_part(&reply), None);
    assert!(
        reply
            .parts
            .iter()
            .any(|p| matches!(p, MessagePart::MissionProposal { .. }))
    );
    let results = f.results_of_request(1);
    assert!(!results[0].1);
    assert!(results[1].1);
    assert!(
        results[1]
            .0
            .starts_with("Non ammesso in questo turno: hai già usato")
    );

    // Lesson first: the mission tool is refused and stores nothing.
    let f = fixture(vec![vec![request(), save_draft("Per lavoro")], vec![]]).await;
    f.set_state(TopicState::Ready);
    let reply = f.turn("Lezione!").await;
    assert_eq!(f.generation_jobs().len(), 1);
    assert!(lesson_part(&reply).is_some());
    assert_eq!(f.repo.0.lock().unwrap().missions.len(), 0);
    assert!(
        !reply
            .parts
            .iter()
            .any(|p| matches!(p, MessagePart::MissionProposal { .. }))
    );
    let results = f.results_of_request(1);
    assert!(!results[0].1);
    assert!(results[1].1);
    assert!(results[1].0.contains("hai già avviato una lezione"));
}

#[tokio::test]
async fn the_part_stays_when_the_message_fails_after_the_tool() {
    let f = fixture(vec![
        vec![Step::Delta("Preparo."), request()],
        vec![Step::Fail(ChatErrorCode::UpstreamFailed)],
    ])
    .await;
    f.set_state(TopicState::Ready);

    let reply = f.turn("Lezione!").await;

    assert_eq!(reply.status, MessageStatus::Failed);
    let jobs = f.generation_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(
        reply.parts.last(),
        Some(&MessagePart::LessonRequest { job_id: jobs[0].id })
    );
}

/// A job store that refuses the first `conflicts` inserts as if another job
/// held the slot, then ended before anyone could read it: the race between
/// `request_generation` and `active_generation`.
struct RacyJobs {
    inner: Arc<crate::fakes::MemRepo>,
    conflicts: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl domain::JobRepository for RacyJobs {
    async fn insert(&self, job: &domain::Job) -> Result<(), domain::RepositoryError> {
        use std::sync::atomic::Ordering;
        let left = self.conflicts.load(Ordering::SeqCst);
        if left > 0 {
            self.conflicts.store(left - 1, Ordering::SeqCst);
            return Err(domain::RepositoryError::Conflict);
        }
        self.inner.insert(job).await
    }
    async fn get(
        &self,
        user: domain::UserId,
        topic_id: domain::TopicId,
        id: domain::JobId,
    ) -> Result<Option<domain::Job>, domain::RepositoryError> {
        domain::JobRepository::get(&*self.inner, user, topic_id, id).await
    }
    async fn list(
        &self,
        topic_id: domain::TopicId,
        after: Option<domain::JobCursor>,
        limit: u32,
    ) -> Result<Vec<domain::Job>, domain::RepositoryError> {
        self.inner.list(topic_id, after, limit).await
    }
    async fn latest(
        &self,
        topic_id: domain::TopicId,
        kind: JobKind,
        lesson_id: Option<domain::LessonId>,
    ) -> Result<Option<domain::Job>, domain::RepositoryError> {
        self.inner.latest(topic_id, kind, lesson_id).await
    }
    async fn next_queued(
        &self,
        kinds: &[JobKind],
    ) -> Result<Option<domain::Job>, domain::RepositoryError> {
        self.inner.next_queued(kinds).await
    }
    async fn running(&self) -> Result<Vec<domain::Job>, domain::RepositoryError> {
        self.inner.running().await
    }
    async fn update(
        &self,
        job: &domain::Job,
        expected: JobStatus,
    ) -> Result<(), domain::RepositoryError> {
        self.inner.update(job, expected).await
    }
}

/// Rewires the fixture's Tutor to a job store that loses the slot race
/// `conflicts` times.
fn with_racy_jobs(f: &mut Fixture, conflicts: usize) {
    let racy = Arc::new(RacyJobs {
        inner: f.repo.clone(),
        conflicts: std::sync::atomic::AtomicUsize::new(conflicts),
    });
    let jobs = Arc::new(JobService::new(
        racy,
        f.repo.clone(),
        f.events.clone(),
        f.clock.clone(),
        Arc::new(crate::fakes::SeqIds::default()),
        crate::fakes::owner_keys(),
    ));
    let lessons = Arc::new(LessonService::new(
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        jobs,
        crate::fakes::owner_keys(),
        f.clock.clone(),
    ));
    f.chat = ChatService::new(
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.events.clone(),
        f.llm.clone(),
        TutorPrompts::embedded(),
        f.clock.clone(),
        Arc::new(crate::fakes::SeqIds::default()),
        crate::fakes::owner_keys(),
        ChatSettings {
            flush_interval: SLOW,
            history_token_budget: 8000,
            max_tool_iterations: 4,
        },
    )
    .with_lesson_requests(lessons);
}

#[tokio::test]
async fn a_job_that_ends_during_the_request_is_retried_once_and_starts() {
    let mut f = fixture(vec![vec![request()], vec![Step::Delta("Avviata.")]]).await;
    f.set_state(TopicState::Ready);
    with_racy_jobs(&mut f, 1);

    let reply = f.turn("Lezione!").await;

    let jobs = f.generation_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(lesson_part(&reply), Some(jobs[0].id));
    let results = f.results_of_request(1);
    assert!(!results[0].1);
    assert!(
        results[0].0.starts_with("Generazione avviata"),
        "{results:?}"
    );
}

#[tokio::test]
async fn a_second_failed_attempt_after_the_race_is_an_internal_error_without_part() {
    let mut f = fixture(vec![vec![request()], vec![Step::Delta("Errore.")]]).await;
    f.set_state(TopicState::Ready);
    with_racy_jobs(&mut f, 2);

    let reply = f.turn("Lezione!").await;

    assert_eq!(f.generation_jobs().len(), 0);
    assert_eq!(lesson_part(&reply), None);
    let results = f.results_of_request(1);
    assert!(results[0].1, "{results:?}");
    assert!(results[0].0.contains("Errore interno"), "{results:?}");
}

#[tokio::test]
async fn retrying_a_failed_message_whose_job_succeeded_starts_a_new_job() {
    let f = fixture(vec![
        vec![Step::Delta("Preparo."), request()],
        vec![Step::Fail(ChatErrorCode::UpstreamFailed)],
        vec![request()],
        vec![Step::Delta("Di nuovo.")],
    ])
    .await;
    f.set_state(TopicState::Ready);
    let failed = f.turn("Lezione!").await;
    assert_eq!(failed.status, MessageStatus::Failed);
    let first = lesson_part(&failed).expect("part of the first attempt");
    // The first job finishes while the message sits failed.
    {
        let mut s = f.repo.0.lock().unwrap();
        let job = s.jobs.iter_mut().find(|j| j.id == first).unwrap();
        job.status = JobStatus::Succeeded;
    }

    f.chat
        .retry(domain::UserId::OWNER, f.topic, failed.id)
        .await
        .unwrap();
    let done = f.wait_status(failed.id, MessageStatus::Complete).await;

    let jobs = f.generation_jobs();
    assert_eq!(jobs.len(), 2, "{jobs:?}");
    let second = jobs.iter().find(|j| j.id != first).unwrap();
    assert_eq!(lesson_part(&done), Some(second.id));
    assert_eq!(done.parts.len(), 2, "{:?}", done.parts);
}
