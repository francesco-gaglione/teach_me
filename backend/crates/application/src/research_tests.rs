//! The Researcher: the `research` job handler and the resources use cases,
//! over in-memory fakes and a scripted model. No test calls a real model.

use std::sync::Arc;
use std::time::Duration;

use domain::llm::{
    LlmError, LlmEvent, MessageRole as R, OutputFormat, Part, StopReason, ToolDef, Usage,
};
use domain::{
    Job, JobErrorCode, JobKind, JobResult, JobStatus, Mission, MissionContent, ResearchInfo,
    ResourceKind, ResourceRepository, TopicEventPayload, TopicId, TopicResources, TopicState,
};
use serde_json::{Value, json};
use time::macros::datetime;
use uuid::Uuid;

use crate::chat_tests::{Fixture, SLOW, fixture_with};
use crate::error::{AppError, ConflictReason};
use crate::fakes::Step;
use crate::{
    JobRunner, JobSettings, ResearchSettings, ResearcherPrompts, ResourceService, RunResearch,
};

const BOOK: &str = "https://doc.rust-lang.org/book/";
const AXUM: &str = "https://docs.rs/axum/latest/axum/";
const FORUM: &str = "https://users.rust-lang.org/";

async fn fixture(scripts: Vec<Vec<Step>>) -> Fixture {
    fixture_with(scripts, SLOW, 4, |llm| llm).await
}

impl Fixture {
    /// The topic has a confirmed mission and is being researched, as after
    /// the first confirmation.
    fn researching(&self) {
        let mut s = self.repo.0.lock().unwrap();
        let at = datetime!(2026-11-08 10:00:00 UTC);
        let content = MissionContent::parse(
            "Voglio leggere codice axum",
            "Programmo in Kotlin; di Rust ho letto solo i primi capitoli",
            &["Leggo un servizio axum".to_owned()],
            &["30 minuti al giorno".to_owned()],
            &[],
        )
        .unwrap();
        let mut mission = Mission::draft(self.topic, 2, content, at);
        mission.confirm(at).unwrap();
        s.missions.push(mission);
        s.topics
            .iter_mut()
            .find(|t| t.id == self.topic)
            .unwrap()
            .state = TopicState::Researching;
    }

    fn handler(&self, settings: ResearchSettings) -> Arc<RunResearch> {
        Arc::new(RunResearch::new(
            self.repo.clone(),
            self.repo.clone(),
            self.repo.clone(),
            self.repo.clone(),
            self.events.clone(),
            self.llm.clone(),
            ResearcherPrompts::embedded(),
            self.clock.clone(),
            Arc::new(crate::fakes::SeqIds::default()),
            settings,
        ))
    }

    /// Queues a research job and runs it to its end.
    async fn research(&self, settings: ResearchSettings) -> Job {
        self.enqueue(self.topic).await;
        let runner = JobRunner::new(
            self.jobs.clone(),
            JobSettings {
                max_concurrent: 1,
                job_timeout: Duration::from_secs(30),
                max_attempts: 3,
                store_retry_delay: Duration::from_millis(1),
            },
        )
        .with_handler(JobKind::Research, self.handler(settings));
        assert!(runner.run_next().await.unwrap());
        self.stored_job(self.topic)
    }

    fn sources(&self) -> TopicResources {
        let s = self.repo.0.lock().unwrap();
        s.resources
            .iter()
            .find(|(t, _)| *t == self.topic)
            .map_or_default(|(_, r)| r.clone())
    }

    fn topic_state(&self) -> TopicState {
        self.repo
            .0
            .lock()
            .unwrap()
            .topics
            .iter()
            .find(|t| t.id == self.topic)
            .unwrap()
            .state
    }

    /// `step` of every progress report published, in order, without repeats.
    fn steps(&self) -> Vec<String> {
        let mut steps: Vec<String> = Vec::new();
        for e in self.events.snapshot() {
            if let TopicEventPayload::JobUpdated(j) = e.payload
                && let Some(p) = j.progress
                && steps.last() != Some(&p.step)
            {
                steps.push(p.step);
            }
        }
        steps
    }

    fn messages(&self) -> Vec<String> {
        self.events
            .snapshot()
            .into_iter()
            .filter_map(|e| match e.payload {
                TopicEventPayload::JobUpdated(j) => j.progress.and_then(|p| p.message),
                _ => None,
            })
            .collect()
    }
}

