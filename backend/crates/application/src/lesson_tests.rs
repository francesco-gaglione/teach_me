#![allow(
    clippy::needless_pass_by_value,
    clippy::assert_is_empty,
    clippy::too_many_lines,
    clippy::cast_possible_truncation
)] // test helpers favour readability over lint-driven shapes
//! Phases 6 and 7 over in-memory fakes and a scripted model: lesson
//! generation, attempts, grading, completion. No test calls a real model.

use std::sync::Arc;
use std::time::Duration;

use domain::llm::{LlmError, LlmEvent, MessageRole as R, OutputFormat, StopReason, Usage};
use domain::{
    Attempt, CardOrigin, CardStatus, ComponentRequest, GeneratedLesson, GlossaryTerm,
    GlossaryTermId, IdGenerator, Job, JobErrorCode, JobKind, JobResult, JobStatus, LearningRecord,
    LearningRecordId, Lesson, LessonId, LessonMeta, LessonRepository, LessonStatus, Mission,
    MissionContent, Outcome, RecordKind, RecordStatus, Resource, ResourceId, ResourceKind,
    ReviewCard, ReviewCardId, SourceType, TopicResources, TopicState,
};
use serde_json::{Value, json};
use time::macros::datetime;
use uuid::Uuid;

use crate::chat_tests::{Fixture, SLOW, fixture_with};
use crate::error::{AppError, ConflictReason};
use crate::fakes::{SeqIds, Step};
use crate::{
    AttemptService, GenerateLesson, Grader, JobRunner, JobSettings, LessonPrompts, LessonService,
    LessonSettings, RunLessonCompletion,
};

const RESOURCE: u128 = 0x51;
const RESOURCE_2: u128 = 0x52;
const COMMUNITY: u128 = 0x53;
/// The ids `seed_ready_document(.., 1, ..)` gives.
const DOC_ID: domain::DocumentId = domain::DocumentId::new(Uuid::from_u128(0xd001));
const RES_ID: ResourceId = ResourceId::new(Uuid::from_u128(0xe001));
const FIXTURE: &str =
    include_str!("../../../../docs/contracts/fixtures/lesson-photo-exposure.json");

/// A scripted reply: the text the model "writes". Leaked because scripts hold
/// `&'static str`; it is a test.
fn say(text: &str) -> Vec<Step> {
    vec![Step::Delta(Box::leak(text.to_owned().into_boxed_str()))]
}

fn resource(n: u128, kind: ResourceKind, url: Option<&str>, topic: domain::TopicId) -> Resource {
    Resource {
        id: ResourceId::new(Uuid::from_u128(n)),
        topic_id: topic,
        kind,
        source_type: SourceType::Documentation,
        title: format!("Fonte {n}"),
        url: url.map(str::to_owned),
        language: "en".into(),
        annotation: "Copre l'esposizione.".into(),
        use_for: "capire gli stop".into(),
        document_id: None,
        created_at: datetime!(2026-11-08 10:00:00 UTC),
    }
}

/// The photo fixture as the Validator wants it in production: sources point to
/// real resources of the topic.
fn lesson_doc() -> Value {
    let mut doc: Value = serde_json::from_str(FIXTURE).unwrap();
    for (i, id) in [RESOURCE, RESOURCE_2].into_iter().enumerate() {
        doc["sources"][i]["resource_id"] = json!(Uuid::from_u128(id).to_string());
        // The model's URL is not trusted: the backend replaces it.
        doc["sources"][i]["url"] = json!("https://invented.example/page");
    }
    doc
}

fn plan_json() -> Value {
    json!({
        "title": "Gli stop",
        "objective": "Saper compensare uno stop tolto a una leva con uno dato a un'altra.",
        "why_now": "La mission chiede di fotografare scene in movimento.",
        "estimated_minutes": 8,
        "prerequisites": [],
        "concepts": [
            {"name": "Stop", "key_point": "Uno stop raddoppia o dimezza la luce.", "example": "A f/4 entra il doppio della luce che a f/5.6.", "steps": ["f/4 ha il foro più largo di f/5.6", "uno stop di differenza raddoppia la luce"], "pitfall": "Pensare che uno stop sommi una quantità fissa di luce."},
            {"name": "Compensazione", "key_point": "Uno stop tolto a una leva si ridà a un'altra.", "example": "Da 1/125 a 1/250 perdi uno stop: apri a f/4 per ritrovarlo.", "steps": ["1/250 dimezza la luce rispetto a 1/125", "f/4 la raddoppia rispetto a f/5.6"], "pitfall": ""},
            {"name": "Tempo", "key_point": "Il tempo di scatto dimezza o raddoppia la luce.", "example": "Da 1/125 a 1/250 entra metà della luce.", "steps": ["1/250 è metà di 1/125", "entra metà della luce"], "pitfall": ""},
            {"name": "Diaframma", "key_point": "Il diaframma regola la luce che entra.", "example": "Da f/5.6 a f/4 entra il doppio della luce.", "steps": ["f/4 è più aperto di f/5.6", "entra il doppio della luce"], "pitfall": ""}
        ],
        "terms": {"introduce": [{"key": "stop", "term": "stop", "hint": "unità di luce"}], "reuse": []},
        "primary_source_id": Uuid::from_u128(RESOURCE).to_string(),
        "other_source_ids": [Uuid::from_u128(RESOURCE_2).to_string()],
        "exercises": [
            {"type": "quiz_single", "purpose": "Riconoscere la compensazione", "verifies_objective": false},
            {"type": "open_answer", "purpose": "Spiegare la scelta", "verifies_objective": true}
        ],
        "lesson_links": [],
        "document_sections": []
    })
}

const FAST_GRADER: Duration = Duration::from_millis(60);

struct LessonEnv {
    f: Fixture,
    ids: Arc<SeqIds>,
    settings: LessonSettings,
    /// Read-aloud wiring; `None` is the feature off.
    audio: Option<crate::audio_queue::tests::Env>,
}

impl LessonEnv {
    async fn new(scripts: Vec<Vec<Step>>) -> Self {
        let f = fixture_with(scripts, SLOW, 4, |llm| llm).await;
        let ids = Arc::new(SeqIds::default());
        for _ in 0..1000 {
            ids.new_id();
        }
        {
            let mut s = f.repo.0.lock().unwrap();
            let at = datetime!(2026-11-08 10:00:00 UTC);
            let content = MissionContent::parse(
                "Voglio fotografare bambini in movimento",
                "Fotografo da due anni in automatico, mai con soggetti in movimento",
                &["Scatto nitido in poca luce".to_owned()],
                &[],
                &[],
            )
            .unwrap();
            let mut mission = Mission::draft(f.topic, 1, content, at);
            mission.confirm(at).unwrap();
            s.missions.push(mission);
            s.topics.iter_mut().find(|t| t.id == f.topic).unwrap().state = TopicState::Ready;
            s.resources.push((
                f.topic,
                TopicResources {
                    resources: vec![
                        resource(
                            RESOURCE,
                            ResourceKind::Knowledge,
                            Some("https://a.example/exposure"),
                            f.topic,
                        ),
                        resource(
                            RESOURCE_2,
                            ResourceKind::Knowledge,
                            Some("https://b.example/stops"),
                            f.topic,
                        ),
                        resource(
                            COMMUNITY,
                            ResourceKind::Community,
                            Some("https://forum.example"),
                            f.topic,
                        ),
                    ],
                    ..TopicResources::default()
                },
            ));
        }
        Self {
            f,
            ids,
            settings: LessonSettings {
                grader_timeout: FAST_GRADER,
                ..LessonSettings::default()
            },
            audio: None,
        }
    }

    fn with_audio(mut self) -> Self {
        self.audio = Some(crate::audio_queue::tests::env());
        self
    }

    fn grader(&self) -> Arc<Grader> {
        Arc::new(Grader::new(
            self.f.llm.clone(),
            LessonPrompts::embedded().grader,
            self.settings.grader_max_output_tokens,
        ))
    }

    fn generate_handler(&self) -> Arc<GenerateLesson> {
        let handler = GenerateLesson::new(
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.llm.clone(),
            LessonPrompts::embedded(),
            self.f.clock.clone(),
            self.ids.clone(),
            self.settings,
        )
        .with_documents(self.f.repo.clone());
        Arc::new(match &self.audio {
            Some(a) => handler.with_audio(a.preparer.clone()),
            None => handler,
        })
    }

    fn completion_handler(&self) -> Arc<RunLessonCompletion> {
        Arc::new(RunLessonCompletion::new(
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.grader(),
            self.f.llm.clone(),
            LessonPrompts::embedded().recorder,
            self.f.clock.clone(),
            self.ids.clone(),
            self.settings,
        ))
    }

    fn runner(&self) -> JobRunner {
        JobRunner::new(
            self.f.jobs.clone(),
            JobSettings {
                max_concurrent: 1,
                job_timeout: Duration::from_secs(30),
                max_attempts: 3,
                store_retry_delay: Duration::from_millis(1),
            },
        )
        .with_handler(JobKind::LessonGeneration, self.generate_handler())
        .with_handler(JobKind::LessonCompletion, self.completion_handler())
    }

    fn lessons(&self) -> LessonService {
        let service = self.lessons_without_audio();
        match &self.audio {
            Some(a) => service.with_audio(a.preparer.clone()),
            None => service,
        }
    }

    fn lessons_without_audio(&self) -> LessonService {
        LessonService::new(
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.jobs.clone(),
            crate::fakes::owner_keys(),
            self.f.clock.clone(),
        )
        .with_documents(self.f.repo.clone())
    }

    fn attempts(&self) -> AttemptService {
        AttemptService::new(
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.f.repo.clone(),
            self.grader(),
            crate::fakes::owner_keys(),
            self.f.clock.clone(),
            self.ids.clone(),
            self.settings.grader_timeout,
        )
    }

    /// Requests a generation and runs the job to its end.
    async fn generate(&self) -> Job {
        let job = self
            .lessons()
            .request_generation(domain::UserId::OWNER, self.f.topic)
            .await
            .unwrap();
        assert!(self.runner().run_next().await.unwrap());
        self.job(job.id)
    }

    fn job(&self, id: domain::JobId) -> Job {
        let s = self.f.repo.0.lock().unwrap();
        s.jobs.iter().find(|j| j.id == id).unwrap().clone()
    }

    /// Stores a lesson directly (no model involved), the document as given.
    async fn store_lesson(&self, document: Value) -> Lesson {
        self.store_lesson_with_plan(document, plan_json()).await
    }

    async fn store_lesson_with_plan(&self, document: Value, plan: Value) -> Lesson {
        let n = self.f.repo.0.lock().unwrap().lessons.len() as u128;
        let new = GeneratedLesson {
            quality_warnings: vec![],
            id: LessonId::new(Uuid::from_u128(0x700 + n)),
            topic_id: self.f.topic,
            job_id: domain::JobId::new(Uuid::from_u128(0x800 + n)),
            meta: LessonMeta::from_document(&document).unwrap(),
            mission_version: 1,
            document,
            plan,
            component_requests: vec![],
            created_at: datetime!(2026-11-08 10:00:00 UTC),
        };
        self.f.repo.insert_generated(&new).await.unwrap()
    }

    fn calls(&self) -> usize {
        self.f.llm.requests.lock().unwrap().len()
    }

    fn lesson_rows(&self) -> usize {
        self.f.repo.0.lock().unwrap().lessons.len()
    }

    fn steps(&self) -> Vec<String> {
        let mut steps: Vec<String> = Vec::new();
        for e in self.f.events.snapshot() {
            if let domain::TopicEventPayload::JobUpdated(j) = e.payload
                && let Some(p) = j.progress
                && steps.last() != Some(&p.step)
            {
                steps.push(p.step);
            }
        }
        steps
    }

    fn attempts_of(&self, lesson: LessonId) -> Vec<Attempt> {
        let s = self.f.repo.0.lock().unwrap();
        s.attempts
            .iter()
            .filter(|a| a.lesson_id == lesson)
            .cloned()
            .collect()
    }

    fn lesson(&self, id: LessonId) -> Lesson {
        let s = self.f.repo.0.lock().unwrap();
        s.lessons
            .iter()
            .find(|l| l.lesson.id == id)
            .unwrap()
            .lesson
            .clone()
    }
}

// ----------------------------------------------------------- generation

