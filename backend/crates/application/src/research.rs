//! The Researcher (phase 4): the `research` job handler and the use cases
//! behind `/topics/{id}/resources`. Design: `backend/prompts/llm-port.md` §12;
//! contract: `docs/contracts/resources.md`.

use std::fmt::Write as _;
use std::sync::Arc;

use async_trait::async_trait;
use domain::llm::{
    LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmRole, MessageRole, SystemBlock, ToolDef,
};
use domain::{
    Clock, DocumentRepository, DocumentView, Gap, IdGenerator, Job, JobErrorCode, JobKind,
    JobProgress, JobResult, Mission, MissionRepository, Preference, PreferenceRepository,
    RepositoryError, ResearchInfo, ResearchOutcome, Resource, ResourceId, ResourceKind,
    ResourceRepository, Topic, TopicEvent, TopicEventPayload, TopicEventPublisher, TopicId,
    TopicRepository, TopicResources, UserId,
};

use crate::context::{
    PREFERENCES_SHOWN, bullet_list, one_line, render_preferences_section, starting_point_text,
};
use crate::document_context::documents_index;
use crate::error::{AppError, ConflictReason};
use crate::jobs::{JobContext, JobFailure, JobHandler, JobService};
use crate::prompts::{DocumentsSection, ResearcherPrompts, render_with_documents};
use crate::research_output::{SeenUrls, ValidOutput, parse_and_validate};
use crate::structured::{self, Continuations, OutputSchema, RepairSpec, Spent, TurnObserver};
use crate::time_util::now_millis;

/// The only user turn of the research conversation. The brief is in the
/// system and context blocks.
const KICKOFF: &str = "Procedi con la ricerca delle fonti.";

/// Allowed URLs listed in a repair request; more would only cost tokens.
const REPAIR_URLS_SHOWN: usize = 80;

#[derive(Debug, Clone, Copy)]
pub struct ResearchSettings {
    /// `max_uses` of the web search, across continuations.
    pub max_searches: u32,
    /// How many times a paused turn is continued before giving up.
    pub max_continuations: u32,
}

impl Default for ResearchSettings {
    fn default() -> Self {
        Self {
            max_searches: 4,
            max_continuations: 5,
        }
    }
}

/// Use case `RunResearch`, registered as the handler of `research` jobs.
pub struct RunResearch {
    topics: Arc<dyn TopicRepository>,
    missions: Arc<dyn MissionRepository>,
    preferences: Arc<dyn PreferenceRepository>,
    resources: Arc<dyn ResourceRepository>,
    events: Arc<dyn TopicEventPublisher>,
    llm: Arc<dyn LlmProvider>,
    prompts: ResearcherPrompts,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    settings: ResearchSettings,
    documents: Option<Arc<dyn DocumentRepository>>,
}

impl RunResearch {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        topics: Arc<dyn TopicRepository>,
        missions: Arc<dyn MissionRepository>,
        preferences: Arc<dyn PreferenceRepository>,
        resources: Arc<dyn ResourceRepository>,
        events: Arc<dyn TopicEventPublisher>,
        llm: Arc<dyn LlmProvider>,
        prompts: ResearcherPrompts,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        settings: ResearchSettings,
    ) -> Self {
        Self {
            topics,
            missions,
            preferences,
            resources,
            events,
            llm,
            prompts,
            clock,
            ids,
            settings,
            documents: None,
        }
    }

    /// Lets the Researcher see the topic's uploaded documents, to look for
    /// sources that complete them.
    #[must_use]
    pub fn with_documents(mut self, documents: Arc<dyn DocumentRepository>) -> Self {
        self.documents = Some(documents);
        self
    }
}

#[async_trait]
impl JobHandler for RunResearch {
    fn needs_llm(&self) -> bool {
        true
    }

    async fn run(&self, ctx: &JobContext) -> Result<JobResult, JobFailure> {
        let job = ctx.job().await;
        self.research(ctx, &job).await.map_err(JobFailure)
    }
}

