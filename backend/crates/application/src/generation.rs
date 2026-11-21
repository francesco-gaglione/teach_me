//! Lesson generation (phase 6): the `lesson_generation` job handler.
//! Planner → Generator → Validator → up to two repairs → atomic save.
//! Contract: `docs/contracts/lessons.md` §1 and §4.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use domain::ContentLanguage;
use domain::llm::{
    LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmRole, MessageRole, SystemBlock,
};
use domain::{
    Clock, ComponentRequest, ComponentRequestId, DocumentRepository, DocumentStatus, DocumentView,
    GeneratedLesson, GlossaryTerm, IdGenerator, Job, JobErrorCode, JobProgress, JobResult,
    LearningRecord, LearningRepository, Lesson, LessonId, LessonMeta, LessonRepository,
    LessonValidator, Mission, MissionRepository, Preference, PreferenceRepository, RepositoryError,
    Resource, ResourceKind, ResourceRepository, Topic, TopicRepository, TopicState,
    ValidationContext, ValidatorConfig, component_request_drafts,
};
use serde_json::Value;

use crate::audio_queue::AudioPreparer;
use crate::context::{
    PREFERENCES_SHOWN, bullet_list, one_line, render_preferences_section, starting_point_text,
};
use crate::document_context::{
    ExcerptDocument, ExcerptSection, documents_index, excerpts_section, sections_used,
};
use crate::jobs::{JobContext, JobFailure, JobHandler};
use crate::lesson_context::{
    generator_plan_section, generator_records_section, generator_sources_section, glossary_section,
    lesson_links_section, lessons_section, planner_resources_section, records_section,
    usable_sources,
};
use crate::lesson_plan::{PlanDocument, PlanFacts, ValidPlan, validate_plan};
use crate::prompts::{
    DocumentsSection, LessonPrompts, RolePrompts, render_template_in, render_with_documents,
};
use crate::structured::{
    self, Continuations, OutputSchema, RepairSpec, Spent, TurnObserver, bullet_problems,
    final_text, parse_object,
};
use crate::time_util::now_millis;
use crate::web_fetch::plan_web_fetch;

/// How many records and terms of the Topic the Planner is shown.
const CONTEXT_ROWS: u32 = 200;
/// Problems listed in one repair request: enough to fix a document, short
/// enough not to bury it.
const MAX_REPORTED_ISSUES: usize = 30;

/// The task is in the context block; the one user turn is fixed.
const KICKOFF: &str = "Procedi.";
/// Longest previous document quoted in a repair (`README.md`).
const PREVIOUS_DOCUMENT_MAX_CHARS: usize = 40_000;
const PREVIOUS_DOCUMENT_OMITTED: &str =
    "(omesso perché troppo lungo: riscrivi la lezione più corta)";
/// With the web fetch tool the repair goes on in the same conversation, where
/// the document is already the last answer: quoting it again cost ~7k tokens.
const PREVIOUS_DOCUMENT_IN_CONVERSATION: &str =
    "(è la tua ultima risposta in questa conversazione, qui sopra)";

/// Knobs of phases 6 and 7.
#[derive(Debug, Clone, Copy)]
pub struct LessonSettings {
    /// Repairs of the Generator's document: 2 means up to 3 documents.
    pub max_document_repairs: u32,
    pub recorder_max_output_tokens: u32,
    pub grader_max_output_tokens: u32,
    /// Longest wait for the Grader inside a request (`lessons.md` §5.1).
    pub grader_timeout: Duration,
    /// Pages the Generator may read with the provider's web fetch tool per
    /// lesson (`[llm.roles.generator] max_fetches`); 0 switches the tool off
    /// (ADR 0009).
    pub max_web_fetches: u32,
    /// How many times a paused Generator turn is continued before giving up.
    pub max_continuations: u32,
}

impl Default for LessonSettings {
    fn default() -> Self {
        Self {
            max_document_repairs: 2,
            recorder_max_output_tokens: 6_000,
            grader_max_output_tokens: 1_500,
            grader_timeout: Duration::from_secs(20),
            max_web_fetches: 0,
            max_continuations: 5,
        }
    }
}

/// What the Topic knows, read once at the start of a generation.
struct Brief {
    topic: Topic,
    mission: Mission,
    preferences: Vec<Preference>,
    resources: Vec<Resource>,
    /// Active, oldest first.
    records: Vec<LearningRecord>,
    glossary: Vec<GlossaryTerm>,
    lessons: Vec<Lesson>,
    /// Uploaded documents, newest first (empty when the app has none).
    documents: Vec<DocumentView>,
    /// What each lesson covered of the documents, by lesson id.
    covered: HashMap<LessonId, String>,
}

/// Use case `GenerateLesson`, registered as the handler of `lesson_generation`.
pub struct GenerateLesson {
    topics: Arc<dyn TopicRepository>,
    missions: Arc<dyn MissionRepository>,
    preferences: Arc<dyn PreferenceRepository>,
    resources: Arc<dyn ResourceRepository>,
    lessons: Arc<dyn LessonRepository>,
    learning: Arc<dyn LearningRepository>,
    llm: Arc<dyn LlmProvider>,
    prompts: LessonPrompts,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    settings: LessonSettings,
    documents: Option<Arc<dyn DocumentRepository>>,
    /// `None` when read-aloud is off.
    audio: Option<Arc<AudioPreparer>>,
}