#[tokio::test]
async fn a_valid_document_at_the_first_try_is_saved_with_its_extras() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let lesson_id = match job.result {
        Some(JobResult::LessonGeneration {
            lesson_id,
            lesson_number,
        }) => {
            assert_eq!(lesson_number, 1);
            lesson_id
        }
        other => panic!("unexpected result {other:?}"),
    };
    let lesson = env.lesson(lesson_id);
    assert_eq!(lesson.status, LessonStatus::NotStarted);
    assert_eq!(
        (
            lesson.number,
            lesson.mission_version,
            lesson.catalog_version
        ),
        (1, 1, 1)
    );
    assert_eq!(lesson.title, "Esposizione: diaframma, tempo e ISO");

    let (document, plan, requests): (Value, Value, Vec<ComponentRequest>) = {
        let s = env.f.repo.0.lock().unwrap();
        let row = &s.lessons[0];
        (row.document.clone(), row.plan.clone(), row.requests.clone())
    };
    // The model's URL never reaches the learner: the Resource's does.
    assert_eq!(document["sources"][0]["url"], "https://a.example/exposure");
    assert_eq!(document["sources"][1]["url"], "https://b.example/stops");
    assert!(
        document.to_string().contains("rubric"),
        "the stored document is complete"
    );
    assert_eq!(plan["objective"], plan_json()["objective"]);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        (requests[0].block_id.as_str(), requests[0].wanted.as_str()),
        ("mc_sim", "exposure_simulator")
    );
    assert_eq!(requests[0].fallback.as_array().unwrap().len(), 2);

    assert_eq!(env.steps(), ["planning", "writing", "checking", "saving"]);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].role, domain::llm::LlmRole::Planner);
    assert_eq!(calls[1].role, domain::llm::LlmRole::Generator);
    for call in calls.iter() {
        assert_eq!(call.system.len(), 2);
        assert!(call.system[0].cache_breakpoint);
        assert_eq!(call.messages.len(), 1);
        assert_eq!(call.messages[0].role, R::User);
    }
    assert!(matches!(calls[0].output, OutputFormat::JsonSchema { .. }));
    assert_eq!(
        calls[1].output,
        OutputFormat::Text,
        "the document is emulated JSON text"
    );
    // The community was not offered to the Planner; the knowledge sources were.
    let planner_context = &calls[0].system[1].text;
    assert!(planner_context.contains(&Uuid::from_u128(RESOURCE).to_string()));
    assert!(!planner_context.contains(&Uuid::from_u128(COMMUNITY).to_string()));
    // The Generator sees the plan and only the plan's sources.
    assert!(calls[1].system[1].text.contains("[PRIMARIA]"));
    // Both roles read the mission's starting point.
    for call in calls.iter() {
        assert!(
            call.system[1].text.contains(
                "Punto di partenza: Fotografo da due anni in automatico, mai con soggetti in movimento\n"
            ),
            "{}",
            call.system[1].text
        );
    }
    // No placeholder is left over, and a topic without records says so.
    for call in calls.iter() {
        assert!(
            !call.system[1].text.contains("{{"),
            "{}",
            call.system[1].text
        );
    }
    assert!(
        calls[1].system[1]
            .text
            .contains("(record attivi)\nNessuno.\n"),
        "{}",
        calls[1].system[1].text
    );
}

#[tokio::test]
async fn a_mission_without_starting_point_reaches_planner_and_generator_as_undeclared() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    {
        // A mission saved before contract v1.2.
        let mut s = env.f.repo.0.lock().unwrap();
        let m = &mut s.missions[0];
        m.content = MissionContent::parse_stored(
            m.content.why(),
            None,
            m.content.success_criteria(),
            &[],
            &[],
        )
        .unwrap();
    }
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls.len(), 2);
    for call in calls.iter() {
        let context = &call.system[1].text;
        assert!(
            context.contains("Punto di partenza: non dichiarato\n"),
            "{context}"
        );
        assert!(!context.contains("{{"), "{context}");
    }
}

#[tokio::test]
async fn the_generator_sees_the_plan_by_basis_and_the_active_records() {
    let mut plan = plan_json();
    plan["prerequisites"] = json!([
        {"statement": "Sa cos'è la luce", "basis": "general", "record_numbers": []},
        {"statement": "Conosce il diaframma", "basis": "record", "record_numbers": [2]}
    ]);
    let env = LessonEnv::new(vec![say(&plan.to_string()), say(&lesson_doc().to_string())]).await;
    let earlier = env.store_lesson(lesson_doc()).await;
    {
        let topic = env.f.topic;
        let mut s = env.f.repo.0.lock().unwrap();
        for (n, status) in [(1, RecordStatus::Superseded), (2, RecordStatus::Active)] {
            s.records.push(LearningRecord {
                id: LearningRecordId::new(Uuid::from_u128(0x90 + u128::from(n))),
                topic_id: topic,
                number: n,
                kind: RecordKind::Demonstrated,
                title: format!("Record {n}"),
                body: "Spiega il diaframma.".into(),
                evidence: "e".into(),
                status,
                superseded_by: None,
                lesson_id: earlier.id,
                created_at: datetime!(2026-11-08 10:00 UTC),
            });
        }
    }
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);

    let calls = env.f.llm.requests.lock().unwrap();
    let generator = &calls[1].system[1].text;
    assert!(!generator.contains("{{"), "{generator}");
    for line in [
        "Prerequisiti da richiamare (nessuna prova: serve la sezione di richiamo):\n- Sa cos'è la luce\n",
        "Prerequisiti dichiarati dalla persona (nessuna sezione, richiamo di una clausola):\n- nessuno\n",
        "Prerequisiti dimostrati (non si ripassano):\n- Conosce il diaframma [record 2]\n",
        "1. Stop: Uno stop raddoppia o dimezza la luce.\n   Esempio: ",
        "(record attivi)\n- [2] (demonstrated, lezione 1) Record 2: Spiega il diaframma.\n",
    ] {
        assert!(generator.contains(line), "{line}\n---\n{generator}");
    }
    // Only active records are shown.
    assert!(!generator.contains("[1] (demonstrated"), "{generator}");
}

#[tokio::test]
async fn an_invalid_document_is_repaired_with_the_same_system_blocks() {
    let mut broken = lesson_doc();
    broken["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x999).to_string());
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&format!("Ecco la lezione:\n```json\n{broken}\n```")),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.lesson_rows(), 1);
    assert_eq!(env.calls(), 3);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[2].trace_label, "generator.repair");
    assert_eq!(
        calls[2].system, calls[1].system,
        "same blocks: the cache is reused"
    );
    let message = match &calls[2].messages[..] {
        [m] => format!("{:?}", m.parts),
        other => panic!("one user message expected, got {other:?}"),
    };
    assert!(message.contains("unknown_resource"), "{message}");
    assert!(message.contains("/sources/0/resource_id"), "{message}");
    drop(calls);
    let repair_steps: Vec<_> = env
        .f
        .events
        .snapshot()
        .into_iter()
        .filter_map(|e| match e.payload {
            domain::TopicEventPayload::JobUpdated(j) => j.progress,
            _ => None,
        })
        .filter(|p| p.message.as_deref() == Some("Correggo la lezione"))
        .collect();
    assert_eq!(repair_steps[0].step, "writing");
    assert_eq!(repair_steps[0].done, Some(2));
}

/// The photo fixture with two code blocks in a row that read as a comparison:
/// valid, with one `consecutive_code_blocks` warning.
fn doc_with_warning() -> Value {
    let mut doc = lesson_doc();
    let at = doc["blocks"].as_array().unwrap().len() / 2;
    let code = |id: &str, caption: &str| json!({"id": id, "type": "code", "language": "rust", "code": "let x = 1;", "caption": caption});
    let blocks = doc["blocks"].as_array_mut().unwrap();
    blocks.insert(at, code("w2", "Versione invece corretta"));
    blocks.insert(at, code("w1", "Esempio"));
    doc
}

fn warning_codes(env: &LessonEnv) -> Vec<String> {
    let s = env.f.repo.0.lock().unwrap();
    s.lessons[0]
        .lesson
        .quality_warnings
        .iter()
        .map(|w| w.code.clone())
        .collect()
}

#[tokio::test]
async fn a_valid_document_without_warnings_costs_no_extra_call() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 2);
    assert!(warning_codes(&env).is_empty());
}

#[tokio::test]
async fn a_better_improvement_is_kept() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&doc_with_warning().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    assert_eq!(
        env.f.llm.requests.lock().unwrap()[2].trace_label,
        "generator.improve"
    );
    assert!(warning_codes(&env).is_empty());
    let id = env.f.repo.0.lock().unwrap().lessons[0].lesson.id;
    let stored = env.f.repo.0.lock().unwrap().lessons[0].document.clone();
    assert!(
        stored["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["id"] != "w1"),
        "{id}"
    );
}

#[tokio::test]
async fn a_worse_or_invalid_improvement_keeps_the_first_document() {
    for second in [doc_with_warning().to_string(), "non è json".to_owned()] {
        let env = LessonEnv::new(vec![
            say(&plan_json().to_string()),
            say(&doc_with_warning().to_string()),
            say(&second),
        ])
        .await;
        let job = env.generate().await;
        assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
        assert_eq!(env.calls(), 3, "one improvement only");
        assert_eq!(warning_codes(&env), ["consecutive_code_blocks"]);
    }
}

#[tokio::test]
async fn a_failed_improvement_call_does_not_fail_the_job() {
    // The script ends after the document: the next call errors.
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&doc_with_warning().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(warning_codes(&env), ["consecutive_code_blocks"]);
}

#[tokio::test]
async fn warnings_do_not_take_attempts_from_the_errors() {
    let mut broken = lesson_doc();
    broken["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x999).to_string());
    // Invalid, then valid with a warning: the improvement fits (3 documents).
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&broken.to_string()),
        say(&doc_with_warning().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 4);
    assert!(warning_codes(&env).is_empty());

    // Two invalid, then valid with a warning: no attempt left, kept as it is.
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&broken.to_string()),
        say(&broken.to_string()),
        say(&doc_with_warning().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 4);
    assert_eq!(warning_codes(&env), ["consecutive_code_blocks"]);
}

#[tokio::test]
async fn a_source_that_is_not_usable_cannot_be_cited() {
    // The community resource exists in the topic but was never offered.
    let mut doc = lesson_doc();
    doc["sources"][0]["resource_id"] = json!(Uuid::from_u128(COMMUNITY).to_string());
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&doc.to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert!(format!("{:?}", calls[2].messages).contains("unknown_resource"));
}

/// A reply that stops for `reason` instead of ending its turn.
fn cut_short(text: &str, reason: StopReason) -> Vec<Step> {
    vec![
        Step::Delta(Box::leak(text.to_owned().into_boxed_str())),
        Step::Event(LlmEvent::Finished {
            stop_reason: reason,
            usage: Usage::default(),
        }),
    ]
}

#[tokio::test]
async fn a_truncated_or_refused_answer_is_an_upstream_failure_not_a_repair() {
    // The Generator hits the token limit: the JSON is cut, repairing is useless.
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        cut_short("{\"title\": \"a metà", StopReason::MaxTokens),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
    assert_eq!(env.calls(), 2);
    assert_eq!(env.lesson_rows(), 0);

    // The Planner refuses.
    let env = LessonEnv::new(vec![cut_short("", StopReason::Refusal)]).await;
    let job = env.generate().await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
    assert_eq!(env.calls(), 1);

    // A repair that is cut short fails the same way.
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say("{\"blocks\": []}"),
        cut_short("{\"title\"", StopReason::MaxTokens),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
    assert_eq!(env.calls(), 3);

    // The Recorder too: nothing is stored.
    let env = LessonEnv::new(vec![cut_short("{\"records\": [", StopReason::MaxTokens)]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
    assert_eq!(env.calls(), 1);
    assert!(env.f.repo.0.lock().unwrap().completions.is_empty());
}

#[tokio::test]
async fn three_invalid_documents_fail_the_job_with_invalid_output() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say("{\"title\": 1}"),
        say("non è nemmeno json"),
        say("{\"blocks\": []}"),
    ])
    .await;
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
    assert_eq!(env.calls(), 4, "planner + 3 documents, no fourth");
    assert_eq!(env.lesson_rows(), 0);
    assert!(env.steps().contains(&"checking".to_owned()));
}

#[tokio::test]
async fn a_plan_with_invented_references_is_repaired_once_then_rejected() {
    let mut bad = plan_json();
    bad["primary_source_id"] = json!(Uuid::from_u128(COMMUNITY).to_string());
    let env = LessonEnv::new(vec![
        say(&bad.to_string()),
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    {
        let calls = env.f.llm.requests.lock().unwrap();
        assert_eq!(calls[1].trace_label, "planner.repair");
        assert!(format!("{:?}", calls[1].messages).contains("primary_source_id"));
    }

    let env = LessonEnv::new(vec![say(&bad.to_string()), say(&bad.to_string())]).await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
    assert_eq!(env.calls(), 2, "the Generator is never called");
}

#[tokio::test]
async fn without_a_usable_source_the_planner_is_not_called() {
    let env = LessonEnv::new(vec![]).await;
    env.f.repo.0.lock().unwrap().resources[0]
        .1
        .resources
        .clear();
    let job = env.generate().await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
    assert_eq!(env.calls(), 0);
}

#[tokio::test]
async fn a_storage_failure_while_saving_leaves_no_lesson_and_fails_the_job() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.f.repo.0.lock().unwrap().fail_learning_writes = true;
    let job = env.generate().await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::Internal);
    assert_eq!(env.lesson_rows(), 0);
}

#[tokio::test]
async fn a_generation_run_again_after_a_crash_does_not_write_a_second_lesson() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded);
    // The backend died after saving but before the job was marked done.
    {
        let mut s = env.f.repo.0.lock().unwrap();
        let j = s.jobs.iter_mut().find(|j| j.id == job.id).unwrap();
        j.status = JobStatus::Queued;
        j.result = None;
        j.finished_at = None;
    }
    assert!(env.runner().run_next().await.unwrap());
    assert_eq!(env.lesson_rows(), 1);
    assert_eq!(env.calls(), 2, "no new model calls");
    assert_eq!(env.job(job.id).status, JobStatus::Succeeded);
}

#[tokio::test]
async fn generation_needs_a_ready_topic_and_no_active_generation() {
    let env = LessonEnv::new(vec![]).await;
    env.lessons()
        .request_generation(domain::UserId::OWNER, env.f.topic)
        .await
        .unwrap();
    assert!(matches!(
        env.lessons()
            .request_generation(domain::UserId::OWNER, env.f.topic)
            .await,
        Err(AppError::Conflict(ConflictReason::JobActive))
    ));
    env.f.repo.0.lock().unwrap().topics[0].state = TopicState::Researching;
    assert!(matches!(
        env.lessons()
            .request_generation(domain::UserId::OWNER, env.f.topic)
            .await,
        Err(AppError::Conflict(ConflictReason::TopicNotReady))
    ));
}

