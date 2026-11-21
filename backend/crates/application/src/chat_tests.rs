use std::sync::Arc;
use std::time::Duration;

use domain::llm::{LlmError, LlmEvent, MessageRole as R, Part, StopReason, Usage};
use domain::{
    ChatErrorCode, ChatMessage, FolderId, IssueKind, MessageId, MessageStatus, PageRequest,
    TopicEventPayload, TopicId, TopicRepository, TopicState,
};
use time::macros::datetime;
use tokio::sync::Notify;

use crate::error::{AppError, ConflictReason};
use crate::fakes::{FakeLlm, MemRepo, RecordingEvents, SeqIds, Step, TestClock};
use crate::{
    ChatService, ChatSettings, FolderService, JobService, TopicService, TutorPrompts, UpdateTopic,
};

pub(crate) struct Fixture {
    pub(crate) chat: ChatService,
    pub(crate) topics: TopicService,
    pub(crate) folders: FolderService,
    pub(crate) repo: Arc<MemRepo>,
    pub(crate) events: Arc<RecordingEvents>,
    pub(crate) llm: Arc<FakeLlm>,
    pub(crate) clock: Arc<TestClock>,
    pub(crate) jobs: Arc<JobService>,
    pub(crate) topic: TopicId,
}

async fn fixture(scripts: Vec<Vec<Step>>, flush: Duration) -> Fixture {
    fixture_with(scripts, flush, 4, |llm| llm).await
}

/// `wrap` decorates the fake model (e.g. with the retry policy).
pub(crate) async fn fixture_with(
    scripts: Vec<Vec<Step>>,
    flush: Duration,
    max_tool_iterations: usize,
    wrap: impl FnOnce(Arc<FakeLlm>) -> Arc<dyn domain::llm::LlmProvider>,
) -> Fixture {
    let repo = Arc::new(MemRepo::default());
    let clock = Arc::new(TestClock::new(datetime!(2026-11-08 10:00:00 UTC)));
    let ids = Arc::new(SeqIds::default());
    let events = Arc::new(RecordingEvents::default());
    let llm = FakeLlm::with(scripts);
    let folders = FolderService::new(
        repo.clone(),
        repo.clone(),
        events.clone(),
        clock.clone(),
        ids.clone(),
    );
    let topics = TopicService::new(
        repo.clone(),
        repo.clone(),
        clock.clone(),
        ids.clone(),
        events.clone(),
    );
    let folder = folders
        .create(domain::UserId::OWNER, Some("Lavoro"))
        .await
        .unwrap();
    let topic = topics
        .create(
            domain::UserId::OWNER,
            FolderId::new(folder.folder.id.as_uuid()),
            Some("Rust"),
        )
        .await
        .unwrap();
    let jobs = Arc::new(JobService::new(
        repo.clone(),
        repo.clone(),
        events.clone(),
        clock.clone(),
        ids.clone(),
        crate::fakes::owner_keys(),
    ));
    let lessons = Arc::new(crate::LessonService::new(
        repo.clone(),
        repo.clone(),
        repo.clone(),
        repo.clone(),
        repo.clone(),
        repo.clone(),
        jobs.clone(),
        crate::fakes::owner_keys(),
        clock.clone(),
    ));
    let chat = ChatService::new(
        repo.clone(),
        repo.clone(),
        repo.clone(),
        repo.clone(),
        events.clone(),
        wrap(llm.clone()),
        TutorPrompts::embedded(),
        clock.clone(),
        ids.clone(),
        crate::fakes::owner_keys(),
        ChatSettings {
            flush_interval: flush,
            history_token_budget: 8000,
            max_tool_iterations,
        },
    )
    .with_lesson_requests(lessons);
    Fixture {
        chat,
        topics,
        folders,
        repo,
        events,
        llm,
        clock,
        jobs,
        topic: topic.id,
    }
}

pub(crate) const SLOW: Duration = Duration::from_secs(3600);

pub(crate) async fn wait_for(mut cond: impl FnMut() -> bool) {
    for _ in 0..400 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("condition not reached in time");
}

