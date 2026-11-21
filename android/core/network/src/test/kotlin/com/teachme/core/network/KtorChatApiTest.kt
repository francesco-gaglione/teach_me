package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.ChatRole
import com.teachme.core.model.ChatStreamSignal
import com.teachme.core.model.MessagePart
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.TopicState
import com.teachme.core.model.UnreachableReason
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.toByteArray
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import io.ktor.utils.io.ByteChannel
import io.ktor.utils.io.ByteReadChannel
import io.ktor.utils.io.writeStringUtf8
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

class KtorChatApiTest {
    private val json = headersOf(HttpHeaders.ContentType, "application/json")
    private val sse = headersOf(HttpHeaders.ContentType, "text/event-stream")
    private val topicId = "01933b4e-9a1b-7c3d-9e1f-2a3b4c5d6e70"

    private class FakeSettings : SettingsRepository {
        private val state = MutableStateFlow(BackendSettings("http://10.0.2.2:8080", "secret-token"))
        override val settings: Flow<BackendSettings> = state

        override suspend fun current() = state.value

        override suspend fun update(settings: BackendSettings) {
            state.value = settings
        }
    }

    private fun api(
        idleMs: Long = SSE_IDLE_TIMEOUT_MS,
        handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData,
    ) = KtorChatApi(createBackendHttpClient(MockEngine(handler), FakeSettings(), AppVersion("0.1.0"))).also {
        it.idleTimeoutMs = idleMs
    }

    private fun messageJson(
        id: String = "m1",
        role: String = "assistant",
        status: String = "complete",
        parts: String = """[{"type":"text","text":"Ciao"}]""",
        error: String = "null",
        lastEventId: String = "null",
    ) =
        """{"id":"$id","topic_id":"$topicId","role":"$role","status":"$status","parts":$parts,"error":$error,"last_event_id":$lastEventId,
        |"created_at":"2026-11-08T14:36:01.129Z","updated_at":"2026-11-08T14:36:04.877Z","extra":1}
        """.trimMargin()
            .replace("\n", "")

    private fun envelope(
        type: String,
        payload: String,
    ) = """{"type":"$type","topic_id":"$topicId","at":"2026-11-08T14:36:01.129Z","payload":$payload}"""

    private fun frame(
        id: Int,
        type: String,
        payload: String,
    ) = "id: $id\nevent: $type\ndata: ${envelope(type, payload)}\n\n"

    @Test
    fun `list parses messages skips unknown roles and maps unknown parts`() =
        runBlocking {
            val api =
                api { request ->
                    assertEquals("/api/v1/topics/$topicId/messages", request.url.encodedPath)
                    assertEquals("50", request.url.parameters["limit"])
                    assertEquals("Bearer secret-token", request.headers[HttpHeaders.Authorization])
                    respond(
                        """{"items":[
                        |${messageJson(
                            "m3",
                            parts = """[{"type":"mission_proposal","x":1},{"type":"text","text":"B"}]""",
                            lastEventId = "1762612561129042",
                        )},
                        |${messageJson("m2", role = "system")},
                        |${messageJson("m1", role = "user", status = "streaming", parts = "[]")}],
                        |"next_cursor":"abc"}
                        """.trimMargin(),
                        HttpStatusCode.OK,
                        headersOf(
                            HttpHeaders.ContentType to listOf("application/json"),
                            "X-Last-Event-Id" to listOf("1762612561129100"),
                        ),
                    )
                }
            val page = (api.listMessages(topicId, limit = 50) as ApiResult.Success).value
            assertEquals("abc", page.nextCursor)
            assertEquals(1762612561129100L, page.streamPosition)
            assertEquals(1762612561129042L, page.items[0].lastEventId)
            assertNull(page.items[1].lastEventId)
            assertEquals(listOf("m3", "m1"), page.items.map { it.id })
            assertEquals(listOf(MessagePart.Unknown, MessagePart.Text("B")), page.items[0].parts)
            assertEquals(ChatRole.User, page.items[1].role)
            assertEquals(ChatMessageStatus.Streaming, page.items[1].status)
            assertEquals(Instant.parse("2026-11-08T14:36:01.129Z"), page.items[0].createdAt)
        }