impl RunResearch {
    async fn research(&self, ctx: &JobContext, job: &Job) -> Result<JobResult, JobErrorCode> {
        let topic_id = job.topic_id;
        let internal = |what: &str, e: &dyn std::fmt::Display| {
            tracing::error!(job = %job.id, error = %e, "research: cannot {what}");
            JobErrorCode::Internal
        };
        let Some(topic) = self
            .topics
            .get_system(topic_id)
            .await
            .map_err(|e| internal("read the topic", &e))?
        else {
            tracing::warn!(job = %job.id, "research: the topic is gone");
            return Err(JobErrorCode::Internal);
        };
        let Some(mission) = self
            .missions
            .current(topic_id)
            .await
            .map_err(|e| internal("read the mission", &e))?
        else {
            tracing::error!(job = %job.id, "research: the topic has no confirmed mission");
            return Err(JobErrorCode::Internal);
        };
        let preferences = self
            .preferences
            .latest_for_topic(
                // The topic's owner: another user's global preferences never
                // reach this prompt.
                topic.user_id,
                topic_id,
                u32::try_from(PREFERENCES_SHOWN + 1).unwrap_or(u32::MAX),
            )
            .await
            .map_err(|e| internal("read the preferences", &e))?;
        let existing = self
            .resources
            .get(topic_id)
            .await
            .map_err(|e| internal("read the current sources", &e))?;

        let documents = match &self.documents {
            Some(repo) => repo
                .list(topic_id)
                .await
                .map_err(|e| internal("read the documents", &e))?,
            None => Vec::new(),
        };

        if !self.llm.capabilities(LlmRole::Researcher).server_web_search {
            tracing::error!("the model configured for the researcher cannot run web searches");
            return Err(JobErrorCode::Internal);
        }

        let mut seen = SeenUrls::default();
        for resource in &existing.resources {
            if let Some(url) = &resource.url {
                seen.add(url);
            }
        }

        let mut request =
            self.build_request(&topic, &mission, &preferences, &existing, &documents)?;
        let lang = ctx.language();
        ctx.progress(
            JobProgress::step("planning")
                .with_message(lang.pick("Ho letto la mission", "I read the mission")),
        )
        .await;

        let mut spent = Spent::default();
        let answer = self
            .search(ctx, &mut request, &mut seen, &mut spent)
            .await?;

        let valid = self.validated(ctx, job, &answer, &seen, &mut spent).await?;
        for warning in &valid.warnings {
            tracing::warn!(job = %job.id, warning, "research: output warning");
        }

        let count = u32::try_from(valid.resources.len()).unwrap_or(u32::MAX);
        let gap_count = u32::try_from(valid.gaps.len()).unwrap_or(u32::MAX);
        ctx.progress(JobProgress::step("saving").with_message(lang.pick(
            &format!("Ho verificato {count} fonti"),
            &format!("I checked {count} sources"),
        )))
        .await;
        self.save(job, &mission, valid).await?;
        tracing::info!(
            job = %job.id,
            prompt_version = self.prompts.version.trim(),
            calls = spent.calls,
            input_tokens = spent.usage.input_tokens,
            output_tokens = spent.usage.output_tokens,
            cache_read_tokens = spent.usage.cache_read_tokens,
            cache_write_tokens = spent.usage.cache_write_tokens,
            web_searches = spent.usage.web_searches,
            resources = count,
            gaps = gap_count,
            "research finished"
        );
        Ok(JobResult::Research {
            resource_count: count,
            gap_count,
        })
    }

    /// Calls the model until it stops for a reason other than a pause,
    /// continuing paused turns (the loop shared with the Generator), and
    /// collects the URLs it was shown.
    async fn search(
        &self,
        ctx: &JobContext,
        request: &mut LlmRequest,
        seen: &mut SeenUrls,
        spent: &mut Spent,
    ) -> Result<LlmResponse, JobErrorCode> {
        let mut observer = SearchProgress {
            ctx,
            seen,
            searches: 0,
        };
        structured::run_turn(
            self.llm.as_ref(),
            self.prompts.version,
            request,
            spent,
            Continuations {
                max: self.settings.max_continuations,
                strict: false,
                continue_when_spent: false,
            },
            &mut observer,
        )
        .await
    }

    /// The answer as a valid list: as given, or after one repair.
    async fn validated(
        &self,
        ctx: &JobContext,
        job: &Job,
        answer: &LlmResponse,
        seen: &SeenUrls,
        spent: &mut Spent,
    ) -> Result<ValidOutput, JobErrorCode> {
        ctx.progress(
            JobProgress::step("evaluating").with_message(ctx.language().pick(
                "Ho ricevuto la proposta di fonti",
                "I received the proposed sources",
            )),
        )
        .await;
        let text = structured::final_text(&answer.message);
        let problems = match parse_and_validate(&text, seen) {
            Ok(valid) => return Ok(valid),
            Err(problems) => problems,
        };
        tracing::warn!(job = %job.id, problems = problems.len(), "research: output rejected, repairing");
        ctx.progress(
            JobProgress::step("evaluating").with_message(ctx.language().pick(
                "Ho chiesto di correggere l'elenco",
                "I asked for the list to be corrected",
            )),
        )
        .await;
        let fixed = self.repair(&text, &problems, seen, spent).await?;
        parse_and_validate(&fixed, seen).map_err(|problems| {
            tracing::warn!(job = %job.id, ?problems, "research: repair did not help");
            JobErrorCode::InvalidOutput
        })
    }