impl Fixture {
    pub(crate) fn stored(&self, id: MessageId) -> ChatMessage {
        self.repo
            .0
            .lock()
            .unwrap()
            .messages
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .expect("message stored")
    }

    pub(crate) async fn wait_status(&self, id: MessageId, status: MessageStatus) -> ChatMessage {
        wait_for(|| {
            self.repo
                .0
                .lock()
                .unwrap()
                .messages
                .iter()
                .any(|m| m.id == id && m.status == status)
        })
        .await;
        self.stored(id)
    }

    /// Event kinds in order, deltas shown with their text.
    pub(crate) fn kinds(&self) -> Vec<String> {
        self.events
            .snapshot()
            .iter()
            .map(|e| match &e.payload {
                TopicEventPayload::MessageCreated(m) => format!("created:{}", m.role),
                TopicEventPayload::MessageDelta { text, .. } => format!("delta:{text}"),
                TopicEventPayload::MessageCompleted(_) => "completed".into(),
                TopicEventPayload::MessageFailed(_) => "failed".into(),
                TopicEventPayload::TopicUpdated(t) => format!("topic:{}", t.state),
                TopicEventPayload::MissionProposed(m) => format!("mission_proposed:{}", m.version),
                TopicEventPayload::MissionConfirmed(m) => {
                    format!("mission_confirmed:{}", m.version)
                }
                TopicEventPayload::PreferenceSaved(p) => {
                    format!("preference_saved:{}", p.text.as_str())
                }
                TopicEventPayload::PreferenceDeleted { .. } => "preference_deleted".into(),
                TopicEventPayload::JobUpdated(j) => format!("job:{}:{}", j.kind, j.status.as_str()),
            })
            .collect()
    }

    pub(crate) fn page() -> PageRequest<domain::ChatCursor> {
        PageRequest {
            after: None,
            limit: 50,
        }
    }
}

#[tokio::test]
async fn send_streams_the_reply_and_completes() {
    let f = fixture(vec![vec![Step::Delta("Ciao "), Step::Delta("mondo")]], SLOW).await;
    let sent = f
        .chat
        .send(
            domain::UserId::OWNER,
            f.topic,
            Some("  Voglio imparare Rust "),
        )
        .await
        .unwrap();
    assert_eq!(sent.user.text(), "Voglio imparare Rust");
    assert_eq!(sent.user.status, MessageStatus::Complete);
    assert_eq!(sent.assistant.status, MessageStatus::Streaming);
    assert_eq!(sent.assistant.parts.len(), 0);
    assert!(sent.user.id < sent.assistant.id);

    let done = f
        .wait_status(sent.assistant.id, MessageStatus::Complete)
        .await;
    assert_eq!(done.text(), "Ciao mondo");
    assert_eq!(
        f.kinds(),
        [
            "created:user",
            "created:assistant",
            "topic:interviewing",
            "delta:Ciao ",
            "delta:mondo",
            "completed"
        ]
    );
    // The completed event carries the final message.
    let last = f.events.snapshot().pop().unwrap();
    assert!(matches!(
        last.payload,
        TopicEventPayload::MessageCompleted(m) if m.parts == done.parts && m.status == done.status
    ));
}

#[tokio::test]
async fn first_message_moves_topic_to_interviewing_once() {
    let f = fixture(vec![vec![Step::Delta("a")], vec![Step::Delta("b")]], SLOW).await;
    assert_eq!(
        f.topics
            .get(domain::UserId::OWNER, f.topic)
            .await
            .unwrap()
            .state,
        TopicState::New
    );
    let first = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("uno"))
        .await
        .unwrap();
    f.wait_status(first.assistant.id, MessageStatus::Complete)
        .await;
    let topic = f.topics.get(domain::UserId::OWNER, f.topic).await.unwrap();
    assert_eq!(topic.state, TopicState::Interviewing);
    assert_eq!(topic.updated_at, datetime!(2026-11-08 10:00:00 UTC));

    let second = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("due"))
        .await
        .unwrap();
    f.wait_status(second.assistant.id, MessageStatus::Complete)
        .await;
    let updates = f.kinds().iter().filter(|k| k.starts_with("topic:")).count();
    assert_eq!(updates, 1);
}

