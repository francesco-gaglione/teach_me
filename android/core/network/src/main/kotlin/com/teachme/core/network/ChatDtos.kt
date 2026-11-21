package com.teachme.core.network

import com.teachme.core.model.ChatEvent
import com.teachme.core.model.ChatMessage
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.ChatRole
import com.teachme.core.model.MessageError
import com.teachme.core.model.MessagePart
import com.teachme.core.model.PreferenceScope
import com.teachme.core.model.SentMessages
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.time.Instant
import java.time.OffsetDateTime

@Serializable
internal data class MessageErrorDto(
    val code: String,
    val message: String = "",
)

@Serializable
internal data class ChatMessageDto(
    val id: String,
    @SerialName("topic_id") val topicId: String,
    val role: String,
    val status: String,
    val parts: List<JsonElement> = emptyList(),
    val error: MessageErrorDto? = null,
    @SerialName("created_at") val createdAt: String,
    @SerialName("updated_at") val updatedAt: String,
    @SerialName("last_event_id") val lastEventId: Long? = null,
)

@Serializable
internal data class SendMessageRequest(
    val text: String,
    @SerialName("lesson_id") val lessonId: String? = null,
)

@Serializable
internal data class SendMessageResponseDto(
    @SerialName("user_message") val userMessage: ChatMessageDto,
    @SerialName("assistant_message") val assistantMessage: ChatMessageDto,
)

/** Envelope of every SSE `data` (api-conventions section 10). */
@Serializable
internal data class EventEnvelopeDto(
    val type: String? = null,
    val payload: JsonObject = JsonObject(emptyMap()),
)

private fun parseInstant(value: String): Instant = OffsetDateTime.parse(value).toInstant()

/**
 * `text`, `mission_proposal` and `lesson_request` parts are known; anything else, or a known type with a broken shape,
 * becomes [MessagePart.Unknown] (contract chat.md section 1, mission.md section 3).
 */
internal fun JsonElement.toPart(): MessagePart {
    val obj = this as? JsonObject ?: return MessagePart.Unknown
    val type = (obj["type"] as? JsonPrimitive)?.contentOrNull
    return when (type) {
        "text" -> (obj["text"] as? JsonPrimitive)?.takeIf { it.isString }?.content?.let { MessagePart.Text(it) }
        "mission_proposal" ->
            try {
                val mission =
                    ApiJson.decodeFromJsonElement(MissionDto.serializer(), obj.getValue("mission")).toModel()
                val replaces = (obj["replaces_version"] as? JsonPrimitive)?.intOrNull
                MessagePart.MissionProposal(mission, replaces)
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                null
            }
        "lesson_request" ->
            (obj["job_id"] as? JsonPrimitive)
                ?.takeIf { it.isString && it.content.isNotEmpty() }
                ?.let { MessagePart.LessonRequest(it.content) }
        else -> null
    } ?: MessagePart.Unknown
}

/** Null when the role is one this app does not know: such a message is skipped, never a crash. */
internal fun ChatMessageDto.toModelOrNull(): ChatMessage? {
    val roleModel =
        when (role) {
            "user" -> ChatRole.User
            "assistant" -> ChatRole.Assistant
            else -> return null
        }
    val statusModel =
        when (status) {
            "streaming" -> ChatMessageStatus.Streaming
            "failed" -> ChatMessageStatus.Failed
            else -> ChatMessageStatus.Complete
        }
    return ChatMessage(
        id = id,
        topicId = topicId,
        role = roleModel,
        status = statusModel,
        parts = parts.map { it.toPart() },
        error = error?.let { MessageError(it.code, it.message) },
        createdAt = parseInstant(createdAt),
        updatedAt = parseInstant(updatedAt),
        lastEventId = lastEventId,
    )
}

internal fun ChatMessageDto.toModel(): ChatMessage = requireNotNull(toModelOrNull()) { "unknown role: $role" }

internal fun SendMessageResponseDto.toModel() = SentMessages(userMessage.toModel(), assistantMessage.toModel())

/**
 * Decodes the `data` of an SSE frame. [eventName] is the SSE `event:` field (falls back to `data.type`).
 * Returns null for unknown event types and for payloads that do not have the documented shape: the
 * stream must survive anything the backend adds later.
 */
internal fun decodeChatEvent(
    eventName: String?,
    data: String,
): ChatEvent? =
    try {
        val envelope = ApiJson.decodeFromString<EventEnvelopeDto>(data)
        val payload = envelope.payload

        fun missionOf(p: JsonObject) =
            ApiJson.decodeFromJsonElement(MissionDto.serializer(), p.getValue("mission")).toModel()

        fun message() =
            ApiJson.decodeFromJsonElement(ChatMessageDto.serializer(), payload.getValue("message")).toModel()
        when (eventName ?: envelope.type) {
            "message_created" -> ChatEvent.MessageCreated(message())
            "message_completed" -> ChatEvent.MessageCompleted(message())
            "message_failed" -> ChatEvent.MessageFailed(message())
            "message_delta" ->
                ChatEvent.MessageDelta(
                    messageId = payload.getValue("message_id").jsonPrimitive.content,
                    partIndex = payload.getValue("part_index").jsonPrimitive.intOrNull ?: return null,
                    text = payload.getValue("text").jsonPrimitive.content,
                )
            "topic_updated" ->
                ChatEvent.TopicUpdated(
                    ApiJson
                        .decodeFromJsonElement(
                            TopicDto.serializer(),
                            payload.getValue("topic").jsonObject,
                        ).toModel(),
                )
            "mission_proposed" -> ChatEvent.MissionProposed(missionOf(payload))
            "mission_confirmed" -> ChatEvent.MissionConfirmed(missionOf(payload))
            "preference_saved" ->
                ChatEvent.PreferenceSaved(
                    ApiJson
                        .decodeFromJsonElement(PreferenceDto.serializer(), payload.getValue("preference"))
                        .toModelOrNull() ?: return null,
                )
            "preference_deleted" ->
                ChatEvent.PreferenceDeleted(
                    preferenceId = payload.getValue("preference_id").jsonPrimitive.content,
                    scope =
                        when (payload["scope"]?.jsonPrimitive?.contentOrNull) {
                            "global" -> PreferenceScope.Global
                            "topic" -> PreferenceScope.Topic
                            else -> return null
                        },
                    topicId = payload["topic_id"]?.jsonPrimitive?.contentOrNull,
                )
            "job_updated" ->
                ChatEvent.JobUpdated(
                    ApiJson.decodeFromJsonElement(JobDto.serializer(), payload.getValue("job")).toModel(),
                )
            "resync_required" -> ChatEvent.ResyncRequired
            else -> null
        }
    } catch (e: kotlinx.coroutines.CancellationException) {
        throw e
    } catch (e: Exception) {
        null
    }