// ------------------------------------------------------------- attempts

fn doc_with_retryable_exercises() -> Value {
    let mut doc = lesson_doc();
    let blocks = doc["blocks"].as_array_mut().unwrap();
    blocks.push(json!({"id": "qm", "type": "quiz_multi", "question": "Quali?", "required": true,
        "options": [{"id": "a", "text": "x"}, {"id": "b", "text": "y"}, {"id": "c", "text": "z"}], "correct": ["a"]}));
    blocks.push(json!({"id": "fb", "type": "fill_blank", "text": "Un {{b1}}", "blanks": [{"id": "b1", "answers": ["stop"]}],
        "feedback": {"correct": "ok", "incorrect": "no"}}));
    doc
}

fn t(s: &str) -> Value {
    json!(format!("2026-11-09T15:{s}.000Z"))
}

fn grade_json(verdict: &str, misconception: Value) -> String {
    json!({"verdict": verdict, "feedback": "Hai colto l'idea; manca il prezzo.", "misconception": misconception}).to_string()
}

#[tokio::test]
async fn closed_attempts_are_recorded_and_the_first_one_wins() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(doc_with_retryable_exercises()).await;
    let svc = env.attempts();
    let first = json!({"attempts": [
        {"block_id": "q1", "attempt_number": 1, "answer": {"option": "c"}, "outcome": "correct", "answered_at": t("08:40")},
        {"block_id": "fc1", "attempt_number": 1, "answer": {"rating": "hard"}, "answered_at": t("08:50")},
        {"block_id": "qm", "attempt_number": 1, "answer": {"options": ["b"]}, "outcome": "incorrect", "answered_at": t("09:00")},
    ]});
    let stored = svc
        .record(domain::UserId::OWNER, lesson.id, &first)
        .await
        .unwrap();
    assert_eq!(stored.len(), 3);
    assert_eq!(stored[0].outcome, Outcome::Correct);
    assert_eq!(
        stored[1].outcome,
        Outcome::Partial,
        "flashcard `hard` is partial"
    );
    assert_eq!(env.lesson(lesson.id).status, LessonStatus::InProgress);

    // The app resends its queue, with a different outcome: the stored one wins.
    let again = json!({"attempts": [
        {"block_id": "q1", "attempt_number": 1, "answer": {"option": "a"}, "outcome": "incorrect", "answered_at": t("10:00")},
        {"block_id": "qm", "attempt_number": 2, "answer": {"options": ["a"]}, "outcome": "correct", "answered_at": t("10:01")},
    ]});
    let stored = svc
        .record(domain::UserId::OWNER, lesson.id, &again)
        .await
        .unwrap();
    assert_eq!(stored[0].id, env.attempts_of(lesson.id)[0].id);
    assert_eq!(
        (stored[0].outcome, stored[0].answer.to_json()),
        (Outcome::Correct, json!({"option": "c"}))
    );
    assert_eq!(stored[1].attempt_number, 2);
    assert_eq!(env.attempts_of(lesson.id).len(), 4, "no duplicates");
}

#[tokio::test]
async fn invalid_batches_are_rejected_whole_with_the_exact_fields() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(doc_with_retryable_exercises()).await;
    let svc = env.attempts();
    let bad = json!({"attempts": [
        {"block_id": "q1", "attempt_number": 1, "answer": {"option": "c"}, "outcome": "correct", "answered_at": t("08:40")},
        {"block_id": "nope", "attempt_number": 1, "answer": {"option": "c"}, "outcome": "correct", "answered_at": t("08:40")},
        {"block_id": "q1", "attempt_number": 2, "answer": {"option": "z"}, "answered_at": t("08:40")},
        {"block_id": "fc1", "attempt_number": 1, "answer": {"rating": "good"}, "outcome": "correct", "answered_at": t("08:40")},
        {"block_id": "qm", "attempt_number": 2, "answer": {"options": ["a"]}, "outcome": "correct", "answered_at": t("08:40")},
        {"block_id": "oa1", "attempt_number": 1, "answer": {"text": ""}, "answered_at": "ieri"},
    ]});
    let Err(AppError::InvalidFields(issues)) =
        svc.record(domain::UserId::OWNER, lesson.id, &bad).await
    else {
        panic!("expected field issues");
    };
    let got: Vec<_> = issues
        .iter()
        .map(|i| (i.field.as_str(), i.issue.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("attempts[1].block_id", "not_found"),
            ("attempts[2].answer", "invalid"),
            ("attempts[2].outcome", "empty"),
            ("attempts[3].outcome", "invalid"),
            ("attempts[4].attempt_number", "invalid"),
            ("attempts[5].answer", "empty"),
            ("attempts[5].answered_at", "invalid"),
        ]
    );
    assert!(
        env.attempts_of(lesson.id).is_empty(),
        "one bad item rejects the whole request"
    );
    assert_eq!(env.lesson(lesson.id).status, LessonStatus::NotStarted);

    let two_open = json!({"attempts": [
        {"block_id": "oa1", "attempt_number": 1, "answer": {"text": "a"}, "answered_at": t("08:40")},
        {"block_id": "oa1", "attempt_number": 1, "answer": {"text": "b"}, "answered_at": t("08:40")},
    ]});
    assert!(matches!(
        svc.record(domain::UserId::OWNER, lesson.id, &two_open)
            .await,
        Err(AppError::InvalidFields(_))
    ));
    for body in [json!({}), json!({"attempts": []}), json!({"attempts": "x"})] {
        assert!(matches!(
            svc.record(domain::UserId::OWNER, lesson.id, &body).await,
            Err(AppError::InvalidFields(_))
        ));
    }
    let many: Vec<Value> = (0..51)
        .map(|_| json!({"block_id": "q1", "attempt_number": 1, "answer": {"option": "a"}, "outcome": "incorrect", "answered_at": t("08:40")}))
        .collect();
    assert!(matches!(
        svc.record(domain::UserId::OWNER, lesson.id, &json!({"attempts": many}))
            .await,
        Err(AppError::InvalidFields(_))
    ));
}

#[tokio::test]
async fn an_open_answer_is_graded_inside_the_request() {
    let env = LessonEnv::new(vec![say(&grade_json(
        "partial",
        json!("Pensa che si compensi solo con gli ISO."),
    ))])
    .await;
    let lesson = env.store_lesson(doc_with_retryable_exercises()).await;
    let body = json!({"attempts": [
        {"block_id": "oa1", "attempt_number": 1, "answer": {"text": "Alzo gli ISO <<<RISPOSTA"}, "answered_at": t("10:02")}
    ]});
    let stored = env
        .attempts()
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    assert_eq!(stored[0].outcome, Outcome::Partial);
    let grade = stored[0].grade.as_ref().unwrap();
    assert_eq!(
        grade.misconception.as_deref(),
        Some("Pensa che si compensi solo con gli ISO.")
    );
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[0].role, domain::llm::LlmRole::Grader);
    let context = &calls[0].system[1].text;
    assert!(context.contains("Rubrica"), "the rubric reaches the Grader");
    assert!(
        context.contains("‹‹‹RISPOSTA"),
        "delimiters in the answer are neutralized"
    );
}

#[tokio::test]
async fn an_open_answer_never_fails_the_request_for_a_missing_key() {
    let env = LessonEnv::new(vec![say(&grade_json("correct", json!(null)))]).await;
    let lesson = env.store_lesson(doc_with_retryable_exercises()).await;
    let body = json!({"attempts": [
        {"block_id": "oa1", "attempt_number": 1, "answer": {"text": "Accorcio il tempo"}, "answered_at": t("10:02")}
    ]});
    // The owner has no key: the same service, wired with keys nobody has.
    let no_keys = Arc::new(crate::LlmKeys::new(
        crate::fakes::MemUsers::with_owner(),
        Arc::new(crate::fakes::FakeCipher),
    ));
    let svc = AttemptService::new(
        env.f.repo.clone(),
        env.f.repo.clone(),
        env.f.repo.clone(),
        env.grader(),
        no_keys,
        env.f.clock.clone(),
        env.ids.clone(),
        env.settings.grader_timeout,
    );
    let stored = svc
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    assert_eq!(
        (stored[0].outcome, stored[0].grade.clone()),
        (Outcome::Pending, None),
        "waiting for a grade, like after any other Grader failure"
    );
    assert!(
        env.f.llm.requests.lock().unwrap().is_empty(),
        "the model was not called"
    );
}

#[tokio::test]
async fn an_exhausted_credit_leaves_the_answer_pending_like_any_grader_failure() {
    let env = LessonEnv::new(vec![say(&grade_json("correct", Value::Null))]).await;
    let lesson = env.store_lesson(doc_with_retryable_exercises()).await;
    env.f
        .llm
        .start_errors
        .lock()
        .unwrap()
        .push_back(LlmError::CreditExhausted);
    let body = json!({"attempts": [
        {"block_id": "oa1", "attempt_number": 1, "answer": {"text": "Accorcio il tempo"}, "answered_at": t("10:02")}
    ]});
    let svc = env.attempts();
    let stored = svc
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    assert_eq!(
        (stored[0].outcome, stored[0].grade.clone()),
        (Outcome::Pending, None)
    );
    let stored = svc
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    assert_eq!(stored[0].outcome, Outcome::Correct);
}

#[tokio::test]
async fn a_failed_or_slow_grader_leaves_the_answer_pending_until_it_is_sent_again() {
    // First call fails, second hangs past the 60 ms limit, third works.
    let gate = Arc::new(tokio::sync::Notify::new());
    let env = LessonEnv::new(vec![
        vec![Step::Delta("non è json")],
        vec![Step::Wait(gate.clone())],
        say(&grade_json("correct", Value::Null)),
    ])
    .await;
    let lesson = env.store_lesson(doc_with_retryable_exercises()).await;
    let body = json!({"attempts": [
        {"block_id": "oa1", "attempt_number": 1, "answer": {"text": "Accorcio il tempo"}, "answered_at": t("10:02")}
    ]});
    let svc = env.attempts();

    let stored = svc
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    assert_eq!(
        (stored[0].outcome, stored[0].grade.clone()),
        (Outcome::Pending, None)
    );
    let id = stored[0].id;

    let stored = svc
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    assert_eq!(
        stored[0].outcome,
        Outcome::Pending,
        "timed out: still pending"
    );

    let stored = svc
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    assert_eq!((stored[0].id, stored[0].outcome), (id, Outcome::Correct));
    assert_eq!(env.attempts_of(lesson.id).len(), 1);

    // Already graded: sending it again does not grade it again.
    let calls = env.calls();
    let stored = svc
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    assert_eq!(stored[0].outcome, Outcome::Correct);
    assert_eq!(env.calls(), calls);
    gate.notify_one();
}

// ----------------------------------------------------------- completion