fn resource(url: &str) -> Value {
    json!({
        "kind": "knowledge", "source_type": "documentation", "title": "The Rust Book",
        "url": url, "language": "en",
        "annotation": "Il libro ufficiale del team di Rust.", "use_for": "capire ownership"
    })
}

fn answer(resources: &[Value], skipped: bool) -> String {
    json!({
        "resources": resources,
        "gaps": [{ "area": "Migrazioni", "note": "Solo post datati." }],
        "communities_skipped": skipped
    })
    .to_string()
}

/// One search with `urls` as results, then the final text.
fn searched(query: &str, urls: &[&str], text: String) -> Vec<Step> {
    let mut steps = search_steps(query, urls);
    steps.push(Step::Event(LlmEvent::TextDelta { block: 5, text }));
    steps
}

fn search_steps(query: &str, urls: &[&str]) -> Vec<Step> {
    vec![
        Step::Event(LlmEvent::ServerToolUse {
            block: 0,
            name: "web_search".into(),
            input: json!({ "query": query }),
        }),
        Step::Event(LlmEvent::ProviderBlock {
            block: 0,
            provider: "anthropic".into(),
            payload: json!({ "type": "server_tool_use", "id": "srv_1", "name": "web_search",
                             "input": { "query": query } }),
        }),
        Step::Event(LlmEvent::ServerSearchResults {
            block: 1,
            urls: urls.iter().map(|u| (*u).to_owned()).collect(),
        }),
        Step::Event(LlmEvent::ProviderBlock {
            block: 1,
            provider: "anthropic".into(),
            payload: json!({ "type": "web_search_tool_result", "tool_use_id": "srv_1", "content": [] }),
        }),
        Step::Event(LlmEvent::Usage(Usage {
            web_searches: 1,
            ..Usage::default()
        })),
    ]
}

fn paused(mut steps: Vec<Step>) -> Vec<Step> {
    steps.push(Step::Event(LlmEvent::Finished {
        stop_reason: StopReason::PauseTurn,
        usage: Usage::default(),
    }));
    steps
}

fn text(text: String) -> Vec<Step> {
    vec![Step::Event(LlmEvent::TextDelta { block: 0, text })]
}

#[tokio::test]
async fn a_research_stores_the_sources_readies_the_topic_and_reports_each_step() {
    let reply = answer(&[resource("https://DOC.rust-lang.org/book#intro")], false);
    let f = fixture(vec![searched(
        "rust ownership",
        &[BOOK, AXUM],
        format!("```json\n{reply}\n```"),
    )])
    .await;
    f.researching();

    let job = f.research(ResearchSettings::default()).await;

    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(
        job.result,
        Some(JobResult::Research {
            resource_count: 1,
            gap_count: 1
        })
    );
    let stored = f.sources();
    assert_eq!(stored.resources.len(), 1);
    // The stored URL is the one the search returned, not the model's spelling.
    assert_eq!(stored.resources[0].url.as_deref(), Some(BOOK));
    assert_eq!(
        stored.resources[0].domain().as_deref(),
        Some("doc.rust-lang.org")
    );
    assert_eq!(stored.gaps[0].area, "Migrazioni");
    assert_eq!(
        stored.research,
        Some(ResearchInfo {
            job_id: job.id,
            mission_version: 2,
            finished_at: stored.research.unwrap().finished_at
        })
    );
    assert_eq!(f.topic_state(), TopicState::Ready);

    // The topic change is announced before the job is reported as done.
    let kinds = f.kinds();
    let ready = kinds.iter().position(|k| k == "topic:ready").unwrap();
    let done = kinds
        .iter()
        .position(|k| k == "job:research:succeeded")
        .unwrap();
    assert!(ready < done, "{kinds:?}");

    assert_eq!(
        f.steps(),
        ["planning", "searching", "reading", "evaluating", "saving"]
    );
    let messages = f.messages();
    assert!(
        messages.contains(&"Ho cercato: rust ownership".to_owned()),
        "{messages:?}"
    );
    assert!(messages.iter().all(|m| m.chars().count() <= 80));

    // One call: the brief, the search tool, a plain-text answer.
    let requests = f.requests();
    assert_eq!(requests.len(), 1);
    let req = &requests[0];
    assert_eq!(req.trace_label, "researcher.search");
    assert_eq!(req.output, OutputFormat::Text);
    assert_eq!(
        req.tools,
        [ToolDef::ServerWebSearch {
            max_uses: 4,
            allowed_domains: vec![],
            blocked_domains: vec![]
        }]
    );
    assert_eq!(req.system.len(), 2);
    assert!(req.system[0].cache_breakpoint && !req.system[1].cache_breakpoint);
    let context = &req.system[1].text;
    assert!(context.contains("Voglio leggere codice axum"), "{context}");
    assert!(context.contains("versione 2"));
    assert!(context.contains("2026-11-08"));
    assert!(context.contains("al massimo 4 ricerche"));
    assert!(
        context.contains(
            "Punto di partenza: Programmo in Kotlin; di Rust ho letto solo i primi capitoli\n"
        ),
        "{context}"
    );
    assert!(
        context.contains("Nessuna."),
        "no preferences and no sources yet"
    );
}