#[tokio::test]
async fn failure_midway_keeps_partial_text() {
    let f = fixture(
        vec![vec![
            Step::Delta("Perché "),
            Step::Fail(ChatErrorCode::UpstreamFailed),
        ]],
        SLOW,
    )
    .await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    let failed = f
        .wait_status(sent.assistant.id, MessageStatus::Failed)
        .await;
    assert_eq!(failed.text(), "Perché ");
    let error = failed.error.unwrap();
    assert_eq!(error.code, ChatErrorCode::UpstreamFailed);
    assert_ne!(error.message, "");
    assert_eq!(f.kinds().last().unwrap(), "failed");
    assert!(!f.kinds().contains(&"completed".to_owned()));
}

#[tokio::test]
async fn failure_before_the_first_delta_and_empty_replies_fail() {
    let f = fixture(vec![], SLOW).await;
    f.llm
        .start_errors
        .lock()
        .unwrap()
        .push_back(LlmError::RateLimited { retry_after: None });
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    let failed = f
        .wait_status(sent.assistant.id, MessageStatus::Failed)
        .await;
    assert_eq!(failed.parts.len(), 0);
    assert_eq!(failed.error.unwrap().code, ChatErrorCode::RateLimited);

    // A stream that ends without text is an upstream fault.
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ancora"))
        .await
        .unwrap();
    let failed = f
        .wait_status(sent.assistant.id, MessageStatus::Failed)
        .await;
    assert_eq!(failed.error.unwrap().code, ChatErrorCode::UpstreamFailed);
}

#[tokio::test]
async fn failures_are_written_in_the_language_of_the_topic() {
    let f = fixture(vec![], SLOW).await;
    f.repo.0.lock().unwrap().topics[0].language = domain::ContentLanguage::En;
    f.llm
        .start_errors
        .lock()
        .unwrap()
        .push_back(LlmError::RateLimited { retry_after: None });
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("hello"))
        .await
        .unwrap();
    let failed = f
        .wait_status(sent.assistant.id, MessageStatus::Failed)
        .await;
    assert_eq!(
        failed.error.unwrap().message,
        "Too many requests to the model, try again shortly."
    );
}

#[tokio::test]
async fn second_send_while_streaming_conflicts() {
    let gate = Arc::new(Notify::new());
    let f = fixture(
        vec![
            vec![Step::Delta("a"), Step::Wait(gate.clone()), Step::Delta("b")],
            vec![Step::Delta("c")],
        ],
        SLOW,
    )
    .await;
    let first = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("uno"))
        .await
        .unwrap();
    let err = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("due"))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        AppError::Conflict(ConflictReason::TurnInProgress)
    ));
    // Nothing was stored by the refused attempt.
    assert_eq!(f.repo.0.lock().unwrap().messages.len(), 2);

    gate.notify_one();
    f.wait_status(first.assistant.id, MessageStatus::Complete)
        .await;
    let second = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("due"))
        .await
        .unwrap();
    f.wait_status(second.assistant.id, MessageStatus::Complete)
        .await;
}

#[tokio::test]
async fn send_validates_text_after_checking_the_topic() {
    let f = fixture(vec![], SLOW).await;
    for (text, kind) in [(None, IssueKind::Empty), (Some("   "), IssueKind::Empty)] {
        match f
            .chat
            .send(domain::UserId::OWNER, f.topic, text)
            .await
            .unwrap_err()
        {
            AppError::Validation(v) => assert_eq!((v[0].field, v[0].issue), ("text", kind)),
            other => panic!("{other:?}"),
        }
    }
    let long = "a".repeat(8001);
    assert!(matches!(
        f.chat
            .send(domain::UserId::OWNER, f.topic, Some(&long))
            .await
            .unwrap_err(),
        AppError::Validation(_)
    ));
    let missing = TopicId::new(uuid::Uuid::from_u128(999));
    assert!(matches!(
        f.chat
            .send(domain::UserId::OWNER, missing, Some(""))
            .await
            .unwrap_err(),
        AppError::NotFound
    ));
    assert_eq!(f.repo.0.lock().unwrap().messages.len(), 0);
}