    @Test
    fun `list passes the cursor`() =
        runBlocking {
            val api =
                api { request ->
                    assertEquals("c1", request.url.parameters["cursor"])
                    respond("""{"items":[],"next_cursor":null}""", HttpStatusCode.OK, json)
                }
            assertTrue(api.listMessages(topicId, "c1", 50) is ApiResult.Success)
        }

    @Test
    fun `send posts the text and parses the 202`() =
        runBlocking {
            val api =
                api { request ->
                    assertEquals(HttpMethod.Post, request.method)
                    assertEquals("""{"text":"Voglio imparare Rust"}""", String(request.body.toByteArray()))
                    respond(
                        """{"user_message":${messageJson("u1", "user")},
                        |"assistant_message":${messageJson("a1", status = "streaming", parts = "[]")}}
                        """.trimMargin(),
                        HttpStatusCode.Accepted,
                        json,
                    )
                }
            val sent = (api.sendMessage(topicId, "Voglio imparare Rust") as ApiResult.Success).value
            assertEquals("u1", sent.user.id)
            assertEquals(ChatMessageStatus.Streaming, sent.assistant.status)
            assertTrue(sent.assistant.parts.isEmpty())
        }

    @Test
    fun `send adds the lesson id only when there is one`() =
        runBlocking {
            val bodies = mutableListOf<String>()
            val api =
                api { request ->
                    bodies += String(request.body.toByteArray())
                    respond(
                        """{"user_message":${messageJson("u1", "user")},
                        |"assistant_message":${messageJson("a1", status = "streaming", parts = "[]")}}
                        """.trimMargin(),
                        HttpStatusCode.Accepted,
                        json,
                    )
                }
            api.sendMessage(topicId, "Non ho capito", "lesson-1")
            api.sendMessage(topicId, "Domanda libera")
            assertEquals("""{"text":"Non ho capito","lesson_id":"lesson-1"}""", bodies[0])
            assertEquals("""{"text":"Domanda libera"}""", bodies[1])
        }

    @Test
    fun `send maps 409 to conflict and 422 to validation`() =
        runBlocking {
            val conflict =
                api { respond("""{"error":{"code":"conflict","message":"busy"}}""", HttpStatusCode.Conflict, json) }
            assertEquals(ApiResult.Failure(ApiFailure.Conflict), conflict.sendMessage(topicId, "x"))

            val invalid =
                api {
                    respond(
                        """{"error":{"code":"validation_failed","message":"m",
                        |"details":[{"field":"text","issue":"too_long"}]}}
                        """.trimMargin(),
                        HttpStatusCode.UnprocessableEntity,
                        json,
                    )
                }
            val failure = (invalid.sendMessage(topicId, "x") as ApiResult.Failure).failure
            assertTrue((failure as ApiFailure.Validation).hasIssue("text", "too_long"))
        }

    @Test
    fun `retry posts to the retry path`() =
        runBlocking {
            val api =
                api { request ->
                    assertEquals(HttpMethod.Post, request.method)
                    assertEquals("/api/v1/topics/$topicId/messages/a1/retry", request.url.encodedPath)
                    respond(messageJson("a1", status = "streaming", parts = "[]"), HttpStatusCode.Accepted, json)
                }
            val message = (api.retryMessage(topicId, "a1") as ApiResult.Success).value
            assertEquals(ChatMessageStatus.Streaming, message.status)
        }

    @Test
    fun `stream sends the headers and no body request ids`() =
        runBlocking {
            var seen: HttpRequestData? = null
            val api =
                api { request ->
                    seen = request
                    respond(ByteReadChannel(": ping\n\n"), HttpStatusCode.OK, sse)
                }
            val signals = api.openEvents(topicId, 41L).toList()
            assertEquals("text/event-stream", seen!!.headers[HttpHeaders.Accept])
            assertEquals("41", seen.headers["Last-Event-ID"])
            assertEquals("Bearer secret-token", seen.headers[HttpHeaders.Authorization])
            assertEquals(listOf(ChatStreamSignal.Opened, ChatStreamSignal.Ended(null)), signals)
        }

    @Test
    fun `stream without last id sends no Last-Event-ID`() =
        runBlocking {
            var seen: HttpRequestData? = null
            val api =
                api { request ->
                    seen = request
                    respond(ByteReadChannel(""), HttpStatusCode.OK, sse)
                }
            api.openEvents(topicId, null).toList()
            assertNull(seen!!.headers["Last-Event-ID"])
        }

