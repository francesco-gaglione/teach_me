//! Chat use cases: send, retry, list, and the background generation of replies.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use domain::llm::{
    LlmError, LlmEvent, LlmEventStream, LlmMessage, LlmProvider, LlmRole, MessageRole as LlmRoleOf,
    Part, StopReason, ToolDef, Usage,
};
use domain::{
    ChatCursor, ChatErrorCode, ChatMessage, ChatRepository, ChatRuleError, Clock, ContentLanguage,
    DocumentRepository, IdGenerator, LessonId, LessonRepository, MessageError, MessageId,
    MessageText, Mission, MissionRepository, Page, PageRequest, Preference, PreferenceRepository,
    RepositoryError, Secret, Topic, TopicEvent, TopicEventPayload, TopicEventPublisher, TopicId,
    TopicRepository, UserId, has_active_turn,
};
use time::OffsetDateTime;
use tokio::time::Instant;
use tracing::Instrument as _;

use crate::context::{
    AssembleError, HISTORY_FETCH, HistoryLimits, LessonNote, PREFERENCES_SHOWN, TutorContext,
    assemble_tutor_request,
};
use crate::error::{AppError, ConflictReason};
use crate::lessons::LessonService;
use crate::llm_keys::{LlmKeys, with_key};
use crate::paging::paginate;
use crate::prompts::TutorPrompts;
use crate::time_util::now_millis;
use crate::tutor_tools::{
    Proposal, ToolAbort, ToolCall, ToolKit, TurnState, TutorTools, loggable_tool_name,
};

#[derive(Debug, Clone, Copy)]
pub struct ChatSettings {
    /// Minimum time between two writes of the partial text while streaming.
    /// Deltas are broadcast immediately; only persistence is throttled.
    pub flush_interval: Duration,
    /// Estimated tokens of chat history sent to the Tutor.
    pub history_token_budget: usize,
    /// How many times a turn may run tools and call the model again. Past it
    /// the message fails with `internal`: a model that keeps asking for tools
    /// is looping, not working. The model is called at most this + 1 times.
    pub max_tool_iterations: usize,
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            flush_interval: Duration::from_millis(750),
            history_token_budget: 8000,
            max_tool_iterations: 4,
        }
    }
}

/// A page of messages plus the position of the event stream it corresponds to.
#[derive(Debug)]
pub struct MessagePage {
    pub page: Page<ChatMessage, ChatCursor>,
    /// Last event id emitted for the topic before the page was read (the run's
    /// base if none).
    pub last_event_id: u64,
}

/// Result of [`ChatService::send`].
#[derive(Debug, Clone)]
pub struct SentTurn {
    pub user: ChatMessage,
    pub assistant: ChatMessage,
}

pub struct ChatService {
    inner: Arc<Inner>,
}

struct Inner {
    topics: Arc<dyn TopicRepository>,
    messages: Arc<dyn ChatRepository>,
    missions: Arc<dyn MissionRepository>,
    preferences: Arc<dyn PreferenceRepository>,
    events: Arc<dyn TopicEventPublisher>,
    llm: Arc<dyn LlmProvider>,
    prompts: TutorPrompts,
    tools: TutorTools,
    clock: Arc<dyn Clock>,
    settings: ChatSettings,
    ids: Arc<dyn IdGenerator>,
    keys: Arc<LlmKeys>,
    /// Needed only to check the `lesson_id` of a message.
    lessons: Option<Arc<dyn LessonRepository>>,
    documents: Option<Arc<dyn DocumentRepository>>,
}

