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
    include_str!("../../../../../docs/contracts/fixtures/lesson-photo-exposure.json");

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

mod annotations;
mod attempts;
mod chat;
mod completion;
mod documents;
mod generation;
mod improvements;
mod read_aloud;
mod web_fetch;

fn warning_codes(env: &LessonEnv) -> Vec<String> {
    let s = env.f.repo.0.lock().unwrap();
    s.lessons[0]
        .lesson
        .quality_warnings
        .iter()
        .map(|w| w.code.clone())
        .collect()
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

fn t(s: &str) -> Value {
    json!(format!("2026-11-09T15:{s}.000Z"))
}

fn grade_json(verdict: &str, misconception: Value) -> String {
    json!({"verdict": verdict, "feedback": "Hai colto l'idea; manca il prezzo.", "misconception": misconception}).to_string()
}

fn doc_with_retryable_exercises() -> Value {
    let mut doc = lesson_doc();
    let blocks = doc["blocks"].as_array_mut().unwrap();
    blocks.push(json!({"id": "qm", "type": "quiz_multi", "question": "Quali?", "required": true,
        "options": [{"id": "a", "text": "x"}, {"id": "b", "text": "y"}, {"id": "c", "text": "z"}], "correct": ["a"]}));
    blocks.push(json!({"id": "fb", "type": "fill_blank", "text": "Un {{b1}}", "blanks": [{"id": "b1", "answers": ["stop"]}],
        "feedback": {"correct": "ok", "incorrect": "no"}}));
    doc
}

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

async fn answer_q1_right(env: &LessonEnv, lesson: &Lesson) {
    let body = json!({"attempts": [
        {"block_id": "q1", "attempt_number": 1, "answer": {"option": "c"}, "outcome": "correct", "answered_at": t("08:40")}
    ]});
    env.attempts()
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
}