async fn completed(env: &LessonEnv, lesson: &Lesson) -> Job {
    let job = env
        .lessons()
        .complete(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert_eq!(job.lesson_id, Some(lesson.id));
    assert!(env.runner().run_next().await.unwrap());
    env.job(job.id)
}

fn recorder_reply(extra: Value) -> String {
    let mut reply = json!({
        "records": [{
            "kind": "demonstrated",
            "title": "Compensare uno stop con un altro",
            "body": "Hai scelto la compensazione senza rumore.",
            "evidence": "Hai risposto bene alla domanda sulla compensazione.",
            "evidence_blocks": ["q1"],
            "supersedes": []
        }],
        "terms": [{
            "key": "stop", "term": "stop", "definition": "Unità di misura della luce.",
            "avoid": [], "evidence_blocks": ["q1"]
        }],
        "cards": [
            {"front": "Da f/8 a f/5.6 la luce?", "back": "Raddoppia.", "record_index": 0, "term_key": null, "unpassed_block": null},
            {"front": "Che cos'è uno stop?", "back": "Un raddoppio o dimezzamento della luce.", "record_index": null, "term_key": "stop", "unpassed_block": null}
        ]
    });
    if let (Some(base), Some(over)) = (reply.as_object_mut(), extra.as_object()) {
        for (k, v) in over {
            base.insert(k.clone(), v.clone());
        }
    }
    reply.to_string()
}

async fn answer_q1_right(env: &LessonEnv, lesson: &Lesson) {
    let body = json!({"attempts": [
        {"block_id": "q1", "attempt_number": 1, "answer": {"option": "c"}, "outcome": "correct", "answered_at": t("08:40")}
    ]});
    env.attempts()
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
}

#[tokio::test]
async fn completing_closes_the_lesson_and_refuses_later_attempts() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    let job = env
        .lessons()
        .complete(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert_eq!(
        (job.kind, job.status),
        (JobKind::LessonCompletion, JobStatus::Queued)
    );
    assert_eq!(env.lesson(lesson.id).status, LessonStatus::Completed);
    assert!(env.lesson(lesson.id).completed_at.is_some());
    assert!(matches!(
        env.lessons()
            .complete(domain::UserId::OWNER, lesson.id)
            .await,
        Err(AppError::Conflict(ConflictReason::LessonCompleted))
    ));
    let body = json!({"attempts": [
        {"block_id": "q1", "attempt_number": 1, "answer": {"option": "c"}, "outcome": "correct", "answered_at": t("08:40")}
    ]});
    assert!(matches!(
        env.attempts()
            .record(domain::UserId::OWNER, lesson.id, &body)
            .await,
        Err(AppError::Conflict(ConflictReason::LessonCompleted))
    ));
}

#[tokio::test]
async fn two_lessons_can_be_completing_at_the_same_time() {
    let env = LessonEnv::new(vec![]).await;
    let one = env.store_lesson(lesson_doc()).await;
    let two = env.store_lesson(lesson_doc()).await;
    env.lessons()
        .complete(domain::UserId::OWNER, one.id)
        .await
        .unwrap();
    env.lessons()
        .complete(domain::UserId::OWNER, two.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_recorder_output_is_stored_atomically_with_cards_and_terms() {
    let env = LessonEnv::new(vec![
        say(&grade_json("correct", Value::Null)),
        say(&recorder_reply(json!({}))),
    ])
    .await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    // An open answer left pending (the grader had failed): the job grades it.
    env.f.repo.0.lock().unwrap().attempts.push(Attempt {
        id: domain::AttemptId::new(Uuid::from_u128(0x4444)),
        lesson_id: lesson.id,
        block_id: "oa1".into(),
        kind: domain::ExerciseKind::OpenAnswer,
        attempt_number: 1,
        answer: domain::Answer::Text("Accorcio il tempo e apro il diaframma".into()),
        outcome: Outcome::Pending,
        grade: None,
        answered_at: datetime!(2026-11-09 15:10 UTC),
        created_at: datetime!(2026-11-09 15:10 UTC),
    });
    let job = completed(&env, &lesson).await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(
        job.result,
        Some(JobResult::LessonCompletion {
            lesson_id: lesson.id,
            record_count: 1,
            term_count: 1,
            review_card_count: 2
        })
    );
    assert_eq!(env.steps(), ["grading", "recording", "saving"]);
    let s = env.f.repo.0.lock().unwrap();
    assert_eq!(
        s.attempts
            .iter()
            .find(|a| a.block_id == "oa1")
            .unwrap()
            .outcome,
        Outcome::Correct
    );
    assert_eq!((s.records.len(), s.terms.len(), s.cards.len()), (1, 1, 2));
    assert_eq!(s.records[0].number, 1);
    assert_eq!(s.records[0].lesson_id, lesson.id);
    assert!(matches!(s.cards[0].origin, CardOrigin::Record(_)));
    assert!(matches!(s.cards[1].origin, CardOrigin::Term(_)));
    drop(s);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[1].role, domain::llm::LlmRole::Recorder);
    let ctx = &calls[1].system[1].text;
    assert!(ctx.contains("[q1] quiz_single"), "{ctx}");
    assert!(ctx.contains("Tentativo 1:"), "{ctx}");
    assert!(
        ctx.contains("Nessun tentativo."),
        "exercises without attempts are listed too"
    );
}

#[tokio::test]
async fn without_a_passed_exercise_nothing_is_recorded_and_the_recorder_is_not_called() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    let body = json!({"attempts": [
        {"block_id": "q1", "attempt_number": 1, "answer": {"option": "a"}, "outcome": "incorrect", "answered_at": t("08:40")}
    ]});
    env.attempts()
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(
        job.result,
        Some(JobResult::LessonCompletion {
            lesson_id: lesson.id,
            record_count: 0,
            term_count: 0,
            review_card_count: 0
        })
    );
    assert_eq!(env.calls(), 0);
    assert!(env.f.repo.0.lock().unwrap().records.is_empty());
}

#[tokio::test]
async fn a_pending_answer_that_cannot_be_graded_does_not_count_as_passed() {
    // The request's grading fails, then the job's two tries fail too: no
    // passed attempt remains, so the Recorder is never asked.
    let body = json!({"attempts": [
        {"block_id": "oa1", "attempt_number": 1, "answer": {"text": "risposta"}, "answered_at": t("08:40")}
    ]});
    let env = LessonEnv::new(vec![say("no"), say("no"), say("no")]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    env.attempts()
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(env.attempts_of(lesson.id)[0].outcome, Outcome::Pending);
    assert_eq!(
        env.calls(),
        3,
        "one in the request, two in the job, no Recorder"
    );
}

#[tokio::test]
async fn a_misconception_needs_a_first_failure_and_a_later_pass() {
    // q1 passed at the first try: calling it a corrected misconception is invalid.
    let wrong = recorder_reply(json!({"records": [{
        "kind": "misconception_corrected", "title": "t", "body": "b", "evidence": "e",
        "evidence_blocks": ["q1"], "supersedes": []
    }], "cards": []}));
    let env = LessonEnv::new(vec![
        say(&wrong),
        say(&recorder_reply(json!({"cards": []}))),
    ])
    .await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    {
        let calls = env.f.llm.requests.lock().unwrap();
        assert_eq!(calls[1].trace_label, "recorder.repair");
        assert_eq!(calls[1].system, calls[0].system);
        let text = format!("{:?}", calls[1].messages);
        assert!(text.contains("misconception_corrected"), "{text}");
    }
    assert_eq!(
        env.f.repo.0.lock().unwrap().records[0].kind,
        RecordKind::Demonstrated
    );

    // Still wrong after the repair: invalid_output, nothing stored.
    let env = LessonEnv::new(vec![say(&wrong), say(&wrong)]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
    let s = env.f.repo.0.lock().unwrap();
    assert_eq!(
        (
            s.records.len(),
            s.terms.len(),
            s.cards.len(),
            s.completions.len()
        ),
        (0, 0, 0, 0)
    );
}

#[tokio::test]
async fn a_misconception_corrected_by_a_second_attempt_is_recorded() {
    let reply = recorder_reply(json!({"records": [{
        "kind": "misconception_corrected", "title": "Più opzioni corrette", "body": "Hai visto che serve ogni opzione corretta.",
        "evidence": "Hai sbagliato e poi corretto.", "evidence_blocks": ["qm"], "supersedes": []
    }], "terms": [], "cards": []}));
    let env = LessonEnv::new(vec![say(&reply)]).await;
    let lesson = env.store_lesson(doc_with_retryable_exercises()).await;
    for (n, answer, outcome) in [(1, "b", "incorrect"), (2, "a", "correct")] {
        let body = json!({"attempts": [
            {"block_id": "qm", "attempt_number": n, "answer": {"options": [answer]}, "outcome": outcome, "answered_at": t("08:40")}
        ]});
        env.attempts()
            .record(domain::UserId::OWNER, lesson.id, &body)
            .await
            .unwrap();
    }
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(
        env.f.repo.0.lock().unwrap().records[0].kind,
        RecordKind::MisconceptionCorrected
    );
}

#[tokio::test]
async fn a_superseding_record_suspends_the_cards_of_the_one_it_replaces() {
    let reply = recorder_reply(json!({
        "records": [{
            "kind": "demonstrated", "title": "Compensazione completa", "body": "Ora compensi con ogni leva.",
            "evidence": "Hai risposto bene.", "evidence_blocks": ["q1"], "supersedes": [1]
        }],
        "terms": [{"key": "stop", "term": "Stop", "definition": "Raddoppio o dimezzamento della luce.", "avoid": ["step"], "evidence_blocks": ["q1"]}],
        "cards": [{"front": "Compensare?", "back": "Con un'altra leva.", "record_index": 0, "term_key": null, "unpassed_block": null}]
    }));
    let env = LessonEnv::new(vec![say(&reply)]).await;
    let earlier = env.store_lesson(lesson_doc()).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    let topic = env.f.topic;
    {
        let mut s = env.f.repo.0.lock().unwrap();
        let old = LearningRecordId::new(Uuid::from_u128(0x91));
        s.records.push(LearningRecord {
            id: old,
            topic_id: topic,
            number: 1,
            kind: RecordKind::Demonstrated,
            title: "Primo".into(),
            body: "b".into(),
            evidence: "e".into(),
            status: RecordStatus::Active,
            superseded_by: None,
            lesson_id: earlier.id,
            created_at: datetime!(2026-11-08 10:00 UTC),
        });
        let term = GlossaryTermId::new(Uuid::from_u128(0x92));
        s.terms.push(GlossaryTerm {
            id: term,
            topic_id: topic,
            key: "stop".into(),
            term: "stop".into(),
            definition: "Vecchia.".into(),
            avoid: vec![],
            lesson_id: earlier.id,
            first_lesson_id: Some(earlier.id),
            created_at: datetime!(2026-11-08 10:00 UTC),
            updated_at: datetime!(2026-11-08 10:00 UTC),
        });
        for (n, origin) in [
            (0x93, CardOrigin::Record(old)),
            (0x94, CardOrigin::Term(term)),
        ] {
            s.cards.push(ReviewCard {
                id: ReviewCardId::new(Uuid::from_u128(n)),
                topic_id: topic,
                lesson_id: earlier.id,
                origin,
                front: "f".into(),
                back: "b".into(),
                status: CardStatus::Active,
                suspension: None,
                schedule: domain::CardSchedule::initial(datetime!(2026-11-08 10:00 UTC)),
                created_at: datetime!(2026-11-08 10:00 UTC),
                to_consolidate: false,
            });
        }
    }
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);

    let s = env.f.repo.0.lock().unwrap();
    let old = s.records.iter().find(|r| r.number == 1).unwrap();
    let new = s.records.iter().find(|r| r.number == 2).unwrap();
    assert_eq!(
        (old.status, old.superseded_by),
        (RecordStatus::Superseded, Some(new.id))
    );
    assert_eq!(new.status, RecordStatus::Active);
    let by_front = |f: &str| s.cards.iter().find(|c| c.front == f).unwrap();
    assert_eq!(
        by_front("f").status,
        CardStatus::Suspended,
        "the replaced record's card"
    );
    assert_eq!(
        s.cards
            .iter()
            .filter(|c| c.status == CardStatus::Suspended)
            .count(),
        1,
        "the term's card stays"
    );
    assert_eq!(by_front("Compensare?").status, CardStatus::Active);
    // The term was revised in place, not duplicated.
    assert_eq!(s.terms.len(), 1);
    assert_eq!(
        (s.terms[0].term.as_str(), s.terms[0].lesson_id),
        ("Stop", lesson.id)
    );
    assert_eq!(s.terms[0].avoid, ["step"]);
    assert_eq!(s.terms[0].created_at, datetime!(2026-11-08 10:00 UTC));
}

#[tokio::test]
async fn superseding_a_record_that_is_not_active_is_rejected() {
    let reply = recorder_reply(json!({"records": [{
        "kind": "demonstrated", "title": "t", "body": "b", "evidence": "e",
        "evidence_blocks": ["q1"], "supersedes": [7]
    }], "terms": [], "cards": []}));
    let env = LessonEnv::new(vec![say(&reply), say(&reply)]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
}

#[tokio::test]
async fn two_records_replacing_the_same_one_are_repaired_not_an_internal_error() {
    let two = |()| {
        json!({"records": [
            {"kind": "demonstrated", "title": "a", "body": "b", "evidence": "e", "evidence_blocks": ["q1"], "supersedes": [1]},
            {"kind": "demonstrated", "title": "c", "body": "d", "evidence": "e", "evidence_blocks": ["q1"], "supersedes": [1]}
        ], "terms": [], "cards": []})
        .to_string()
    };
    let env = LessonEnv::new(vec![say(&two(())), say(&two(()))]).await;
    let earlier = env.store_lesson(lesson_doc()).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    env.f.repo.0.lock().unwrap().records.push(LearningRecord {
        id: LearningRecordId::new(Uuid::from_u128(0x91)),
        topic_id: env.f.topic,
        number: 1,
        kind: RecordKind::Demonstrated,
        title: "Primo".into(),
        body: "b".into(),
        evidence: "e".into(),
        status: RecordStatus::Active,
        superseded_by: None,
        lesson_id: earlier.id,
        created_at: datetime!(2026-11-08 10:00 UTC),
    });
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
    assert_eq!(env.calls(), 2, "one repair was asked");
    let calls = env.f.llm.requests.lock().unwrap();
    assert!(format!("{:?}", calls[1].messages).contains("già sostituito"));
}

#[tokio::test]
async fn a_storage_failure_while_recording_stores_nothing_and_fails_the_job() {
    let env = LessonEnv::new(vec![say(&recorder_reply(json!({})))]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    env.f.repo.0.lock().unwrap().fail_learning_writes = true;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::Internal);
    let s = env.f.repo.0.lock().unwrap();
    assert!(s.records.is_empty() && s.terms.is_empty() && s.cards.is_empty());
}

#[tokio::test]
async fn the_view_hides_rubrics_and_lists_what_the_completion_produced() {
    let env = LessonEnv::new(vec![say(&recorder_reply(json!({})))]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    let before = env
        .lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert!(before.completion.is_none());
    assert!(!before.document.to_string().contains("rubric"));
    assert_eq!(before.attempts.len(), 1);
    assert_eq!(before.lesson.status, LessonStatus::InProgress);

    completed(&env, &lesson).await;
    let after = env
        .lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    let completion = after.completion.unwrap();
    assert_eq!(completion.job.unwrap().status, JobStatus::Succeeded);
    assert_eq!(
        (
            completion.outputs.records.len(),
            completion.outputs.terms.len(),
            completion.outputs.review_card_count
        ),
        (1, 1, 2)
    );
}

#[tokio::test]
async fn a_failed_completion_job_can_be_retried_per_lesson() {
    let env = LessonEnv::new(vec![say("non json"), say("ancora no")]).await;
    let one = env.store_lesson(lesson_doc()).await;
    let two = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &one).await;
    let failed = completed(&env, &one).await;
    assert_eq!(failed.status, JobStatus::Failed);
    // Another lesson's completion does not make this failure "not the newest".
    env.lessons()
        .complete(domain::UserId::OWNER, two.id)
        .await
        .unwrap();
    let retried = env
        .f
        .jobs
        .retry(domain::UserId::OWNER, env.f.topic, failed.id)
        .await
        .unwrap();
    assert_eq!(
        (retried.status, retried.attempt, retried.lesson_id),
        (JobStatus::Queued, 0, Some(one.id))
    );
}

#[tokio::test]
async fn lessons_list_newest_first_and_the_glossary_is_alphabetical() {
    let env = LessonEnv::new(vec![]).await;
    for _ in 0..3 {
        env.store_lesson(lesson_doc()).await;
    }
    let page = env
        .lessons()
        .list(
            domain::UserId::OWNER,
            env.f.topic,
            domain::PageRequest {
                after: None,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        page.items
            .iter()
            .map(|l| l.lesson.number)
            .collect::<Vec<_>>(),
        [3, 2]
    );
    let next = env
        .lessons()
        .list(
            domain::UserId::OWNER,
            env.f.topic,
            domain::PageRequest {
                after: page.next,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        next.items
            .iter()
            .map(|l| l.lesson.number)
            .collect::<Vec<_>>(),
        [1]
    );
    assert!(next.next.is_none());

    let lesson_id = LessonId::new(Uuid::from_u128(0x700));
    {
        let mut s = env.f.repo.0.lock().unwrap();
        for (i, (key, term)) in [("b", "beta"), ("a", "Zeta"), ("c", "alfa")]
            .into_iter()
            .enumerate()
        {
            s.terms.push(GlossaryTerm {
                id: GlossaryTermId::new(Uuid::from_u128(i as u128 + 1)),
                topic_id: env.f.topic,
                key: key.into(),
                term: term.into(),
                definition: "d".into(),
                avoid: vec![],
                lesson_id,
                first_lesson_id: Some(lesson_id),
                created_at: datetime!(2026-11-08 10:00 UTC),
                updated_at: datetime!(2026-11-08 10:00 UTC),
            });
        }
    }
    let terms = env
        .lessons()
        .glossary(
            domain::UserId::OWNER,
            env.f.topic,
            domain::PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        terms
            .items
            .iter()
            .map(|t| t.term.as_str())
            .collect::<Vec<_>>(),
        ["alfa", "beta", "Zeta"]
    );
}

// ----------------------------------------------------------------- chat

#[tokio::test]
async fn a_chat_message_from_a_lesson_tells_the_tutor_which_one() {
    let env = LessonEnv::new(vec![say("Certo.")]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    let chat = crate::ChatService::new(
        env.f.repo.clone(),
        env.f.repo.clone(),
        env.f.repo.clone(),
        env.f.repo.clone(),
        env.f.events.clone(),
        env.f.llm.clone(),
        crate::TutorPrompts::embedded(),
        env.f.clock.clone(),
        env.ids.clone(),
        crate::fakes::owner_keys(),
        crate::ChatSettings {
            flush_interval: SLOW,
            history_token_budget: 8000,
            max_tool_iterations: 4,
        },
    )
    .with_lessons(env.f.repo.clone());

    let unknown = chat
        .send_about(
            domain::UserId::OWNER,
            env.f.topic,
            Some("ciao"),
            Some(LessonId::new(Uuid::from_u128(5))),
        )
        .await;
    assert!(
        matches!(unknown, Err(AppError::Validation(v)) if v[0].field == "lesson_id" && v[0].issue.as_str() == "not_found")
    );

    chat.send_about(
        domain::UserId::OWNER,
        env.f.topic,
        Some("Non capisco lo stop"),
        Some(lesson.id),
    )
    .await
    .unwrap();
    for _ in 0..200 {
        if env.calls() > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let calls = env.f.llm.requests.lock().unwrap();
    let context = &calls[0].system[1].text;
    assert!(
        context.contains("Lezione 1: \"Esposizione: diaframma, tempo e ISO\""),
        "{context}"
    );
    assert!(context.contains("Obiettivo: "), "{context}");
    let _ = (StopReason::EndTurn, LlmEvent::Usage(Usage::default()));
}

// ----------------------------------------------------------- documents

use crate::document_tests::{section, seed_ready_document};

/// A ready 12-page PDF with two sections, 5000 characters per page.
fn with_pdf(env: &LessonEnv) -> (domain::DocumentId, ResourceId) {
    let pages: Vec<String> = (1..=12)
        .map(|p| format!("Testo della pagina {p}. {}", "x".repeat(5000)))
        .collect();
    seed_ready_document(
        &env.f,
        1,
        "Fotografia pratica",
        pages,
        vec![
            section("s1", "1 Esposizione", 1, 1, 6),
            section("s2", "2 Stop", 1, 7, 12),
        ],
    )
}

fn plan_with_pdf(doc: domain::DocumentId, resource: ResourceId, sections: &[&str]) -> Value {
    let mut plan = plan_json();
    plan["primary_source_id"] = json!(resource.to_string());
    plan["other_source_ids"] = json!([Uuid::from_u128(RESOURCE).to_string()]);
    plan["document_sections"] = json!([{"document_id": doc.to_string(), "section_ids": sections}]);
    plan
}

fn lesson_with_pdf(resource: ResourceId) -> Value {
    let mut doc = lesson_doc();
    doc["sources"][0]["resource_id"] = json!(resource.to_string());
    doc["sources"][0]["url"] = Value::Null;
    doc
}

#[tokio::test]
async fn a_lesson_follows_the_chosen_pdf_sections_and_cites_it_without_a_url() {
    let env = LessonEnv::new(vec![
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s2"]).to_string()),
        say(&lesson_with_pdf(RES_ID).to_string()),
    ])
    .await;
    assert_eq!(with_pdf(&env), (DOC_ID, RES_ID));
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    // The Planner sees the index with ids, the document among the sources.
    let planner = &calls[0].system[1].text;
    assert!(planner.contains("## Documenti caricati"), "{planner}");
    assert!(planner.contains(&DOC_ID.to_string()));
    assert!(planner.contains("[s2] 2 Stop (pp. 7-12)"));
    assert!(planner.contains(&format!("{RES_ID} · Fotografia pratica")));
    // The Generator gets the text of section s2 only, cut at the limit.
    let generator = &calls[1].system[1].text;
    assert!(generator.contains("#### 2 Stop (pp. 7-12)"), "{generator}");
    assert!(generator.contains("[p. 7]") && generator.contains("Testo della pagina 7."));
    assert!(!generator.contains("Testo della pagina 3."));
    // 24,000 characters hold four pages of about 5,000; the rest is noted.
    assert!(generator.contains("[p. 10]") && !generator.contains("[p. 11]"));
    assert!(generator.contains(
        "Pagine omesse perché il testo supera il limite: \"Fotografia pratica\": pagine 11-12"
    ));
    let excerpt_len = generator
        .split("## Estratti delle fonti")
        .nth(1)
        .unwrap()
        .split("## Glossario")
        .next()
        .unwrap()
        .chars()
        .count();
    assert!(excerpt_len <= 24_000 + 60, "{excerpt_len}");
    drop(calls);

    let s = env.f.repo.0.lock().unwrap();
    let saved = &s.lessons[0].document;
    assert_eq!(
        saved["sources"][0]["url"],
        Value::Null,
        "a PDF has no address"
    );
    assert_eq!(saved["sources"][1]["url"], "https://b.example/stops");
}

#[tokio::test]
async fn a_pdf_source_with_a_url_or_a_web_source_without_one_is_repaired() {
    let mut wrong = lesson_with_pdf(RES_ID);
    wrong["sources"][0]["url"] = json!("https://invented.example/pdf");
    let env = LessonEnv::new(vec![
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s1"]).to_string()),
        say(&wrong.to_string()),
        say(&lesson_with_pdf(RES_ID).to_string()),
    ])
    .await;
    with_pdf(&env);
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[2].trace_label, "generator.repair");
    let message = format!("{:?}", calls[2].messages[0].parts);
    assert!(message.contains("/sources/0/url"), "{message}");
    assert!(message.contains("deve essere null"), "{message}");
}

#[tokio::test]
async fn a_plan_that_invents_a_section_is_repaired_by_the_planner() {
    let env = LessonEnv::new(vec![
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s9"]).to_string()),
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s1"]).to_string()),
        say(&lesson_with_pdf(RES_ID).to_string()),
    ])
    .await;
    with_pdf(&env);
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[1].trace_label, "planner.repair");
    let message = format!("{:?}", calls[1].messages[0].parts);
    assert!(
        message.contains("non è una sezione del documento"),
        "{message}"
    );
}

#[tokio::test]
async fn a_plan_from_web_sources_only_has_no_excerpts_and_the_old_context() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    with_pdf(&env);
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    let generator = &calls[1].system[1].text;
    assert!(
        generator.contains("## Estratti delle fonti\nNessuno."),
        "{generator}"
    );
}

#[tokio::test]
async fn the_planner_sees_document_ids_and_which_sections_earlier_lessons_used() {
    let env = LessonEnv::new(vec![
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s2"]).to_string()),
        say(&lesson_with_pdf(RES_ID).to_string()),
    ])
    .await;
    with_pdf(&env);
    // An earlier lesson followed section s1 of the PDF; another used web only.
    env.store_lesson_with_plan(
        lesson_with_pdf(RES_ID),
        plan_with_pdf(DOC_ID, RES_ID, &["s1"]),
    )
    .await;
    env.store_lesson(lesson_doc()).await;

    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    let planner = &calls[0].system[1].text;
    assert!(
        planner.contains(&format!("{RES_ID} · Fotografia pratica — nessun sito: PDF caricato dalla persona · documento {DOC_ID}")),
        "{planner}"
    );
    let lessons: Vec<&str> = planner
        .lines()
        .filter(|l| l.contains("· lezione "))
        .collect();
    assert_eq!(lessons.len(), 2);
    assert!(
        lessons[0]
            .ends_with(" · sezioni del PDF usate: \"Fotografia pratica\" 1 Esposizione (pp. 1-6)"),
        "{}",
        lessons[0]
    );
    assert!(!lessons[1].contains("sezioni del PDF"), "{}", lessons[1]);
    // The Generator's source line names the document too.
    assert!(
        calls[1].system[1]
            .text
            .contains(&format!("documento {DOC_ID}"))
    );
}

// ------------------------------------------------- web fetch (ADR 0009)

fn fetch_use(url: &str, id: &str) -> Value {
    json!({ "type": "server_tool_use", "id": id, "name": "web_fetch", "input": { "url": url } })
}

fn fetch_page(url: &str, id: &str, data: &str) -> Value {
    json!({ "type": "web_fetch_tool_result", "tool_use_id": id, "content": {
        "type": "web_fetch_result", "url": url, "retrieved_at": "2026-11-11T10:00:00Z",
        "content": { "type": "document", "title": "T",
                     "source": { "type": "text", "media_type": "text/plain", "data": data } } } })
}

fn fetch_failure(id: &str, code: &str) -> Value {
    json!({ "type": "web_fetch_tool_result", "tool_use_id": id,
            "content": { "type": "web_fetch_tool_result_error", "error_code": code } })
}

/// The provider read `url` (result block `result`), then the model wrote
/// `text`; `stop` is how the answer ended.
fn fetched(url: &str, id: &str, result: Value, text: Option<&str>, stop: StopReason) -> Vec<Step> {
    fetched_reporting(url, id, result, text, stop, 1)
}

/// Like [`fetched`], with the read count the provider reports in the usage
/// (0 is what `OpenRouter` does).
fn fetched_reporting(
    url: &str,
    id: &str,
    result: Value,
    text: Option<&str>,
    stop: StopReason,
    reported: u32,
) -> Vec<Step> {
    let usage = Usage {
        web_fetches: reported,
        ..Usage::default()
    };
    let mut steps = vec![
        Step::Event(LlmEvent::ServerToolUse {
            block: 0,
            name: "web_fetch".into(),
            input: json!({ "url": url }),
        }),
        Step::Event(LlmEvent::ProviderBlock {
            block: 0,
            provider: "anthropic".into(),
            payload: fetch_use(url, id),
        }),
        Step::Event(LlmEvent::ProviderBlock {
            block: 1,
            provider: "anthropic".into(),
            payload: result.clone(),
        }),
    ];
    if result["content"]["type"] == "web_fetch_tool_result_error" {
        steps.push(Step::Event(LlmEvent::ServerFetchError {
            block: 1,
            error_code: result["content"]["error_code"].as_str().unwrap().to_owned(),
        }));
    }
    if let Some(text) = text {
        steps.push(Step::Event(LlmEvent::TextDelta {
            block: 2,
            text: text.to_owned(),
        }));
    }
    steps.push(Step::Event(LlmEvent::Finished {
        stop_reason: stop,
        usage,
    }));
    steps
}

fn user_text(m: &domain::llm::LlmMessage) -> String {
    m.parts
        .iter()
        .filter_map(|p| match p {
            domain::llm::Part::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

fn fetch_tool(max_uses: u32, domains: &[&str]) -> domain::llm::ToolDef {
    domain::llm::ToolDef::ServerWebFetch {
        max_uses,
        allowed_domains: domains.iter().map(|d| (*d).to_owned()).collect(),
        max_content_tokens: 8_000,
    }
}

#[tokio::test]
async fn with_the_tool_off_the_generator_request_is_what_it_always_was() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[1].tools, []);
    assert_eq!(calls[1].messages.len(), 1);
    assert_eq!(user_text(&calls[1].messages[0]), "Procedi.");
    assert_eq!(calls[1].output, OutputFormat::Text);
    assert_eq!(calls[1].trace_label, "generator.write");
}

#[tokio::test]
async fn with_the_tool_on_the_generator_gets_it_and_the_sources_to_read() {
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(
        env.steps(),
        ["planning", "reading", "writing", "checking", "saving"]
    );

    let calls = env.f.llm.requests.lock().unwrap();
    // The Planner is not touched.
    assert_eq!(calls[0].tools, []);
    assert_eq!(user_text(&calls[0].messages[0]), "Procedi.");
    let generator = &calls[1];
    assert_eq!(
        generator.tools,
        [fetch_tool(2, &["a.example", "b.example"])]
    );
    let message = user_text(&generator.messages[0]);
    let list = format!(
        "- {} · Fonte 81 — https://a.example/exposure\n- {} · Fonte 82 — https://b.example/stops",
        Uuid::from_u128(RESOURCE),
        Uuid::from_u128(RESOURCE_2)
    );
    assert!(message.starts_with("Procedi.\n\n"), "{message}");
    assert!(message.trim_end().ends_with(&list), "{message}");
    assert!(!message.contains("{{"), "{message}");
    // The URLs of the list are the ones of the sources section.
    assert!(
        generator.system[1]
            .text
            .contains("https://a.example/exposure")
    );
    assert!(generator.system[1].text.contains("https://b.example/stops"));
}

#[tokio::test]
async fn the_ceiling_is_the_smaller_of_the_setting_and_the_web_sources() {
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 1;
    env.generate().await;
    let calls = env.f.llm.requests.lock().unwrap();
    // Only the primary source is listed and allowed: a second listed page
    // would fail with `max_uses_exceeded`.
    assert_eq!(calls[1].tools, [fetch_tool(1, &["a.example"])]);
}

#[tokio::test]
async fn a_provider_without_web_fetch_gets_neither_the_tool_nor_the_list() {
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    env.f
        .llm
        .without_web_fetch
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[1].tools, []);
    assert_eq!(user_text(&calls[1].messages[0]), "Procedi.");
}

#[tokio::test]
async fn a_paused_fetch_turn_is_continued_with_its_blocks_untouched() {
    let url = "https://a.example/exposure";
    let page = fetch_page(url, "srv_1", "Gli stop raddoppiano la luce.");
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched(url, "srv_1", page.clone(), None, StopReason::PauseTurn),
        fetched(
            "https://b.example/stops",
            "srv_2",
            fetch_page("https://b.example/stops", "srv_2", "Compensazione."),
            Some(&lesson_doc().to_string()),
            StopReason::EndTurn,
        ),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls.len(), 3);
    let second = &calls[2];
    // One read was spent: one is left, the hosts do not change.
    assert_eq!(second.tools, [fetch_tool(1, &["a.example", "b.example"])]);
    assert_eq!(second.messages.len(), 2);
    assert_eq!(second.messages[1].role, R::Assistant, "nothing after it");
    let blocks: Vec<&Value> = second.messages[1]
        .parts
        .iter()
        .filter_map(|p| match p {
            domain::llm::Part::ProviderBlock { payload, .. } => Some(payload),
            _ => None,
        })
        .collect();
    assert_eq!(blocks, [&fetch_use(url, "srv_1"), &page]);
}

#[tokio::test]
async fn the_repair_goes_on_with_the_same_conversation_and_the_same_tool() {
    let url = "https://a.example/exposure";
    let page = fetch_page(url, "srv_1", "Gli stop raddoppiano la luce.");
    let mut broken = lesson_doc();
    broken["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x999).to_string());
    let broken_text = format!("```json\n{broken}\n```");
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched(
            url,
            "srv_1",
            page.clone(),
            Some(&broken_text),
            StopReason::EndTurn,
        ),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls.len(), 3);
    let (first, repair) = (&calls[1], &calls[2]);
    assert_eq!(repair.trace_label, "generator.repair");
    assert_eq!(repair.system, first.system);
    // The same tool and hosts, to keep the history valid, with what is left
    // of the job's reads: one of the two was spent.
    assert_eq!(first.tools, [fetch_tool(2, &["a.example", "b.example"])]);
    assert_eq!(repair.tools, [fetch_tool(1, &["a.example", "b.example"])]);
    // [kickoff] [everything the model answered, pages included] [the repair]
    assert_eq!(repair.messages.len(), 3);
    assert_eq!(repair.messages[0], first.messages[0]);
    assert_eq!(repair.messages[1].role, R::Assistant);
    assert_eq!(
        repair.messages[1].parts,
        [
            domain::llm::Part::ProviderBlock {
                provider: "anthropic".into(),
                payload: fetch_use(url, "srv_1")
            },
            domain::llm::Part::ProviderBlock {
                provider: "anthropic".into(),
                payload: page
            },
            domain::llm::Part::Text(broken_text),
        ]
    );
    assert_eq!(repair.messages[2].role, R::User);
    assert!(user_text(&repair.messages[2]).contains("unknown_resource"));
    // The document is the assistant message above: not quoted a second time.
    assert!(user_text(&repair.messages[2]).contains("ultima risposta in questa conversazione"));
}

#[tokio::test]
async fn without_the_tool_the_repair_is_a_new_conversation_as_before() {
    let mut broken = lesson_doc();
    broken["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x999).to_string());
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&broken.to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.generate().await;
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[2].messages.len(), 1);
    assert_eq!(calls[2].tools, []);
}

#[tokio::test]
async fn a_failed_read_is_something_the_model_sees_not_a_failure_of_the_job() {
    let url = "https://a.example/exposure";
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched(
            url,
            "srv_1",
            fetch_failure("srv_1", "url_not_accessible"),
            Some(&lesson_doc().to_string()),
            StopReason::EndTurn,
        ),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.lesson_rows(), 1);
    assert_eq!(env.calls(), 2, "no repair, no second try");
}

#[tokio::test]
async fn a_rejection_of_the_request_with_the_tool_fails_the_job_without_a_fallback() {
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        vec![Step::Err(domain::llm::LlmError::InvalidRequest(
            "tools.0: unknown tool type web_fetch_20250910".into(),
        ))],
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::Internal);
    assert_eq!(env.calls(), 2, "no silent second request without the tool");
    assert_eq!(env.lesson_rows(), 0);
}

#[tokio::test]
async fn too_many_paused_turns_time_the_generator_out() {
    let url = "https://a.example/exposure";
    let page = fetch_page(url, "srv_1", "x");
    let mut scripts = vec![say(&plan_json().to_string())];
    for _ in 0..3 {
        scripts.push(fetched(
            url,
            "srv_1",
            page.clone(),
            None,
            StopReason::PauseTurn,
        ));
    }
    let mut env = LessonEnv::new(scripts).await;
    env.settings.max_web_fetches = 3;
    env.settings.max_continuations = 1;
    let job = env.generate().await;
    // The second pause exceeds the single continuation allowed.
    assert_eq!(job.error.unwrap().code, JobErrorCode::Timeout);
}

fn broken_text() -> String {
    let mut broken = lesson_doc();
    broken["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x999).to_string());
    format!("```json\n{broken}\n```")
}

#[tokio::test]
async fn the_read_budget_is_the_jobs_across_two_repairs() {
    let (a, b) = ("https://a.example/exposure", "https://b.example/stops");
    let broken = broken_text();
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        // 1 read of 2.
        fetched(
            a,
            "srv_1",
            fetch_page(a, "srv_1", "x"),
            Some(&broken),
            StopReason::EndTurn,
        ),
        // The first repair reads the second: the budget is now all spent.
        fetched(
            b,
            "srv_2",
            fetch_page(b, "srv_2", "y"),
            Some(&broken),
            StopReason::EndTurn,
        ),
        // The second repair has nothing left to read.
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    let limits: Vec<Option<u32>> = calls[1..]
        .iter()
        .map(|c| match &c.tools[..] {
            [domain::llm::ToolDef::ServerWebFetch { max_uses, .. }] => Some(*max_uses),
            [] => None,
            other => panic!("{other:?}"),
        })
        .collect();
    // 2 at the start, 1 left for the first repair, none left for the second:
    // the tool goes (the API rejects `max_uses: 0`), the pages read stay.
    assert_eq!(limits, [Some(2), Some(1), None]);
    assert_eq!(calls[3].messages.len(), 5);
    assert_eq!(calls[3].trace_label, "generator.repair");
}

#[tokio::test]
async fn a_repair_without_the_tool_still_answers_the_latest_document() {
    let url = "https://a.example/exposure";
    let broken = broken_text();
    let mut second = lesson_doc();
    second["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x998).to_string());
    let second = second.to_string();
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        // The only read of the job goes in the first call.
        fetched(
            url,
            "srv_1",
            fetch_page(url, "srv_1", "x"),
            Some(&broken),
            StopReason::EndTurn,
        ),
        // The first repair has no tool and is still wrong.
        say(&second),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 1;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    let last = &calls[3];
    assert!(last.tools.is_empty());
    let roles: Vec<_> = last.messages.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        [R::User, R::Assistant, R::User, R::Assistant, R::User]
    );
    // The document the last repair points to is the one that just failed.
    assert_eq!(user_text(&last.messages[3]), second);
}

#[tokio::test]
async fn a_failed_read_spends_the_budget_too() {
    let (a, b) = ("https://a.example/exposure", "https://b.example/stops");
    let broken = broken_text();
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched(
            a,
            "srv_1",
            fetch_failure("srv_1", "url_not_accessible"),
            Some(&broken),
            StopReason::EndTurn,
        ),
        fetched(
            b,
            "srv_2",
            fetch_page(b, "srv_2", "y"),
            Some(&lesson_doc().to_string()),
            StopReason::EndTurn,
        ),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[2].tools, [fetch_tool(1, &["a.example", "b.example"])]);
}

#[tokio::test]
async fn a_generator_paused_with_the_budget_spent_goes_on_instead_of_returning_the_pause() {
    let url = "https://a.example/exposure";
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched(
            url,
            "srv_1",
            fetch_page(url, "srv_1", "x"),
            None,
            StopReason::PauseTurn,
        ),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 1;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[2].tools, [fetch_tool(1, &["a.example"])]);
    // The pause is sent back whole, with nothing after it.
    assert_eq!(calls[2].messages.len(), 2);
    assert_eq!(calls[2].messages[1].role, R::Assistant);
}