#[tokio::test]
async fn retry_regenerates_the_same_message_with_the_same_context() {
    let f = fixture(
        vec![
            vec![Step::Delta("parz"), Step::Fail(ChatErrorCode::Internal)],
            vec![Step::Delta("ok")],
        ],
        SLOW,
    )
    .await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    f.wait_status(sent.assistant.id, MessageStatus::Failed)
        .await;

    let restarted = f
        .chat
        .retry(domain::UserId::OWNER, f.topic, sent.assistant.id)
        .await
        .unwrap();
    assert_eq!(restarted.id, sent.assistant.id);
    assert_eq!(restarted.status, MessageStatus::Streaming);
    assert!(restarted.parts.is_empty() && restarted.error.is_none());
    let done = f
        .wait_status(sent.assistant.id, MessageStatus::Complete)
        .await;
    assert_eq!(done.text(), "ok");
    assert!(done.error.is_none());

    // Both attempts saw the same single user message; the partial text of
    // the failed attempt was not fed back.
    let requests = f.llm.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].messages, requests[1].messages);
    assert_eq!(requests[1].messages.len(), 1);
    assert_eq!(requests[1].messages[0].role, domain::llm::MessageRole::User);
    assert!(requests[1].system[1].text.contains("Rust"));
    // Clients learn about the restart through `message_created`.
    assert_eq!(
        f.kinds()
            .iter()
            .filter(|k| *k == "created:assistant")
            .count(),
        2
    );
}

#[tokio::test]
async fn retry_is_refused_unless_last_failed_assistant() {
    let f = fixture(
        vec![
            vec![Step::Fail(ChatErrorCode::Internal)],
            vec![Step::Delta("ok")],
        ],
        SLOW,
    )
    .await;
    let first = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("uno"))
        .await
        .unwrap();
    f.wait_status(first.assistant.id, MessageStatus::Failed)
        .await;
    let second = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("due"))
        .await
        .unwrap();
    let done = f
        .wait_status(second.assistant.id, MessageStatus::Complete)
        .await;

    for id in [
        first.assistant.id, // failed but not the last
        done.id,            // last but complete
        first.user.id,      // a user message
    ] {
        assert!(matches!(
            f.chat
                .retry(domain::UserId::OWNER, f.topic, id)
                .await
                .unwrap_err(),
            AppError::Conflict(ConflictReason::NotRetryable)
        ));
    }
    assert!(matches!(
        f.chat
            .retry(
                domain::UserId::OWNER,
                f.topic,
                MessageId::new(uuid::Uuid::from_u128(999))
            )
            .await
            .unwrap_err(),
        AppError::NotFound
    ));
    let other = TopicId::new(uuid::Uuid::from_u128(999));
    assert!(matches!(
        f.chat
            .retry(domain::UserId::OWNER, other, done.id)
            .await
            .unwrap_err(),
        AppError::NotFound
    ));
}

#[tokio::test]
async fn retry_while_streaming_is_a_conflict() {
    let gate = Arc::new(Notify::new());
    let f = fixture(vec![vec![Step::Wait(gate.clone()), Step::Delta("x")]], SLOW).await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("uno"))
        .await
        .unwrap();
    assert!(matches!(
        f.chat
            .retry(domain::UserId::OWNER, f.topic, sent.assistant.id)
            .await
            .unwrap_err(),
        AppError::Conflict(ConflictReason::NotRetryable)
    ));
    gate.notify_one();
    f.wait_status(sent.assistant.id, MessageStatus::Complete)
        .await;
}

