//! Mission, preference and tool-loop use cases, over in-memory fakes.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use domain::llm::{LlmError, LlmEvent, MessageRole as R, Part, StopReason, Usage};
use domain::{
    ChatErrorCode, ChatMessage, IssueKind, MessagePart, MessageStatus, Mission, MissionContent,
    MissionRepository, MissionStatus, PageRequest, Preference, PreferenceId, PreferenceRepository,
    PreferenceScope, PreferenceText, ScopeKind, TopicEventPayload, TopicId, TopicState,
};
use serde_json::{Value, json};
use time::macros::datetime;
use uuid::Uuid;

use crate::chat_tests::{Fixture, SLOW, fixture_with, wait_for};
use crate::error::{AppError, ConflictReason};
use crate::fakes::{FakeLlm, Step};
use crate::retry::{RetryEnv, RetryPolicy, RetryingLlm};
use crate::{MissionService, PreferenceService};

fn draft_input(why: &str) -> Value {
    json!({
        "why": why,
        "starting_point": "Programmo in Kotlin; di Rust ho letto solo i primi capitoli",
        "success_criteria": ["Leggo e modifico un servizio axum", "Scrivo test con tokio"],
        "constraints": ["30 minuti al giorno"],
        "out_of_scope": []
    })
}

pub(crate) fn tool(name: &'static str, input: Value) -> Step {
    Step::Tool {
        id: "tu_1",
        name,
        input,
    }
}

pub(crate) fn save_draft(why: &str) -> Step {
    tool("save_mission_draft", draft_input(why))
}

async fn fixture(scripts: Vec<Vec<Step>>) -> Fixture {
    fixture_with(scripts, SLOW, 4, |llm| llm).await
}

impl Fixture {
    fn missions(&self) -> Vec<Mission> {
        self.repo.0.lock().unwrap().missions.clone()
    }

    fn preferences(&self) -> Vec<Preference> {
        self.repo.0.lock().unwrap().preferences.clone()
    }

    fn mission_service(&self) -> MissionService {
        MissionService::new(
            self.repo.clone(),
            self.repo.clone(),
            self.events.clone(),
            self.clock.clone(),
            self.jobs.clone(),
        )
    }

    fn preference_service(&self) -> PreferenceService {
        PreferenceService::new(self.repo.clone(), self.events.clone(), self.clock.clone())
    }

    async fn state(&self) -> TopicState {
        self.topics
            .get(domain::UserId::OWNER, self.topic)
            .await
            .unwrap()
            .state
    }

    /// Sends a message and waits until the reply and every event of the turn
    /// (the last one is the topic update, if there is one) are out.
    pub(crate) async fn turn(&self, text: &str) -> ChatMessage {
        let sent = self
            .chat
            .send(domain::UserId::OWNER, self.topic, Some(text))
            .await
            .unwrap();
        let id = sent.assistant.id;
        wait_for(|| {
            let m = self.stored(id);
            m.status != MessageStatus::Streaming
        })
        .await;
        // `message_completed`/`message_failed` is published after the row is
        // saved, and a topic update follows it.
        wait_for(|| {
            self.kinds()
                .iter()
                .any(|k| k == "completed" || k == "failed")
        })
        .await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        self.stored(id)
    }

    /// A confirmed mission, as if an earlier conversation had produced it.
    async fn seed_confirmed(&self, why: &str) -> Mission {
        self.seed_confirmed_content(content(why)).await
    }

    /// Like `seed_confirmed`, for a version saved before contract v1.2.
    async fn seed_confirmed_without_starting_point(&self, why: &str) -> Mission {
        let old = MissionContent::parse_stored(
            why,
            None,
            &[
                "Leggo e modifico un servizio axum".to_owned(),
                "Scrivo test con tokio".to_owned(),
            ],
            &["30 minuti al giorno".to_owned()],
            &[],
        )
        .unwrap();
        self.seed_confirmed_content(old).await
    }

    async fn seed_confirmed_content(&self, content: MissionContent) -> Mission {
        let mut m = self
            .repo
            .insert_draft(self.topic, &content, self.clock_now(), None)
            .await
            .unwrap();
        m.confirm(self.clock_now()).unwrap();
        self.repo
            .confirm(&m, self.clock_now(), None, None)
            .await
            .unwrap();
        self.repo
            .0
            .lock()
            .unwrap()
            .topics
            .iter_mut()
            .find(|t| t.id == self.topic)
            .unwrap()
            .state = TopicState::Researching;
        m
    }

    fn clock_now(&self) -> time::OffsetDateTime {
        *self.clock.0.lock().unwrap()
    }

    pub(crate) fn requests(&self) -> Vec<domain::llm::LlmRequest> {
        self.llm.requests.lock().unwrap().clone()
    }

    /// The tool results sent back in the request number `n` (0-based).
    pub(crate) fn results_of_request(&self, n: usize) -> Vec<(String, bool)> {
        let req = &self.requests()[n];
        let last = req.messages.last().unwrap();
        assert_eq!(last.role, R::User);
        last.parts
            .iter()
            .map(|p| match p {
                Part::ToolResult {
                    content, is_error, ..
                } => (content.clone(), *is_error),
                other => panic!("{other:?}"),
            })
            .collect()
    }
}

fn content(why: &str) -> MissionContent {
    MissionContent::parse(
        why,
        "Programmo in Kotlin; di Rust ho letto solo i primi capitoli",
        &["Leggo e modifico un servizio axum".to_owned()],
        &[],
        &[],
    )
    .unwrap()
}

fn card(m: &ChatMessage) -> Option<(&Mission, Option<u32>)> {
    m.parts.iter().find_map(|p| match p {
        MessagePart::MissionProposal {
            mission,
            replaces_version,
        } => Some((mission, *replaces_version)),
        MessagePart::Text(_) | MessagePart::LessonRequest { .. } => None,
    })
}