#[tokio::test]
async fn a_mission_without_starting_point_reaches_the_researcher_as_undeclared() {
    let f = fixture(vec![text(answer(&[], false))]).await;
    f.researching();
    {
        // A mission saved before contract v1.2.
        let mut s = f.repo.0.lock().unwrap();
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
    f.research(ResearchSettings::default()).await;
    let context = &f.requests()[0].system[1].text;
    assert!(
        context.contains("Punto di partenza: non dichiarato\n"),
        "{context}"
    );
    assert!(!context.contains("{{"), "{context}");
}

#[tokio::test]
async fn the_search_budget_comes_from_the_settings() {
    let f = fixture(vec![text(answer(&[], false))]).await;
    f.researching();
    // An empty list without gaps is invalid; this only inspects the request.
    f.research(ResearchSettings {
        max_searches: 7,
        ..ResearchSettings::default()
    })
    .await;
    let req = &f.requests()[0];
    assert!(matches!(
        req.tools[0],
        ToolDef::ServerWebSearch { max_uses: 7, .. }
    ));
    assert!(req.system[1].text.contains("al massimo 7 ricerche"));
}

#[tokio::test]
async fn a_paused_turn_is_continued_and_all_urls_seen_along_the_way_count() {
    let reply = answer(&[resource(BOOK), resource(AXUM)], false);
    let f = fixture(vec![
        paused(search_steps("rust book", &[BOOK])),
        searched("axum docs", &[AXUM], reply),
    ])
    .await;
    f.researching();

    let job = f.research(ResearchSettings::default()).await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(f.sources().resources.len(), 2);
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    let second = &requests[1];
    // Same tool, with the searches already made taken off the ceiling.
    assert!(matches!(
        second.tools[0],
        ToolDef::ServerWebSearch { max_uses: 3, .. }
    ));
    assert_eq!(second.messages.len(), 2);
    let last = second.messages.last().unwrap();
    assert_eq!(
        last.role,
        R::Assistant,
        "no user message after a paused turn"
    );
    assert!(last.parts.iter().any(|p| matches!(
        p,
        Part::ProviderBlock { payload, .. } if payload["type"] == "web_search_tool_result"
    )));
    assert_eq!(
        f.steps(),
        [
            "planning",
            "searching",
            "reading",
            "searching",
            "reading",
            "evaluating",
            "saving"
        ]
    );
    assert_eq!(
        f.messages()
            .iter()
            .filter(|m| m.starts_with("Ho cercato"))
            .count(),
        2
    );
}

#[tokio::test]
async fn a_turn_that_keeps_pausing_fails_with_timeout_after_the_cap() {
    let scripts = (0..8).map(|_| paused(search_steps("q", &[BOOK]))).collect();
    let f = fixture(scripts).await;
    f.researching();

    let job = f
        .research(ResearchSettings {
            max_searches: 100,
            ..ResearchSettings::default()
        })
        .await;

    assert_eq!(job.error.unwrap().code, JobErrorCode::Timeout);
    assert_eq!(
        f.requests().len(),
        6,
        "the first call and five continuations"
    );
    assert_eq!(f.topic_state(), TopicState::Researching);
}

#[tokio::test]
async fn an_invented_url_is_repaired_once_with_structured_output_and_no_tools() {
    let bad = answer(&[resource("https://invented.example.org/x")], false);
    let good = answer(&[resource(BOOK)], false);
    let f = fixture(vec![searched("q", &[BOOK], bad), text(good)]).await;
    f.researching();

    let job = f.research(ResearchSettings::default()).await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(f.sources().resources[0].url.as_deref(), Some(BOOK));
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    let repair = &requests[1];
    assert_eq!(repair.trace_label, "researcher.repair");
    assert_eq!(repair.tools.len(), 0);
    assert!(
        matches!(&repair.output, OutputFormat::JsonSchema { name, .. } if name == "researcher_output")
    );
    let Part::Text(prompt) = &repair.messages[0].parts[0] else {
        panic!("the repair is asked in plain text");
    };
    assert!(prompt.contains("invented.example.org"), "the text to fix");
    assert!(prompt.contains("resources[0].url"), "the problem");
    assert!(prompt.contains(BOOK), "the allowed URLs");
}

#[tokio::test]
async fn an_answer_that_stays_invalid_after_the_repair_fails_with_invalid_output() {
    let bad = answer(&[resource("https://invented.example.org/x")], false);
    let f = fixture(vec![
        searched("q", &[BOOK], bad.clone()),
        text(bad),
        // Never reached: one repair only.
        text(answer(&[resource(BOOK)], false)),
    ])
    .await;
    f.researching();

    let job = f.research(ResearchSettings::default()).await;

    let error = job.error.unwrap();
    assert_eq!(error.code, JobErrorCode::InvalidOutput);
    assert_eq!(error.code.as_str(), "invalid_output");
    assert_eq!(f.requests().len(), 2);
    assert_eq!(f.topic_state(), TopicState::Researching);
    assert_eq!(f.sources(), TopicResources::default());
}

#[tokio::test]
async fn unparsable_text_goes_to_the_repair_too() {
    let good = answer(&[resource(BOOK)], false);
    let f = fixture(vec![
        searched("q", &[BOOK], "Ho trovato delle fonti ottime!".into()),
        text(good),
    ])
    .await;
    f.researching();
    let job = f.research(ResearchSettings::default()).await;
    assert_eq!(job.status, JobStatus::Succeeded);
}

#[tokio::test]
async fn communities_are_dropped_when_the_person_does_not_want_them() {
    let community = json!({
        "kind": "community", "source_type": "forum", "title": "Rust Users",
        "url": FORUM, "language": "en", "annotation": "Moderato.", "use_for": "domande pratiche"
    });
    let reply = answer(&[resource(BOOK), community], true);
    let f = fixture(vec![searched("q", &[BOOK, FORUM], reply)]).await;
    f.researching();

    f.research(ResearchSettings::default()).await;

    let stored = f.sources();
    assert!(stored.communities_skipped);
    assert_eq!(stored.resources.len(), 1);
    assert_eq!(stored.resources[0].kind, ResourceKind::Knowledge);
}

#[tokio::test]
async fn a_new_research_replaces_the_set_keeps_a_ready_topic_ready_and_sees_the_old_sources() {
    // First research.
    let first = answer(&[resource(BOOK)], false);
    let second = answer(&[resource(BOOK), resource(AXUM)], false);
    let f = fixture(vec![
        searched("q1", &[BOOK], first),
        // The second run finds only AXUM and keeps the existing BOOK.
        searched("q2", &[AXUM], second),
    ])
    .await;
    f.researching();
    f.research(ResearchSettings::default()).await;
    assert_eq!(f.topic_state(), TopicState::Ready);
    let events_before = f.kinds().iter().filter(|k| *k == "topic:ready").count();

    let service = ResourceService::new(
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.jobs.clone(),
    );
    let queued = service
        .start_research(domain::UserId::OWNER, f.topic)
        .await
        .unwrap();
    assert_eq!(queued.kind, JobKind::Research);
    let runner = JobRunner::new(f.jobs.clone(), JobSettings::default())
        .with_handler(JobKind::Research, f.handler(ResearchSettings::default()));
    assert!(runner.run_next().await.unwrap());

    let stored = f.sources();
    assert_eq!(stored.resources.len(), 2);
    assert_eq!(f.topic_state(), TopicState::Ready);
    assert_eq!(
        f.kinds().iter().filter(|k| *k == "topic:ready").count(),
        events_before,
        "a ready topic is not announced again"
    );
    let context = &f.requests()[1].system[1].text;
    assert!(
        context.contains(&format!(
            "- [knowledge] The Rust Book — {BOOK} — capire ownership"
        )),
        "{context}"
    );
    let view = service.get(domain::UserId::OWNER, f.topic).await.unwrap();
    assert_eq!(view.job.unwrap().id, queued.id);
    assert_eq!(view.resources.len(), 2);
}

#[tokio::test]
async fn model_failures_become_job_errors_and_leave_the_sources_alone() {
    for (error, code) in [
        (
            LlmError::RateLimited { retry_after: None },
            JobErrorCode::RateLimited,
        ),
        (LlmError::Timeout, JobErrorCode::Timeout),
        (LlmError::Unavailable, JobErrorCode::UpstreamFailed),
        (LlmError::Authentication, JobErrorCode::LlmKeyRejected),
        (LlmError::CreditExhausted, JobErrorCode::LlmCreditExhausted),
        (LlmError::InvalidRequest("x".into()), JobErrorCode::Internal),
    ] {
        let f = fixture(vec![]).await;
        f.researching();
        f.llm.start_errors.lock().unwrap().push_back(error.clone());
        let job = f.research(ResearchSettings::default()).await;
        assert_eq!(job.error.unwrap().code, code, "{error:?}");
        assert_eq!(f.topic_state(), TopicState::Researching);
        assert_eq!(f.sources(), TopicResources::default());
    }
}

#[tokio::test]
async fn a_refused_answer_fails_the_job() {
    let f = fixture(vec![vec![Step::Event(LlmEvent::Finished {
        stop_reason: StopReason::Refusal,
        usage: Usage::default(),
    })]])
    .await;
    f.researching();
    let job = f.research(ResearchSettings::default()).await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
}

#[tokio::test]
async fn a_storage_failure_while_saving_is_internal_and_the_topic_does_not_move() {
    let f = fixture(vec![searched(
        "q",
        &[BOOK],
        answer(&[resource(BOOK)], false),
    )])
    .await;
    f.researching();
    f.repo.0.lock().unwrap().fail_replace = true;
    let job = f.research(ResearchSettings::default()).await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::Internal);
    assert_eq!(f.topic_state(), TopicState::Researching);
}