impl GenerateLesson {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        topics: Arc<dyn TopicRepository>,
        missions: Arc<dyn MissionRepository>,
        preferences: Arc<dyn PreferenceRepository>,
        resources: Arc<dyn ResourceRepository>,
        lessons: Arc<dyn LessonRepository>,
        learning: Arc<dyn LearningRepository>,
        llm: Arc<dyn LlmProvider>,
        prompts: LessonPrompts,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        settings: LessonSettings,
    ) -> Self {
        Self {
            topics,
            missions,
            preferences,
            resources,
            lessons,
            learning,
            llm,
            prompts,
            clock,
            ids,
            settings,
            documents: None,
            audio: None,
        }
    }

    /// Lets lessons follow the topic's uploaded documents.
    #[must_use]
    pub fn with_documents(mut self, documents: Arc<dyn DocumentRepository>) -> Self {
        self.documents = Some(documents);
        self
    }

    /// Queues the audio of every lesson it saves (ADR 0013 §5).
    #[must_use]
    pub fn with_audio(mut self, audio: Arc<AudioPreparer>) -> Self {
        self.audio = Some(audio);
        self
    }

    /// Queues the blocks of the saved lesson for the owner's voice. The lesson
    /// is already saved and the job succeeded as far as the user can tell, so
    /// a failure here is logged and never fails the job.
    async fn prepare_audio(&self, job: &Job, lesson: &Lesson) {
        let Some(audio) = &self.audio else { return };
        let result = async {
            let topic = self
                .topics
                .get_system(job.topic_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("the topic is gone")?;
            let stored = self
                .lessons
                .get(topic.user_id, lesson.id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("the lesson is gone")?;
            audio
                .prepare(
                    topic.user_id,
                    lesson.id,
                    &stored.document,
                    topic.language,
                    false,
                )
                .await
                .map_err(|e| e.to_string())
        }
        .await;
        if let Err(reason) = result {
            tracing::warn!(lesson = %lesson.id, error = %reason, "could not queue the audio of a new lesson");
        }
    }
}

#[async_trait]
impl JobHandler for GenerateLesson {
    fn needs_llm(&self) -> bool {
        true
    }

    async fn run(&self, ctx: &JobContext) -> Result<JobResult, JobFailure> {
        let job = ctx.job().await;
        self.generate(ctx, &job).await.map_err(JobFailure)
    }
}

fn internal(job: &Job, what: &str, e: &dyn std::fmt::Display) -> JobErrorCode {
    tracing::error!(job = %job.id, error = %e, "lesson generation: cannot {what}");
    JobErrorCode::Internal
}

fn schema_of(prompts: &RolePrompts, fallback: &str) -> Result<Option<OutputSchema>, JobErrorCode> {
    prompts
        .output_schema
        .map(|raw| OutputSchema::parse(raw, fallback))
        .transpose()
        .map_err(|e| {
            tracing::error!(error = %e, role = fallback, "the output schema is not valid JSON");
            JobErrorCode::Internal
        })
}

fn render_error(e: &crate::prompts::TemplateError) -> JobErrorCode {
    tracing::error!(error = %e, "lesson generation: cannot render a prompt");
    JobErrorCode::Internal
}

impl GenerateLesson {
    async fn generate(&self, ctx: &JobContext, job: &Job) -> Result<JobResult, JobErrorCode> {
        // A run repeated after a crash must not write (and pay for) a second
        // lesson when the first run got as far as saving.
        if let Some(lesson) = self
            .lessons
            .by_generation_job(job.id)
            .await
            .map_err(|e| internal(job, "look for a saved lesson", &e))?
        {
            // The earlier run may have died before queueing the audio.
            self.prepare_audio(job, &lesson).await;
            return Ok(result_of(&lesson));
        }
        let brief = self.load(job).await?;
        let mut spent = Spent::default();

        let lang = ctx.language();
        ctx.progress(
            JobProgress::step("planning")
                .with_message(lang.pick("Ho letto la mission", "I read the mission")),
        )
        .await;
        let plan = self.plan(job, &brief, &mut spent).await?;

        if !plan.document_sections.is_empty() || self.settings.max_web_fetches > 0 {
            ctx.progress(
                JobProgress::step("reading")
                    .with_message(lang.pick("Leggo le fonti", "Reading the sources")),
            )
            .await;
        }
        let objective = plan.raw["objective"].as_str().unwrap_or_default();
        ctx.progress(
            JobProgress::step("writing")
                .with_message(lang.pick(
                    &format!("Ho scelto: {}", one_line(objective)),
                    &format!("I chose: {}", one_line(objective)),
                ))
                .with_done(1),
        )
        .await;
        let (document, warnings) = self.write(ctx, job, &brief, &plan, &mut spent).await?;

        ctx.progress(JobProgress::step("saving")).await;
        let lesson = self.save(job, &brief, plan, document, warnings).await?;
        tracing::info!(
            job = %job.id,
            lesson = %lesson.id,
            calls = spent.calls,
            input_tokens = spent.usage.input_tokens,
            output_tokens = spent.usage.output_tokens,
            cache_read_tokens = spent.usage.cache_read_tokens,
            cache_write_tokens = spent.usage.cache_write_tokens,
            web_fetches = spent.usage.web_fetches,
            "lesson generated"
        );
        self.prepare_audio(job, &lesson).await;
        Ok(result_of(&lesson))
    }

    async fn load(&self, job: &Job) -> Result<Brief, JobErrorCode> {
        let topic_id = job.topic_id;
        let topic = self
            .topics
            .get_system(topic_id)
            .await
            .map_err(|e| internal(job, "read the topic", &e))?
            .ok_or_else(|| internal(job, "read the topic", &"it is gone"))?;
        if topic.state != TopicState::Ready {
            return Err(internal(job, "generate", &"the topic is not ready"));
        }
        let mission = self
            .missions
            .current(topic_id)
            .await
            .map_err(|e| internal(job, "read the mission", &e))?
            .ok_or_else(|| internal(job, "read the mission", &"there is none confirmed"))?;
        let preferences = self
            .preferences
            .latest_for_topic(
                // The topic's owner: no other user's global preferences.
                topic.user_id,
                topic_id,
                u32::try_from(PREFERENCES_SHOWN + 1).unwrap_or(u32::MAX),
            )
            .await
            .map_err(|e| internal(job, "read the preferences", &e))?;
        let resources = self
            .resources
            .get(topic_id)
            .await
            .map_err(|e| internal(job, "read the sources", &e))?
            .resources;
        let mut records = self
            .learning
            .list_records(topic_id, true, None, CONTEXT_ROWS)
            .await
            .map_err(|e| internal(job, "read the records", &e))?;
        records.reverse();
        let glossary = self
            .learning
            .list_terms(topic_id, None, CONTEXT_ROWS)
            .await
            .map_err(|e| internal(job, "read the glossary", &e))?;
        let lessons = self
            .lessons
            .all_in_topic(topic_id)
            .await
            .map_err(|e| internal(job, "read the lessons", &e))?;
        let documents = match &self.documents {
            Some(repo) => repo
                .list(topic_id)
                .await
                .map_err(|e| internal(job, "read the documents", &e))?,
            None => Vec::new(),
        };
        let covered = if documents.is_empty() {
            HashMap::new()
        } else {
            self.lessons
                .plans(topic_id)
                .await
                .map_err(|e| internal(job, "read the lesson plans", &e))?
                .into_iter()
                .filter_map(|(id, plan)| Some((id, sections_used(&plan, &documents)?)))
                .collect()
        };
        Ok(Brief {
            topic,
            mission,
            preferences,
            resources,
            records,
            glossary,
            lessons,
            documents,
            covered,
        })
    }

    /// The system blocks every call of a role shares: static text (cacheable)
    /// then the context of this topic.
    fn system_blocks(role: &RolePrompts, context: String) -> Vec<SystemBlock> {
        vec![
            SystemBlock {
                text: role.system.to_owned(),
                cache_breakpoint: true,
            },
            SystemBlock {
                text: context,
                cache_breakpoint: false,
            },
        ]
    }

    // ----- Planner -----

    #[allow(clippy::too_many_lines)] // one pass: build the brief, validate, repair
    async fn plan(
        &self,
        job: &Job,
        brief: &Brief,
        spent: &mut Spent,
    ) -> Result<ValidPlan, JobErrorCode> {
        let role = &self.prompts.planner;
        let schema = schema_of(role, "lesson_plan")?;
        let mission = &brief.mission;
        let sources = usable_sources(&brief.resources);
        if sources.is_empty() {
            tracing::error!(
                job = %job.id,
                "lesson generation: the topic has no usable source (a knowledge source with an https URL, or an uploaded document); the Planner is not called"
            );
            return Err(JobErrorCode::InvalidOutput);
        }
        let index = documents_index(&brief.documents, true);
        let context = render_with_documents(
            role.context_template,
            brief.topic.language,
            &[
                ("topic_title", &one_line(brief.topic.title.as_str())),
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
                    &render_preferences_section(&brief.preferences),
                ),
                (
                    "records_section",
                    &records_section(&brief.records, &brief.lessons),
                ),
                ("glossary_section", &glossary_section(&brief.glossary)),
                (
                    "lessons_section",
                    &lessons_section(&brief.lessons, &brief.covered),
                ),
                ("resources_section", &planner_resources_section(&sources)),
            ],
            &DocumentsSection(index.as_deref()),
        )
        .map_err(|e| render_error(&e))?;
        let system = Self::system_blocks(role, context);

        let mut request = LlmRequest::new(LlmRole::Planner, "planner.plan");
        request.system.clone_from(&system);
        request.messages = vec![LlmMessage::text(MessageRole::User, KICKOFF)];
        if let Some(schema) = &schema {
            request.output = schema.format();
        }
        // No cap of its own: the Planner's is `[llm.roles.planner] max_output_tokens`.

        let resource_ids: HashSet<_> = sources.iter().map(|r| r.id.as_uuid()).collect();
        let lesson_ids: HashSet<_> = brief.lessons.iter().map(|l| l.id.as_uuid()).collect();
        let record_numbers: HashSet<_> = brief.records.iter().map(|r| r.number).collect();
        let glossary_keys: HashSet<_> = brief.glossary.iter().map(|t| t.key.clone()).collect();
        let plan_documents = plan_documents(&brief.documents);
        let plan_facts = PlanFacts {
            resource_ids: &resource_ids,
            lesson_ids: &lesson_ids,
            active_record_numbers: &record_numbers,
            glossary_keys: &glossary_keys,
            documents: &plan_documents,
        };
        let check =
            |text: &str| validate_plan(text, schema.as_ref().map(|s| &s.schema), &plan_facts);

        let response = self.call(&request, role, spent).await?;
        let text = final_text(&response.message);
        let problems = match check(&text) {
            Ok(plan) => return Ok(plan),
            Err(problems) => problems,
        };
        tracing::warn!(job = %job.id, problems = problems.len(), "planner output rejected, repairing");
        let Some(schema_ref) = schema.as_ref() else {
            return Err(JobErrorCode::InvalidOutput);
        };
        let message = render_template_in(
            role.repair_template,
            brief.topic.language,
            &[
                ("errors", &bullet_problems(&problems)),
                ("previous_output", &text),
            ],
        )
        .map_err(|e| render_error(&e))?;
        let spec = RepairSpec {
            role: LlmRole::Planner,
            trace_label: "planner.repair",
            system,
            schema: schema_ref,
            prompt_version: role.version,
            max_output_tokens: None,
            strict: true,
        };
        let fixed = structured::repair_once(self.llm.as_ref(), &spec, message, spent).await?;
        check(&fixed).map_err(|problems| {
            tracing::warn!(job = %job.id, ?problems, "the planner's repair did not help");
            JobErrorCode::InvalidOutput
        })
    }

    // ----- Generator -----

    #[allow(clippy::too_many_lines)] // one pass: build the brief, validate, repair
    async fn write(
        &self,
        ctx: &JobContext,
        job: &Job,
        brief: &Brief,
        plan: &ValidPlan,
        spent: &mut Spent,
    ) -> Result<(Value, Vec<domain::LessonQualityWarning>), JobErrorCode> {
        let role = &self.prompts.generator;
        let schema = schema_of(role, "lesson_document")?;
        let mission = &brief.mission;

        // Sources of the plan, the primary first (the plan was checked
        // against the same list, so both lookups succeed).
        let by_id = |id: uuid::Uuid| brief.resources.iter().find(|r| r.id.as_uuid() == id);
        let primary = by_id(plan.primary_source)
            .ok_or_else(|| internal(job, "find the primary source", &"missing"))?;
        let others: Vec<&Resource> = plan
            .other_sources
            .iter()
            .copied()
            .filter_map(by_id)
            .collect();
        let links: Vec<(&Lesson, String)> = plan
            .links
            .iter()
            .filter_map(|(id, reason)| {
                brief
                    .lessons
                    .iter()
                    .find(|l| l.id.as_uuid() == *id)
                    .map(|l| (l, reason.clone()))
            })
            .collect();
        let excerpts = self.excerpts(job, brief, plan).await?;
        let context = render_template_in(
            role.context_template,
            brief.topic.language,
            &[
                ("topic_title", &one_line(brief.topic.title.as_str())),
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
                    &render_preferences_section(&brief.preferences),
                ),
                ("plan_section", &generator_plan_section(&plan.raw)),
                (
                    "records_section",
                    &generator_records_section(&brief.records, &brief.lessons, &plan.raw),
                ),
                (
                    "sources_section",
                    &generator_sources_section(primary, &others),
                ),
                // The text of the uploaded-document sections the plan follows;
                // web sources have no excerpts (`generator/system.md` covers it).
                ("excerpts_section", &excerpts),
                ("glossary_section", &glossary_section(&brief.glossary)),
                ("lesson_links_section", &lesson_links_section(&links)),
            ],
        )
        .map_err(|e| render_error(&e))?;
        let system = Self::system_blocks(role, context);

        let validator = LessonValidator::new(
            ValidatorConfig::default(),
            // Only the sources the Planner could choose from: a resource
            // without an https URL cannot give the document a valid one.
            ValidationContext::production(
                usable_sources(&brief.resources)
                    .iter()
                    .map(|r| r.id.as_uuid())
                    .collect(),
                brief.lessons.iter().map(|l| l.id.as_uuid()).collect(),
            )
            .with_document_resources(
                brief
                    .resources
                    .iter()
                    .filter(|r| r.kind == ResourceKind::Document)
                    .map(|r| r.id.as_uuid())
                    .collect(),
            ),
        );
        let check = |text: &str| -> Result<Value, Vec<String>> {
            let mut document = parse_object(text).map_err(|e| {
                e.into_iter()
                    .map(|m| format!("(radice) [schema] {m}"))
                    .collect::<Vec<_>>()
            })?;
            // The model's `estimated_minutes` is replaced, so it must not be
            // able to fail validation (nor cost a repair).
            domain::set_computed_minutes(&mut document);
            let issues = validator.validate(&document);
            if issues.is_empty() {
                return Ok(document);
            }
            let mut listed: Vec<String> = issues
                .iter()
                .take(MAX_REPORTED_ISSUES)
                .map(|i| format!("{} [{}] {}", or_root(&i.path), i.code.as_str(), i.message))
                .collect();
            if issues.len() > MAX_REPORTED_ISSUES {
                listed.push(format!(
                    "(altri {} errori omessi)",
                    issues.len() - MAX_REPORTED_ISSUES
                ));
            }
            Err(listed)
        };

        // The web fetch tool: only when it is switched on, the provider can
        // run it and the plan has a web source to read. Otherwise the request
        // is what it was before ADR 0009, to the letter.
        let web = self.web_fetch(job, primary, &others);
        let kickoff = match &web {
            Some(plan) => format!(
                "{KICKOFF}\n\n{}",
                render_template_in(
                    role.kickoff_web_template,
                    brief.topic.language,
                    &[("web_sources_list", &plan.sources_list)],
                )
                .map_err(|e| render_error(&e))?
            ),
            None => KICKOFF.to_owned(),
        };

        let mut request = LlmRequest::new(LlmRole::Generator, "generator.write");
        request.system.clone_from(&system);
        request.messages = vec![LlmMessage::text(MessageRole::User, kickoff)];
        if let Some(schema) = &schema {
            request.output = schema.format();
        }
        // No cap of its own: the Generator's is `[llm.roles.generator]
        // max_output_tokens`, the one source, for the repair too.
        if let Some(plan) = &web {
            request.tools = vec![plan.tool.clone()];
        }

        // The read budget is the job's, not each turn's: a repair continues
        // the same conversation as a new request, with what is left.
        let fetch_budget = web.as_ref().and_then(|p| match &p.tool {
            domain::llm::ToolDef::ServerWebFetch { max_uses, .. } => Some(*max_uses),
            _ => None,
        });
        let fetches_before = spent.fetch_uses;
        let total = self.settings.max_document_repairs + 1;
        let response = self.turn(job, &mut request, role, spent).await?;
        let mut text = final_text(&response.message);
        let mut number = 1;
        loop {
            ctx.progress(JobProgress::step("checking")).await;
            let problems = match check(&text) {
                Ok(document) => {
                    let warnings = validator.quality_warnings(&document, brief.topic.language);
                    // The figures the plan asked for and the document lacks are
                    // improvement points too, but internal: never stored.
                    let mut missing =
                        crate::planned_visual::planned_visual_missing(&plan.raw, &document);
                    missing.extend(crate::planned_visual::pitfalls_missing(
                        &plan.raw, &document,
                    ));
                    // Kept as it is: nothing to improve, or no attempt left (warnings never take
                    // an attempt from the errors).
                    if (warnings.is_empty() && missing.is_empty()) || number >= total {
                        return Ok((document, to_lesson_warnings(&warnings)));
                    }
                    number += 1;
                    ctx.progress(
                        JobProgress::step("writing")
                            .with_message(
                                brief
                                    .topic
                                    .language
                                    .pick("Rifinisco la lezione", "Polishing the lesson"),
                            )
                            .with_done(number),
                    )
                    .await;
                    let candidate = self
                        .improve_document(
                            job,
                            role,
                            brief.topic.language,
                            schema.as_ref(),
                            &system,
                            &mut request,
                            web.is_some(),
                            fetch_budget,
                            fetches_before,
                            &text,
                            &warnings,
                            &missing,
                            spent,
                        )
                        .await;
                    let before = domain::warning_score(&warnings);
                    let met_before = crate::planned_visual::satisfied(&plan.raw, &document)
                        + crate::planned_visual::pitfalls_satisfied(&plan.raw, &document);
                    if let Some(candidate_text) = candidate
                        && let Ok(better) = check(&candidate_text)
                    {
                        let after_warnings =
                            validator.quality_warnings(&better, brief.topic.language);
                        let after = domain::warning_score(&after_warnings);
                        tracing::info!(
                            job = %job.id,
                            before,
                            after,
                            before_codes = ?codes(&warnings),
                            after_codes = ?codes(&after_warnings),
                            "quality improvement answered"
                        );
                        let met_after = crate::planned_visual::satisfied(&plan.raw, &better)
                            + crate::planned_visual::pitfalls_satisfied(&plan.raw, &better);
                        // A planned figure already there is never given up.
                        let lost = crate::planned_visual::lost_any(&plan.raw, &document, &better)
                            || crate::planned_visual::pitfalls_lost(&plan.raw, &document, &better);
                        if !lost && (after < before || (after == before && met_after > met_before))
                        {
                            return Ok((better, to_lesson_warnings(&after_warnings)));
                        }
                    } else {
                        tracing::warn!(job = %job.id, before, "quality improvement unusable, keeping the valid document");
                    }
                    return Ok((document, to_lesson_warnings(&warnings)));
                }
                Err(problems) => problems,
            };
            tracing::warn!(
                job = %job.id,
                document = number,
                problems = problems.len(),
                "generated document rejected"
            );
            if number >= total {
                return Err(JobErrorCode::InvalidOutput);
            }
            number += 1;
            ctx.progress(
                JobProgress::step("writing")
                    .with_message(
                        brief
                            .topic
                            .language
                            .pick("Correggo la lezione", "Fixing the lesson"),
                    )
                    .with_done(number),
            )
            .await;
            let previous = if web.is_some() {
                PREVIOUS_DOCUMENT_IN_CONVERSATION.to_owned()
            } else {
                previous_document(&text, &problems)
            };
            let message = render_template_in(
                role.repair_template,
                brief.topic.language,
                &[
                    ("errors", &bullet_problems(&problems)),
                    ("previous_document", &previous),
                ],
            )
            .map_err(|e| render_error(&e))?;
            text = if web.is_some() {
                // The same conversation goes on, so the pages read stay in
                // the context and are not read again (`llm-port.md` §14.3).
                request
                    .messages
                    .push(LlmMessage::text(MessageRole::User, message));
                request.trace_label = "generator.repair";
                if let Some(budget) = fetch_budget {
                    limit_fetches(
                        &mut request,
                        budget.saturating_sub(spent.fetch_uses.saturating_sub(fetches_before)),
                    );
                }
                let response = self.turn(job, &mut request, role, spent).await?;
                final_text(&response.message)
            } else {
                self.repair_document(role, schema.as_ref(), &system, message, spent)
                    .await?
            };
        }
    }

    /// The web fetch plan of this lesson, if the tool applies (see `write`).
    fn web_fetch(
        &self,
        job: &Job,
        primary: &Resource,
        others: &[&Resource],
    ) -> Option<crate::web_fetch::WebFetchPlan> {
        let max = self.settings.max_web_fetches;
        if max == 0 {
            return None;
        }
        if !self.llm.capabilities(LlmRole::Generator).server_web_fetch {
            tracing::warn!(
                job = %job.id,
                "max_fetches is set but the provider of the generator cannot read web pages: writing from the annotations"
            );
            return None;
        }
        let sources: Vec<&Resource> = std::iter::once(primary)
            .chain(others.iter().copied())
            .collect();
        let plan = plan_web_fetch(&sources, max);
        if plan.is_none() {
            tracing::info!(job = %job.id, "the plan has no web source to read: no web fetch");
        }
        plan
    }

    /// One Generator turn with a web fetch tool: continues paused turns and
    /// leaves `request.messages` as the conversation so far, the answer
    /// included as one assistant message. Without a tool it is a single call.
    async fn turn(
        &self,
        job: &Job,
        request: &mut LlmRequest,
        role: &RolePrompts,
        spent: &mut Spent,
    ) -> Result<LlmResponse, JobErrorCode> {
        let mut observer = FetchLog { job: job.id };
        let response = structured::run_turn(
            self.llm.as_ref(),
            role.version,
            request,
            spent,
            Continuations {
                max: self.settings.max_continuations,
                strict: true,
                continue_when_spent: true,
            },
            &mut observer,
        )
        .await?;
        // Always kept, also when the tool was removed (no read left): the
        // next repair must answer this document, not an older one.
        push_assistant(&mut request.messages, response.message.clone());
        Ok(response)
    }

    /// The text of the sections the plan chose, within the length limit.
    async fn excerpts(
        &self,
        job: &Job,
        brief: &Brief,
        plan: &ValidPlan,
    ) -> Result<String, JobErrorCode> {
        let Some(repo) = &self.documents else {
            return Ok("Nessuno.".to_owned());
        };
        let mut selected = Vec::new();
        for (document_id, section_ids) in &plan.document_sections {
            let Some(view) = brief
                .documents
                .iter()
                .find(|v| v.document.id.as_uuid() == *document_id)
            else {
                continue;
            };
            let sections: Vec<ExcerptSection> = section_ids
                .iter()
                .filter_map(|id| view.sections.iter().find(|s| &s.id == id))
                .map(|s| ExcerptSection {
                    title: s.title.clone(),
                    page_start: s.page_start,
                    page_end: s.page_end,
                })
                .collect();
            let (Some(first), Some(last)) = (
                sections.iter().map(|s| s.page_start).min(),
                sections.iter().map(|s| s.page_end).max(),
            ) else {
                continue;
            };
            let pages = repo
                .pages(view.document.id, first, last)
                .await
                .map_err(|e| internal(job, "read the document pages", &e))?;
            selected.push(ExcerptDocument {
                title: view.document.title.clone(),
                sections,
                pages: pages.into_iter().collect(),
            });
        }
        Ok(excerpts_section(&selected))
    }

    /// One quality improvement call: `None` when it cannot be rendered or
    /// the call fails (the valid document is kept then).
    #[allow(clippy::too_many_arguments)]
    async fn improve_document(
        &self,
        job: &Job,
        role: &RolePrompts,
        language: ContentLanguage,
        schema: Option<&OutputSchema>,
        system: &[SystemBlock],
        request: &mut LlmRequest,
        // The conversation goes on (web fetch) and its read budget.
        web: bool,
        fetch_budget: Option<u32>,
        fetches_before: u32,
        text: &str,
        warnings: &[domain::QualityWarning],
        // Internal points (planned figures missing), listed after the warnings.
        extra: &[String],
        spent: &mut Spent,
    ) -> Option<String> {
        let listed = warnings
            .iter()
            .map(|w| match &w.block_id {
                Some(id) => format!("- ({id}) {}", w.generator_message),
                None => format!("- {}", w.generator_message),
            })
            .chain(extra.iter().map(|e| format!("- {e}")))
            .collect::<Vec<_>>()
            .join("\n");
        let previous = if web {
            PREVIOUS_DOCUMENT_IN_CONVERSATION.to_owned()
        } else {
            previous_document(text, &[])
        };
        let message = render_template_in(
            role.improve_template,
            language,
            &[("warnings", &listed), ("previous_document", &previous)],
        )
        .ok()?;
        let result = if web {
            request
                .messages
                .push(LlmMessage::text(MessageRole::User, message));
            request.trace_label = "generator.improve";
            if let Some(budget) = fetch_budget {
                limit_fetches(
                    request,
                    budget.saturating_sub(spent.fetch_uses.saturating_sub(fetches_before)),
                );
            }
            self.turn(job, request, role, spent)
                .await
                .map(|r| final_text(&r.message))
        } else {
            self.improve_once(role, schema, system, message, spent)
                .await
        };
        match result {
            Ok(text) => Some(text),
            Err(code) => {
                tracing::warn!(job = %job.id, ?code, "quality improvement failed, keeping the valid document");
                None
            }
        }
    }

    async fn improve_once(
        &self,
        role: &RolePrompts,
        schema: Option<&OutputSchema>,
        system: &[SystemBlock],
        message: String,
        spent: &mut Spent,
    ) -> Result<String, JobErrorCode> {
        let mut request = LlmRequest::new(LlmRole::Generator, "generator.improve");
        request.system = system.to_vec();
        if let Some(schema) = schema {
            request.output = schema.format();
        }
        request.messages = vec![LlmMessage::text(MessageRole::User, message)];
        let response = self.call(&request, role, spent).await?;
        Ok(final_text(&response.message))
    }

    async fn repair_document(
        &self,
        role: &RolePrompts,
        schema: Option<&OutputSchema>,
        system: &[SystemBlock],
        message: String,
        spent: &mut Spent,
    ) -> Result<String, JobErrorCode> {
        let mut request = LlmRequest::new(LlmRole::Generator, "generator.repair");
        request.system = system.to_vec();
        if let Some(schema) = schema {
            request.output = schema.format();
        }
        // Same cap as the first document (see `write`).
        request.messages = vec![LlmMessage::text(MessageRole::User, message)];
        let response = self.call(&request, role, spent).await?;
        Ok(final_text(&response.message))
    }

    async fn call(
        &self,
        request: &LlmRequest,
        role: &RolePrompts,
        spent: &mut Spent,
    ) -> Result<domain::llm::LlmResponse, JobErrorCode> {
        structured::call_complete(self.llm.as_ref(), role.version, request, spent).await
    }

    // ----- save -----

    async fn save(
        &self,
        job: &Job,
        brief: &Brief,
        plan: ValidPlan,
        mut document: Value,
        quality_warnings: Vec<domain::LessonQualityWarning>,
    ) -> Result<Lesson, JobErrorCode> {
        restore_source_urls(&mut document, &brief.resources);
        // The model's figure is replaced by the formula over the real prose
        // words and exercises (`domain::lesson_minutes`).
        let minutes = domain::document_minutes(&document)
            .ok_or_else(|| internal(job, "measure the document", &"not parsable"))?;
        document["estimated_minutes"] = Value::from(minutes);
        let meta = LessonMeta::from_document(&document)
            .ok_or_else(|| internal(job, "read the document header", &"missing fields"))?;
        let now = now_millis(self.clock.as_ref());
        let lesson_id = LessonId::new(self.ids.new_id());
        let component_requests = component_request_drafts(&document)
            .into_iter()
            .map(|d| ComponentRequest {
                id: ComponentRequestId::new(self.ids.new_id()),
                topic_id: job.topic_id,
                lesson_id,
                block_id: d.block_id,
                wanted: d.wanted,
                purpose: d.purpose,
                fallback: d.fallback,
                created_at: now,
            })
            .collect();
        let new = GeneratedLesson {
            quality_warnings,
            id: lesson_id,
            topic_id: job.topic_id,
            job_id: job.id,
            meta,
            mission_version: brief.mission.version,
            document,
            plan: plan.raw,
            component_requests,
            created_at: now,
        };
        match self.lessons.insert_generated(&new).await {
            Ok(lesson) => Ok(lesson),
            // Saved by an earlier run of this same job: that lesson is the result.
            Err(RepositoryError::Conflict) => self
                .lessons
                .by_generation_job(job.id)
                .await
                .map_err(|e| internal(job, "read the saved lesson", &e))?
                .ok_or_else(|| internal(job, "save the lesson", &"conflict")),
            Err(e) => Err(internal(job, "save the lesson", &e)),
        }
    }
}