#[tokio::test]
async fn recover_turns_streaming_messages_into_retryable_failures() {
    let gate = Arc::new(Notify::new());
    let f = fixture(
        vec![
            vec![Step::Delta("meta"), Step::Wait(gate.clone())],
            vec![Step::Delta("ok")],
        ],
        Duration::ZERO,
    )
    .await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("uno"))
        .await
        .unwrap();
    wait_for(|| f.stored(sent.assistant.id).text() == "meta").await;

    // Simulate a restart: a fresh service over the same storage.
    let restarted = ChatService::new(
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.repo.clone(),
        f.events.clone(),
        f.llm.clone(),
        TutorPrompts::embedded(),
        Arc::new(TestClock::new(datetime!(2026-11-08 11:00:00 UTC))),
        Arc::new(SeqIds::default()),
        crate::fakes::owner_keys(),
        ChatSettings::default(),
    );
    assert_eq!(restarted.recover_interrupted().await.unwrap(), 1);
    let failed = f.stored(sent.assistant.id);
    assert_eq!(failed.status, MessageStatus::Failed);
    assert_eq!(failed.text(), "meta");
    assert_eq!(failed.error.unwrap().code, ChatErrorCode::Internal);
    assert_eq!(restarted.recover_interrupted().await.unwrap(), 0);

    let again = restarted
        .retry(domain::UserId::OWNER, f.topic, sent.assistant.id)
        .await
        .unwrap();
    assert_eq!(again.status, MessageStatus::Streaming);
    // The old worker is still parked on the gate in this simulation; let it go.
    gate.notify_one();
}

#[tokio::test]
async fn partial_text_is_saved_at_the_flush_interval_not_per_delta() {
    for (interval, expect_saved) in [(Duration::ZERO, "a"), (SLOW, "")] {
        let gate = Arc::new(Notify::new());
        let f = fixture(
            vec![vec![
                Step::Delta("a"),
                Step::Wait(gate.clone()),
                Step::Delta("b"),
            ]],
            interval,
        )
        .await;
        let sent = f
            .chat
            .send(domain::UserId::OWNER, f.topic, Some("uno"))
            .await
            .unwrap();
        // The delta event is out as soon as the text arrives...
        wait_for(|| f.kinds().contains(&"delta:a".to_owned())).await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        // ...while what is persisted depends on the interval.
        assert_eq!(f.stored(sent.assistant.id).text(), expect_saved);
        gate.notify_one();
        let done = f
            .wait_status(sent.assistant.id, MessageStatus::Complete)
            .await;
        assert_eq!(done.text(), "ab");
    }
}

#[tokio::test]
async fn deleting_the_topic_stops_the_generation_quietly() {
    let gate = Arc::new(Notify::new());
    let f = fixture(
        vec![vec![
            Step::Delta("a"),
            Step::Wait(gate.clone()),
            Step::Delta("b"),
        ]],
        Duration::ZERO,
    )
    .await;
    f.chat
        .send(domain::UserId::OWNER, f.topic, Some("uno"))
        .await
        .unwrap();
    wait_for(|| f.kinds().contains(&"delta:a".to_owned())).await;
    assert!(
        TopicRepository::delete(f.repo.as_ref(), domain::UserId::OWNER, f.topic)
            .await
            .unwrap()
    );
    gate.notify_one();

    wait_for(|| f.events.closed.lock().unwrap().contains(&f.topic)).await;
    assert_eq!(f.repo.0.lock().unwrap().messages.len(), 0);
    assert!(!f.kinds().contains(&"completed".to_owned()));
    assert!(!f.kinds().contains(&"failed".to_owned()));
}

#[tokio::test]
async fn a_panicking_generator_still_ends_the_message() {
    let f = fixture(vec![vec![Step::Delta("a"), Step::Panic]], Duration::ZERO).await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("uno"))
        .await
        .unwrap();
    let failed = f
        .wait_status(sent.assistant.id, MessageStatus::Failed)
        .await;
    assert_eq!(failed.error.unwrap().code, ChatErrorCode::Internal);
}