#[tokio::test]
async fn asking_for_a_research_needs_a_confirmed_mission_and_no_active_research() {
    let f = fixture(vec![]).await;
    let service = ResourceService::new(
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.jobs.clone(),
    );
    assert!(matches!(
        service.start_research(domain::UserId::OWNER, f.topic).await,
        Err(AppError::Conflict(ConflictReason::NoConfirmedMission))
    ));
    assert!(matches!(
        service
            .start_research(domain::UserId::OWNER, TopicId::new(Uuid::from_u128(999)))
            .await,
        Err(AppError::NotFound)
    ));

    f.researching();
    let job = service
        .start_research(domain::UserId::OWNER, f.topic)
        .await
        .unwrap();
    assert_eq!(job.status, JobStatus::Queued);
    assert!(f.kinds().contains(&"job:research:queued".to_owned()));
    assert!(matches!(
        service.start_research(domain::UserId::OWNER, f.topic).await,
        Err(AppError::Conflict(ConflictReason::JobActive))
    ));
}

#[tokio::test]
async fn an_unresearched_topic_has_an_empty_view_and_no_job() {
    let f = fixture(vec![]).await;
    let service = ResourceService::new(
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.jobs.clone(),
    );
    let view = service.get(domain::UserId::OWNER, f.topic).await.unwrap();
    assert!(view.resources.is_empty() && view.gaps.is_empty());
    assert!(view.research.is_none() && view.job.is_none() && !view.communities_skipped);
    assert!(matches!(
        service
            .get(domain::UserId::OWNER, TopicId::new(Uuid::from_u128(999)))
            .await,
        Err(AppError::NotFound)
    ));
    // Keeps the trait in scope for the helper above.
    let _ = f.repo.get(f.topic).await.unwrap();
}

