//! Closing a lesson: the `lesson_completion` job. It grades what is
//! still pending, asks the Recorder what the learner showed, checks the answer
//! with the backend's own rules (`lessons.md` §6.3) and stores it atomically.

use std::sync::Arc;

use async_trait::async_trait;
use domain::ContentLanguage;
use domain::llm::{LlmMessage, LlmProvider, LlmRequest, LlmRole, MessageRole, SystemBlock};
use domain::{
    Answer, Attempt, AttemptRepository, CardDraft, CardTarget, Clock, CompletionSave,
    CompletionSummary, ExerciseKind, GlossaryTermId, IdGenerator, Job, JobErrorCode, JobProgress,
    JobResult, LearningRecordId, LearningRepository, Lesson, LessonExercises, LessonRepository,
    MissionRepository, NewCard, NewRecord, Outcome, RecordDraft, RecordKind, RecorderContext,
    RecorderDraft, RepositoryError, ReviewCardId, StoredLesson, TermDraft, TermUpsert,
    TopicRepository, has_passed_attempt, validate_recorder,
};
use serde_json::Value;

use crate::context::one_line;
use crate::generation::LessonSettings;
use crate::grader::Grader;
use crate::jobs::{JobContext, JobFailure, JobHandler};
use crate::lesson_context::{
    document_glossary_section, exercises_section, glossary_section, recorder_plan_section,
    records_section,
};
use crate::prompts::{RolePrompts, render_template_in};
use crate::structured::{
    self, OutputSchema, RepairSpec, Spent, bullet_problems, check_schema, final_text, parse_object,
};
use crate::time_util::now_millis;

const KICKOFF: &str = "Procedi.";
/// The glossary and the records the Recorder is shown come from the repository
/// in bulk; the context functions apply the prompt's own limits.
const CONTEXT_ROWS: u32 = 500;

/// Use case `CompleteLesson`, registered as the handler of `lesson_completion`.
pub struct RunLessonCompletion {
    topics: Arc<dyn TopicRepository>,
    missions: Arc<dyn MissionRepository>,
    lessons: Arc<dyn LessonRepository>,
    attempts: Arc<dyn AttemptRepository>,
    learning: Arc<dyn LearningRepository>,
    grader: Arc<Grader>,
    llm: Arc<dyn LlmProvider>,
    prompts: RolePrompts,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    settings: LessonSettings,
}

impl RunLessonCompletion {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        topics: Arc<dyn TopicRepository>,
        missions: Arc<dyn MissionRepository>,
        lessons: Arc<dyn LessonRepository>,
        attempts: Arc<dyn AttemptRepository>,
        learning: Arc<dyn LearningRepository>,
        grader: Arc<Grader>,
        llm: Arc<dyn LlmProvider>,
        prompts: RolePrompts,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        settings: LessonSettings,
    ) -> Self {
        Self {
            topics,
            missions,
            lessons,
            attempts,
            learning,
            grader,
            llm,
            prompts,
            clock,
            ids,
            settings,
        }
    }
}

#[async_trait]
impl JobHandler for RunLessonCompletion {
    fn needs_llm(&self) -> bool {
        true
    }

    async fn run(&self, ctx: &JobContext) -> Result<JobResult, JobFailure> {
        let job = ctx.job().await;
        self.complete(ctx, &job).await.map_err(JobFailure)
    }
}

fn internal(job: &Job, what: &str, e: &dyn std::fmt::Display) -> JobErrorCode {
    tracing::error!(job = %job.id, error = %e, "lesson completion: cannot {what}");
    JobErrorCode::Internal
}

fn result_of(lesson: &Lesson, s: CompletionSummary) -> JobResult {
    JobResult::LessonCompletion {
        lesson_id: lesson.id,
        record_count: s.record_count,
        term_count: s.term_count,
        review_card_count: s.review_card_count,
    }
}