#[tokio::test]
async fn a_generator_that_stays_paused_with_the_budget_spent_times_out() {
    let url = "https://a.example/exposure";
    let mut scripts = vec![say(&plan_json().to_string())];
    for _ in 0..3 {
        scripts.push(fetched(
            url,
            "srv_1",
            fetch_page(url, "srv_1", "x"),
            Some("Ecco quasi"),
            StopReason::PauseTurn,
        ));
    }
    let mut env = LessonEnv::new(scripts).await;
    env.settings.max_web_fetches = 1;
    env.settings.max_continuations = 2;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::Timeout);
    assert_eq!(env.lesson_rows(), 0, "a pause is never taken as a document");
}

#[tokio::test]
async fn reads_the_provider_does_not_report_still_spend_the_budget() {
    let url = "https://a.example/exposure";
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched_reporting(
            url,
            "srv_1",
            fetch_page(url, "srv_1", "x"),
            None,
            StopReason::PauseTurn,
            0,
        ),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    // The plan allows 2 reads (two sources); one seen in the blocks while the
    // usage said 0: 1 left.
    assert_eq!(calls[2].tools, [fetch_tool(1, &["a.example", "b.example"])]);
}

#[tokio::test]
async fn the_saved_minutes_are_computed_by_the_backend_not_taken_from_the_model() {
    let mut plan = plan_json();
    plan["estimated_minutes"] = json!(19);
    let mut doc = lesson_doc();
    doc["estimated_minutes"] = json!(17);
    let env = LessonEnv::new(vec![say(&plan.to_string()), say(&doc.to_string())]).await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);

    let (document, stored_plan, minutes) = {
        let s = env.f.repo.0.lock().unwrap();
        let row = &s.lessons[0];
        (
            row.document.clone(),
            row.plan.clone(),
            row.lesson.estimated_minutes,
        )
    };
    // The plan: 4 concepts, 2 exercises, 1 open answer, no recall.
    assert_eq!(stored_plan["estimated_minutes"], 11);
    // The document: the same formula over the prose the Validator counts.
    let expected = domain::document_minutes(&document).unwrap();
    assert_eq!(document["estimated_minutes"], expected);
    assert_eq!(minutes, expected);
    assert_ne!(expected, 17);
    assert!((1..=20).contains(&expected));
}

