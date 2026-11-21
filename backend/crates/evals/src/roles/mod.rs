//! One module per role: build the case's world, run the real use case, score
//! its output. Everything role-independent is here.

use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;
use domain::JobKind;
use domain::TopicId;
use domain::llm::{LlmProvider, LlmRole};
use serde_json::Value;

use crate::judge::Judge;
use crate::llm::{CallRecord, InjectingLlm, Ledger, PlannedLlm, Source};
use crate::report::RoleRun;
use crate::seed::{App, Tuning};
use crate::suite::{Case, Role, Suite};

pub mod fetch;
pub mod generator;
pub mod grader;
pub mod language;
pub mod planner;
pub mod recorder;
pub mod researcher;
pub mod sdui;
pub mod shape;
pub mod tutor;

/// What a role's run needs from the outside.
pub struct Env {
    /// The provider the use cases talk to (records, and answers the Planner
    /// from the suite when asked to).
    pub llm: Arc<dyn LlmProvider>,
    pub planned: Arc<PlannedLlm>,
    /// Puts a `fixture_pages` page in the Generator's conversation as already
    /// read (the provider cannot read a page nobody publishes).
    pub injector: Arc<InjectingLlm>,
    pub judge: Arc<Judge>,
    pub ledger: Arc<Ledger>,
    pub tuning: Tuning,
}

pub async fn run(env: &Env, suite: &Suite, case: &Case) -> anyhow::Result<RoleRun> {
    match suite.role {
        Role::Tutor => tutor::run(env, suite, case).await,
        Role::Researcher => researcher::run(env, suite, case).await,
        Role::Planner => planner::run(env, suite, case).await,
        Role::Generator => generator::run(env, suite, case).await,
        Role::Grader => grader::run(env, suite, case).await,
        Role::Recorder => recorder::run(env, suite, case).await,
    }
}

/// The version string of the role's prompts, as the backend logs it.
#[must_use]
pub fn prompt_version(role: Role) -> String {
    use application::{LessonPrompts, ResearcherPrompts, TutorPrompts};
    let lessons = LessonPrompts::embedded();
    match role {
        Role::Tutor => TutorPrompts::embedded().version,
        Role::Researcher => ResearcherPrompts::embedded().version,
        Role::Planner => lessons.planner.version,
        Role::Generator => lessons.generator.version,
        Role::Grader => lessons.grader.version,
        Role::Recorder => lessons.recorder.version,
    }
    .trim()
    .to_owned()
}

/// How much of a rejected answer a report keeps: enough to see what the model
/// wrote (a cut-off JSON, a refusal, prose instead of the object).
pub const RESPONSE_HEAD_CHARS: usize = 2_000;

/// The first [`RESPONSE_HEAD_CHARS`] characters of `text`.
#[must_use]
pub fn response_head(text: &str) -> String {
    text.chars().take(RESPONSE_HEAD_CHARS).collect()
}

/// Calls of the model under evaluation recorded since `mark`, for one role.
pub fn subject_calls(env: &Env, mark: usize, role: LlmRole) -> Vec<CallRecord> {
    env.ledger
        .since(mark)
        .into_iter()
        .filter(|c| c.source == Source::Subject && c.role == role)
        .collect()
}

/// Waits until the latest job of `kind` for the topic (and lesson) is over.
pub async fn wait_for_job(
    app: &App,
    kind: JobKind,
    lesson: Option<domain::LessonId>,
    timeout: Duration,
) -> anyhow::Result<domain::Job> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let job = app
            .state
            .jobs
            .latest_matching(app.topic, kind, lesson)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        if let Some(job) = job
            && matches!(
                job.status,
                domain::JobStatus::Succeeded | domain::JobStatus::Failed
            )
        {
            return Ok(job);
        }
        if tokio::time::Instant::now() > deadline {
            bail!("the {kind:?} job did not finish in {timeout:?}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A one-line description of a failed job.
#[must_use]
pub fn job_failure(job: &domain::Job) -> String {
    job.error.as_ref().map_or_else(
        || "failed".to_owned(),
        |e| format!("{:?}: {}", e.code, e.message),
    )
}

pub fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub fn topic_id(app: &App) -> TopicId {
    app.topic
}