impl RunLessonCompletion {
    async fn complete(&self, ctx: &JobContext, job: &Job) -> Result<JobResult, JobErrorCode> {
        let lesson_id = job
            .lesson_id
            .ok_or_else(|| internal(job, "find the lesson", &"the job has none"))?;
        // The job acts for the owner of its topic.
        let topic = self
            .topics
            .get_system(job.topic_id)
            .await
            .map_err(|e| internal(job, "read the topic", &e))?
            .ok_or_else(|| internal(job, "read the topic", &"it is gone"))?;
        let owner = topic.user_id;
        let StoredLesson {
            lesson,
            document,
            plan,
        } = self
            .lessons
            .get(owner, lesson_id)
            .await
            .map_err(|e| internal(job, "read the lesson", &e))?
            .ok_or_else(|| internal(job, "read the lesson", &"it is gone"))?;
        // Run again after a crash that came after the save: nothing to redo.
        if let Some(done) = self
            .learning
            .completion_summary(lesson_id)
            .await
            .map_err(|e| internal(job, "read the completion", &e))?
        {
            return Ok(result_of(&lesson, done));
        }

        let exercises = LessonExercises::from_document(&document);
        ctx.progress(JobProgress::step("grading")).await;
        self.grade_pending(job, &lesson, &exercises, topic.language)
            .await?;
        let attempts = self
            .attempts
            .list(lesson_id)
            .await
            .map_err(|e| internal(job, "read the attempts", &e))?;

        let mut spent = Spent::default();
        let draft = if has_passed_attempt(&attempts) {
            ctx.progress(
                JobProgress::step("recording").with_message(
                    ctx.language()
                        .pick("Ho riletto i tuoi tentativi", "I reviewed your attempts"),
                ),
            )
            .await;
            self.record(
                job, &lesson, &document, &plan, &exercises, &attempts, &mut spent,
            )
            .await?
        } else {
            // Nothing can be recorded without a passed exercise ("coverage is
            // not learning"): the Recorder is not even asked.
            RecorderDraft::default()
        };

        ctx.progress(JobProgress::step("saving")).await;
        let save = self.build_save(&lesson, &draft);
        let summary = match self.learning.save_completion(&save).await {
            Ok(s) => s,
            Err(RepositoryError::Conflict) => self
                .learning
                .completion_summary(lesson_id)
                .await
                .map_err(|e| internal(job, "read the completion", &e))?
                // Not saved before, so the refusal is about the content (a
                // replaced record that is not active any more): the output
                // is not valid, whatever the model was told.
                .ok_or_else(|| {
                    tracing::warn!(job = %job.id, "the completion was refused by the store");
                    JobErrorCode::InvalidOutput
                })?,
            Err(e) => return Err(internal(job, "save the completion", &e)),
        };
        tracing::info!(
            job = %job.id,
            lesson = %lesson_id,
            calls = spent.calls,
            input_tokens = spent.usage.input_tokens,
            output_tokens = spent.usage.output_tokens,
            records = summary.record_count,
            terms = summary.term_count,
            cards = summary.review_card_count,
            "lesson completed"
        );
        Ok(result_of(&lesson, summary))
    }