impl ChatService {
    // One argument per injected port: a parameter struct would only rename them.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        topics: Arc<dyn TopicRepository>,
        messages: Arc<dyn ChatRepository>,
        missions: Arc<dyn MissionRepository>,
        preferences: Arc<dyn PreferenceRepository>,
        events: Arc<dyn TopicEventPublisher>,
        llm: Arc<dyn LlmProvider>,
        prompts: TutorPrompts,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        keys: Arc<LlmKeys>,
        settings: ChatSettings,
    ) -> Self {
        // The prompt files are embedded and checked by a test (`ToolKit::parse`
        // of the embedded prompts), so this cannot fail in a released binary.
        let kit = ToolKit::parse(&prompts).expect("embedded tool prompts are valid");
        let tools = TutorTools::new(
            kit,
            topics.clone(),
            missions.clone(),
            preferences.clone(),
            clock.clone(),
            ids.clone(),
        );
        Self {
            inner: Arc::new(Inner {
                topics,
                messages,
                missions,
                preferences,
                events,
                llm,
                prompts,
                tools,
                clock,
                settings,
                ids,
                keys,
                lessons: None,
                documents: None,
            }),
        }
    }

    /// Lets messages carry a `lesson_id`.
    #[must_use]
    pub fn with_lessons(mut self, lessons: Arc<dyn LessonRepository>) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.lessons = Some(lessons);
        }
        self
    }

    /// Lets the Tutor start a lesson with `request_lesson`.
    #[must_use]
    pub fn with_lesson_requests(mut self, lessons: Arc<LessonService>) -> Self {
        let wired = Arc::get_mut(&mut self.inner).map(|inner| inner.tools.set_lessons(lessons));
        debug_assert!(
            wired.is_some(),
            "with_lesson_requests must be called before the service is shared"
        );
        self
    }

    /// Lets the Tutor see the topic's uploaded documents.
    #[must_use]
    pub fn with_documents(mut self, documents: Arc<dyn DocumentRepository>) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.documents = Some(documents);
        }
        self
    }

    /// Newest first (`created_at`, `id` descending).
    pub async fn list(
        &self,
        user: UserId,
        topic_id: TopicId,
        request: PageRequest<ChatCursor>,
    ) -> Result<MessagePage, AppError> {
        self.inner.require_topic(user, topic_id).await?;
        // Read before the query: every event up to this id is then reflected
        // in the messages returned (those after it may or may not be).
        let last_event_id = self.inner.events.latest_event_id(topic_id);
        let page = paginate(
            request,
            |after, limit| async move { self.inner.messages.list(topic_id, after, limit).await },
            ChatCursor::after,
        )
        .await
        .map_err(AppError::from)?;
        Ok(MessagePage {
            page,
            last_event_id,
        })
    }

    /// Stores the user message and an empty `streaming` assistant message,
    /// then generates the reply in the background.
    pub async fn send(
        &self,
        user: UserId,
        topic_id: TopicId,
        text: Option<&str>,
    ) -> Result<SentTurn, AppError> {
        self.send_about(user, topic_id, text, None).await
    }

    /// Like [`send`](Self::send); with `lesson_id` the Tutor also learns which
    /// lesson of the topic the question comes from, for this turn only
    /// (`lessons.md` §7). A lesson that is not the topic's is `422 not_found`.
    pub async fn send_about(
        &self,
        user: UserId,
        topic_id: TopicId,
        text: Option<&str>,
        lesson_id: Option<LessonId>,
    ) -> Result<SentTurn, AppError> {
        let inner = &self.inner;
        let mut topic = inner.require_topic(user, topic_id).await?;
        let text = MessageText::parse(text.unwrap_or_default()).map_err(AppError::validation)?;
        let note = match lesson_id {
            None => None,
            Some(id) => {
                let found = match &inner.lessons {
                    Some(repo) => repo.get(user, id).await?.map(|s| s.lesson),
                    None => None,
                };
                let lesson = found.filter(|l| l.topic_id == topic_id).ok_or_else(|| {
                    AppError::validation(domain::ValidationIssue::new(
                        "lesson_id",
                        domain::IssueKind::NotFound,
                    ))
                })?;
                Some(LessonNote {
                    number: lesson.number,
                    title: lesson.title,
                    objective: lesson.objective,
                })
            }
        };

        // The reply needs the owner's key: no key, no messages (`users.md` §3).
        let key = inner.keys.require(user).await?;

        // Fast path for the common refusal; the unique index behind
        // `insert_turn` is what settles concurrent sends.
        let latest = inner.messages.latest(topic_id).await?;
        if has_active_turn(latest.as_ref()) {
            return Err(AppError::Conflict(ConflictReason::TurnInProgress));
        }

        let now = now_millis(inner.clock.as_ref());
        let user = ChatMessage::user(MessageId::new(inner.ids.new_id()), topic_id, text, now);
        let assistant =
            ChatMessage::assistant_streaming(MessageId::new(inner.ids.new_id()), topic_id, now);
        inner
            .messages
            .insert_turn(&user, &assistant)
            .await
            .map_err(|e| match e {
                RepositoryError::Conflict => AppError::Conflict(ConflictReason::TurnInProgress),
                RepositoryError::MissingParent | RepositoryError::NotFound => AppError::NotFound,
                other => other.into(),
            })?;
        let (mut user, mut assistant) = (user, assistant);
        inner.announce(&mut user, now).await;
        inner.announce(&mut assistant, now).await;

        // The state change is not critical, and the turn is already stored: a
        // failure here must not leave an assistant message `streaming` with no
        // worker (the topic would answer 409 until the next restart). So it is
        // logged and the reply is generated anyway.
        if let Some(change) = topic.on_user_message(now) {
            match inner
                .topics
                .transition_state(topic_id, change.from, change.to, now)
                .await
            {
                // Publish what is stored now, not our snapshot: another writer
                // may have renamed or moved the topic meanwhile.
                Ok(true) => match inner.topics.get(topic.user_id, topic_id).await {
                    Ok(Some(fresh)) => {
                        inner.publish(
                            topic_id,
                            now,
                            TopicEventPayload::TopicUpdated(fresh.clone()),
                        );
                        topic = fresh;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "cannot reload topic after its state change");
                    }
                },
                Ok(false) => {}
                Err(e) => tracing::warn!(error = %e, "cannot move the topic to `interviewing`"),
            }
        }

        Inner::spawn_generation(&self.inner, topic, assistant.clone(), note, key);
        Ok(SentTurn { user, assistant })
    }

    /// Regenerates a failed reply, keeping the message id.
    pub async fn retry(
        &self,
        user: UserId,
        topic_id: TopicId,
        message_id: MessageId,
    ) -> Result<ChatMessage, AppError> {
        let inner = &self.inner;
        let topic = inner.require_topic(user, topic_id).await?;
        let key = inner.keys.require(user).await?;
        let mut message = inner
            .messages
            .get(topic_id, message_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let latest = inner.messages.latest(topic_id).await?;
        let is_last = latest.as_ref().is_some_and(|m| m.id == message.id);

        let now = now_millis(inner.clock.as_ref());
        message.restart(is_last, now).map_err(rule_to_conflict)?;
        inner
            .messages
            .restart_failed(&message)
            .await
            .map_err(|e| match e {
                RepositoryError::Conflict => AppError::Conflict(ConflictReason::NotRetryable),
                RepositoryError::NotFound => AppError::NotFound,
                other => other.into(),
            })?;
        // Same id as before: clients upsert on `message_created`.
        inner.announce(&mut message, now).await;
        Inner::spawn_generation(&self.inner, topic, message.clone(), None, key);
        Ok(message)
    }

    /// Startup recovery: no generation survives a restart, so messages left
    /// `streaming` become `failed` (`internal`) and can be retried.
    pub async fn recover_interrupted(&self) -> Result<u64, AppError> {
        let now = now_millis(self.inner.clock.as_ref());
        // The messages' topics are not loaded here: the repository picks each
        // text in the language of the message's own topic.
        let error = |language| MessageError::from_code(ChatErrorCode::Internal, language);
        Ok(self.inner.messages.fail_streaming(&error, now).await?)
    }
}