#[tokio::test]
async fn a_wrong_or_missing_model_minutes_figure_costs_no_repair() {
    for bad in [Some(json!(0)), Some(json!(99)), Some(json!("molti")), None] {
        let mut doc = lesson_doc();
        match &bad {
            Some(v) => doc["estimated_minutes"] = v.clone(),
            None => {
                doc.as_object_mut().unwrap().remove("estimated_minutes");
            }
        }
        let env = LessonEnv::new(vec![say(&plan_json().to_string()), say(&doc.to_string())]).await;
        let job = env.generate().await;
        assert_eq!(job.status, JobStatus::Succeeded, "{bad:?}: {:?}", job.error);
        assert_eq!(env.calls(), 2, "{bad:?}: no repair");
        let document = env.f.repo.0.lock().unwrap().lessons[0].document.clone();
        assert_eq!(
            document["estimated_minutes"],
            domain::document_minutes(&document).unwrap()
        );
    }
}

#[tokio::test]
async fn the_generator_cap_is_the_roles_not_the_requests() {
    // `[llm.roles.generator] max_output_tokens` is the only source: the use
    // case sends no cap of its own, for the first document or for its repair.
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say("{\"blocks\": []}"),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[1].trace_label, "generator.write");
    assert_eq!(calls[2].trace_label, "generator.repair");
    assert_eq!(calls[1].max_output_tokens, None);
    assert_eq!(calls[2].max_output_tokens, None);
}