/// The ready documents the Planner may choose sections from.
fn plan_documents(documents: &[DocumentView]) -> Vec<PlanDocument> {
    documents
        .iter()
        .filter(|v| v.document.status == DocumentStatus::Ready)
        .filter_map(|v| {
            Some(PlanDocument {
                document_id: v.document.id.as_uuid(),
                resource_id: v.document.resource_id?.as_uuid(),
                section_ids: v.sections.iter().map(|s| s.id.clone()).collect(),
            })
        })
        .collect()
}

fn to_lesson_warnings(warnings: &[domain::QualityWarning]) -> Vec<domain::LessonQualityWarning> {
    warnings
        .iter()
        .map(|w| domain::LessonQualityWarning {
            code: w.code.as_str().to_owned(),
            block_id: w.block_id.clone(),
            message: w.learner_message.clone(),
        })
        .collect()
}

fn codes(warnings: &[domain::QualityWarning]) -> Vec<&'static str> {
    warnings.iter().map(|w| w.code.as_str()).collect()
}

fn or_root(path: &str) -> &str {
    if path.is_empty() { "(radice)" } else { path }
}

/// The previous document as the repair quotes it: compact JSON when it
/// parses, cut to the limit; left out when the error is its size.
fn previous_document(text: &str, problems: &[String]) -> String {
    if problems
        .iter()
        .any(|p| p.starts_with("(radice) [too_long]"))
    {
        return PREVIOUS_DOCUMENT_OMITTED.to_owned();
    }
    let compact = crate::structured::extract_json(text)
        .and_then(|json| serde_json::from_str::<Value>(json).ok())
        .map_or_else(|| text.to_owned(), |v| v.to_string());
    if compact.chars().count() > PREVIOUS_DOCUMENT_MAX_CHARS {
        PREVIOUS_DOCUMENT_OMITTED.to_owned()
    } else {
        compact
    }
}