#[tokio::test]
async fn list_is_newest_first_and_paginates() {
    let f = fixture(vec![vec![Step::Delta("a")], vec![Step::Delta("b")]], SLOW).await;
    for text in ["uno", "due"] {
        let sent = f
            .chat
            .send(domain::UserId::OWNER, f.topic, Some(text))
            .await
            .unwrap();
        f.wait_status(sent.assistant.id, MessageStatus::Complete)
            .await;
    }
    let all = f
        .chat
        .list(domain::UserId::OWNER, f.topic, Fixture::page())
        .await
        .unwrap();
    let texts: Vec<_> = all.page.items.iter().map(ChatMessage::text).collect();
    assert_eq!(texts, ["b", "due", "a", "uno"]);
    assert!(all.page.next.is_none());

    let first = f
        .chat
        .list(
            domain::UserId::OWNER,
            f.topic,
            PageRequest {
                after: None,
                limit: 3,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.page.items.len(), 3);
    let rest = f
        .chat
        .list(
            domain::UserId::OWNER,
            f.topic,
            PageRequest {
                after: first.page.next,
                limit: 3,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        rest.page
            .items
            .iter()
            .map(ChatMessage::text)
            .collect::<Vec<_>>(),
        ["uno"]
    );

    let missing = TopicId::new(uuid::Uuid::from_u128(999));
    assert!(matches!(
        f.chat
            .list(domain::UserId::OWNER, missing, Fixture::page())
            .await
            .unwrap_err(),
        AppError::NotFound
    ));
}

#[tokio::test]
async fn context_has_history_oldest_first_without_the_message_in_flight() {
    let f = fixture(vec![vec![Step::Delta("r1")], vec![Step::Delta("r2")]], SLOW).await;
    let a = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("uno"))
        .await
        .unwrap();
    f.wait_status(a.assistant.id, MessageStatus::Complete).await;
    let b = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("due"))
        .await
        .unwrap();
    f.wait_status(b.assistant.id, MessageStatus::Complete).await;
    let request = f.llm.requests.lock().unwrap()[1].clone();
    let seen: Vec<_> = request
        .messages
        .iter()
        .map(|m| (m.role, m.parts[0].clone()))
        .collect();
    assert_eq!(
        seen,
        [
            (R::User, Part::Text("uno".into())),
            (R::Assistant, Part::Text("r1".into())),
            (R::User, Part::Text("due".into())),
        ]
    );
}

#[tokio::test]
async fn patching_a_topic_publishes_topic_updated_only_on_change() {
    let f = fixture(vec![], SLOW).await;
    f.topics
        .update(
            domain::UserId::OWNER,
            f.topic,
            UpdateTopic {
                title: Some("Rust avanzato".into()),
                folder_id: None,
            },
        )
        .await
        .unwrap();
    f.topics
        .update(
            domain::UserId::OWNER,
            f.topic,
            UpdateTopic {
                title: Some("Rust avanzato".into()),
                folder_id: None,
            },
        )
        .await
        .unwrap();
    let events = f.events.snapshot();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0].payload,
        TopicEventPayload::TopicUpdated(t) if t.title.as_str() == "Rust avanzato"
    ));
}

fn finished(reason: StopReason) -> Step {
    Step::Event(LlmEvent::Finished {
        stop_reason: reason,
        usage: Usage::default(),
    })
}

#[tokio::test]
async fn a_refusal_fails_the_message_with_its_own_text() {
    let f = fixture(
        vec![vec![Step::Delta("no"), finished(StopReason::Refusal)]],
        SLOW,
    )
    .await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    let failed = f
        .wait_status(sent.assistant.id, MessageStatus::Failed)
        .await;
    let error = failed.error.unwrap();
    assert_eq!(error.code, ChatErrorCode::UpstreamFailed);
    assert!(error.message.contains("rifiutato"));
}

#[tokio::test]
async fn hitting_the_token_limit_still_completes() {
    let f = fixture(
        vec![vec![
            Step::Delta("tagliato"),
            finished(StopReason::MaxTokens),
        ]],
        SLOW,
    )
    .await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    let done = f
        .wait_status(sent.assistant.id, MessageStatus::Complete)
        .await;
    assert_eq!(done.text(), "tagliato");
}

