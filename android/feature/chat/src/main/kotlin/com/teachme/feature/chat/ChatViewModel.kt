package com.teachme.feature.chat

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.ChatMessage
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.ChatRepository
import com.teachme.core.model.ChatRole
import com.teachme.core.model.ChatStreamSignal
import com.teachme.core.model.KeyNotice
import com.teachme.core.model.KeyNoticeSource
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.LoadError
import com.teachme.core.model.Topic
import com.teachme.core.model.budgetResetInstant
import com.teachme.core.model.toLoadError
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.transformWhile
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.time.Instant
import kotlin.time.Duration.Companion.seconds
import kotlin.time.TimeMark
import kotlin.time.TimeSource

/** Constants of chat.md section 13 that the ViewModel needs. */
internal const val STREAM_APPLY_INTERVAL_MS = 100L
internal const val RECONNECT_HINT_MS = 30_000L
private const val MAX_IMMEDIATE_RESYNCS = 3
private const val MISSION_EVENT_BUFFER = 64
private const val SAVED_LAST_EVENT_ID = "last_event_id"
private val HEALTHY_STREAM = 15.seconds

/** Reconnection delays of the contract (chat.md section 4): 1, 2, 5, then every 10 seconds. */
internal val RECONNECT_BACKOFF_MS = longArrayOf(1_000, 2_000, 5_000, 10_000)

sealed interface ChatLoad {
    data object Loading : ChatLoad

    data class Failed(
        val error: LoadError,
    ) : ChatLoad

    data object Ready : ChatLoad
}

/** State of the SSE connection. Only shown while a turn is running (chat.md 9.6). */
enum class ChatConnection { Online, Reconnecting, Unreachable }

/** Why the last send failed; the text stays in the composer. */
sealed interface SendError {
    data object Network : SendError

    data object Server : SendError

    data object TooLong : SendError

    /** Daily LLM budget used up; [resetAt] is when it comes back (the server's Retry-After, else the next 00:00 UTC). */
    data class BudgetExceeded(
        val resetAt: Instant,
    ) : SendError
}

/** A send or a retry refused because of the LLM key (llm-key.md section 5); [fromRetry] picks the wording. */
data class ChatKeyNotice(
    val notice: KeyNotice,
    val fromRetry: Boolean = false,
)

/** Only the exhausted-credit panel stays on screen while the action goes out again (llm-key.md 9.1). */
private fun ChatKeyNotice?.keptForRetry(): ChatKeyNotice? = this?.takeIf { it.notice.variant == LlmKeyVariant.Credit }

/** Information shown above the composer (not an error). */
enum class ComposerNotice { ChatUpdated, }

/** What to say to a screen reader (chat.md section 11); [id] makes two equal announcements distinct. */
enum class AnnouncementKind { Writing, Answered, Failed, MissionProposed, LessonRequested }

data class Announcement(
    val kind: AnnouncementKind,
    val id: Int,
)

data class ChatUiState(
    val load: ChatLoad = ChatLoad.Loading,
    /** Oldest first. */
    val messages: List<ChatMessage> = emptyList(),
    val olderCursor: String? = null,
    val olderLoading: Boolean = false,
    val olderFailed: Boolean = false,
    val connection: ChatConnection = ChatConnection.Online,
    val sending: Boolean = false,
    val sendError: SendError? = null,
    val keyNotice: ChatKeyNotice? = null,
    /** How many times "Riprova" of a failed reply met an exhausted credit again: announced by the panel. */
    val creditRepeats: Int = 0,
    val notice: ComposerNotice? = null,
    val retrying: Boolean = false,
    /**
     * When "Riprova" of a failed message was refused again for the daily budget: the reset instant of that answer (its
     * Retry-After, else the next 00:00 UTC). The failed message's panel waits for the later of this and its own.
     */
    val budgetRetryResetAt: Instant? = null,
    /** Last Topic received on the stream; the Topic screen mirrors it into its title and chip. */
    val topic: Topic? = null,
    val announcement: Announcement? = null,
) {
    /** A turn is running when an assistant message is still streaming (contract: one per Topic). */
    val turnInProgress: Boolean
        get() = messages.any { it.role == ChatRole.Assistant && it.status == ChatMessageStatus.Streaming }

    val lastMessage: ChatMessage? get() = messages.lastOrNull()

    val canSend: Boolean get() = !turnInProgress && !sending

    /** The failed assistant message that can be retried: only the last message of the Topic (contract). */
    val retryableMessageId: String?
        get() =
            lastMessage
                ?.takeIf { it.role == ChatRole.Assistant && it.status == ChatMessageStatus.Failed }
                ?.id
}