// ---------- the tool loop ----------

#[tokio::test]
async fn a_saved_draft_becomes_a_card_after_the_text_and_moves_the_topic() {
    let f = fixture(vec![
        vec![
            Step::Delta("Ho capito. "),
            save_draft("Per il nuovo lavoro"),
        ],
        vec![],
    ])
    .await;
    let done = f.turn("Voglio imparare Rust").await;

    assert_eq!(done.status, MessageStatus::Complete);
    assert_eq!(done.parts.len(), 2);
    assert_eq!(done.parts[0], MessagePart::Text("Ho capito. ".into()));
    let (mission, replaces) = card(&done).unwrap();
    assert_eq!(mission.version, 1);
    assert_eq!(mission.status, MissionStatus::Draft);
    assert_eq!(replaces, None);
    assert!(matches!(done.parts[1], MessagePart::MissionProposal { .. }));

    // Events: the draft just before the message, the new state right after.
    assert_eq!(
        f.kinds(),
        [
            "created:user",
            "created:assistant",
            "topic:interviewing",
            "delta:Ho capito. ",
            "mission_proposed:1",
            "completed",
            "topic:mission_proposed"
        ]
    );
    assert_eq!(f.state().await, TopicState::MissionProposed);
    let stored = f.missions();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].content.why(), "Per il nuovo lavoro");

    // Two model calls: the second carries the assistant turn and the result.
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].tools.len(), 4);
    assert_eq!(requests[1].tools, requests[0].tools);
    let echoed = &requests[1].messages[requests[1].messages.len() - 2];
    assert_eq!(echoed.role, R::Assistant);
    assert!(matches!(echoed.parts[0], Part::Text(_)));
    assert!(matches!(&echoed.parts[1], Part::ToolUse { name, .. } if name == "save_mission_draft"));
    let results = f.results_of_request(1);
    assert_eq!(results.len(), 1);
    assert!(!results[0].1);
    assert!(results[0].0.contains("versione 1"), "{}", results[0].0);
    // The results message is the cache breakpoint of the loop.
    assert!(requests[1].messages.last().unwrap().cache_breakpoint);
}