#[tokio::test]
async fn provider_errors_map_to_the_contract_codes() {
    for (error, code) in [
        (LlmError::Overloaded, ChatErrorCode::UpstreamFailed),
        (LlmError::Authentication, ChatErrorCode::LlmKeyRejected),
        (LlmError::CreditExhausted, ChatErrorCode::LlmCreditExhausted),
        (
            LlmError::MalformedResponse("x".into()),
            ChatErrorCode::UpstreamFailed,
        ),
        (LlmError::Timeout, ChatErrorCode::UpstreamFailed),
        (
            LlmError::RateLimited { retry_after: None },
            ChatErrorCode::RateLimited,
        ),
        (
            LlmError::InvalidRequest("secret provider text".into()),
            ChatErrorCode::Internal,
        ),
        (LlmError::Unsupported("x".into()), ChatErrorCode::Internal),
    ] {
        let f = fixture(vec![vec![Step::Delta("a"), Step::Err(error.clone())]], SLOW).await;
        let sent = f
            .chat
            .send(domain::UserId::OWNER, f.topic, Some("ciao"))
            .await
            .unwrap();
        let failed = f
            .wait_status(sent.assistant.id, MessageStatus::Failed)
            .await;
        let e = failed.error.unwrap();
        assert_eq!(e.code, code, "{error:?}");
        assert!(!e.message.contains("secret"));
    }
}

#[tokio::test]
async fn context_too_long_is_retried_once_with_half_the_history() {
    let f = fixture(vec![vec![Step::Delta("ok")]], SLOW).await;
    f.llm
        .start_errors
        .lock()
        .unwrap()
        .push_back(LlmError::ContextTooLong);
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some(&"a".repeat(3000)))
        .await
        .unwrap();
    let done = f
        .wait_status(sent.assistant.id, MessageStatus::Complete)
        .await;
    assert_eq!(done.text(), "ok");
    assert_eq!(f.llm.requests.lock().unwrap().len(), 2);

    // A second failure is final (`internal`).
    let f = fixture(vec![], SLOW).await;
    f.llm
        .start_errors
        .lock()
        .unwrap()
        .extend([LlmError::ContextTooLong, LlmError::ContextTooLong]);
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    let failed = f
        .wait_status(sent.assistant.id, MessageStatus::Failed)
        .await;
    assert_eq!(failed.error.unwrap().code, ChatErrorCode::Internal);
}

#[tokio::test]
async fn stream_without_a_stop_reason_fails() {
    let f = fixture(vec![vec![Step::Delta("a"), Step::NoFinish]], SLOW).await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    let failed = f
        .wait_status(sent.assistant.id, MessageStatus::Failed)
        .await;
    assert_eq!(failed.text(), "a");
    assert_eq!(failed.error.unwrap().code, ChatErrorCode::UpstreamFailed);
}

