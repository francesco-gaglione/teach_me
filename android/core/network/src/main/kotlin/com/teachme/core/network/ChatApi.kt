package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatMessage
import com.teachme.core.model.ChatPage
import com.teachme.core.model.ChatStreamSignal
import com.teachme.core.model.SentMessages
import com.teachme.core.model.UnreachableReason
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.plugins.HttpTimeoutConfig
import io.ktor.client.plugins.timeout
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.parameter
import io.ktor.client.request.post
import io.ktor.client.request.prepareGet
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsChannel
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.contentType
import io.ktor.http.isSuccess
import io.ktor.http.path
import io.ktor.utils.io.readLine
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withTimeout
import javax.inject.Inject
import javax.inject.Singleton

/**
 * If nothing at all (not even the `: ping` heartbeat, every 15 s) arrives for this long the connection
 * is considered dead and reported as ended, so the caller reconnects. Not an HTTP read timeout: a quiet
 * but healthy stream is never cut.
 */
internal const val SSE_IDLE_TIMEOUT_MS = 45_000L

/** Raw endpoints of `docs/contracts/chat.md` section 2. */
interface ChatApi {
    suspend fun listMessages(
        topicId: String,
        cursor: String? = null,
        limit: Int? = null,
    ): ApiResult<ChatPage>

    suspend fun sendMessage(
        topicId: String,
        text: String,
        lessonId: String? = null,
    ): ApiResult<SentMessages>

    suspend fun retryMessage(
        topicId: String,
        messageId: String,
    ): ApiResult<ChatMessage>

    fun openEvents(
        topicId: String,
        lastEventId: Long?,
    ): Flow<ChatStreamSignal>
}

@Singleton
class KtorChatApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : ChatApi {
        internal var idleTimeoutMs: Long = SSE_IDLE_TIMEOUT_MS

        override suspend fun listMessages(
            topicId: String,
            cursor: String?,
            limit: Int?,
        ): ApiResult<ChatPage> =
            apiCall({
                client.get {
                    url { path("api", "v1", "topics", topicId, "messages") }
                    cursor?.let { parameter("cursor", it) }
                    limit?.let { parameter("limit", it) }
                }
            }) { response ->
                val dto = response.body<PageDto<ChatMessageDto>>()
                ChatPage(
                    dto.items.mapNotNull { it.toModelOrNull() },
                    dto.nextCursor,
                    response.headers["X-Last-Event-Id"]?.trim()?.toLongOrNull(),
                )
            }

        override suspend fun sendMessage(
            topicId: String,
            text: String,
            lessonId: String?,
        ): ApiResult<SentMessages> =
            apiCall({
                client.post {
                    url { path("api", "v1", "topics", topicId, "messages") }
                    contentType(ContentType.Application.Json)
                    setBody(SendMessageRequest(text, lessonId))
                }
            }) { it.body<SendMessageResponseDto>().toModel() }

        override suspend fun retryMessage(
            topicId: String,
            messageId: String,
        ): ApiResult<ChatMessage> =
            apiCall({
                client.post { url { path("api", "v1", "topics", topicId, "messages", messageId, "retry") } }
            }) { it.body<ChatMessageDto>().toModel() }

        override fun openEvents(
            topicId: String,
            lastEventId: Long?,
        ): Flow<ChatStreamSignal> =
            flow {
                val failure =
                    try {
                        client
                            .prepareGet {
                                url { path("api", "v1", "topics", topicId, "events") }
                                header(HttpHeaders.Accept, "text/event-stream")
                                lastEventId?.let { header("Last-Event-ID", it.toString()) }
                                // api-conventions section 12: the stream has no timeout (REST has 30 s).
                                timeout {
                                    requestTimeoutMillis = HttpTimeoutConfig.INFINITE_TIMEOUT_MS
                                    socketTimeoutMillis = HttpTimeoutConfig.INFINITE_TIMEOUT_MS
                                }
                            }.execute { response ->
                                if (!response.status.isSuccess()) return@execute response.toFailure()
                                emit(ChatStreamSignal.Opened)
                                val channel = response.bodyAsChannel()
                                val parser = SseParser()
                                while (true) {
                                    val line = withTimeout(idleTimeoutMs) { channel.readLine() } ?: break
                                    val frame = parser.feed(line) ?: continue
                                    val event = decodeChatEvent(frame.event, frame.data) ?: continue
                                    emit(ChatStreamSignal.Event(frame.id?.toLongOrNull(), event))
                                }
                                null
                            }
                    } catch (e: TimeoutCancellationException) {
                        ApiFailure.Network(UnreachableReason.Timeout)
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        val reason = e.toUnreachableOrNull()
                        if (reason != null) ApiFailure.Network(reason) else ApiFailure.Unexpected(httpStatus = null)
                    }
                emit(ChatStreamSignal.Ended(failure))
            }
    }