#[tokio::test]
async fn tool_calls_and_results_are_not_visible_messages() {
    let f = fixture(vec![vec![save_draft("x")], vec![Step::Delta("Ecco.")]]).await;
    f.turn("ciao").await;
    let all = f
        .chat
        .list(
            domain::UserId::OWNER,
            f.topic,
            PageRequest {
                after: None,
                limit: 50,
            },
        )
        .await
        .unwrap()
        .page
        .items;
    // Only the user message and the single assistant message.
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn text_after_a_tool_round_is_kept_before_the_card_and_not_glued() {
    let f = fixture(vec![
        vec![Step::Delta("Uno"), save_draft("x")],
        vec![Step::Delta("Due")],
    ])
    .await;
    let done = f.turn("ciao").await;
    assert_eq!(done.parts.len(), 2);
    assert_eq!(done.parts[0], MessagePart::Text("Uno\n\nDue".into()));
    assert!(matches!(done.parts[1], MessagePart::MissionProposal { .. }));
    let kinds = f.kinds();
    assert!(kinds.contains(&"delta:\n\nDue".to_owned()), "{kinds:?}");
    // The delta names the text part, not the card's slot.
    let part_index = f
        .events
        .snapshot()
        .into_iter()
        .rev()
        .find_map(|e| match e.payload {
            TopicEventPayload::MessageDelta { part_index, .. } => Some(part_index),
            _ => None,
        })
        .unwrap();
    assert_eq!(part_index, 0);
}

#[tokio::test]
async fn a_validation_error_goes_back_to_the_model_which_corrects_it() {
    let mut bad = draft_input("x");
    bad["success_criteria"] = json!([]);
    bad["why"] = json!("x".repeat(501));
    let f = fixture(vec![
        vec![tool("save_mission_draft", bad)],
        vec![save_draft("Per lavoro")],
        vec![Step::Delta("Ecco la bozza.")],
    ])
    .await;
    let done = f.turn("ciao").await;

    let first = f.results_of_request(1);
    assert!(first[0].1, "validation errors are flagged");
    assert!(first[0].0.contains("Bozza NON salvata"), "{}", first[0].0);
    assert!(first[0].0.contains("`why`"), "{}", first[0].0);
    assert!(first[0].0.contains("501"), "{}", first[0].0);
    assert!(first[0].0.contains("`success_criteria`"), "{}", first[0].0);
    assert!(!f.results_of_request(2)[0].1);

    // Nothing was stored for the bad attempt; the user saw no error.
    assert_eq!(f.missions().len(), 1);
    assert_eq!(done.status, MessageStatus::Complete);
    assert_eq!(card(&done).unwrap().0.version, 1);
    assert_eq!(done.text(), "Ecco la bozza.");
}

/// The tool input with `starting_point` replaced (or removed with `None`).
fn draft_with_starting_point(tool_name: &'static str, sp: Option<Value>) -> Step {
    let mut input = draft_input("Prima");
    match sp {
        Some(v) => input["starting_point"] = v,
        None => {
            input.as_object_mut().unwrap().remove("starting_point");
        }
    }
    tool(tool_name, input)
}

#[tokio::test]
async fn starting_point_is_required_and_bounded_in_both_mission_tools() {
    // (value, accepted). Lengths count characters after the trim.
    let cases: Vec<(Option<Value>, bool)> = vec![
        (None, false),
        (Some(json!("")), false),
        (Some(json!("   ")), false),
        (Some(json!("è".repeat(600))), true),
        (Some(json!(format!("  {}  ", "è".repeat(600)))), true),
        (Some(json!("è".repeat(601))), false),
    ];
    for (sp, accepted) in cases {
        for change in [false, true] {
            let name = if change {
                "propose_mission_change"
            } else {
                "save_mission_draft"
            };
            let f = fixture(vec![
                vec![draft_with_starting_point(name, sp.clone())],
                vec![Step::Delta("ok")],
            ])
            .await;
            if change {
                f.seed_confirmed("Altra").await;
            }
            let done = f.turn("ciao").await;
            let r = f.results_of_request(1);
            assert_eq!(!r[0].1, accepted, "{name} {sp:?}: {}", r[0].0);
            assert_eq!(card(&done).is_some(), accepted, "{name} {sp:?}");
            if accepted {
                let stored = f.missions().pop().unwrap();
                assert_eq!(
                    stored.content.starting_point().map(|s| s.chars().count()),
                    Some(600)
                );
            } else {
                assert!(r[0].0.contains("`starting_point`"), "{}", r[0].0);
                assert_eq!(f.missions().len(), usize::from(change));
            }
        }
    }
}

#[tokio::test]
async fn a_change_that_only_adds_the_starting_point_to_an_old_mission_is_a_new_version() {
    let f = fixture(vec![
        vec![tool("propose_mission_change", draft_input("Prima"))],
        vec![Step::Delta("ok")],
    ])
    .await;
    // Saved before v1.2: same text otherwise, no starting point.
    f.seed_confirmed_without_starting_point("Prima").await;
    let done = f.turn("ciao").await;
    assert_eq!(card(&done).unwrap().0.version, 2);
    assert_eq!(f.missions().len(), 2);
}

#[tokio::test]
async fn an_input_that_is_not_an_object_is_reported_as_shape_error() {
    let f = fixture(vec![
        vec![tool("save_mission_draft", json!("oops"))],
        vec![Step::Delta("ok")],
    ])
    .await;
    f.turn("ciao").await;
    let r = f.results_of_request(1);
    assert!(r[0].1);
    assert!(r[0].0.contains("Input non valido"), "{}", r[0].0);
    assert_eq!(f.missions().len(), 0);
}

#[tokio::test]
async fn unknown_extra_and_mistyped_fields_are_listed() {
    let f = fixture(vec![
        vec![tool(
            "save_mission_draft",
            json!({"why": 3, "success_criteria": ["a", 4], "bonus": true}),
        )],
        vec![Step::Delta("ok")],
    ])
    .await;
    f.turn("ciao").await;
    let text = &f.results_of_request(1)[0].0;
    for needle in [
        "`bonus` non esiste",
        "`why` deve essere una stringa",
        "`starting_point` manca",
        "`constraints` manca",
        "`out_of_scope` manca",
        "`success_criteria` deve essere una stringa",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in {text}");
    }
}

#[tokio::test]
async fn draft_tool_is_refused_once_a_mission_is_confirmed_and_change_tool_before() {
    // Not allowed: a draft when a confirmed mission exists.
    let f = fixture(vec![vec![save_draft("x")], vec![Step::Delta("ok")]]).await;
    f.seed_confirmed("Prima").await;
    let done = f.turn("ciao").await;
    let r = f.results_of_request(1);
    assert!(r[0].1);
    assert!(r[0].0.contains("già una mission confermata"), "{}", r[0].0);
    assert!(card(&done).is_none());
    assert_eq!(f.missions().len(), 1);

    // Not allowed: a change when there is nothing confirmed.
    let g = fixture(vec![
        vec![tool("propose_mission_change", draft_input("x"))],
        vec![Step::Delta("ok")],
    ])
    .await;
    g.turn("ciao").await;
    let r = g.results_of_request(1);
    assert!(r[0].1);
    assert!(r[0].0.contains("non ha ancora una mission confermata"));
    assert_eq!(g.missions().len(), 0);
    // A refused tool does not move the topic.
    assert_eq!(g.state().await, TopicState::Interviewing);
}

#[tokio::test]
async fn a_mission_change_keeps_the_topic_state_and_names_the_replaced_version() {
    let f = fixture(vec![
        vec![
            Step::Delta("Va bene. "),
            tool("propose_mission_change", draft_input("Nuovo motivo")),
        ],
        vec![],
    ])
    .await;
    let confirmed = f.seed_confirmed("Prima").await;
    let done = f.turn("Cambiamo la mission").await;

    let (mission, replaces) = card(&done).unwrap();
    assert_eq!((mission.version, replaces), (2, Some(confirmed.version)));
    assert_eq!(f.state().await, TopicState::Researching);
    let kinds = f.kinds();
    assert!(kinds.contains(&"mission_proposed:2".to_owned()));
    assert!(
        !kinds.iter().any(|k| k == "topic:mission_proposed"),
        "{kinds:?}"
    );
    // The confirmed mission stays valid until the user confirms.
    let current = f.repo.current(f.topic).await.unwrap().unwrap();
    assert_eq!(current.version, 1);
    let text = &f.results_of_request(1)[0].0;
    assert!(
        text.contains("versione 2") && text.contains("mission 1"),
        "{text}"
    );
}

#[tokio::test]
async fn a_change_identical_to_the_confirmed_mission_creates_nothing() {
    let f = fixture(vec![
        vec![tool(
            "propose_mission_change",
            json!({
                "why": "Prima",
                "starting_point": " Programmo in Kotlin; di Rust ho letto solo i primi capitoli ",
                "success_criteria": ["  Leggo e modifico un servizio axum "],
                "constraints": [],
                "out_of_scope": []
            }),
        )],
        vec![Step::Delta("Identica.")],
    ])
    .await;
    f.seed_confirmed("Prima").await;
    let done = f.turn("ciao").await;
    assert!(card(&done).is_none());
    assert_eq!(f.missions().len(), 1);
    let r = f.results_of_request(1);
    assert!(!r[0].1);
    assert!(r[0].0.contains("identica"), "{}", r[0].0);
}

#[tokio::test]
async fn only_one_mission_tool_per_turn() {
    let f = fixture(vec![
        vec![save_draft("Primo"), save_draft("Secondo")],
        vec![Step::Delta("ok")],
    ])
    .await;
    let done = f.turn("ciao").await;
    let r = f.results_of_request(1);
    assert_eq!(r.len(), 2);
    assert!(!r[0].1);
    assert!(
        r[1].1 && r[1].0.contains("già salvato una bozza"),
        "{}",
        r[1].0
    );
    assert_eq!(f.missions().len(), 1);
    assert_eq!(card(&done).unwrap().0.content.why(), "Primo");
}

#[tokio::test]
async fn an_identical_draft_in_a_later_turn_is_not_versioned_again_and_has_no_card() {
    let f = fixture(vec![
        vec![save_draft("Per lavoro")],
        vec![Step::Delta("Ecco.")],
        vec![save_draft("Per lavoro")],
        vec![Step::Delta("Già proposta.")],
    ])
    .await;
    f.turn("uno").await;
    f.clock.set(datetime!(2026-11-08 11:00:00 UTC));
    let second = f.turn("due").await;
    assert_eq!(f.missions().len(), 1);
    assert!(card(&second).is_none());
    let r = f.results_of_request(3);
    assert!(!r[0].1 && r[0].0.contains("identica"), "{}", r[0].0);
    // The topic went back to interviewing for the second message and stays so:
    // the draft is still open and confirmable.
    assert_eq!(f.state().await, TopicState::Interviewing);
    assert!(f.repo.open_draft(f.topic).await.unwrap().is_some());
}

#[tokio::test]
async fn a_corrected_draft_supersedes_the_open_one() {
    let f = fixture(vec![
        vec![save_draft("Uno")],
        vec![Step::Delta("a")],
        vec![save_draft("Due")],
        vec![Step::Delta("b")],
    ])
    .await;
    f.turn("uno").await;
    f.clock.set(datetime!(2026-11-08 11:00:00 UTC));
    let second = f.turn("correggi").await;
    assert_eq!(card(&second).unwrap().0.version, 2);
    let stored = f.missions();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].status, MissionStatus::Superseded);
    assert_eq!(
        stored[0].superseded_at,
        Some(datetime!(2026-11-08 11:00:00 UTC))
    );
    assert_eq!(stored[1].status, MissionStatus::Draft);
}