    /// The brief: static system block, then the block that varies per topic.
    fn build_request(
        &self,
        topic: &Topic,
        mission: &Mission,
        preferences: &[Preference],
        existing: &TopicResources,
        documents: &[DocumentView],
    ) -> Result<LlmRequest, JobErrorCode> {
        let today = self.clock.now().date().to_string();
        let index = documents_index(documents, false);
        let context = render_with_documents(
            self.prompts.context_template,
            topic.language,
            &[
                ("topic_title", &one_line(topic.title.as_str())),
                ("today", &today),
                ("mission_version", &mission.version.to_string()),
                ("mission_why", &one_line(mission.content.why())),
                (
                    "mission_success",
                    &bullet_list(mission.content.success_criteria()),
                ),
                ("mission_starting_point", &starting_point_text(mission)),
                (
                    "mission_constraints",
                    &bullet_list(mission.content.constraints()),
                ),
                (
                    "mission_out_of_scope",
                    &bullet_list(mission.content.out_of_scope()),
                ),
                (
                    "preferences_section",
                    &render_preferences_section(preferences),
                ),
                (
                    "existing_resources_section",
                    &render_existing_resources(&existing.resources),
                ),
                ("search_budget", &self.settings.max_searches.to_string()),
            ],
            &DocumentsSection(index.as_deref()),
        )
        .map_err(|e| {
            tracing::error!(error = %e, "research: cannot render the prompt");
            JobErrorCode::Internal
        })?;

        let mut request = LlmRequest::new(LlmRole::Researcher, "researcher.search");
        request.system = vec![
            SystemBlock {
                text: self.prompts.system.to_owned(),
                cache_breakpoint: true,
            },
            SystemBlock {
                text: context,
                cache_breakpoint: false,
            },
        ];
        request.messages = vec![LlmMessage::text(MessageRole::User, KICKOFF)];
        request.tools = vec![ToolDef::ServerWebSearch {
            max_uses: self.settings.max_searches,
            allowed_domains: Vec::new(),
            blocked_domains: Vec::new(),
        }];
        Ok(request)
    }

    /// One attempt to fix a rejected answer: no tools, native structured
    /// output. It cannot add sources the searches did not return, because the
    /// Validator checks the same set again.
    async fn repair(
        &self,
        text: &str,
        problems: &[String],
        seen: &SeenUrls,
        spent: &mut Spent,
    ) -> Result<String, JobErrorCode> {
        let schema =
            OutputSchema::parse(self.prompts.output_schema, "researcher_output").map_err(|e| {
                tracing::error!(error = %e, "the researcher output schema is not valid JSON");
                JobErrorCode::Internal
            })?;
        let mut allowed = String::from("URL ammessi:\n");
        if seen.is_empty() {
            allowed.push_str("(nessuno)\n");
        }
        for url in seen.urls().iter().take(REPAIR_URLS_SHOWN) {
            let _ = writeln!(allowed, "- {url}");
        }
        let spec = RepairSpec {
            role: LlmRole::Researcher,
            trace_label: "researcher.repair",
            system: vec![SystemBlock {
                text: self.prompts.repair_system.trim().to_owned(),
                cache_breakpoint: false,
            }],
            schema: &schema,
            prompt_version: self.prompts.version,
            max_output_tokens: None,
            strict: false,
        };
        let message = structured::repair_message(text, problems, Some(&allowed));
        structured::repair_once(self.llm.as_ref(), &spec, message, spent).await
    }

    /// Replaces the topic's sources in one transaction (which also moves a
    /// `researching` topic to `ready`) and announces the topic change.
    async fn save(
        &self,
        job: &Job,
        mission: &Mission,
        valid: ValidOutput,
    ) -> Result<(), JobErrorCode> {
        let now = now_millis(self.clock.as_ref());
        let outcome = ResearchOutcome {
            topic_id: job.topic_id,
            info: ResearchInfo {
                job_id: job.id,
                mission_version: mission.version,
                finished_at: now,
            },
            communities_skipped: valid.communities_skipped,
            resources: valid
                .resources
                .into_iter()
                .map(|draft| (ResourceId::new(self.ids.new_id()), draft))
                .collect(),
            gaps: valid.gaps,
        };
        let moved = self.resources.replace(&outcome, now).await.map_err(|e| {
            if !matches!(e, RepositoryError::NotFound) {
                tracing::error!(job = %job.id, error = %e, "research: cannot store the sources");
            }
            JobErrorCode::Internal
        })?;
        if moved {
            match self.topics.get_system(job.topic_id).await {
                Ok(Some(topic)) => self.events.publish(TopicEvent {
                    topic_id: job.topic_id,
                    at: now,
                    payload: TopicEventPayload::TopicUpdated(topic),
                }),
                Ok(None) => None,
                Err(e) => {
                    tracing::warn!(error = %e, "cannot reload the topic after the research");
                    None
                }
            };
        }
        Ok(())
    }
}