#[tokio::test]
async fn a_failing_state_transition_does_not_strand_the_reply() {
    let f = fixture(vec![vec![Step::Delta("ok")]], SLOW).await;
    f.repo.0.lock().unwrap().fail_transitions = true;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    let done = f
        .wait_status(sent.assistant.id, MessageStatus::Complete)
        .await;
    assert_eq!(done.text(), "ok");
    // The topic simply stays `new`, and nothing claimed otherwise.
    assert_eq!(
        f.topics
            .get(domain::UserId::OWNER, f.topic)
            .await
            .unwrap()
            .state,
        TopicState::New
    );
    assert!(!f.kinds().iter().any(|k| k.starts_with("topic:")));
    // And the next message is not blocked by a ghost `streaming` row.
    f.llm.start_errors.lock().unwrap().clear();
    assert!(
        f.chat
            .send(domain::UserId::OWNER, f.topic, Some("ancora"))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn topic_updated_after_the_transition_carries_the_stored_topic() {
    let f = fixture(vec![vec![Step::Delta("a")]], SLOW).await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    f.wait_status(sent.assistant.id, MessageStatus::Complete)
        .await;
    let stored = f.topics.get(domain::UserId::OWNER, f.topic).await.unwrap();
    let published = f
        .events
        .snapshot()
        .into_iter()
        .find_map(|e| match e.payload {
            TopicEventPayload::TopicUpdated(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(published, stored);
}

#[tokio::test]
async fn deleting_a_topic_or_a_folder_closes_its_streams() {
    let f = fixture(vec![], SLOW).await;
    f.topics
        .delete(domain::UserId::OWNER, f.topic)
        .await
        .unwrap();
    assert_eq!(*f.events.closed.lock().unwrap(), [f.topic]);

    // A folder takes all of its topics with it.
    let f = fixture(vec![], SLOW).await;
    let folder = f
        .topics
        .get(domain::UserId::OWNER, f.topic)
        .await
        .unwrap()
        .folder_id;
    let second = f
        .topics
        .create(domain::UserId::OWNER, folder, Some("Go"))
        .await
        .unwrap();
    f.folders
        .delete(domain::UserId::OWNER, folder)
        .await
        .unwrap();
    let mut closed = f.events.closed.lock().unwrap().clone();
    closed.sort();
    let mut expected = vec![f.topic, second.id];
    expected.sort();
    assert_eq!(closed, expected);
}

#[tokio::test]
async fn messages_record_the_last_event_they_reflect() {
    let gate = Arc::new(Notify::new());
    let f = fixture(
        vec![vec![
            Step::Delta("a"),
            Step::Delta("b"),
            Step::Wait(gate.clone()),
            Step::Delta("c"),
        ]],
        SLOW, // nothing is flushed mid-stream
    )
    .await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    wait_for(|| f.kinds().contains(&"delta:b".to_owned())).await;

    // Events so far: 1 user created, 2 assistant created, 3 topic, 4-5 deltas.
    let user = f.stored(sent.user.id);
    let mid = f.stored(sent.assistant.id);
    assert_eq!(user.last_event_id, Some(1));
    assert_eq!(sent.assistant.last_event_id, Some(2));
    // Delayed flush: the stored text lags, and says exactly how far.
    assert_eq!((mid.text().as_str(), mid.last_event_id), ("", Some(2)));
    let listed = f
        .chat
        .list(domain::UserId::OWNER, f.topic, Fixture::page())
        .await
        .unwrap();
    assert_eq!(listed.last_event_id, 5);

    gate.notify_one();
    let done = f
        .wait_status(sent.assistant.id, MessageStatus::Complete)
        .await;
    // 6 = delta c, 7 = message_completed; the second write may land after.
    wait_for(|| f.stored(sent.assistant.id).last_event_id == Some(7)).await;
    assert_eq!(done.text(), "abc");
}

#[tokio::test]
async fn flushing_stores_the_id_of_the_last_delta_in_the_saved_text() {
    let gate = Arc::new(Notify::new());
    let f = fixture(
        vec![vec![Step::Delta("a"), Step::Delta("b"), Step::Wait(gate)]],
        Duration::ZERO,
    )
    .await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    wait_for(|| f.stored(sent.assistant.id).text() == "ab").await;
    assert_eq!(f.stored(sent.assistant.id).last_event_id, Some(5));
}

#[tokio::test]
async fn failure_and_retry_keep_the_event_id_current() {
    let f = fixture(
        vec![
            vec![Step::Fail(ChatErrorCode::Internal)],
            vec![Step::Delta("ok")],
        ],
        SLOW,
    )
    .await;
    let sent = f
        .chat
        .send(domain::UserId::OWNER, f.topic, Some("ciao"))
        .await
        .unwrap();
    f.wait_status(sent.assistant.id, MessageStatus::Failed)
        .await;
    wait_for(|| f.stored(sent.assistant.id).last_event_id == Some(4)).await; // message_failed
    let restarted = f
        .chat
        .retry(domain::UserId::OWNER, f.topic, sent.assistant.id)
        .await
        .unwrap();
    assert_eq!(restarted.last_event_id, Some(5)); // the new message_created
}