thread_local! {
    /// What the test thread has logged (a `#[tokio::test]` runs on one thread).
    static LOG: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Writes to [`LOG`].
struct LogWriter;

impl std::io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        LOG.with(|log| log.borrow_mut().extend_from_slice(bytes));
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A global subscriber, installed once: a per-test one would race with the
/// other tests over `tracing`'s cache of which callsites are enabled.
fn capture_logs() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_writer(|| LogWriter)
                .with_ansi(false)
                .finish(),
        );
    });
}

#[tokio::test]
async fn running_out_of_tokens_is_logged_with_its_role_cap_effort_and_remedy() {
    capture_logs();
    // Generator, then Planner: both name themselves.
    for (steps, role) in [
        (
            vec![
                say(&plan_json().to_string()),
                cut_short("{\"title\": \"a metà", StopReason::MaxTokens),
            ],
            "generator",
        ),
        (
            vec![cut_short("{\"title\"", StopReason::MaxTokens)],
            "planner",
        ),
    ] {
        let env = LessonEnv::new(steps).await;
        let job = env.generate().await;
        // No new error code and no retry: the same `upstream_failed` as before.
        assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
        let text = LOG.with(|log| String::from_utf8(log.borrow().clone()).unwrap());
        let line = text
            .lines()
            .find(|l| {
                l.contains("ran out of output tokens") && l.contains(&format!("role=\"{role}\""))
            })
            .unwrap_or_else(|| panic!("no warning for {role}: {text}"));
        for needle in [
            "WARN",
            "output_tokens=0",
            "max_output_tokens=",
            "effort=\"low\"",
            "lower `effort` or raise `max_output_tokens` in [llm.roles.",
        ] {
            assert!(line.contains(needle), "{needle}: {line}");
        }
    }
}

// ----------------------------------------------------------- highlights

fn highlight_doc() -> Value {
    json!({"catalog_version": 1, "title": "Stop", "objective": "Saper compensare.",
    "estimated_minutes": 8, "blocks": [
        {"id": "h1", "type": "heading", "level": 1, "text": "Stop"},
        {"id": "p1", "type": "paragraph", "text": "Uno stop raddoppia la luce."},
        {"id": "c1", "type": "callout", "variant": "info", "title": "Nota", "text": "Occhio a 😀 qui."}
    ]})
}

fn hl(block: &str, start: u32, end: u32, color: &str) -> Value {
    json!({"block_id": block, "start": start, "end": end, "color": color})
}

fn highlights(env: &LessonEnv) -> crate::HighlightService {
    crate::HighlightService::new(env.f.repo.clone(), env.f.repo.clone())
}

fn as_json(stored: &[domain::Highlight]) -> Value {
    Value::Array(
        stored
            .iter()
            .map(|h| hl(&h.block_id, h.start, h.end, h.color.as_str()))
            .collect(),
    )
}

#[tokio::test]
async fn highlights_replace_the_set_in_document_order_and_resending_changes_nothing() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    let svc = highlights(&env);
    let user = domain::UserId::OWNER;

    // Sent out of order, one touching another; the callout's text has a
    // character outside the BMP (2 UTF-16 units): "Occhio a 😀 qui." is 16.
    let body = json!({"highlights": [
        hl("c1", 10, 16, "pink"),
        hl("p1", 4, 8, "green"),
        hl("p1", 0, 4, "yellow"),
    ]});
    let first = svc.replace(user, lesson.id, &body).await.unwrap();
    assert_eq!(
        as_json(&first),
        json!([
            hl("p1", 0, 4, "yellow"),
            hl("p1", 4, 8, "green"),
            hl("c1", 10, 16, "pink"),
        ])
    );
    let view = env.lessons().view(user, lesson.id).await.unwrap();
    assert_eq!(view.highlights, first, "the view uses the same order");

    let again = svc.replace(user, lesson.id, &body).await.unwrap();
    assert_eq!(again, first);
    assert_eq!(
        env.lessons()
            .view(user, lesson.id)
            .await
            .unwrap()
            .highlights,
        first
    );

    // A different set replaces, it does not add.
    let second = svc
        .replace(
            user,
            lesson.id,
            &json!({"highlights": [hl("p1", 5, 9, "blue")]}),
        )
        .await
        .unwrap();
    assert_eq!(as_json(&second), json!([hl("p1", 5, 9, "blue")]));
    assert_eq!(
        env.lessons()
            .view(user, lesson.id)
            .await
            .unwrap()
            .highlights,
        second
    );

    // An empty set clears.
    assert!(
        svc.replace(user, lesson.id, &json!({"highlights": []}))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        env.lessons()
            .view(user, lesson.id)
            .await
            .unwrap()
            .highlights
            .is_empty()
    );
}

#[tokio::test]
async fn a_rejected_set_changes_nothing_and_reports_the_contract_fields() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    let svc = highlights(&env);
    let user = domain::UserId::OWNER;
    let good = json!({"highlights": [hl("p1", 0, 4, "yellow")]});
    svc.replace(user, lesson.id, &good).await.unwrap();

    let fields = |r: Result<Vec<domain::Highlight>, AppError>| match r {
        Err(AppError::InvalidFields(f)) => f
            .into_iter()
            .map(|i| (i.field, i.issue.as_str()))
            .collect::<Vec<_>>(),
        other => panic!("expected InvalidFields, got {other:?}"),
    };
    let cases = [
        (json!({}), "highlights", "invalid"),
        (json!({"highlights": "x"}), "highlights", "invalid"),
        (
            json!({"highlights": [hl("zz", 0, 1, "yellow")]}),
            "highlights[0].block_id",
            "invalid",
        ),
        (
            json!({"highlights": [hl("h1", 0, 1, "yellow")]}),
            "highlights[0].block_id",
            "invalid",
        ),
        (
            json!({"highlights": [hl("p1", 0, 1, "red")]}),
            "highlights[0].color",
            "invalid",
        ),
        (
            json!({"highlights": [hl("p1", 0, 99, "blue")]}),
            "highlights[0].end",
            "out_of_range",
        ),
        (
            json!({"highlights": [hl("p1", 3, 3, "blue")]}),
            "highlights[0].start",
            "out_of_range",
        ),
        (
            json!({"highlights": [{"block_id": "p1", "start": 0, "end": u64::MAX, "color": "blue"}]}),
            "highlights[0].end",
            "out_of_range",
        ),
        (
            json!({"highlights": [{"block_id": "p1", "start": -1e30, "end": 3, "color": "blue"}]}),
            "highlights[0].start",
            "out_of_range",
        ),
        (
            json!({"highlights": [{"block_id": "p1", "start": 1.5, "end": 3, "color": "blue"}]}),
            "highlights[0].start",
            "invalid",
        ),
        (
            json!({"highlights": [hl("p1", 0, 5, "blue"), hl("p1", 4, 8, "blue")]}),
            "highlights[1].start",
            "overlap",
        ),
    ];
    for (body, field, issue) in cases {
        assert_eq!(
            fields(svc.replace(user, lesson.id, &body).await),
            [(field.to_owned(), issue)],
            "{body}"
        );
    }
    // 501 entries (the limit is on the count, checked first).
    let many: Vec<Value> = (0..501).map(|_| hl("p1", 0, 1, "yellow")).collect();
    assert_eq!(
        fields(
            svc.replace(user, lesson.id, &json!({"highlights": many}))
                .await
        ),
        [("highlights".to_owned(), "too_many")]
    );
    // One bad entry among good ones rejects the lot.
    let mixed = json!({"highlights": [hl("p1", 0, 4, "green"), hl("p1", 0, 99, "green")]});
    assert!(svc.replace(user, lesson.id, &mixed).await.is_err());

    assert_eq!(
        as_json(
            &env.lessons()
                .view(user, lesson.id)
                .await
                .unwrap()
                .highlights
        ),
        json!([hl("p1", 0, 4, "yellow")]),
        "the stored set is untouched"
    );
}

#[tokio::test]
async fn highlights_are_accepted_on_a_completed_lesson_and_do_not_change_it() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    env.lessons()
        .complete(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    let svc = highlights(&env);
    let stored = svc
        .replace(
            domain::UserId::OWNER,
            lesson.id,
            &json!({"highlights": [hl("p1", 0, 4, "yellow")]}),
        )
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    let after = env.lesson(lesson.id);
    assert_eq!(after.status, LessonStatus::Completed);
    assert!(after.completed_at.is_some());
}

#[tokio::test]
async fn highlights_of_a_missing_lesson_are_not_found() {
    let env = LessonEnv::new(vec![]).await;
    let r = highlights(&env)
        .replace(
            domain::UserId::OWNER,
            LessonId::new(Uuid::from_u128(0x999)),
            &json!({"highlights": []}),
        )
        .await;
    assert!(matches!(r, Err(AppError::NotFound)));
}

// -------------------------------------------------------------- strokes

fn stroke_json(id: u128, block: &str, points: &[i64]) -> Value {
    json!({"id": Uuid::from_u128(id).to_string(), "block_id": block, "color": "blue",
           "width": "thin", "points": points})
}

fn strokes(env: &LessonEnv) -> crate::StrokeService {
    crate::StrokeService::new(env.f.repo.clone(), env.f.repo.clone())
}

#[tokio::test]
async fn strokes_replace_the_set_keep_the_drawing_order_and_resending_changes_nothing() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    let svc = strokes(&env);
    let user = domain::UserId::OWNER;

    // Not in document order: the last block first.
    let body = json!({"strokes": [
        stroke_json(2, "c1", &[10, 20, 30, 40]),
        stroke_json(1, "p1", &[5, 6]),
    ]});
    let first = svc.replace(user, lesson.id, &body).await.unwrap();
    let ids: Vec<_> = first.iter().map(|s| s.block_id.as_str()).collect();
    assert_eq!(ids, ["c1", "p1"]);
    assert_eq!(first[0].points, [10, 20, 30, 40]);
    let view = env.lessons().view(user, lesson.id).await.unwrap();
    assert_eq!(view.strokes, first, "the view uses the same order");

    let again = svc.replace(user, lesson.id, &body).await.unwrap();
    assert_eq!(again, first);

    // A smaller set replaces; an empty one clears.
    let one = json!({"strokes": [stroke_json(3, "h1", &[1, 1])]});
    assert_eq!(svc.replace(user, lesson.id, &one).await.unwrap().len(), 1);
    svc.replace(user, lesson.id, &json!({"strokes": []}))
        .await
        .unwrap();
    assert!(
        env.lessons()
            .view(user, lesson.id)
            .await
            .unwrap()
            .strokes
            .is_empty()
    );
}

#[tokio::test]
async fn a_rejected_stroke_set_changes_nothing_and_reports_the_contract_fields() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    let svc = strokes(&env);
    let user = domain::UserId::OWNER;
    let good = json!({"strokes": [stroke_json(1, "p1", &[1, 2])]});
    let stored = svc.replace(user, lesson.id, &good).await.unwrap();

    let bad = json!({"strokes": [stroke_json(1, "p1", &[1, 2]), stroke_json(1, "zz", &[1, 2, 3])]});
    let fields = match svc.replace(user, lesson.id, &bad).await {
        Err(AppError::InvalidFields(f)) => f
            .iter()
            .map(|i| (i.field.clone(), i.issue.as_str()))
            .collect::<Vec<_>>(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        fields,
        [
            ("strokes[1].id".to_owned(), "duplicate"),
            ("strokes[1].block_id".to_owned(), "invalid"),
            ("strokes[1].points".to_owned(), "invalid"),
        ]
    );
    assert_eq!(
        env.lessons().view(user, lesson.id).await.unwrap().strokes,
        stored
    );
}

#[tokio::test]
async fn strokes_are_accepted_on_a_completed_lesson_and_do_not_change_it() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    env.lessons()
        .complete(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    let stored = strokes(&env)
        .replace(
            domain::UserId::OWNER,
            lesson.id,
            &json!({"strokes": [stroke_json(1, "p1", &[1, 2])]}),
        )
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    let after = env.lesson(lesson.id);
    assert_eq!(after.status, LessonStatus::Completed);
    assert!(after.completed_at.is_some());
}

#[tokio::test]
async fn strokes_of_a_missing_lesson_are_not_found() {
    let env = LessonEnv::new(vec![]).await;
    let r = strokes(&env)
        .replace(
            domain::UserId::OWNER,
            LessonId::new(Uuid::from_u128(0x999)),
            &json!({"strokes": []}),
        )
        .await;
    assert!(matches!(r, Err(AppError::NotFound)));
}

// ----------------------------------------------------------- read aloud

fn readable_count(document: &Value) -> usize {
    domain::readable_blocks(document).len()
}

async fn queued_blocks(env: &LessonEnv, lesson: LessonId) -> usize {
    let a = env.audio.as_ref().unwrap();
    let voice = a.preparer.voice_of(domain::UserId::OWNER).await.unwrap();
    a.preparer.states(lesson, &voice).await.unwrap().len()
}

#[tokio::test]
async fn a_generated_lesson_has_its_audio_queued_and_made_in_the_background() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await
    .with_audio();
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let Some(JobResult::LessonGeneration { lesson_id, .. }) = job.result else {
        panic!("no result");
    };
    let readable = readable_count(&lesson_doc());
    assert!(readable > 0);
    assert_eq!(queued_blocks(&env, lesson_id).await, readable);

    let a = env.audio.as_ref().unwrap();
    for _ in 0..readable {
        assert_eq!(
            a.worker.step().await.unwrap(),
            crate::audio_queue::Step::Made
        );
    }
    assert_eq!(
        a.worker.step().await.unwrap(),
        crate::audio_queue::Step::Idle
    );
    assert_eq!(queued_blocks(&env, lesson_id).await, 0);
}