/// Format fixed by `backend/prompts/README.md`.
fn render_existing_resources(resources: &[Resource]) -> String {
    if resources.iter().all(|r| r.kind == ResourceKind::Document) {
        return "Nessuna.".to_owned();
    }
    // Uploaded documents are listed in their own section: they are not the
    // Researcher's to keep, replace or repeat.
    resources
        .iter()
        .filter(|r| r.kind != ResourceKind::Document)
        .map(|r| {
            format!(
                "- [{}] {} — {} — {}",
                r.kind,
                one_line(&r.title),
                r.url.as_deref().unwrap_or("nessun sito"),
                one_line(&r.use_for)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What `GET /topics/{id}/resources` returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcesView {
    pub resources: Vec<Resource>,
    pub gaps: Vec<Gap>,
    pub communities_skipped: bool,
    pub research: Option<ResearchInfo>,
    /// The latest `research` job, whatever its status.
    pub job: Option<Job>,
}

/// The user's side of the Researcher: reading the sources and asking for a
/// new research.
pub struct ResourceService {
    topics: Arc<dyn TopicRepository>,
    missions: Arc<dyn MissionRepository>,
    resources: Arc<dyn ResourceRepository>,
    jobs: Arc<JobService>,
}

impl ResourceService {
    #[must_use]
    pub fn new(
        topics: Arc<dyn TopicRepository>,
        missions: Arc<dyn MissionRepository>,
        resources: Arc<dyn ResourceRepository>,
        jobs: Arc<JobService>,
    ) -> Self {
        Self {
            topics,
            missions,
            resources,
            jobs,
        }
    }

    pub async fn get(&self, user: UserId, topic_id: TopicId) -> Result<ResourcesView, AppError> {
        self.require_topic(user, topic_id).await?;
        let TopicResources {
            resources,
            gaps,
            communities_skipped,
            research,
        } = self.resources.get(topic_id).await?;
        Ok(ResourcesView {
            resources,
            gaps,
            communities_skipped,
            research,
            job: self.jobs.latest(topic_id, JobKind::Research).await?,
        })
    }

    /// Queues a new research. Needs a confirmed mission and no research
    /// already queued or running. The topic keeps its state.
    pub async fn start_research(&self, user: UserId, topic_id: TopicId) -> Result<Job, AppError> {
        self.require_topic(user, topic_id).await?;
        if self.missions.current(topic_id).await?.is_none() {
            return Err(AppError::Conflict(ConflictReason::NoConfirmedMission));
        }
        self.jobs.enqueue(user, topic_id, JobKind::Research).await
    }

    async fn require_topic(&self, user: UserId, id: TopicId) -> Result<(), AppError> {
        self.topics
            .get(user, id)
            .await?
            .map(|_| ())
            .ok_or(AppError::NotFound)
    }
}

/// Tells the job what each answer of the search turn did and remembers the
/// URLs the model was shown (the Validator's set of allowed URLs).
struct SearchProgress<'a> {
    ctx: &'a JobContext,
    seen: &'a mut SeenUrls,
    /// Searches run so far, for the progress counter.
    searches: u32,
}

#[async_trait]
impl TurnObserver for SearchProgress<'_> {
    async fn observe(&mut self, response: &LlmResponse) {
        for url in &response.search_result_urls {
            self.seen.add(url);
        }
        for call in response
            .server_tool_uses
            .iter()
            .filter(|c| c.name == "web_search")
        {
            self.searches += 1;
            let lang = self.ctx.language();
            let message = match call.query() {
                Some(query) => lang
                    .pick(
                        &format!("Ho cercato: {query}"),
                        &format!("I searched: {query}"),
                    )
                    .to_owned(),
                None => lang
                    .pick("Ho fatto una ricerca", "I ran a search")
                    .to_owned(),
            };
            self.ctx
                .progress(
                    JobProgress::step("searching")
                        .with_message(&message)
                        .with_done(self.searches),
                )
                .await;
        }
        if !response.search_result_urls.is_empty() {
            self.ctx
                .progress(JobProgress::step("reading").with_message(&{
                    let n = response.search_result_urls.len();
                    self.ctx
                        .language()
                        .pick(
                            &format!("Ho ricevuto {n} risultati"),
                            &format!("I received {n} results"),
                        )
                        .to_owned()
                }))
                .await;
        }
    }
}
