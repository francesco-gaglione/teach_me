package com.teachme.core.model

import java.time.Instant

enum class ChatRole { User, Assistant }

enum class ChatMessageStatus { Streaming, Complete, Failed }

/** One piece of a message. Types the app does not know yet are kept as [Unknown] so indexes stay valid. */
sealed interface MessagePart {
    data class Text(
        val text: String,
    ) : MessagePart

    /**
     * The mission card (contract mission.md section 3): a snapshot of the draft at proposal time. Always the
     * last part of its message. [replacesVersion] is the confirmed version it would replace, if any.
     */
    data class MissionProposal(
        val mission: Mission,
        val replacesVersion: Int?,
    ) : MessagePart

    /**
     * The Tutor started a lesson (chat.md v1.3, `lesson_request`): [jobId] is the generation job to follow. A reference,
     * not a snapshot: the card derives its state from the job.
     */
    data class LessonRequest(
        val jobId: String,
    ) : MessagePart

    /** A part of a type this app version cannot show (chat.md contract section 1): skipped when rendering. */
    data object Unknown : MessagePart
}

/** `error` of a failed message (`code` and `message` as in api-conventions section 7). */
data class MessageError(
    val code: String,
    val message: String,
)

data class ChatMessage(
    val id: String,
    val topicId: String,
    val role: ChatRole,
    val status: ChatMessageStatus,
    val parts: List<MessagePart>,
    val error: MessageError?,
    val createdAt: Instant,
    val updatedAt: Instant,
    /** Id of the last SSE event already reflected in this message (contract section 1); null on old data. */
    val lastEventId: Long? = null,
) {
    val isStreaming: Boolean get() = status == ChatMessageStatus.Streaming

    /** The text parts joined with a blank line: what "copy" puts on the clipboard (chat.md 4.1). */
    val plainText: String
        get() = parts.filterIsInstance<MessagePart.Text>().joinToString("\n\n") { it.text }

    /** True when at least one part can be shown by this app version. */
    val hasVisibleParts: Boolean get() = parts.any { it !is MessagePart.Unknown }

    /** The lesson request carried by this message, if any (shown as a card). */
    val lessonRequest: MessagePart.LessonRequest? get() =
        parts
            .filterIsInstance<MessagePart.LessonRequest>()
            .firstOrNull()

    /** The mission proposal carried by this message, if any. */
    val missionProposal: MessagePart.MissionProposal? get() =
        parts
            .filterIsInstance<MessagePart.MissionProposal>()
            .lastOrNull()
}

/** A page of messages (newest first) plus the stream position read before the query (`X-Last-Event-Id`). */
data class ChatPage(
    val items: List<ChatMessage>,
    val nextCursor: String?,
    val streamPosition: Long?,
)

/** Response of `POST /topics/{id}/messages` (202). */
data class SentMessages(
    val user: ChatMessage,
    val assistant: ChatMessage,
)

/** Events of the Topic stream (chat.md contract section 3). Unknown event types never reach this type. */
sealed interface ChatEvent {
    data class MessageCreated(
        val message: ChatMessage,
    ) : ChatEvent

    data class MessageDelta(
        val messageId: String,
        val partIndex: Int,
        val text: String,
    ) : ChatEvent

    data class MessageCompleted(
        val message: ChatMessage,
    ) : ChatEvent

    data class MessageFailed(
        val message: ChatMessage,
    ) : ChatEvent

    data class TopicUpdated(
        val topic: Topic,
    ) : ChatEvent

    data object ResyncRequired : ChatEvent

    /** A new mission draft was saved (mission.md section 5). */
    data class MissionProposed(
        val mission: Mission,
    ) : ChatEvent

    data class MissionConfirmed(
        val mission: Mission,
    ) : ChatEvent

    data class PreferenceSaved(
        val preference: Preference,
    ) : ChatEvent

    /** A job of the Topic changed (jobs.md section 4); idempotent, the client upserts by id. */
    data class JobUpdated(
        val job: TopicJob,
    ) : ChatEvent

    data class PreferenceDeleted(
        val preferenceId: String,
        val scope: PreferenceScope,
        val topicId: String?,
    ) : ChatEvent
}

/** What a single SSE connection reports, in order. */
sealed interface ChatStreamSignal {
    /** The server accepted the stream (HTTP 200 headers received). */
    data object Opened : ChatStreamSignal

    /** [id] is the SSE `id` (null if the frame had none). */
    data class Event(
        val id: Long?,
        val event: ChatEvent,
    ) : ChatStreamSignal

    /** Last signal: the connection ended, [failure] null if the server closed it cleanly. */
    data class Ended(
        val failure: ApiFailure?,
    ) : ChatStreamSignal
}

/** Chat of a Topic (docs/contracts/chat.md). No offline cache in this phase. */
interface ChatRepository {
    /** One page of messages, newest first; pass the previous [Page.nextCursor] for older ones. */
    suspend fun listMessages(
        topicId: String,
        cursor: String? = null,
    ): ApiResult<ChatPage>

    /** [lessonId]: the chat was opened from a lesson, the Tutor gets it as context for this turn (lessons.md section 7). */
    suspend fun sendMessage(
        topicId: String,
        text: String,
        lessonId: String? = null,
    ): ApiResult<SentMessages>

    suspend fun retryMessage(
        topicId: String,
        messageId: String,
    ): ApiResult<ChatMessage>

    /**
     * Opens one SSE connection to the Topic events and emits its signals; the flow ends after
     * [ChatStreamSignal.Ended]. Cancel the collector to close the connection. Reconnecting is the caller's job.
     */
    fun openEvents(
        topicId: String,
        lastEventId: Long?,
    ): kotlinx.coroutines.flow.Flow<ChatStreamSignal>
}

/** Max length of a message in Unicode code points (contract section 2). */
const val CHAT_MAX_CHARS = 8000