#[tokio::test]
async fn a_new_message_reopens_the_interview_but_keeps_the_draft_open() {
    let f = fixture(vec![
        vec![save_draft("x")],
        vec![Step::Delta("a")],
        vec![Step::Delta("b")],
    ])
    .await;
    f.turn("uno").await;
    assert_eq!(f.state().await, TopicState::MissionProposed);
    f.turn("due").await;
    assert_eq!(f.state().await, TopicState::Interviewing);
    assert!(
        f.kinds()
            .iter()
            .filter(|k| *k == "topic:interviewing")
            .count()
            >= 2
    );
    assert_eq!(
        f.repo.open_draft(f.topic).await.unwrap().unwrap().version,
        1
    );
}

#[tokio::test]
async fn the_iteration_limit_fails_the_message_with_internal() {
    let looping: Vec<Vec<Step>> = (0..6)
        .map(|_| vec![tool("no_such_tool", json!({}))])
        .collect();
    let f = fixture_with(looping, SLOW, 4, |llm| llm).await;
    let done = f.turn("ciao").await;
    assert_eq!(done.status, MessageStatus::Failed);
    assert_eq!(done.error.unwrap().code, ChatErrorCode::Internal);
    // 4 tool rounds, then the fifth reply asking again is refused.
    assert_eq!(f.requests().len(), 5);
    let r = f.results_of_request(4);
    assert!(r[0].1 && r[0].0.contains("Strumento sconosciuto: no_such_tool"));
}

#[tokio::test]
async fn at_the_iteration_limit_existing_text_is_kept_and_the_message_completes() {
    let mut scripts = vec![vec![
        Step::Delta("Intanto: "),
        tool("no_such_tool", json!({})),
    ]];
    scripts.extend((0..5).map(|_| vec![tool("no_such_tool", json!({}))]));
    let f = fixture_with(scripts, SLOW, 2, |llm| llm).await;
    let done = f.turn("ciao").await;
    assert_eq!(done.status, MessageStatus::Complete);
    assert_eq!(done.text(), "Intanto: ");
    // Two tool rounds ran; the third request for tools was not honoured.
    assert_eq!(f.requests().len(), 3);
}

#[tokio::test]
async fn tools_are_not_run_when_the_reply_was_cut_or_refused() {
    let cut = Step::Event(LlmEvent::Finished {
        stop_reason: StopReason::MaxTokens,
        usage: Usage::default(),
    });
    let f = fixture(vec![vec![Step::Delta("Tagliato"), save_draft("x"), cut]]).await;
    let done = f.turn("ciao").await;
    assert_eq!(done.status, MessageStatus::Complete);
    assert_eq!(done.text(), "Tagliato");
    assert_eq!(f.missions().len(), 0);
    assert_eq!(f.requests().len(), 1);

    let refused = Step::Event(LlmEvent::Finished {
        stop_reason: StopReason::Refusal,
        usage: Usage::default(),
    });
    let g = fixture(vec![vec![save_draft("x"), refused]]).await;
    let done = g.turn("ciao").await;
    assert_eq!(done.status, MessageStatus::Failed);
    assert_eq!(g.missions().len(), 0);
}

