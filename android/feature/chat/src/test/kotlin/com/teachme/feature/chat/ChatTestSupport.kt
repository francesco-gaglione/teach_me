package com.teachme.feature.chat

import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatMessage
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.ChatPage
import com.teachme.core.model.ChatRepository
import com.teachme.core.model.ChatRole
import com.teachme.core.model.ChatStreamSignal
import com.teachme.core.model.MessageError
import com.teachme.core.model.MessagePart
import com.teachme.core.model.SentMessages
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import java.time.Instant

internal fun msg(
    id: String,
    role: ChatRole = ChatRole.Assistant,
    status: ChatMessageStatus = ChatMessageStatus.Complete,
    text: String? = "t-$id",
    second: Long = id.filter { it.isDigit() }.toLongOrNull() ?: 0,
    updated: Long = second,
    error: MessageError? = null,
    lastEventId: Long? = null,
) = ChatMessage(
    id = id,
    topicId = "t1",
    role = role,
    status = status,
    parts = if (text == null) emptyList() else listOf(MessagePart.Text(text)),
    error = error,
    createdAt = Instant.ofEpochSecond(second),
    updatedAt = Instant.ofEpochSecond(updated),
    lastEventId = lastEventId,
)

internal class FakeChatRepository : ChatRepository {
    val openedWith = mutableListOf<Long?>()
    val connections = mutableListOf<Channel<ChatStreamSignal>>()
    var list: (String?) -> ApiResult<ChatPage> = { ApiResult.Success(page(emptyList())) }
    var send: (String) -> ApiResult<SentMessages> = { error("unused") }
    var retry: (String) -> ApiResult<ChatMessage> = { error("unused") }
    val sent = mutableListOf<String>()

    /** The `lesson_id` of every send, in the same order as [sent]. */
    val sentLessonIds = mutableListOf<String?>()
    var listCalls = 0

    override suspend fun listMessages(
        topicId: String,
        cursor: String?,
    ): ApiResult<ChatPage> {
        listCalls++
        return list(cursor)
    }

    override suspend fun sendMessage(
        topicId: String,
        text: String,
        lessonId: String?,
    ): ApiResult<SentMessages> {
        sent += text
        sentLessonIds += lessonId
        return send(text)
    }

    override suspend fun retryMessage(
        topicId: String,
        messageId: String,
    ): ApiResult<ChatMessage> = retry(messageId)

    override fun openEvents(
        topicId: String,
        lastEventId: Long?,
    ): Flow<ChatStreamSignal> =
        flow {
            openedWith += lastEventId
            val channel = Channel<ChatStreamSignal>(Channel.UNLIMITED)
            connections += channel
            for (signal in channel) {
                emit(signal)
                if (signal is ChatStreamSignal.Ended) return@flow
            }
        }

    fun push(signal: ChatStreamSignal) {
        connections.last().trySend(signal)
    }
}

internal fun page(
    items: List<ChatMessage>,
    next: String? = null,
    position: Long? = null,
) = ChatPage(items, next, position)