/// The model never writes a URL that reaches the learner: each source takes
/// the one of the `Resource` it points to (`lessons.md` §4).
fn restore_source_urls(document: &mut Value, resources: &[Resource]) {
    let Some(sources) = document.get_mut("sources").and_then(Value::as_array_mut) else {
        return;
    };
    for source in sources {
        let resource = source
            .get("resource_id")
            .and_then(Value::as_str)
            .and_then(domain::parse_uuid)
            .and_then(|id| resources.iter().find(|r| r.id.as_uuid() == id));
        // An uploaded PDF has no address: its source gets `null`.
        let url = resource.map(|r| r.url.clone().map_or(Value::Null, Value::String));
        if let (Some(url), Some(object)) = (url, source.as_object_mut()) {
            object.insert("url".to_owned(), url);
        }
    }
}

fn result_of(lesson: &Lesson) -> JobResult {
    JobResult::LessonGeneration {
        lesson_id: lesson.id,
        lesson_number: lesson.number,
    }
}

/// Logs what the provider read for the Generator: which pages and which
/// reads failed. A failed read is not a failure of the job (ADR 0009): the
/// model saw the error and wrote the lesson with what it had.
struct FetchLog {
    job: domain::JobId,
}

#[async_trait]
impl TurnObserver for FetchLog {
    async fn observe(&mut self, response: &LlmResponse) {
        for read in response.web_fetch_reads() {
            tracing::info!(
                job = %self.job,
                url = read.url.as_deref().unwrap_or("(url not derivable from the block)"),
                "generator: web page read"
            );
        }
        for code in &response.fetch_errors {
            tracing::warn!(job = %self.job, error_code = code, "generator: a web page could not be read");
        }
    }
}