#[tokio::test]
async fn lessons_generate_and_open_with_read_aloud_off() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let lesson = env.store_lesson(lesson_doc()).await;
    env.lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn opening_a_lesson_queues_its_missing_audio_and_requeues_failed_blocks() {
    let env = LessonEnv::new(vec![]).await.with_audio();
    let lesson = env.store_lesson(lesson_doc()).await;
    let readable = readable_count(&lesson_doc());
    assert_eq!(queued_blocks(&env, lesson.id).await, 0);

    env.lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert_eq!(queued_blocks(&env, lesson.id).await, readable);

    // Every block fails three times.
    let a = env.audio.as_ref().unwrap();
    *a.tts.fail.lock().unwrap() = crate::audio::tests::FailMode::Rejects;
    for _ in 0..3 * readable {
        a.worker.step().await.unwrap();
    }
    let voice = a.preparer.voice_of(domain::UserId::OWNER).await.unwrap();
    let states = a.preparer.states(lesson.id, &voice).await.unwrap();
    assert!(
        states
            .values()
            .all(|s| *s == domain::BlockAudioState::Failed)
    );

    // Reopened: all queued again.
    env.lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    let states = a.preparer.states(lesson.id, &voice).await.unwrap();
    assert_eq!(states.len(), readable);
    assert!(
        states
            .values()
            .all(|s| *s == domain::BlockAudioState::Queued)
    );
}

#[tokio::test]
async fn opening_a_lesson_moves_it_ahead_of_the_others() {
    let env = LessonEnv::new(vec![]).await.with_audio();
    let first = env.store_lesson(lesson_doc()).await;
    let second = env.store_lesson(lesson_doc()).await;
    let a = env.audio.as_ref().unwrap();
    let doc = lesson_doc();
    let owner = domain::UserId::OWNER;
    a.preparer
        .prepare(owner, first.id, &doc, domain::ContentLanguage::It, false)
        .await
        .unwrap();
    a.preparer
        .prepare(owner, second.id, &doc, domain::ContentLanguage::It, false)
        .await
        .unwrap();

    env.lessons().view(owner, second.id).await.unwrap();
    let next = domain::AudioQueue::claim_next(a.queue.as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.request.lesson_id, second.id);
}

struct BrokenQueue;

#[async_trait::async_trait]
impl domain::AudioQueue for BrokenQueue {
    async fn enqueue(
        &self,
        _: LessonId,
        _: &str,
        _: &[domain::AudioRequest],
        _: bool,
    ) -> Result<(), domain::RepositoryError> {
        Err(domain::RepositoryError::Storage("down".into()))
    }
    async fn claim_next(&self) -> Result<Option<domain::QueuedAudio>, domain::RepositoryError> {
        Ok(None)
    }
    async fn complete(&self, _: i64) -> Result<(), domain::RepositoryError> {
        Ok(())
    }
    async fn release(&self, _: i64) -> Result<(), domain::RepositoryError> {
        Ok(())
    }
    async fn record_failure(
        &self,
        _: i64,
        _: u32,
    ) -> Result<domain::BlockAudioState, domain::RepositoryError> {
        Ok(domain::BlockAudioState::Failed)
    }
    async fn recover(&self) -> Result<u64, domain::RepositoryError> {
        Ok(0)
    }
    async fn states(
        &self,
        _: LessonId,
        _: &str,
    ) -> Result<std::collections::HashMap<String, domain::BlockAudioState>, domain::RepositoryError>
    {
        Ok(std::collections::HashMap::new())
    }
}

#[tokio::test]
async fn a_queue_that_fails_never_fails_the_opening_or_the_generation() {
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await
    .with_audio();
    // The same worker env, but its queue refuses every write.
    let a = env.audio.take().unwrap();
    let (preparer, worker) = crate::AudioPreparer::new(
        a.cache.clone(),
        Arc::new(BrokenQueue),
        crate::fakes::MemUsers::with_owner(),
        crate::AudioSettings::default(),
    );
    env.audio = Some(crate::audio_queue::tests::Env {
        preparer,
        worker,
        ..a
    });

    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let lesson = env.store_lesson(lesson_doc()).await;
    let view = env
        .lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert_eq!(view.lesson.id, lesson.id);
}

#[tokio::test]
async fn the_grader_gets_the_code_of_an_open_answer_with_its_question() {
    let env = LessonEnv::new(vec![say(&grade_json("good", json!(null)))]).await;
    let mut doc = lesson_doc();
    let oa = doc["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["id"] == "oa1")
        .unwrap();
    oa["code"] = json!({"language": "rust", "code": "let a = 1;\nlet b = a;"});
    let lesson = env.store_lesson(doc).await;
    let body = json!({"attempts": [
        {"block_id": "oa1", "attempt_number": 1, "answer": {"text": "Compensa."}, "answered_at": t("10:02")}
    ]});
    env.attempts()
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    let calls = env.f.llm.requests.lock().unwrap();
    let context = &calls[0].system[1].text;
    assert!(
        context.contains("```rust\nlet a = 1;\nlet b = a;\n```"),
        "{context}"
    );
}

#[tokio::test]
async fn a_saved_lesson_with_long_inline_code_is_still_served() {
    // Lessons stored before contract v1.4 may break `inline_code_too_long`:
    // reading never re-validates.
    let env = LessonEnv::new(vec![]).await;
    let mut doc = lesson_doc();
    doc["blocks"][1]["text"] = json!("Vedi `let a = 1; let b = a; let c = b; println!(\"{c}\");`.");
    let lesson = env.store_lesson(doc).await;
    let view = env
        .lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert!(view.document.to_string().contains("let c = b;"));
}

#[tokio::test]
async fn a_plan_with_document_sections_has_a_reading_step_and_shows_the_sources() {
    let env = LessonEnv::new(vec![
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s2", "s1"]).to_string()),
        say(&lesson_with_pdf(RES_ID).to_string()),
    ])
    .await;
    with_pdf(&env);
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(
        env.steps(),
        ["planning", "reading", "writing", "checking", "saving"]
    );

    let owner = domain::UserId::OWNER;
    let lesson = env.f.repo.0.lock().unwrap().lessons[0].lesson.clone();
    let expected = vec![crate::DocumentSource {
        document_id: DOC_ID,
        title: "Fotografia pratica".into(),
        page_start: 1,
        page_end: 12,
    }];
    let view = env.lessons().view(owner, lesson.id).await.unwrap();
    assert_eq!(view.document_sources, expected);
    let page = env
        .lessons()
        .list(
            owner,
            env.f.topic,
            domain::PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].document_sources, expected);

    // Deleting the document removes it from the sources.
    env.f.repo.0.lock().unwrap().documents.clear();
    let view = env.lessons().view(owner, lesson.id).await.unwrap();
    assert!(view.document_sources.is_empty());
}

fn plan_with_flow() -> Value {
    let mut p = plan_json();
    p["concepts"][0]["visual"] = json!("flow");
    p
}

fn doc_with_flow() -> Value {
    let mut doc = lesson_doc();
    let flow = json!({
        "id": "fig", "type": "diagram", "layout": "flow", "title": "Ordine",
        "nodes": [{"id": "a", "label": "Apri"}, {"id": "b", "label": "Scatta"}],
        "edges": [{"from": "a", "to": "b"}],
        "caption": "Guarda **l'ordine**.", "alt": "Due passi in fila."
    });
    doc["blocks"].as_array_mut().unwrap().insert(2, flow);
    doc
}

fn has_figure(env: &LessonEnv) -> bool {
    env.f.repo.0.lock().unwrap().lessons[0].document["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["type"] == "diagram")
}

#[tokio::test]
async fn a_missing_planned_figure_triggers_an_improvement_that_is_kept() {
    let env = LessonEnv::new(vec![
        say(&plan_with_flow().to_string()),
        say(&lesson_doc().to_string()),
        say(&doc_with_flow().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    {
        let calls = env.f.llm.requests.lock().unwrap();
        assert_eq!(calls[2].trace_label, "generator.improve");
        let message = format!("{:?}", calls[2].messages);
        assert!(
            message.contains("Il piano chiede per il concetto"),
            "{message}"
        );
    }
    assert!(has_figure(&env));
    assert!(warning_codes(&env).is_empty(), "never stored");
}

#[tokio::test]
async fn an_improvement_that_loses_the_planned_figure_keeps_the_original() {
    let env = LessonEnv::new(vec![
        say(&plan_with_flow().to_string()),
        say(&doc_with_flow().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    // The figure is there: no improvement at all.
    assert_eq!(env.calls(), 2);
    assert!(has_figure(&env));
}

#[tokio::test]
async fn an_improvement_without_the_figure_keeps_the_first_draft() {
    // The first draft has the figure and a quality warning: improve runs, and
    // the answer drops the figure although it has fewer warnings.
    let mut first = doc_with_flow();
    let at = first["blocks"].as_array().unwrap().len() / 2;
    let code = |id: &str, caption: &str| json!({"id": id, "type": "code", "language": "rust", "code": "let x = 1;", "caption": caption});
    let blocks = first["blocks"].as_array_mut().unwrap();
    blocks.insert(at, code("w2", "Versione invece corretta"));
    blocks.insert(at, code("w1", "Esempio"));
    let env = LessonEnv::new(vec![
        say(&plan_with_flow().to_string()),
        say(&first.to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    assert!(has_figure(&env));
    assert_eq!(warning_codes(&env), ["consecutive_code_blocks"]);
}

#[tokio::test]
async fn two_flows_requested_one_present_keeps_the_draft_when_improve_drops_it() {
    let mut plan = plan_with_flow();
    plan["concepts"][1]["visual"] = json!("flow");
    let env = LessonEnv::new(vec![
        say(&plan.to_string()),
        say(&doc_with_flow().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    assert!(has_figure(&env));
}

#[tokio::test]
async fn no_improvement_runs_when_the_repairs_are_exhausted() {
    let env = LessonEnv::new(vec![
        say(&plan_with_flow().to_string()),
        say("not json"),
        say("still not json"),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 4);
    let calls = env.f.llm.requests.lock().unwrap();
    assert!(calls.iter().all(|c| c.trace_label != "generator.improve"));
}

/// The fixture with a fourth concept section, so its concept sections line
/// up with the four concepts of `plan_json`; `warning` puts the typical
/// mistake callout in the first one.
fn doc_aligned(warning: bool, flow: bool) -> Value {
    let mut doc = if flow { doc_with_flow() } else { lesson_doc() };
    let blocks = doc["blocks"].as_array_mut().unwrap();
    let at = blocks
        .iter()
        .position(|b| b["type"] == "heading" && b["text"] == "Ripasso e pratica")
        .unwrap();
    blocks.insert(
        at,
        json!({"id": "p4", "type": "paragraph", "text": "Il diaframma regola la luce."}),
    );
    blocks.insert(
        at,
        json!({"id": "h4", "type": "heading", "level": 2, "text": "Diaframma e luce"}),
    );
    if warning {
        // The fixture's only callout of the first section becomes the
        // typical mistake (a second callout would trip `too_many_callouts`).
        let callout = blocks.iter_mut().find(|b| b["type"] == "callout").unwrap();
        callout["variant"] = json!("warning");
    }
    doc
}

fn has_warning_callout(env: &LessonEnv) -> bool {
    env.f.repo.0.lock().unwrap().lessons[0].document["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["type"] == "callout" && b["variant"] == "warning")
}

#[tokio::test]
async fn a_missing_typical_mistake_triggers_an_improvement_that_is_kept() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&doc_aligned(false, false).to_string()),
        say(&doc_aligned(true, false).to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    {
        let calls = env.f.llm.requests.lock().unwrap();
        assert_eq!(calls[2].trace_label, "generator.improve");
        let message = format!("{:?}", calls[2].messages);
        assert!(
            message.contains("Il piano dà per il concetto «Stop» un errore tipico"),
            "{message}"
        );
    }
    assert!(has_warning_callout(&env));
    assert!(warning_codes(&env).is_empty(), "never stored");
}

#[tokio::test]
async fn an_improvement_that_drops_the_typical_mistake_keeps_the_original() {
    // The draft has the callout and a quality warning: improve runs and
    // answers with fewer warnings but without the callout. Better on the
    // warnings alone, so only the lost callout can reject it.
    let mut first = doc_aligned(true, false);
    let at = first["blocks"].as_array().unwrap().len() / 2;
    let code = |id: &str, caption: &str| json!({"id": id, "type": "code", "language": "rust", "code": "let x = 1;", "caption": caption});
    let blocks = first["blocks"].as_array_mut().unwrap();
    blocks.insert(at, code("w2", "Versione invece corretta"));
    blocks.insert(at, code("w1", "Esempio"));
    let candidate = doc_aligned(false, false);
    let score = |doc: &Value| {
        domain::warning_score(
            &domain::LessonValidator::new(
                domain::ValidatorConfig::default(),
                domain::ValidationContext::fixture(),
            )
            .quality_warnings(doc, domain::ContentLanguage::It),
        )
    };
    assert!(
        score(&candidate) < score(&first),
        "the candidate must improve the warnings"
    );
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&first.to_string()),
        say(&candidate.to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    assert!(has_warning_callout(&env));
    assert!(
        !warning_codes(&env).is_empty(),
        "the original, warnings included, is kept"
    );
}

#[tokio::test]
async fn an_improvement_that_adds_the_callout_but_loses_the_figure_is_discarded() {
    let mut plan = plan_with_flow();
    plan["concepts"][0]["pitfall"] = json!("Pensare che uno stop sommi luce.");
    let env = LessonEnv::new(vec![
        say(&plan.to_string()),
        say(&doc_aligned(false, true).to_string()),
        say(&doc_aligned(true, false).to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    assert!(has_figure(&env));
    assert!(!has_warning_callout(&env));
}