fn refusal(language: ContentLanguage) -> Outcome {
    Outcome::Failed(MessageError::refused(language))
}

fn internal_failure(language: ContentLanguage) -> Outcome {
    Outcome::Failed(MessageError::from_code(ChatErrorCode::Internal, language))
}

/// Maps a provider failure to the code the client sees (ADR 0004): only rate
/// limiting is its own case; our own mistakes are `internal`; everything the
/// provider or the network did wrong is `upstream_failed`.
#[must_use]
pub fn chat_error_code(error: &LlmError) -> ChatErrorCode {
    match error {
        LlmError::RateLimited { .. } => ChatErrorCode::RateLimited,
        LlmError::Authentication => ChatErrorCode::LlmKeyRejected,
        LlmError::CreditExhausted => ChatErrorCode::LlmCreditExhausted,
        LlmError::InvalidRequest(_) | LlmError::Unsupported(_) | LlmError::ContextTooLong => {
            ChatErrorCode::Internal
        }
        _ => ChatErrorCode::UpstreamFailed,
    }
}

fn rule_to_conflict(rule: ChatRuleError) -> AppError {
    match rule {
        ChatRuleError::TurnInProgress => AppError::Conflict(ConflictReason::TurnInProgress),
        ChatRuleError::NotStreaming | ChatRuleError::NotRetryable => {
            AppError::Conflict(ConflictReason::NotRetryable)
        }
    }
}

/// What a turn changed besides the text of the reply; announced (and the card
/// added to the message) when the message ends. Shared with the supervising
/// task so that even after a panic what the tools already did is announced.
#[derive(Default)]
struct TurnEffects {
    /// Drafts whose card is in the message, in order.
    proposals: Vec<Proposal>,
    /// Job of the `lesson_request` card, if the Tutor requested a lesson.
    lesson_job: Option<domain::JobId>,
    /// A tool moved the topic to another state.
    topic_changed: bool,
}