#[tokio::test]
async fn a_failure_in_a_later_call_fails_the_message_but_keeps_the_effects() {
    let f = fixture(vec![
        vec![Step::Delta("Ho capito. "), save_draft("x")],
        vec![Step::Fail(ChatErrorCode::UpstreamFailed)],
    ])
    .await;
    let failed = f.turn("ciao").await;
    assert_eq!(failed.status, MessageStatus::Failed);
    assert_eq!(
        failed.error.as_ref().unwrap().code,
        ChatErrorCode::UpstreamFailed
    );
    // The card is part of what was produced; the draft is stored and announced.
    assert!(card(&failed).is_some());
    assert_eq!(f.state().await, TopicState::MissionProposed);
    let kinds = f.kinds();
    let at = |name: &str| kinds.iter().position(|k| k == name).unwrap();
    assert!(at("mission_proposed:1") < at("failed"));
    assert!(at("failed") < at("topic:mission_proposed"));
}

#[tokio::test]
async fn retrying_a_failed_message_reuses_its_own_draft_and_attaches_the_card() {
    let f = fixture(vec![
        vec![save_draft("x")],
        vec![Step::Fail(ChatErrorCode::UpstreamFailed)],
        // The retry: the model proposes the same draft again.
        vec![save_draft("x")],
        vec![Step::Delta("Ecco.")],
    ])
    .await;
    let failed = f.turn("ciao").await;
    assert_eq!(failed.status, MessageStatus::Failed);

    let retried = f
        .chat
        .retry(domain::UserId::OWNER, f.topic, failed.id)
        .await
        .unwrap();
    wait_for(|| f.stored(retried.id).status == MessageStatus::Complete).await;
    let done = f.stored(retried.id);

    assert_eq!(f.missions().len(), 1, "no second version");
    assert_eq!(card(&done).unwrap().0.version, 1);
    assert_eq!(done.text(), "Ecco.");
}

#[tokio::test]
async fn a_retried_llm_call_does_not_run_its_tools_twice() {
    struct NoWait;
    #[async_trait]
    impl RetryEnv for NoWait {
        async fn sleep(&self, _: Duration) {}
        fn jitter(&self, _: Duration) -> Duration {
            Duration::ZERO
        }
    }
    let pref = tool(
        "save_preference",
        json!({"scope": "topic", "text": "Esempi in Python"}),
    );
    let f = fixture_with(
        vec![
            // First attempt of the first call dies before any content.
            vec![Step::Err(LlmError::Unavailable)],
            vec![pref],
            vec![Step::Delta("Fatto.")],
        ],
        SLOW,
        4,
        |llm: Arc<FakeLlm>| {
            Arc::new(RetryingLlm::new(
                llm,
                RetryPolicy::default(),
                Arc::new(NoWait),
            ))
        },
    )
    .await;
    let done = f.turn("ciao").await;

    assert_eq!(done.status, MessageStatus::Complete);
    assert_eq!(f.requests().len(), 3, "one retry, then the answer");
    assert_eq!(f.preferences().len(), 1);
    let saved = f
        .kinds()
        .iter()
        .filter(|k| k.starts_with("preference_saved"))
        .count();
    assert_eq!(saved, 1);
}

// ---------- preferences ----------

#[tokio::test]
async fn a_preference_is_saved_announced_and_not_duplicated() {
    let topic_pref =
        |text: &'static str| tool("save_preference", json!({"scope": "topic", "text": text}));
    let f = fixture(vec![
        vec![topic_pref("  Esempi in Python  ")],
        vec![Step::Delta("Ok, Python.")],
        vec![topic_pref("esempi in PYTHON")],
        vec![Step::Delta("Già noto.")],
    ])
    .await;
    f.turn("Usa Python").await;
    let stored = f.preferences();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].text.as_str(), "Esempi in Python");
    assert_eq!(stored[0].scope, PreferenceScope::Topic(f.topic));
    assert!(!f.results_of_request(1)[0].1);
    assert!(f.results_of_request(1)[0].0.contains("questo argomento"));
    // Nothing visible: the reply is plain text, no card.
    assert!(
        f.kinds()
            .contains(&"preference_saved:Esempi in Python".to_owned())
    );

    f.turn("Python, ti ho detto").await;
    assert_eq!(f.preferences().len(), 1, "same scope and text up to case");
    let r = f.results_of_request(3);
    assert!(!r[0].1, "a duplicate is not an error");
    assert!(r[0].0.contains("già salvata"), "{}", r[0].0);
    let saved = f
        .kinds()
        .iter()
        .filter(|k| k.starts_with("preference_saved"))
        .count();
    assert_eq!(saved, 1);
}

#[tokio::test]
async fn global_and_topic_preferences_with_the_same_text_are_distinct() {
    let f = fixture(vec![
        vec![
            tool(
                "save_preference",
                json!({"scope": "global", "text": "Risposte brevi"}),
            ),
            tool(
                "save_preference",
                json!({"scope": "topic", "text": "Risposte brevi"}),
            ),
        ],
        vec![Step::Delta("ok")],
    ])
    .await;
    f.turn("brevi").await;
    let stored = f.preferences();
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().any(|p| p.scope == PreferenceScope::Global));
}

#[tokio::test]
async fn an_invalid_preference_is_reported_to_the_model() {
    let f = fixture(vec![
        vec![tool(
            "save_preference",
            json!({"scope": "both", "text": " "}),
        )],
        vec![Step::Delta("ok")],
    ])
    .await;
    f.turn("x").await;
    let (text, is_error) = f.results_of_request(1)[0].clone();
    assert!(is_error);
    assert!(
        text.contains("`scope` deve essere uno tra: global, topic"),
        "{text}"
    );
    assert!(text.contains("`text` è vuoto"), "{text}");
    assert_eq!(f.preferences().len(), 0);
}

// ---------- context ----------