/// Sets what is left of the job's read budget on the fetch tool of the
/// request, or removes the tool when nothing is left: the repair then writes
/// from the pages already in the conversation. A `pause_turn` continuation
/// cannot drop it (a read is pending): there it stays at 1, so a paused turn
/// may read once past the budget (`structured::next_limit`), a known and
/// bounded excess.
fn limit_fetches(request: &mut LlmRequest, left: u32) {
    // The API rejects `max_uses: 0` but accepts a history with pages read and
    // no tool (both tried on 2026-11-15): with no read left the tool goes.
    if left == 0 {
        request
            .tools
            .retain(|tool| !matches!(tool, domain::llm::ToolDef::ServerWebFetch { .. }));
        return;
    }
    for tool in &mut request.tools {
        if let domain::llm::ToolDef::ServerWebFetch { max_uses, .. } = tool {
            *max_uses = left;
        }
    }
}

/// Adds an assistant message to a conversation, joining it to the assistant
/// message before it: the paused turns of a continuation and its final answer
/// are one answer of the model, sent back as one turn.
fn push_assistant(messages: &mut Vec<LlmMessage>, message: LlmMessage) {
    match messages.last_mut() {
        Some(last) if last.role == MessageRole::Assistant => last.parts.extend(message.parts),
        _ => messages.push(message),
    }
}