    /// Grades each `pending` open answer, with one more try on a failure. One
    /// that still fails stays `pending` and does not count as passed.
    async fn grade_pending(
        &self,
        job: &Job,
        lesson: &Lesson,
        exercises: &LessonExercises,
        language: ContentLanguage,
    ) -> Result<(), JobErrorCode> {
        let attempts = self
            .attempts
            .list(lesson.id)
            .await
            .map_err(|e| internal(job, "read the attempts", &e))?;
        for a in attempts
            .iter()
            .filter(|a| a.kind == ExerciseKind::OpenAnswer && a.outcome == Outcome::Pending)
        {
            let (Some(exercise), Answer::Text(text)) = (exercises.get(&a.block_id), &a.answer)
            else {
                continue;
            };
            for try_number in 1..=2 {
                match self.grader.grade(lesson, exercise, text, language).await {
                    Ok(grade) => {
                        self.attempts
                            .set_grade(a.id, &grade)
                            .await
                            .map_err(|e| internal(job, "store a grade", &e))?;
                        break;
                    }
                    Err(e) => tracing::warn!(
                        job = %job.id, block = a.block_id, try_number, error = %e,
                        "grading failed"
                    ),
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)] // a flat sequence of steps
    async fn record(
        &self,
        job: &Job,
        lesson: &Lesson,
        document: &Value,
        plan: &Value,
        exercises: &LessonExercises,
        attempts: &[Attempt],
        spent: &mut Spent,
    ) -> Result<RecorderDraft, JobErrorCode> {
        let role = &self.prompts;
        let schema = role
            .output_schema
            .map(|raw| OutputSchema::parse(raw, "recorder_output"))
            .transpose()
            .map_err(|e| internal(job, "read the output schema", &e))?;
        let topic = self
            .topics
            .get_system(lesson.topic_id)
            .await
            .map_err(|e| internal(job, "read the topic", &e))?
            .ok_or_else(|| internal(job, "read the topic", &"it is gone"))?;
        let mission = self
            .missions
            .current(lesson.topic_id)
            .await
            .map_err(|e| internal(job, "read the mission", &e))?;
        let lessons = self
            .lessons
            .all_in_topic(lesson.topic_id)
            .await
            .map_err(|e| internal(job, "read the lessons", &e))?;
        let records = self
            .learning
            .list_records(lesson.topic_id, true, None, CONTEXT_ROWS)
            .await
            .map_err(|e| internal(job, "read the records", &e))?;
        let terms = self
            .learning
            .list_terms(lesson.topic_id, None, CONTEXT_ROWS)
            .await
            .map_err(|e| internal(job, "read the glossary", &e))?;

        let context = render_template_in(
            role.context_template,
            topic.language,
            &[
                ("topic_title", &one_line(topic.title.as_str())),
                ("lesson_number", &lesson.number.to_string()),
                ("lesson_title", &one_line(&lesson.title)),
                ("lesson_objective", &one_line(&lesson.objective)),
                (
                    "mission_why",
                    &mission
                        .as_ref()
                        .map_or_else(String::new, |m| one_line(m.content.why())),
                ),
                ("plan_section", &recorder_plan_section(plan)),
                (
                    "lesson_glossary_section",
                    &document_glossary_section(document),
                ),
                ("exercises_section", &exercises_section(exercises, attempts)),
                ("records_section", &records_section(&records, &lessons)),
                ("glossary_section", &glossary_section(&terms)),
            ],
        )
        .map_err(|e| internal(job, "render the prompt", &e))?;
        let system = vec![
            SystemBlock {
                text: role.system.to_owned(),
                cache_breakpoint: true,
            },
            SystemBlock {
                text: context,
                cache_breakpoint: false,
            },
        ];

        let active: std::collections::HashSet<u32> = records.iter().map(|r| r.number).collect();
        let check = |text: &str| -> Result<RecorderDraft, Vec<String>> {
            let draft = parse_draft(text, schema.as_ref().map(|s| &s.schema))?;
            let errors = validate_recorder(
                &draft,
                &RecorderContext {
                    attempts,
                    exercises,
                    active_record_numbers: &active,
                },
            );
            if errors.is_empty() {
                Ok(draft)
            } else {
                Err(errors.into_iter().take(30).collect())
            }
        };

        let mut request = LlmRequest::new(LlmRole::Recorder, "recorder.record");
        request.system.clone_from(&system);
        request.messages = vec![LlmMessage::text(MessageRole::User, KICKOFF)];
        if let Some(schema) = &schema {
            request.output = schema.format();
        }
        request.max_output_tokens = Some(self.settings.recorder_max_output_tokens);
        let response =
            structured::call_complete(self.llm.as_ref(), role.version, &request, spent).await?;
        let text = final_text(&response.message);
        let problems = match check(&text) {
            Ok(draft) => return Ok(draft),
            Err(problems) => problems,
        };
        tracing::warn!(job = %job.id, problems = problems.len(), "recorder output rejected, repairing");
        let Some(schema_ref) = schema.as_ref() else {
            return Err(JobErrorCode::InvalidOutput);
        };
        let message = render_template_in(
            role.repair_template,
            topic.language,
            &[
                ("errors", &bullet_problems(&problems)),
                ("previous_output", &text),
            ],
        )
        .map_err(|e| internal(job, "render the repair prompt", &e))?;
        let spec = RepairSpec {
            role: LlmRole::Recorder,
            trace_label: "recorder.repair",
            system,
            schema: schema_ref,
            prompt_version: role.version,
            max_output_tokens: Some(self.settings.recorder_max_output_tokens),
            strict: true,
        };
        let fixed = structured::repair_once(self.llm.as_ref(), &spec, message, spent).await?;
        check(&fixed).map_err(|problems| {
            tracing::warn!(job = %job.id, problems = problems.len(), "the recorder's repair did not help");
            JobErrorCode::InvalidOutput
        })
    }

    fn build_save(&self, lesson: &Lesson, draft: &RecorderDraft) -> CompletionSave {
        CompletionSave {
            topic_id: lesson.topic_id,
            lesson_id: lesson.id,
            at: now_millis(self.clock.as_ref()),
            records: draft
                .records
                .iter()
                .map(|r| NewRecord {
                    id: LearningRecordId::new(self.ids.new_id()),
                    kind: r.kind,
                    title: r.title.trim().to_owned(),
                    body: r.body.trim().to_owned(),
                    evidence: r.evidence.trim().to_owned(),
                    supersedes: r.supersedes.clone(),
                })
                .collect(),
            terms: draft
                .terms
                .iter()
                .map(|t| TermUpsert {
                    id: GlossaryTermId::new(self.ids.new_id()),
                    key: t.key.clone(),
                    term: t.term.trim().to_owned(),
                    definition: t.definition.trim().to_owned(),
                    avoid: t.avoid.iter().map(|a| a.trim().to_owned()).collect(),
                })
                .collect(),
            cards: draft
                .cards
                .iter()
                .map(|c| NewCard {
                    id: ReviewCardId::new(self.ids.new_id()),
                    target: c.target,
                    front: c.front.trim().to_owned(),
                    back: c.back.trim().to_owned(),
                    unpassed_block: c.unpassed_block.clone(),
                })
                .collect(),
        }
    }
}

/// The Recorder's JSON as a draft (`recorder/output_schema.json`). Cards point
/// to a record by position or to a term by key, exactly one of the two.
#[allow(clippy::too_many_lines)] // a flat sequence of independent steps; helpers would only hide the order
pub fn parse_draft(text: &str, schema: Option<&Value>) -> Result<RecorderDraft, Vec<String>> {
    let value = parse_object(text)?;
    if let Some(schema) = schema {
        let shape = check_schema(&value, schema);
        if !shape.is_empty() {
            return Err(shape);
        }
    }
    let list = |v: &Value, key: &str| -> Vec<Value> {
        v.get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let s = |v: &Value, key: &str| {
        v.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let strings = |v: &Value, key: &str| -> Vec<String> {
        list(v, key)
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect()
    };
    let mut errors = Vec::new();

    let mut records = Vec::new();
    for (i, r) in list(&value, "records").iter().enumerate() {
        let Ok(kind) = s(r, "kind").parse::<RecordKind>() else {
            errors.push(format!(
                "records[{i}].kind deve essere demonstrated o misconception_corrected."
            ));
            continue;
        };
        records.push(RecordDraft {
            kind,
            title: s(r, "title"),
            body: s(r, "body"),
            evidence: s(r, "evidence"),
            evidence_blocks: strings(r, "evidence_blocks"),
            supersedes: list(r, "supersedes")
                .iter()
                .filter_map(|n| n.as_u64().and_then(|n| u32::try_from(n).ok()))
                .collect(),
        });
    }
    let terms: Vec<TermDraft> = list(&value, "terms")
        .iter()
        .map(|t| TermDraft {
            key: s(t, "key"),
            term: s(t, "term"),
            definition: s(t, "definition"),
            avoid: strings(t, "avoid"),
            evidence_blocks: strings(t, "evidence_blocks"),
        })
        .collect();
    let mut cards = Vec::new();
    for (i, c) in list(&value, "cards").iter().enumerate() {
        let index = c.get("record_index").and_then(Value::as_u64);
        let key = c.get("term_key").and_then(Value::as_str);
        let target = match (index, key) {
            (Some(n), None) => match usize::try_from(n) {
                Ok(n) if n < records.len() => Some(CardTarget::Record(n)),
                _ => {
                    errors.push(format!(
                        "cards[{i}].record_index {n} non è una posizione valida dell'array records."
                    ));
                    None
                }
            },
            (None, Some(key)) => {
                let position = terms.iter().position(|t| t.key == key);
                if position.is_none() {
                    errors.push(format!(
                        "cards[{i}].term_key '{}' non è la chiave di un termine di questa risposta.",
                        key.chars().take(60).collect::<String>()
                    ));
                }
                position.map(CardTarget::Term)
            }
            _ => {
                errors.push(format!(
                    "cards[{i}]: esattamente uno tra record_index e term_key deve essere valorizzato, l'altro null."
                ));
                None
            }
        };
        if c.get("unpassed_block")
            .is_some_and(|b| !b.is_null() && !b.is_string())
        {
            errors.push(format!(
                "cards[{i}].unpassed_block deve essere una stringa (l'id di un esercizio) oppure null."
            ));
        }
        if let Some(target) = target {
            cards.push(CardDraft {
                target,
                front: s(c, "front"),
                back: s(c, "back"),
                // Absent counts as null: the prompt may predate the field.
                unpassed_block: c
                    .get("unpassed_block")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            });
        }
    }
    if errors.is_empty() {
        Ok(RecorderDraft {
            records,
            terms,
            cards,
        })
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn output(card_extra: &Value) -> String {
        let mut card = json!({
            "record_index": 0, "term_key": null, "front": "f", "back": "b"
        });
        card.as_object_mut()
            .unwrap()
            .extend(card_extra.as_object().unwrap().clone());
        json!({
            "records": [{
                "kind": "demonstrated", "title": "t", "body": "b", "evidence": "e",
                "evidence_blocks": ["q1"], "supersedes": []
            }],
            "terms": [],
            "cards": [card]
        })
        .to_string()
    }

    #[test]
    fn a_missing_unpassed_block_is_null() {
        let d = parse_draft(&output(&json!({})), None).unwrap();
        assert_eq!(d.cards[0].unpassed_block, None);
        let d = parse_draft(&output(&json!({"unpassed_block": null})), None).unwrap();
        assert_eq!(d.cards[0].unpassed_block, None);
    }

    #[test]
    fn an_unpassed_block_is_kept_and_must_be_a_string() {
        let d = parse_draft(&output(&json!({"unpassed_block": "q2"})), None).unwrap();
        assert_eq!(d.cards[0].unpassed_block.as_deref(), Some("q2"));
        let errors = parse_draft(&output(&json!({"unpassed_block": 3})), None).unwrap_err();
        assert!(errors[0].contains("cards[0].unpassed_block"), "{errors:?}");
    }
}
