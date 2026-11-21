package com.teachme.feature.chat

import com.teachme.core.model.ChatEvent
import com.teachme.core.model.ChatMessage
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.MessagePart

/*
 * Pure functions that fold backend data into the ordered list of messages (oldest first). Everything
 * here is idempotent: applying the same event or the same REST snapshot twice gives the same list, so
 * the stream and REST can overlap (contract chat.md section 4).
 */

private val messageOrder = compareBy<ChatMessage>({ it.createdAt }, { it.id })

/**
 * Inserts [incoming] in order, or replaces the message with the same id unless [incoming] is older.
 * Two versions of a message that is still streaming: the one that reflects the later event wins
 * (`lastEventId`), so a REST snapshot or a replayed `message_created` never undoes applied deltas.
 * Returns the same list instance when nothing changed.
 */
internal fun List<ChatMessage>.upsert(incoming: ChatMessage): List<ChatMessage> {
    val index = indexOfFirst { it.id == incoming.id }
    if (index < 0) {
        val at = indexOfFirst { messageOrder.compare(it, incoming) > 0 }
        return toMutableList().apply { add(if (at < 0) size else at, incoming) }
    }
    val existing = this[index]
    val newerEvent = existing.lastEventId?.let { e -> incoming.lastEventId?.let { it >= e } }
    val take =
        when {
            existing == incoming -> false
            incoming.updatedAt < existing.updatedAt -> false
            existing.isStreaming && incoming.isStreaming && newerEvent != null -> newerEvent
            else -> true
        }
    return if (take) toMutableList().apply { set(index, incoming) } else this
}

/** Same as [upsert] for a whole page, keeping order. */
internal fun List<ChatMessage>.upsertAll(incoming: List<ChatMessage>): List<ChatMessage> =
    incoming.fold(this) { list, message -> list.upsert(message) }

/**
 * Appends a delta to the text part `partIndex` of a message that is still streaming. A delta is applied
 * only if its [eventId] is greater than the last event already reflected in the message (contract
 * section 4), so replayed deltas are never applied twice. Deltas for unknown or finished messages are ignored.
 */
internal fun List<ChatMessage>.applyDelta(
    delta: ChatEvent.MessageDelta,
    eventId: Long?,
): List<ChatMessage> {
    val index = indexOfFirst { it.id == delta.messageId }
    if (index < 0) return this
    val message = this[index]
    if (message.status != ChatMessageStatus.Streaming) return this
    val reflected = message.lastEventId
    if (eventId != null && reflected != null && eventId <= reflected) return this
    val parts = message.parts.toMutableList()
    when {
        delta.partIndex < 0 -> return this
        delta.partIndex < parts.size ->
            when (val part = parts[delta.partIndex]) {
                is MessagePart.Text -> parts[delta.partIndex] = MessagePart.Text(part.text + delta.text)
                MessagePart.Unknown, is MessagePart.MissionProposal, is MessagePart.LessonRequest -> return this
            }
        else -> {
            while (parts.size < delta.partIndex) parts += MessagePart.Unknown
            parts += MessagePart.Text(delta.text)
        }
    }
    val updated = message.copy(parts = parts, lastEventId = eventId ?: reflected)
    return toMutableList().apply { set(index, updated) }
}

/**
 * Applies a message event. [ChatEvent.TopicUpdated] and [ChatEvent.ResyncRequired] do not touch the
 * messages and are handled by the caller.
 */
internal fun List<ChatMessage>.applyEvent(event: ChatEvent): List<ChatMessage> =
    when (event) {
        is ChatEvent.MessageCreated -> upsert(event.message)
        is ChatEvent.MessageDelta -> applyDelta(event, null)
        is ChatEvent.MessageCompleted -> upsert(event.message)
        is ChatEvent.MessageFailed -> upsert(event.message)
        is ChatEvent.TopicUpdated,
        ChatEvent.ResyncRequired,
        is ChatEvent.MissionProposed,
        is ChatEvent.MissionConfirmed,
        is ChatEvent.PreferenceSaved,
        is ChatEvent.PreferenceDeleted,
        is ChatEvent.JobUpdated,
        -> this
    }

/** The id of the most recent message that carries a card of [jobId]: the only one that announces and toasts. */
fun List<ChatMessage>.latestLessonRequestMessageId(jobId: String): String? =
    lastOrNull { it.lessonRequest?.jobId == jobId }?.id

/** The ids of the messages whose card announces: the latest one of each job. One pass, to be remembered per list. */
fun List<ChatMessage>.lessonRequestAnnouncers(): Set<String> {
    val latest = HashMap<String, String>()
    forEach { m -> m.lessonRequest?.let { latest[it.jobId] = m.id } }
    return latest.values.toSet()
}

/** True when a card of [jobId] is among these messages (it already tells the end of the generation). */
fun List<ChatMessage>.hasLessonRequest(jobId: String): Boolean = latestLessonRequestMessageId(jobId) != null