#[tokio::test]
async fn the_context_carries_mission_draft_and_preferences() {
    let f = fixture(vec![vec![Step::Delta("ok")]]).await;
    f.seed_confirmed("Per il lavoro").await;
    f.repo
        .insert_draft(f.topic, &content("Nuovo motivo"), f.clock_now(), None)
        .await
        .unwrap();
    for (n, (scope, text)) in [
        (ScopeKind::Global, "Risposte brevi"),
        (ScopeKind::Topic, "Esempi in Python"),
    ]
    .into_iter()
    .enumerate()
    {
        let p = Preference::new(
            PreferenceId::new(Uuid::from_u128(900 + n as u128)),
            domain::UserId::OWNER,
            scope,
            f.topic,
            PreferenceText::parse(text).unwrap(),
            datetime!(2026-11-08 09:00:00 UTC) + time::Duration::minutes(i64::try_from(n).unwrap()),
        );
        PreferenceRepository::insert(&*f.repo, &p).await.unwrap();
    }
    f.turn("ciao").await;
    let system = &f.requests()[0].system[1].text;
    assert!(
        system.contains("Mission confermata dalla persona (versione 1):"),
        "{system}"
    );
    assert!(system.contains("Perché: Per il lavoro"));
    assert!(system.contains(
        "Proposta di modifica in attesa di conferma (versione 2), la mission 1 resta valida"
    ));
    let globals = system
        .find("- [tutti gli argomenti] Risposte brevi")
        .unwrap();
    let topic = system
        .find("- [questo argomento] Esempi in Python")
        .unwrap();
    assert!(globals < topic, "oldest first");
    assert!(!system.contains("{{"));
}

// ---------- mission confirmation ----------

async fn with_draft(f: &Fixture, state: TopicState) -> Mission {
    let draft = f
        .repo
        .insert_draft(f.topic, &content("Per lavoro"), f.clock_now(), None)
        .await
        .unwrap();
    f.repo
        .0
        .lock()
        .unwrap()
        .topics
        .iter_mut()
        .find(|t| t.id == f.topic)
        .unwrap()
        .state = state;
    draft
}

#[tokio::test]
async fn confirming_the_first_mission_moves_the_topic_to_researching() {
    let f = fixture(vec![]).await;
    let svc = f.mission_service();
    with_draft(&f, TopicState::MissionProposed).await;

    let got = svc
        .confirm(domain::UserId::OWNER, f.topic, Some(1))
        .await
        .unwrap();
    let current = got.current.unwrap();
    assert_eq!(current.status, MissionStatus::Confirmed);
    assert!(current.confirmed_at.is_some());
    assert!(got.draft.is_none());
    assert_eq!(f.state().await, TopicState::Researching);
    assert_eq!(
        f.kinds(),
        [
            "mission_confirmed:1",
            "topic:researching",
            "job:research:queued"
        ],
        "no chat message is produced"
    );
}

#[tokio::test]
async fn a_draft_can_still_be_confirmed_after_the_user_wrote_again() {
    let f = fixture(vec![]).await;
    with_draft(&f, TopicState::Interviewing).await;
    f.mission_service()
        .confirm(domain::UserId::OWNER, f.topic, Some(1))
        .await
        .unwrap();
    assert_eq!(f.state().await, TopicState::Researching);
}