sealed interface ChatEffect {
    /** The message was accepted: clear the composer and jump to the bottom. */
    data object Sent : ChatEffect

    /** Retry hit a 409: the chat changed elsewhere. */
    data object ChatUpdatedToast : ChatEffect

    /** The stream answered 404: the Topic may be gone (the Topic screen checks it) or the route is missing. */
    data object StreamNotFound : ChatEffect

    /** Retry could not reach the backend. */
    data object RetryFailedToast : ChatEffect
}

@HiltViewModel(assistedFactory = ChatViewModel.Factory::class)
class ChatViewModel
    @AssistedInject
    constructor(
        @Assisted private val topicId: String,
        private val chat: ChatRepository,
        private val llmKey: LlmKeyMonitor,
        private val savedState: SavedStateHandle,
        private val clock: AppClock = AppClock { System.currentTimeMillis() },
    ) : ViewModel() {
        @AssistedFactory
        interface Factory {
            fun create(topicId: String): ChatViewModel
        }

        private val _state = MutableStateFlow(ChatUiState())
        val state: StateFlow<ChatUiState> = _state.asStateFlow()

        private val effectChannel = Channel<ChatEffect>(Channel.BUFFERED)
        val effects: Flow<ChatEffect> = effectChannel.receiveAsFlow()

        // Mission, preference and job events are not chat state: the Topic screen forwards them to the other tabs.
        private val _missionEvents = MutableSharedFlow<ChatEvent>(extraBufferCapacity = MISSION_EVENT_BUFFER)
        val missionEvents: Flow<ChatEvent> = _missionEvents.asSharedFlow()

        private var sessionJob: Job? = null
        private var olderJob: Job? = null
        private var flushJob: Job? = null
        private var announcementCounter = 0
        private var needsSnapshot = false

        // Stream bookkeeping. Everything runs on the main dispatcher, so no locking is needed.
        // Kept in the SavedStateHandle so a process death does not lose the stream position. The messages are not saved, so
        // the first start after a restore always reads the history again; the saved position is only the fallback for a
        // snapshot that gives none (the server replays by id and messages merge by id, so nothing is shown twice).
        private var lastEventId: Long? = savedState[SAVED_LAST_EVENT_ID]
            set(value) {
                field = value
                savedState[SAVED_LAST_EVENT_ID] = value
            }
        private var restoredEventId: Long? = lastEventId
        private val pendingDeltas = ArrayList<Pair<Long?, ChatEvent.MessageDelta>>()
        private val kick = Channel<Unit>(Channel.CONFLATED)
        private var skipNextWait = false

        init {
            // "Still exhausted" counts credit answers in a row. It ends when the monitor says the credit is back, which it
            // only does for an operation that worked after the credit failure (the monitor's rule by date), never for an
            // old event that is delivered again.
            viewModelScope.launch {
                var seen = false
                llmKey.status.collect { status ->
                    if (seen && !status.creditExhaustedSeen) {
                        _state.update { if (it.creditRepeats == 0) it else it.copy(creditRepeats = 0) }
                    }
                    seen = status.creditExhaustedSeen
                }
            }
        }

        /** The stream stopped because of a `401` and waits for the user; a sign-in over the screen releases it. */
        private var sessionEnded = false

        /** The screen became visible: load the history and open the stream (stream first, REST after). */
        fun onStart() {
            if (sessionJob?.isActive == true) {
                // Back after a sign-in over this screen (login.md): the same user is back, so it reloads by itself, with no
                // "Riprova ora".
                if (sessionEnded) {
                    if (_state.value.load is ChatLoad.Failed) retryLoad() else reconnectNow()
                }
                return
            }
            lastEventId = null
            needsSnapshot = true
            sessionJob = viewModelScope.launch { runSession() }
        }

        /** The screen is no longer visible: close the stream (contract). The server keeps generating. */
        fun onStop() {
            sessionJob?.cancel()
            sessionJob = null
            flushPending()
            _state.update { it.copy(connection = ChatConnection.Online) }
        }

        /** Retry of the first load (the error screen). */
        fun retryLoad() {
            _state.update { it.copy(load = ChatLoad.Loading) }
            needsSnapshot = true
            skipNextWait = true
            kick.trySend(Unit)
        }

        /** "Riprova ora": skip the remaining backoff wait. */
        fun reconnectNow() {
            kick.trySend(Unit)
        }

        /**
         * Contract section 4, "Apertura": REST first (messages + stream position), then the stream with
         * `Last-Event-ID` = min(position, `last_event_id` of the streaming messages). `resync_required`
         * repeats the procedure; any other loss of the connection reconnects from the last event seen.
         */
        private suspend fun runSession() {
            var attempt = 0
            var resyncs = 0
            var unreachableTimer: Job? = null
            var openedAt: TimeMark? = null
            try {
                while (true) {
                    var resync = false
                    var endFailure: ApiFailure? = null
                    val historyOk = !needsSnapshot || loadHistory().also { if (it) needsSnapshot = false }
                    if (historyOk) {
                        chat
                            .openEvents(topicId, lastEventId)
                            .transformWhile { signal ->
                                if (signal is ChatStreamSignal.Event && signal.event == ChatEvent.ResyncRequired) {
                                    resync = true
                                    false
                                } else {
                                    emit(signal)
                                    true
                                }
                            }.collect { signal ->
                                when (signal) {
                                    ChatStreamSignal.Opened -> {
                                        // The backoff resets only after a sign of life (an event, or a stream that
                                        // stayed open for a while), so a server that closes at once is not hammered.
                                        openedAt = TimeSource.Monotonic.markNow()
                                        // A global preference deleted while no stream was open is not replayed
                                        // (mission.md 7): the mission side reloads on every (re)connection, which
                                        // also covers the stream reopened after a resync (so it is emitted once).
                                        _missionEvents.tryEmit(ChatEvent.ResyncRequired)
                                        unreachableTimer?.cancel()
                                        _state.update { it.copy(connection = ChatConnection.Online) }
                                    }
                                    is ChatStreamSignal.Event -> {
                                        attempt = 0
                                        onStreamEvent(signal.id, signal.event)
                                    }
                                    is ChatStreamSignal.Ended -> endFailure = signal.failure
                                }
                            }
                        flushPending()
                        if (openedAt?.let { it.elapsedNow() >= HEALTHY_STREAM } == true) attempt = 0
                        openedAt = null
                    }
                    val fatal = endFailure.takeIf { it == ApiFailure.Unauthorized || it == ApiFailure.NotFound }
                    if (fatal != null) {
                        // Reconnecting cannot fix this: show it, and wait for the user ("Riprova ora").
                        unreachableTimer?.cancel()
                        _state.update {
                            it.copy(
                                connection = ChatConnection.Unreachable,
                                load = if (it.load == ChatLoad.Ready) it.load else ChatLoad.Failed(fatal.toLoadError()),
                            )
                        }
                        if (fatal == ApiFailure.NotFound) effectChannel.trySend(ChatEffect.StreamNotFound)
                        sessionEnded = fatal == ApiFailure.Unauthorized
                        kick.receive()
                        sessionEnded = false
                        continue
                    }
                    if (resync) {
                        needsSnapshot = true
                        // A few immediate repeats are fine; a server that keeps asking is not hammered.
                        if (++resyncs <= MAX_IMMEDIATE_RESYNCS) continue
                    } else {
                        resyncs = 0
                    }
                    if (_state.value.connection == ChatConnection.Online) {
                        _state.update { it.copy(connection = ChatConnection.Reconnecting) }
                        unreachableTimer?.cancel()
                        unreachableTimer =
                            viewModelScope.launch {
                                delay(RECONNECT_HINT_MS)
                                _state.update { it.copy(connection = ChatConnection.Unreachable) }
                            }
                    }
                    val wait = RECONNECT_BACKOFF_MS[minOf(attempt, RECONNECT_BACKOFF_MS.lastIndex)]
                    attempt++
                    if (skipNextWait) {
                        skipNextWait = false
                    } else {
                        // A "retry now" pressed while not waiting must not cancel the next backoff.
                        while (kick.tryReceive().isSuccess) {
                            // drain
                        }
                        withTimeoutOrNull(wait) { kick.receive() }
                    }
                }
            } finally {
                unreachableTimer?.cancel()
            }
        }

        private fun onStreamEvent(
            id: Long?,
            event: ChatEvent,
        ) {
            if (id != null && id > (lastEventId ?: 0)) lastEventId = id
            // The key side of the event (a failure, a success) is reported to the monitor by the repository, with its date;
            // the run of credit answers ends when the monitor drops the credit flag (see the init block), not here: a
            // replayed old event proves nothing.
            when (event) {
                is ChatEvent.MessageDelta -> {
                    pendingDeltas += id to event
                    scheduleFlush()
                }
                else -> {
                    flushPending()
                    reduce(event)
                }
            }
        }

        private fun reduce(event: ChatEvent) {
            when (event) {
                ChatEvent.ResyncRequired -> Unit // handled by the session loop
                is ChatEvent.MissionProposed,
                is ChatEvent.MissionConfirmed,
                is ChatEvent.PreferenceSaved,
                is ChatEvent.PreferenceDeleted,
                is ChatEvent.JobUpdated,
                -> _missionEvents.tryEmit(event)
                is ChatEvent.TopicUpdated -> _state.update { it.copy(topic = event.topic) }
                else ->
                    _state.update { s ->
                        val before = s.messages
                        val after = before.applyEvent(event)
                        val finished =
                            when (event) {
                                is ChatEvent.MessageCompleted ->
                                    when {
                                        event.message.missionProposal != null -> AnnouncementKind.MissionProposed
                                        event.message.lessonRequest != null -> AnnouncementKind.LessonRequested
                                        else -> AnnouncementKind.Answered
                                    }
                                is ChatEvent.MessageFailed -> AnnouncementKind.Failed
                                else -> null
                            }
                        val finishedId = eventMessageId(event)
                        val wasStreaming = finished != null && before.any { it.id == finishedId && it.isStreaming }
                        s.copy(
                            messages = after,
                            announcement = if (wasStreaming) announce(finished) else s.announcement,
                            notice = if (after.any { it.isStreaming }) null else s.notice,
                        )
                    }
            }
        }

        private fun eventMessageId(event: ChatEvent): String? =
            when (event) {
                is ChatEvent.MessageCompleted -> event.message.id
                is ChatEvent.MessageFailed -> event.message.id
                else -> null
            }

        private fun announce(kind: AnnouncementKind) = Announcement(kind, ++announcementCounter)

        private fun scheduleFlush() {
            if (flushJob?.isActive == true) return
            flushJob =
                viewModelScope.launch {
                    delay(STREAM_APPLY_INTERVAL_MS)
                    flushPending()
                }
        }

        /** Applies the buffered deltas now, in arrival order (deltas are applied at most every 100 ms). */
        private fun flushPending() {
            flushJob?.cancel()
            flushJob = null
            if (pendingDeltas.isEmpty()) return
            val deltas = pendingDeltas.toList()
            pendingDeltas.clear()
            _state.update { s ->
                s.copy(messages = deltas.fold(s.messages) { list, (id, delta) -> list.applyDelta(delta, id) })
            }
        }

        /**
         * Step 1-2 of the opening procedure: loads the newest page and sets [lastEventId] to the position
         * the stream must resume from. Returns false (and shows the error on a first load) if REST failed.
         */
        private suspend fun loadHistory(): Boolean {
            flushPending()
            when (val result = chat.listMessages(topicId, null)) {
                is ApiResult.Success -> {
                    val page = result.value
                    var position: Long? = null
                    _state.update { s ->
                        val merged = s.messages.upsertAll(page.items)
                        position =
                            (
                                listOfNotNull(page.streamPosition) +
                                    merged.filter { it.isStreaming }.mapNotNull { it.lastEventId }
                            ).minOrNull()
                        s.copy(
                            load = ChatLoad.Ready,
                            messages = merged,
                            olderCursor = if (s.messages.isEmpty()) page.nextCursor else s.olderCursor,
                        )
                    }
                    lastEventId = position ?: restoredEventId
                    restoredEventId = null
                    return true
                }
                is ApiResult.Failure -> {
                    if (_state.value.load != ChatLoad.Ready) {
                        _state.update { it.copy(load = ChatLoad.Failed(result.failure.toLoadError())) }
                    }
                    return false
                }
            }
        }

        fun loadOlder() {
            val s = _state.value
            val cursor = s.olderCursor ?: return
            if (s.olderLoading || olderJob?.isActive == true) return
            _state.update { it.copy(olderLoading = true, olderFailed = false) }
            olderJob =
                viewModelScope.launch {
                    when (val result = chat.listMessages(topicId, cursor)) {
                        is ApiResult.Success ->
                            _state.update {
                                it.copy(
                                    messages = it.messages.upsertAll(result.value.items),
                                    olderCursor = result.value.nextCursor,
                                    olderLoading = false,
                                )
                            }
                        is ApiResult.Failure -> _state.update { it.copy(olderLoading = false, olderFailed = true) }
                    }
                }
        }

        /**
         * Sends [text] (trimmed). Does nothing when sending is not allowed; the UI shows why. [lessonId] says the chat
         * was opened from a lesson (lessons.md section 7): the Tutor gets it as context for this turn only. If the
         * backend does not know that lesson (`422` on `lesson_id`) the message is sent again without it: the context is
         * a bonus, never a reason to fail (ask_agent.md).
         */
        fun send(
            text: String,
            lessonId: String? = null,
        ) {
            val s = _state.value
            if (!s.canSend) return
            val trimmed = text.trim()
            val info = chatInputInfo(trimmed)
            if (!info.isValid) return
            // An exhausted-credit panel stays while the same message goes out again: its button says "Riprovo...".
            _state.update {
                it.copy(
                    sending = true,
                    sendError = null,
                    keyNotice = it.keyNotice.keptForRetry(),
                    notice = null,
                )
            }
            viewModelScope.launch {
                var result = chat.sendMessage(topicId, trimmed, lessonId)
                val failure = (result as? ApiResult.Failure)?.failure
                if (lessonId != null &&
                    failure is ApiFailure.Validation &&
                    failure.details.any { it.field == "lesson_id" }
                ) {
                    result = chat.sendMessage(topicId, trimmed, null)
                }
                when (result) {
                    is ApiResult.Success -> {
                        flushPending()
                        _state.update {
                            it.copy(
                                sending = false,
                                keyNotice = null,
                                messages = it.messages.upsert(result.value.user).upsert(result.value.assistant),
                                announcement = announce(AnnouncementKind.Writing),
                            )
                        }
                        effectChannel.send(ChatEffect.Sent)
                    }
                    is ApiResult.Failure -> onSendFailed(result)
                }
            }
        }

        private fun onSendFailed(result: ApiResult.Failure) {
            val failure = result.failure
            when (failure) {
                ApiFailure.Conflict -> {
                    // A turn is already running elsewhere (or our state is stale): the text stays, history reloads.
                    _state.update { it.copy(sending = false, keyNotice = null) }
                    viewModelScope.launch {
                        refreshSilently()
                        _state.update {
                            it.copy(notice = if (it.turnInProgress) null else ComposerNotice.ChatUpdated)
                        }
                    }
                }
                ApiFailure.LlmKeyMissing, ApiFailure.LlmKeyRejected, ApiFailure.LlmCreditExhausted ->
                    // Nothing was created and the text stays in the field (llm-key.md sections 5 and 9).
                    _state.update { it.copy(sending = false, keyNotice = notices.of(result)?.let(::ChatKeyNotice)) }
                // A 401 is the global handler's business (login.md): the sign-in opens over this screen, no panel.
                ApiFailure.Unauthorized -> _state.update { it.copy(sending = false, keyNotice = null) }
                else ->
                    _state.update {
                        it.copy(
                            sending = false,
                            keyNotice = null,
                            sendError =
                                when {
                                    failure is ApiFailure.Network -> SendError.Network
                                    failure is ApiFailure.LlmBudgetExceeded ->
                                        SendError.BudgetExceeded(
                                            budgetResetInstant(
                                                Instant.ofEpochMilli(clock.nowMs()),
                                                failure.retryAfterSecs,
                                            ),
                                        )
                                    failure is ApiFailure.Validation && failure.hasIssue("text", "too_long") ->
                                        SendError.TooLong
                                    else -> SendError.Server
                                },
                        )
                    }
            }
        }

        private val notices = KeyNoticeSource(llmKey)

        fun dismissSendError() = _state.update { it.copy(sendError = null) }

        /** Retries the failed last assistant message (`POST .../retry`). */
        fun retry(messageId: String) {
            val s = _state.value
            if (s.retrying || s.retryableMessageId != messageId) return
            _state.update { it.copy(retrying = true, keyNotice = null) }
            viewModelScope.launch {
                when (val result = chat.retryMessage(topicId, messageId)) {
                    is ApiResult.Success ->
                        _state.update {
                            it.copy(
                                retrying = false,
                                keyNotice = null,
                                creditRepeats = 0,
                                messages = it.messages.upsert(result.value),
                                budgetRetryResetAt = null,
                                announcement = announce(AnnouncementKind.Writing),
                            )
                        }
                    is ApiResult.Failure -> {
                        val keyNotice = notices.of(result)
                        val credit = keyNotice?.variant == LlmKeyVariant.Credit
                        // The panel of the failed message stays and waits for the new reset (llm-key.md 10.1): no toast.
                        val budget = result.failure as? ApiFailure.LlmBudgetExceeded
                        val budgetReset =
                            budget?.let { budgetResetInstant(Instant.ofEpochMilli(clock.nowMs()), it.retryAfterSecs) }
                        _state.update {
                            it.copy(
                                retrying = false,
                                // The credit panel is the failed message's own: it stays where "Riprova" was pressed.
                                keyNotice =
                                    keyNotice?.takeIf { !credit }?.let { n ->
                                        ChatKeyNotice(n, fromRetry = true)
                                    },
                                // Another kind of failure ends the run of "still exhausted" answers.
                                creditRepeats = if (credit) it.creditRepeats + 1 else 0,
                                budgetRetryResetAt = budgetReset ?: it.budgetRetryResetAt,
                            )
                        }
                        // With a key notice the panel is the answer; otherwise a toast says what happened.
                        val moved = result.failure == ApiFailure.Conflict || result.failure == ApiFailure.NotFound
                        if (keyNotice == null && moved) {
                            refreshSilently()
                            effectChannel.send(ChatEffect.ChatUpdatedToast)
                        } else if (keyNotice == null && budget == null) {
                            effectChannel.send(ChatEffect.RetryFailedToast)
                        }
                    }
                }
            }
        }

        /** Reloads the newest page and merges it by id without touching the list position. */
        private suspend fun refreshSilently() {
            val result = chat.listMessages(topicId, null)
            if (result is ApiResult.Success) {
                _state.update { it.copy(messages = it.messages.upsertAll(result.value.items)) }
            }
        }
    }