fn paused_after(searches: u32, mut steps: Vec<Step>) -> Vec<Step> {
    steps.push(Step::Event(LlmEvent::Finished {
        stop_reason: StopReason::PauseTurn,
        usage: Usage {
            web_searches: searches,
            ..Usage::default()
        },
    }));
    steps
}

#[tokio::test]
async fn the_search_budget_is_carried_over_paused_turns_and_never_exceeded() {
    let f = fixture(vec![
        paused_after(1, search_steps("q1", &[BOOK])),
        paused_after(1, search_steps("q2", &[AXUM])),
        // Budget spent: the paused turn is not continued; the repair gets the
        // last word.
        text(answer(&[resource(BOOK)], false)),
    ])
    .await;
    f.researching();

    let job = f
        .research(ResearchSettings {
            max_searches: 2,
            ..ResearchSettings::default()
        })
        .await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let requests = f.requests();
    assert_eq!(requests.len(), 3);
    let uses = |i: usize| match requests[i].tools.first() {
        Some(ToolDef::ServerWebSearch { max_uses, .. }) => Some(*max_uses),
        _ => None,
    };
    assert_eq!((uses(0), uses(1)), (Some(2), Some(1)));
    assert_eq!(requests[2].trace_label, "researcher.repair");
    assert_eq!(requests[2].tools.len(), 0, "no more searching");
}