#[tokio::test]
async fn confirming_a_change_supersedes_the_old_version_and_keeps_the_state() {
    let f = fixture(vec![]).await;
    f.seed_confirmed("Prima").await;
    f.clock.set(datetime!(2026-11-09 10:00:00 UTC));
    f.repo
        .insert_draft(f.topic, &content("Dopo"), f.clock_now(), None)
        .await
        .unwrap();
    let svc = f.mission_service();
    let got = svc
        .confirm(domain::UserId::OWNER, f.topic, Some(2))
        .await
        .unwrap();
    assert_eq!(got.current.as_ref().unwrap().version, 2);
    assert_eq!(f.state().await, TopicState::Researching);
    assert_eq!(
        f.kinds(),
        ["mission_confirmed:2"],
        "no topic_updated and no new research"
    );
    assert_eq!(f.repo.0.lock().unwrap().jobs.len(), 0);

    let versions = svc
        .versions(
            domain::UserId::OWNER,
            f.topic,
            PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await
        .unwrap()
        .items;
    assert_eq!(
        versions
            .iter()
            .map(|m| (m.version, m.status))
            .collect::<Vec<_>>(),
        [
            (2, MissionStatus::Confirmed),
            (1, MissionStatus::Superseded)
        ]
    );
    assert_eq!(
        versions[1].superseded_at,
        Some(datetime!(2026-11-09 10:00:00 UTC))
    );
    assert!(versions[1].confirmed_at.is_some());
}

#[tokio::test]
async fn confirm_rejects_wrong_missing_and_repeated_versions() {
    let f = fixture(vec![]).await;
    let svc = f.mission_service();
    // No draft at all.
    assert!(matches!(
        svc.confirm(domain::UserId::OWNER, f.topic, Some(1)).await,
        Err(AppError::Conflict(ConflictReason::MissionNotOpen))
    ));
    with_draft(&f, TopicState::MissionProposed).await;
    // Wrong version.
    assert!(matches!(
        svc.confirm(domain::UserId::OWNER, f.topic, Some(7)).await,
        Err(AppError::Conflict(ConflictReason::MissionNotOpen))
    ));
    // No usable version.
    match svc.confirm(domain::UserId::OWNER, f.topic, None).await {
        Err(AppError::Validation(issues)) => {
            assert_eq!(
                (issues[0].field, issues[0].issue),
                ("version", IssueKind::Invalid)
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(f.state().await, TopicState::MissionProposed);
    assert_eq!(f.kinds().len(), 0);

    svc.confirm(domain::UserId::OWNER, f.topic, Some(1))
        .await
        .unwrap();
    // Twice.
    assert!(matches!(
        svc.confirm(domain::UserId::OWNER, f.topic, Some(1)).await,
        Err(AppError::Conflict(ConflictReason::MissionNotOpen))
    ));
    // A superseded draft cannot be confirmed either.
    let first = f
        .repo
        .insert_draft(f.topic, &content("A"), f.clock_now(), None)
        .await
        .unwrap();
    f.repo
        .insert_draft(f.topic, &content("B"), f.clock_now(), None)
        .await
        .unwrap();
    assert!(matches!(
        svc.confirm(domain::UserId::OWNER, f.topic, Some(first.version))
            .await,
        Err(AppError::Conflict(ConflictReason::MissionNotOpen))
    ));
    assert!(matches!(
        svc.confirm(
            domain::UserId::OWNER,
            TopicId::new(Uuid::from_u128(999)),
            Some(1)
        )
        .await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn confirming_in_a_state_that_cannot_hold_a_first_mission_conflicts() {
    let f = fixture(vec![]).await;
    with_draft(&f, TopicState::Ready).await;
    assert!(matches!(
        f.mission_service()
            .confirm(domain::UserId::OWNER, f.topic, Some(1))
            .await,
        Err(AppError::Conflict(ConflictReason::MissionStateMismatch))
    ));
    // Nothing was stored.
    assert!(f.repo.open_draft(f.topic).await.unwrap().is_some());
    assert!(f.repo.current(f.topic).await.unwrap().is_none());
}

#[tokio::test]
async fn the_mission_view_has_current_and_draft_and_404s_for_unknown_topics() {
    let f = fixture(vec![]).await;
    let svc = f.mission_service();
    let empty = svc.get(domain::UserId::OWNER, f.topic).await.unwrap();
    assert!(empty.current.is_none() && empty.draft.is_none());
    f.seed_confirmed("Prima").await;
    f.repo
        .insert_draft(f.topic, &content("Dopo"), f.clock_now(), None)
        .await
        .unwrap();
    let got = svc.get(domain::UserId::OWNER, f.topic).await.unwrap();
    assert_eq!(got.current.unwrap().version, 1);
    assert_eq!(got.draft.unwrap().version, 2);
    let ghost = TopicId::new(Uuid::from_u128(999));
    assert!(matches!(
        svc.get(domain::UserId::OWNER, ghost).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        svc.versions(
            domain::UserId::OWNER,
            ghost,
            PageRequest {
                after: None,
                limit: 5
            }
        )
        .await,
        Err(AppError::NotFound)
    ));
}

// ---------- preference service ----------

fn pref(n: u128, scope: PreferenceScope, text: &str) -> Preference {
    Preference {
        user_id: domain::UserId::OWNER,
        id: PreferenceId::new(Uuid::from_u128(n)),
        scope,
        text: PreferenceText::parse(text).unwrap(),
        created_at: datetime!(2026-11-08 09:00:00 UTC)
            + time::Duration::minutes(i64::try_from(n).unwrap()),
    }
}

#[tokio::test]
async fn preferences_list_all_or_global_plus_the_topic() {
    let f = fixture(vec![]).await;
    let other = {
        let folder = f
            .folders
            .create(domain::UserId::OWNER, Some("Altro"))
            .await
            .unwrap();
        f.topics
            .create(
                domain::UserId::OWNER,
                domain::FolderId::new(folder.folder.id.as_uuid()),
                Some("Go"),
            )
            .await
            .unwrap()
            .id
    };
    for p in [
        pref(1, PreferenceScope::Global, "G"),
        pref(2, PreferenceScope::Topic(f.topic), "T1"),
        pref(3, PreferenceScope::Topic(other), "T2"),
    ] {
        PreferenceRepository::insert(&*f.repo, &p).await.unwrap();
    }
    let svc = f.preference_service();
    let texts = |items: Vec<Preference>| -> Vec<String> {
        items.iter().map(|p| p.text.as_str().to_owned()).collect()
    };
    let all = svc
        .list(
            domain::UserId::OWNER,
            None,
            PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(texts(all.items), ["G", "T1", "T2"]);
    let mine = svc
        .list(
            domain::UserId::OWNER,
            Some(f.topic),
            PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(texts(mine.items), ["G", "T1"]);
    let unknown = svc
        .list(
            domain::UserId::OWNER,
            Some(TopicId::new(Uuid::from_u128(999))),
            PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(texts(unknown.items), ["G"]);

    let page = svc
        .list(
            domain::UserId::OWNER,
            None,
            PageRequest {
                after: None,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert!(page.next.is_some());
    let rest = svc
        .list(
            domain::UserId::OWNER,
            None,
            PageRequest {
                after: page.next,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(texts(rest.items), ["T2"]);
}

#[tokio::test]
async fn deleting_a_preference_announces_it_where_it_applies() {
    let f = fixture(vec![]).await;
    let svc = f.preference_service();
    PreferenceRepository::insert(&*f.repo, &pref(1, PreferenceScope::Global, "G"))
        .await
        .unwrap();
    PreferenceRepository::insert(&*f.repo, &pref(2, PreferenceScope::Topic(f.topic), "T"))
        .await
        .unwrap();

    svc.delete(domain::UserId::OWNER, PreferenceId::new(Uuid::from_u128(2)))
        .await
        .unwrap();
    let events = f.events.snapshot();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].topic_id, f.topic);
    assert!(matches!(
        events[0].payload,
        TopicEventPayload::PreferenceDeleted { scope: PreferenceScope::Topic(t), .. } if t == f.topic
    ));

    // A global one goes to every open stream, not to a single topic.
    svc.delete(domain::UserId::OWNER, PreferenceId::new(Uuid::from_u128(1)))
        .await
        .unwrap();
    assert_eq!(f.events.snapshot().len(), 1);
    {
        let broadcasts = f.events.broadcasts.lock().unwrap();
        assert_eq!(broadcasts.len(), 1);
        assert!(matches!(
            broadcasts[0],
            TopicEventPayload::PreferenceDeleted {
                scope: PreferenceScope::Global,
                ..
            }
        ));
    }

    assert!(matches!(
        svc.delete(domain::UserId::OWNER, PreferenceId::new(Uuid::from_u128(1)))
            .await,
        Err(AppError::NotFound)
    ));
    assert_eq!(f.preferences().len(), 0);
}

#[tokio::test]
async fn deleting_a_topic_removes_its_missions_and_preferences() {
    let f = fixture(vec![]).await;
    f.seed_confirmed("x").await;
    PreferenceRepository::insert(&*f.repo, &pref(1, PreferenceScope::Topic(f.topic), "T"))
        .await
        .unwrap();
    PreferenceRepository::insert(&*f.repo, &pref(2, PreferenceScope::Global, "G"))
        .await
        .unwrap();
    f.topics
        .delete(domain::UserId::OWNER, f.topic)
        .await
        .unwrap();
    assert_eq!(f.missions().len(), 0);
    assert_eq!(f.preferences().len(), 1);
}

#[tokio::test]
async fn whitespace_only_text_stays_in_the_message_untouched() {
    let f = fixture(vec![vec![Step::Delta("  \n"), save_draft("x")], vec![]]).await;
    let done = f.turn("ciao").await;
    assert_eq!(done.parts[0], MessagePart::Text("  \n".into()));
    assert!(card(&done).is_some());
    // The model gets it back as it wrote it; the adapter drops blank blocks.
    let echoed = &f.requests()[1].messages[f.requests()[1].messages.len() - 2];
    assert_eq!(echoed.parts[0], Part::Text("  \n".into()));
}

#[tokio::test]
async fn a_failing_state_transition_stores_neither_draft_nor_confirmation() {
    // Draft by the Tutor: the topic cannot move, so nothing is saved.
    let f = fixture(vec![vec![save_draft("x")], vec![Step::Delta("ok")]]).await;
    f.repo.0.lock().unwrap().fail_transitions = true;
    let done = f.turn("ciao").await;
    // The turn fails as a storage failure (no draft, no card).
    assert_eq!(done.status, MessageStatus::Failed);
    assert_eq!(f.missions().len(), 0);
    assert_ne!(f.state().await, TopicState::MissionProposed);

    // Confirmation by the user: same.
    let g = fixture(vec![]).await;
    with_draft(&g, TopicState::MissionProposed).await;
    g.repo.0.lock().unwrap().fail_transitions = true;
    assert!(matches!(
        g.mission_service()
            .confirm(domain::UserId::OWNER, g.topic, Some(1))
            .await,
        Err(AppError::Storage(_))
    ));
    assert_eq!(g.state().await, TopicState::MissionProposed);
    let view = g
        .mission_service()
        .get(domain::UserId::OWNER, g.topic)
        .await
        .unwrap();
    assert!(view.current.is_none() && view.draft.is_some());
    assert_eq!(g.kinds().len(), 0, "nothing was announced");
}

#[tokio::test]
async fn preferences_are_capped_per_scope() {
    let f = fixture(vec![
        vec![tool(
            "save_preference",
            json!({"scope": "topic", "text": "Una in più"}),
        )],
        vec![Step::Delta("ok")],
    ])
    .await;
    for n in 0..50u128 {
        PreferenceRepository::insert(
            &*f.repo,
            &pref(100 + n, PreferenceScope::Topic(f.topic), &format!("p{n}")),
        )
        .await
        .unwrap();
    }
    f.turn("ciao").await;
    let (text, is_error) = f.results_of_request(1)[0].clone();
    assert!(is_error);
    assert!(text.contains("limite di 50 preferenze"), "{text}");
    assert_eq!(f.preferences().len(), 50);
    // The global scope has its own budget.
    assert_eq!(
        PreferenceRepository::count(&*f.repo, domain::UserId::OWNER, PreferenceScope::Global)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn a_panic_mid_turn_still_announces_what_the_tools_did() {
    let f = fixture(vec![vec![save_draft("x")], vec![Step::Panic]]).await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    wait_for(|| f.kinds().iter().any(|k| k == "topic:mission_proposed")).await;
    let failed = f.stored(sent.assistant.id);
    assert_eq!(failed.status, MessageStatus::Failed);
    assert!(card(&failed).is_some(), "the card is in the failed message");
    let kinds = f.kinds();
    let at = |name: &str| kinds.iter().position(|k| k == name).unwrap();
    assert!(at("mission_proposed:1") < at("failed"));
    assert!(at("failed") < at("topic:mission_proposed"));
}

#[test]
fn only_known_tool_names_are_logged() {
    use crate::tutor_tools::loggable_tool_name;
    assert_eq!(loggable_tool_name("save_preference"), "save_preference");
    assert_eq!(loggable_tool_name("rm -rf \n secret"), "unknown");
}

#[tokio::test]
async fn a_research_already_active_is_not_reported_as_a_stale_draft() {
    let f = fixture(vec![]).await;
    with_draft(&f, TopicState::MissionProposed).await;
    f.enqueue(f.topic).await;
    assert!(matches!(
        f.mission_service()
            .confirm(domain::UserId::OWNER, f.topic, Some(1))
            .await,
        Err(AppError::Conflict(ConflictReason::JobActive))
    ));
    // Nothing was stored: the draft is still open and the topic did not move.
    assert_eq!(f.state().await, TopicState::MissionProposed);
    assert!(
        f.mission_service()
            .get(domain::UserId::OWNER, f.topic)
            .await
            .unwrap()
            .draft
            .is_some()
    );
}