/// One reply of the model within a turn.
struct Round {
    stop_reason: StopReason,
    /// The assistant message as the model produced it, to send back whole.
    parts: Vec<Part>,
    calls: Vec<ToolCall>,
}

fn take_effects(shared: &Mutex<TurnEffects>) -> TurnEffects {
    // Poisoned only if a panic hit while the (tiny) critical section ran; the
    // data is plain, so use what is there.
    std::mem::take(
        &mut *shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

/// How a generation ended.
enum Outcome {
    Completed,
    Failed(MessageError),
    /// The topic (and with it the message) was deleted meanwhile.
    Gone,
}

impl Inner {
    async fn require_topic(&self, user: UserId, id: TopicId) -> Result<Topic, AppError> {
        self.topics.get(user, id).await?.ok_or(AppError::NotFound)
    }

    fn publish(
        &self,
        topic_id: TopicId,
        at: OffsetDateTime,
        payload: TopicEventPayload,
    ) -> Option<u64> {
        self.events.publish(TopicEvent {
            topic_id,
            at,
            payload,
        })
    }

    /// Publishes `message_created` and records the event's id in the stored
    /// message, so a REST reader knows what the snapshot already includes.
    /// Failing to record it is not fatal: the field only lags.
    async fn announce(&self, message: &mut ChatMessage, at: OffsetDateTime) {
        let id = self.publish(
            message.topic_id,
            at,
            TopicEventPayload::MessageCreated(message.clone()),
        );
        self.remember_event(message, id).await;
    }

    async fn remember_event(&self, message: &mut ChatMessage, id: Option<u64>) {
        let Some(id) = id else { return };
        message.last_event_id = Some(id);
        match self.messages.save(message).await {
            Ok(()) | Err(RepositoryError::NotFound) => {}
            Err(e) => tracing::warn!(error = %e, "cannot record the last event id of a message"),
        }
    }

    fn now(&self) -> OffsetDateTime {
        now_millis(self.clock.as_ref())
    }

    /// Runs the generation detached from the request: it keeps going if the
    /// client disconnects. A second task supervises the first so that a panic
    /// still ends the message as `failed` instead of leaving it `streaming`.
    ///
    /// Cancellation: there is no handle. Deleting a topic deletes its messages;
    /// the next write of the partial text finds nothing (`NotFound`) and the
    /// task stops, so at most one flush interval of tokens is wasted.
    fn spawn_generation(
        this: &Arc<Self>,
        topic: Topic,
        message: ChatMessage,
        lesson: Option<LessonNote>,
        key: Secret,
    ) {
        let inner = Arc::clone(this);
        let (topic_id, message_id, language) = (topic.id, message.id, topic.language);
        let effects = Arc::new(Mutex::new(TurnEffects::default()));
        tokio::spawn(async move {
            let worker = {
                let inner = Arc::clone(&inner);
                let effects = Arc::clone(&effects);
                // The reply is the owner's: their key, scoped to this task.
                let span = tracing::info_span!("reply", user_id = %topic.user_id, %topic_id);
                tokio::spawn(with_key(
                    key,
                    async move {
                        inner.generate(&topic, message, lesson, &effects).await;
                    }
                    .instrument(span),
                ))
            };
            if worker.await.is_err() {
                tracing::error!(%topic_id, %message_id, "reply generation task panicked");
                inner
                    .fail_orphan(topic_id, message_id, language, &effects)
                    .await;
            }
        });
    }

    async fn generate(
        &self,
        topic: &Topic,
        mut message: ChatMessage,
        lesson: Option<LessonNote>,
        shared: &Mutex<TurnEffects>,
    ) {
        let outcome = self
            .run_turn(topic, &mut message, lesson.as_ref(), shared)
            .await;
        let effects = take_effects(shared);
        match outcome {
            Outcome::Gone => {
                tracing::info!(topic_id = %topic.id, "topic deleted during generation; stopping");
                self.events.topic_closed(topic.id);
            }
            Outcome::Completed => {
                self.finish_completed(message, topic.language, effects)
                    .await;
            }
            Outcome::Failed(error) => self.finish_failed(message, error, effects).await,
        }
    }

    /// Assembles the request and opens the reply stream, shrinking the history
    /// once if the context turns out to be too long. `carried` are the
    /// messages of the tool loop so far (assistant turn, tool results).
    async fn open_reply(
        &self,
        topic: &Topic,
        history: &[ChatMessage],
        context: &TutorContext<'_>,
        tools: &[ToolDef],
        carried: &[LlmMessage],
    ) -> Result<Box<dyn LlmEventStream>, Outcome> {
        let mut budget = self.settings.history_token_budget;
        let mut halved = false;
        loop {
            let mut request = match assemble_tutor_request(
                &self.prompts,
                topic,
                context,
                history,
                HistoryLimits {
                    token_budget: budget,
                },
            ) {
                Ok(r) => r,
                Err(e @ (AssembleError::Template(_) | AssembleError::NothingToAnswer)) => {
                    tracing::error!(error = %e, "cannot assemble the Tutor request");
                    return Err(internal_failure(topic.language));
                }
            };
            request.tools = tools.to_vec();
            request.messages.extend(carried.iter().cloned());
            match self.llm.stream(request).await {
                Ok(s) => return Ok(s),
                // One more try with half the history, as the design prescribes.
                Err(LlmError::ContextTooLong) if !halved => {
                    halved = true;
                    budget /= 2;
                    tracing::warn!(budget, "context too long; retrying with less history");
                }
                Err(e) => return Err(self.llm_failure(&e, topic.language)),
            }
        }
    }

    /// Mission, draft and preferences as they are when the turn starts. They
    /// are not refreshed inside the tool loop: the tool results already tell
    /// the model what changed, and a stable context keeps the prompt cache.
    async fn load_context(
        &self,
        topic: &Topic,
    ) -> Result<(Option<Mission>, Option<Mission>, Vec<Preference>), Outcome> {
        let fail = |e: RepositoryError| match e {
            RepositoryError::NotFound | RepositoryError::MissingParent => Outcome::Gone,
            other => {
                tracing::error!(error = %other, "cannot load the Tutor context");
                internal_failure(topic.language)
            }
        };
        let topic_id = topic.id;
        let confirmed = self.missions.current(topic_id).await.map_err(fail)?;
        let draft = self.missions.open_draft(topic_id).await.map_err(fail)?;
        let limit = u32::try_from(PREFERENCES_SHOWN + 1).unwrap_or(u32::MAX);
        let preferences = self
            .preferences
            .latest_for_topic(topic.user_id, topic_id, limit)
            .await
            .map_err(fail)?;
        Ok((confirmed, draft, preferences))
    }

    /// The whole turn: call the model; if it asks for tools, run them, send the
    /// results back and call again, until it answers or the limit is reached.
    ///
    /// Tool effects are applied only once a stream has finished. Retries of a
    /// call (`RetryingLlm`) happen before its first content event, so no tool
    /// of a retried call has run: effects are never doubled by them.
    async fn run_turn(
        &self,
        topic: &Topic,
        message: &mut ChatMessage,
        lesson: Option<&LessonNote>,
        effects: &Mutex<TurnEffects>,
    ) -> Outcome {
        let history = match self.messages.list(topic.id, None, HISTORY_FETCH).await {
            Ok(h) => h,
            Err(RepositoryError::NotFound) => return Outcome::Gone,
            Err(e) => {
                tracing::error!(error = %e, "cannot load chat history");
                return internal_failure(topic.language);
            }
        };
        let (confirmed, draft, preferences) = match self.load_context(topic).await {
            Ok(c) => c,
            Err(outcome) => return outcome,
        };
        // A document list that cannot be read is not worth failing the reply.
        let documents = match &self.documents {
            Some(repo) => repo.list(topic.id).await.unwrap_or_else(|e| {
                tracing::warn!(error = %e, "cannot load the documents for the Tutor");
                Vec::new()
            }),
            None => Vec::new(),
        };
        let context = TutorContext {
            confirmed: confirmed.as_ref(),
            draft: draft.as_ref(),
            preferences_newest_first: &preferences,
            lesson,
            documents: &documents,
        };
        let tools = if self.llm.capabilities(LlmRole::Tutor).tools {
            self.tools.definitions()
        } else {
            Vec::new()
        };

        let mut carried: Vec<LlmMessage> = Vec::new();
        let mut turn = TurnState {
            started_at: message.created_at,
            mission_tool_done: false,
            lesson_requested: false,
        };
        let mut iterations = 0;
        let mut last_flush = Instant::now();
        loop {
            let started = Instant::now();
            let mut stream = match self
                .open_reply(topic, &history, &context, &tools, &carried)
                .await
            {
                Ok(s) => s,
                Err(outcome) => return outcome,
            };
            let round = match self
                .read_round(
                    stream.as_mut(),
                    topic,
                    message,
                    &mut last_flush,
                    started,
                    iterations > 0,
                )
                .await
            {
                Ok(r) => r,
                Err(outcome) => return outcome,
            };

            match round.stop_reason {
                StopReason::Refusal => return refusal(topic.language),
                StopReason::ToolUse if !round.calls.is_empty() => {}
                // The answer. Tool calls next to any other stop reason are not
                // run: with `max_tokens` their input may be cut short.
                ref stop => {
                    if !round.calls.is_empty() {
                        tracing::warn!(stop_reason = ?stop, "ignoring tool calls of a final reply");
                    }
                    return self.conclude(message, stop, topic.language);
                }
            }

            if iterations >= self.settings.max_tool_iterations {
                return Self::tool_limit_reached(message, iterations, topic.language);
            }
            iterations += 1;

            let results = match self
                .run_tools(topic, effects, &mut turn, &round.calls)
                .await
            {
                Ok(r) => r,
                Err(outcome) => return outcome,
            };

            // Only the newest message of the loop is a cache breakpoint.
            for m in &mut carried {
                m.cache_breakpoint = false;
            }
            carried.push(LlmMessage {
                role: LlmRoleOf::Assistant,
                parts: round.parts,
                cache_breakpoint: false,
            });
            carried.push(LlmMessage {
                role: LlmRoleOf::User,
                parts: results,
                cache_breakpoint: true,
            });
        }
    }

    /// Runs the tools of one reply, in order, and returns their results as the
    /// parts of the message that goes back to the model.
    async fn run_tools(
        &self,
        topic: &Topic,
        effects: &Mutex<TurnEffects>,
        turn: &mut TurnState,
        calls: &[ToolCall],
    ) -> Result<Vec<Part>, Outcome> {
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            let run = match self.tools.run(topic.id, call, turn).await {
                Ok(run) => run,
                Err(ToolAbort::Gone) => return Err(Outcome::Gone),
                Err(ToolAbort::Storage) => return Err(internal_failure(topic.language)),
            };
            // Names and outcome only: the input is the user's words.
            tracing::info!(
                tool = loggable_tool_name(&call.name),
                is_error = run.is_error,
                "tutor tool call"
            );
            self.note_tool_run(topic, effects, &run);
            results.push(Part::ToolResult {
                tool_use_id: call.id.clone(),
                content: run.content,
                is_error: run.is_error,
            });
        }
        Ok(results)
    }

    /// A model that keeps asking for tools is looping. If the user already has
    /// text, keep it; otherwise there is no answer.
    fn tool_limit_reached(
        message: &ChatMessage,
        iterations: usize,
        language: ContentLanguage,
    ) -> Outcome {
        if message.text().trim().is_empty() {
            tracing::error!(
                message_id = %message.id,
                iterations,
                "the model kept asking for tools and produced no text; failing the turn"
            );
            return internal_failure(language);
        }
        tracing::warn!(
            message_id = %message.id,
            iterations,
            "the model kept asking for tools; closing the message with its text"
        );
        Outcome::Completed
    }

    /// Ends a turn whose last reply asked for no tools.
    fn conclude(
        &self,
        message: &ChatMessage,
        stop: &StopReason,
        language: ContentLanguage,
    ) -> Outcome {
        // A reply with nothing in it is a provider fault, not an answer.
        if message.parts.is_empty() {
            return Outcome::Failed(MessageError::from_code(
                ChatErrorCode::UpstreamFailed,
                language,
            ));
        }
        if *stop == StopReason::MaxTokens {
            tracing::warn!(
                message_id = %message.id,
                role = "tutor",
                max_output_tokens = self.llm.capabilities(LlmRole::Tutor).max_output_tokens,
                "Tutor reply stopped at the token limit"
            );
        }
        Outcome::Completed
    }

    /// Records what a tool did. The card is not put in the message yet: parts
    /// are persisted while it streams, and a REST snapshot taken mid-turn must
    /// only hold text, like the `message_delta` events it lines up with. The
    /// card is added when the message ends.
    fn note_tool_run(
        &self,
        topic: &Topic,
        effects: &Mutex<TurnEffects>,
        run: &crate::tutor_tools::ToolRun,
    ) {
        if let Some(p) = &run.preference {
            self.publish(
                topic.id,
                self.now(),
                TopicEventPayload::PreferenceSaved(p.clone()),
            );
        }
        let mut fx = effects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        fx.topic_changed |= run.topic_changed;
        if let Some(proposal) = &run.proposal {
            fx.proposals.push(proposal.clone());
        }
        if run.lesson_job.is_some() {
            fx.lesson_job = run.lesson_job;
        }
    }

    /// Reads one model reply: text goes into `message` and to subscribers as it
    /// arrives (partial text saved at most once per flush interval); tool calls
    /// and provider blocks are collected to be sent back.
    async fn read_round(
        &self,
        stream: &mut dyn LlmEventStream,
        topic: &Topic,
        message: &mut ChatMessage,
        last_flush: &mut Instant,
        started: Instant,
        continuing: bool,
    ) -> Result<Round, Outcome> {
        let mut model = String::new();
        let mut request_id = None;
        let mut parts: Vec<Part> = Vec::new();
        let mut calls = Vec::new();
        let mut text_block = None;
        // After a tool round the new text continues the message: keep it from
        // gluing to what came before.
        let mut separator_due = continuing;
        loop {
            match stream.next().await {
                Some(Ok(LlmEvent::Started {
                    model: m,
                    request_id: r,
                })) => {
                    model = m;
                    request_id = r;
                }
                Some(Ok(LlmEvent::TextDelta { block, text })) => {
                    if text.is_empty() {
                        continue;
                    }
                    match parts.last_mut() {
                        Some(Part::Text(t)) if text_block == Some(block) => t.push_str(&text),
                        _ => parts.push(Part::Text(text.clone())),
                    }
                    text_block = Some(block);
                    let mut text = text;
                    if std::mem::take(&mut separator_due) {
                        let glued = message.parts.iter().rev().find_map(|p| match p {
                            domain::MessagePart::Text(t) => Some(t),
                            domain::MessagePart::MissionProposal { .. }
                            | domain::MessagePart::LessonRequest { .. } => None,
                        });
                        if glued.is_some_and(|t| !t.is_empty() && !t.ends_with(char::is_whitespace))
                        {
                            text.insert_str(0, "\n\n");
                        }
                    }
                    let now = self.now();
                    let Some(part_index) = message.append_text_at(&text, now) else {
                        continue;
                    };
                    // The stored text (flushed later) will include this delta, so
                    // its id is what a REST reader needs to know.
                    let event_id = self.publish(
                        topic.id,
                        now,
                        TopicEventPayload::MessageDelta {
                            message_id: message.id,
                            part_index,
                            text,
                        },
                    );
                    message.last_event_id = event_id.or(message.last_event_id);
                    if last_flush.elapsed() >= self.settings.flush_interval {
                        match self.messages.save(message).await {
                            Ok(()) => *last_flush = Instant::now(),
                            Err(RepositoryError::NotFound) => return Err(Outcome::Gone),
                            // Not fatal: the final save carries the whole text.
                            Err(e) => tracing::warn!(error = %e, "cannot save partial reply"),
                        }
                    }
                }
                Some(Ok(LlmEvent::ToolCallFinished {
                    id, name, input, ..
                })) => {
                    // The API wants an object when the call is sent back; the
                    // tool layer still sees (and refuses) what the model sent.
                    let echoed = if input.is_object() {
                        input.clone()
                    } else {
                        serde_json::json!({})
                    };
                    parts.push(Part::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: echoed,
                    });
                    calls.push(ToolCall { id, name, input });
                }
                // Thinking blocks must go back to the model untouched.
                Some(Ok(LlmEvent::ProviderBlock {
                    provider, payload, ..
                })) => parts.push(Part::ProviderBlock { provider, payload }),
                Some(Ok(LlmEvent::Finished { stop_reason, usage })) => {
                    self.log_call(&model, request_id.as_deref(), &stop_reason, usage, started);
                    return Ok(Round {
                        stop_reason,
                        parts,
                        calls,
                    });
                }
                // Usage updates and streamed tool input fragments are not
                // user-visible text.
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(self.llm_failure(&e, topic.language)),
                None => {
                    tracing::error!("LLM stream ended without a stop reason");
                    return Err(Outcome::Failed(MessageError::from_code(
                        ChatErrorCode::UpstreamFailed,
                        topic.language,
                    )));
                }
            }
        }
    }

    fn log_call(
        &self,
        model: &str,
        request_id: Option<&str>,
        stop_reason: &StopReason,
        usage: Usage,
        started: Instant,
    ) {
        tracing::info!(
            role = "tutor",
            model,
            prompt_version = self.prompts.version.trim(),
            latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cache_read_tokens = usage.cache_read_tokens,
            cache_write_tokens = usage.cache_write_tokens,
            stop_reason = ?stop_reason,
            request_id,
            "LLM call finished"
        );
    }

    fn llm_failure(&self, error: &LlmError, language: ContentLanguage) -> Outcome {
        match error {
            LlmError::Authentication => {
                // The user's own key was refused: their problem, not the server's.
                tracing::warn!("LLM authentication failed: the user's key was rejected");
            }
            LlmError::CreditExhausted => {
                tracing::warn!("LLM credit exhausted: the provider account has no credit left");
            }
            other => tracing::warn!(
                error = %other,
                prompt_version = self.prompts.version.trim(),
                "LLM call failed"
            ),
        }
        Outcome::Failed(MessageError::from_code(chat_error_code(error), language))
    }

    /// Announces what the turn did, around the message's own event: the new
    /// drafts just before it, the topic's new state right after.
    fn announce_proposals(&self, topic_id: TopicId, at: OffsetDateTime, effects: &TurnEffects) {
        for proposal in &effects.proposals {
            self.publish(
                topic_id,
                at,
                TopicEventPayload::MissionProposed(proposal.mission.clone()),
            );
        }
    }

    async fn announce_topic_change(
        &self,
        topic_id: TopicId,
        at: OffsetDateTime,
        effects: &TurnEffects,
    ) {
        if !effects.topic_changed {
            return;
        }
        match self.topics.get_system(topic_id).await {
            Ok(Some(fresh)) => {
                self.publish(topic_id, at, TopicEventPayload::TopicUpdated(fresh));
            }
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %e, "cannot reload topic after a tool changed it"),
        }
    }

    /// The card goes last, once the message is closing.
    fn attach_card(message: &mut ChatMessage, effects: &TurnEffects, now: OffsetDateTime) {
        if let Some(p) = effects.proposals.first() {
            message.add_mission_proposal(p.mission.clone(), p.replaces_version, now);
        } else if let Some(job_id) = effects.lesson_job {
            message.add_lesson_request(job_id, now);
        }
    }

    async fn finish_completed(
        &self,
        mut message: ChatMessage,
        language: ContentLanguage,
        effects: TurnEffects,
    ) {
        let now = self.now();
        Self::attach_card(&mut message, &effects, now);
        if message.complete(now).is_err() {
            return;
        }
        match self.messages.save(&message).await {
            Ok(()) => {
                self.announce_proposals(message.topic_id, now, &effects);
                let id = self.publish(
                    message.topic_id,
                    now,
                    TopicEventPayload::MessageCompleted(message.clone()),
                );
                // Saved first so that whoever sees the event finds the row
                // complete; the event's own id can only be recorded afterwards.
                self.remember_event(&mut message, id).await;
                self.announce_topic_change(message.topic_id, now, &effects)
                    .await;
            }
            Err(RepositoryError::NotFound) => self.events.topic_closed(message.topic_id),
            Err(e) => {
                // The streaming row is still in the database; tell clients the
                // truth about what they can rely on and let a retry fix it.
                tracing::error!(error = %e, "cannot save completed reply");
                let mut failed = message;
                failed.status = domain::MessageStatus::Streaming;
                self.finish_failed(
                    failed,
                    MessageError::from_code(ChatErrorCode::Internal, language),
                    effects,
                )
                .await;
            }
        }
    }

    async fn finish_failed(
        &self,
        mut message: ChatMessage,
        error: MessageError,
        effects: TurnEffects,
    ) {
        let now = self.now();
        Self::attach_card(&mut message, &effects, now);
        if message.fail(error, now).is_err() {
            return;
        }
        match self.messages.save(&message).await {
            Ok(()) => {}
            Err(RepositoryError::NotFound) => {
                self.events.topic_closed(message.topic_id);
                return;
            }
            Err(e) => tracing::error!(error = %e, "cannot save failed reply"),
        }
        self.announce_proposals(message.topic_id, now, &effects);
        let id = self.publish(
            message.topic_id,
            now,
            TopicEventPayload::MessageFailed(message.clone()),
        );
        self.remember_event(&mut message, id).await;
        self.announce_topic_change(message.topic_id, now, &effects)
            .await;
    }

    /// Last resort after a panic: whatever was saved last becomes `failed`.
    /// What the tools had already done is announced too.
    async fn fail_orphan(
        &self,
        topic_id: TopicId,
        message_id: MessageId,
        language: ContentLanguage,
        effects: &Mutex<TurnEffects>,
    ) {
        match self.messages.get(topic_id, message_id).await {
            Ok(Some(message)) if message.status == domain::MessageStatus::Streaming => {
                self.finish_failed(
                    message,
                    MessageError::from_code(ChatErrorCode::Internal, language),
                    take_effects(effects),
                )
                .await;
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "cannot load orphaned message"),
        }
    }
}