#[tokio::test]
async fn whether_communities_are_wanted_is_the_models_call_in_v1() {
    // Documents a known limit: with the flag false a community is stored, even
    // if the person opted out (the flag comes from the model, not from code).
    let community = json!({
        "kind": "community", "source_type": "forum", "title": "Rust Users",
        "url": FORUM, "language": "en", "annotation": "Moderato.", "use_for": "domande pratiche"
    });
    let reply = answer(&[resource(BOOK), community], false);
    let f = fixture(vec![searched("q", &[BOOK, FORUM], reply)]).await;
    f.researching();

    f.research(ResearchSettings::default()).await;

    let stored = f.sources();
    assert!(!stored.communities_skipped);
    assert_eq!(stored.resources.len(), 2);
    assert_eq!(stored.resources[1].kind, ResourceKind::Community);
}

#[tokio::test]
async fn a_research_never_touches_the_uploaded_documents_and_the_model_sees_their_index() {
    use crate::document_tests::{section, seed_ready_document};
    let reply = answer(&[resource(BOOK)], false);
    let second_reply = answer(&[resource(AXUM)], false);
    let f = fixture(vec![
        searched("rust ownership", &[BOOK], format!("```json\n{reply}\n```")),
        searched("axum", &[AXUM], format!("```json\n{second_reply}\n```")),
    ])
    .await;
    f.researching();
    let (doc_id, resource_id) = seed_ready_document(
        &f,
        1,
        "Rust in Action",
        vec!["testo".into(); 30],
        vec![section("s1", "1 Intro", 1, 1, 30)],
    );
    let handler = Arc::new(
        RunResearch::new(
            f.repo.clone(),
            f.repo.clone(),
            f.repo.clone(),
            f.repo.clone(),
            f.events.clone(),
            f.llm.clone(),
            ResearcherPrompts::embedded(),
            f.clock.clone(),
            Arc::new(crate::fakes::SeqIds::default()),
            ResearchSettings::default(),
        )
        .with_documents(f.repo.clone()),
    );
    f.enqueue(f.topic).await;
    let runner = JobRunner::new(
        f.jobs.clone(),
        JobSettings {
            max_concurrent: 1,
            job_timeout: Duration::from_secs(30),
            max_attempts: 3,
            store_retry_delay: Duration::from_millis(1),
        },
    )
    .with_handler(JobKind::Research, handler);
    assert!(runner.run_next().await.unwrap());

    let job = f.stored_job(f.topic);
    // The count is of web sources only.
    assert_eq!(
        job.result,
        Some(JobResult::Research {
            resource_count: 1,
            gap_count: 1
        })
    );
    let stored = f.sources();
    assert_eq!(stored.resources.len(), 2);
    assert_eq!(stored.resources[0].id, resource_id, "documents come first");
    assert_eq!(stored.resources[0].document_id, Some(doc_id));
    assert_eq!(stored.resources[1].url.as_deref(), Some(BOOK));

    // A second research replaces the web sources again; the document stays.
    f.repo.0.lock().unwrap().jobs.clear();
    f.enqueue(f.topic).await;
    assert!(runner.run_next().await.unwrap());
    let again = f.sources();
    assert_eq!(again.resources.len(), 2);
    assert_eq!(again.resources[0].id, resource_id);
    assert_eq!(again.resources[1].url.as_deref(), Some(AXUM));

    // The index is in the brief, and the document is not among "saved sources".
    let requests = f.requests();
    let context = &requests[0].system[1].text;
    assert!(context.contains("## Documenti caricati"), "{context}");
    assert!(context.contains("\"Rust in Action\" · 30 pagine · en"));
    assert!(context.contains("- 1 Intro (pp. 1-30)"));
    assert!(!context.contains("[document]"));
    let second = &requests[1].system[1].text;
    assert!(second.contains(BOOK), "the web source is listed for reuse");
}