    @Test
    fun `stream parses events delivered in small fragments`() =
        runBlocking {
            val body =
                ": ping\n\n" +
                    frame(
                        1,
                        "message_created",
                        """{"message":${messageJson("a1", status = "streaming", parts = "[]")}}""",
                    ) +
                    frame(2, "message_delta", """{"message_id":"a1","part_index":0,"text":"Perché "}""") +
                    frame(3, "something_new", """{"x":1}""") +
                    frame(4, "message_completed", """{"message":${messageJson("a1")}}""")
            val channel = ByteChannel(autoFlush = true)
            val api = api { respond(channel, HttpStatusCode.OK, sse) }
            val writer =
                launch {
                    body.chunked(7).forEach {
                        channel.writeStringUtf8(it)
                        delay(1)
                    }
                    channel.flushAndClose()
                }
            val signals = api.openEvents(topicId, null).toList()
            writer.join()

            assertEquals(ChatStreamSignal.Opened, signals.first())
            assertEquals(ChatStreamSignal.Ended(null), signals.last())
            val events = signals.filterIsInstance<ChatStreamSignal.Event>()
            assertEquals(listOf<Long?>(1, 2, 4), events.map { it.id })
            assertTrue(events[0].event is ChatEvent.MessageCreated)
            assertEquals(ChatEvent.MessageDelta("a1", 0, "Perché "), events[1].event)
            assertEquals(ChatMessageStatus.Complete, (events[2].event as ChatEvent.MessageCompleted).message.status)
        }

    @Test
    fun `stream decodes topic_updated and resync_required`() =
        runBlocking {
            val topic =
                """{"id":"$topicId","folder_id":"f","title":"Rust","state":"interviewing",
                |"created_at":"2026-11-08T14:36:01.129Z","updated_at":"2026-11-08T14:36:01.129Z"}
                """.trimMargin().replace("\n", "")
            val body = frame(1, "topic_updated", """{"topic":$topic}""") + frame(2, "resync_required", "{}")
            val api = api { respond(ByteReadChannel(body), HttpStatusCode.OK, sse) }
            val events = api.openEvents(topicId, 9).toList().filterIsInstance<ChatStreamSignal.Event>()
            val updated = events[0].event as ChatEvent.TopicUpdated
            assertEquals(TopicState.Interviewing, updated.topic.state)
            assertEquals(ChatEvent.ResyncRequired, events[1].event)
        }

    @Test
    fun `malformed known events are skipped`() =
        runBlocking {
            val body =
                "id: 1\nevent: message_delta\ndata: {not json\n\n" + frame(2, "message_delta", """{"message_id":"a"}""")
            val api = api { respond(ByteReadChannel(body), HttpStatusCode.OK, sse) }
            val events = api.openEvents(topicId, null).toList().filterIsInstance<ChatStreamSignal.Event>()
            assertTrue(events.isEmpty())
        }

    @Test
    fun `stream http errors end with a failure`() =
        runBlocking {
            val notFound =
                api { respond("""{"error":{"code":"not_found","message":"x"}}""", HttpStatusCode.NotFound, json) }
            assertEquals(
                listOf(ChatStreamSignal.Ended(ApiFailure.NotFound)),
                notFound.openEvents(topicId, null).toList(),
            )
            val unauthorized = api { respond("", HttpStatusCode.Unauthorized, json) }
            assertEquals(
                listOf(ChatStreamSignal.Ended(ApiFailure.Unauthorized)),
                unauthorized.openEvents(topicId, null).toList(),
            )
        }

    @Test
    fun `transport errors end with a network failure`() =
        runBlocking {
            val api = api { throw java.io.IOException("down") }
            assertEquals(
                listOf(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other))),
                api.openEvents(topicId, null).toList(),
            )
        }

    @Test
    fun `a silent stream is reported as ended after the idle timeout`() =
        runBlocking {
            val channel = ByteChannel(autoFlush = true)
            val api = api(idleMs = 100) { respond(channel, HttpStatusCode.OK, sse) }
            val signals = api.openEvents(topicId, null).toList()
            assertEquals(
                listOf(ChatStreamSignal.Opened, ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Timeout))),
                signals,
            )
        }
}
