package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.TopicLanguage
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
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException
import java.net.UnknownHostException
import java.time.Instant

class KtorFolderTopicApiTest {
    private val json = headersOf(HttpHeaders.ContentType, "application/json")
    private val folderId = "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f"
    private val topicId = "01933b4e-9a1b-7c3d-9e1f-2a3b4c5d6e70"

    private class FakeSettings : SettingsRepository {
        private val state = MutableStateFlow(BackendSettings("http://10.0.2.2:8080", "secret-token"))
        override val settings: Flow<BackendSettings> = state

        override suspend fun current() = state.value

        override suspend fun update(settings: BackendSettings) {
            state.value = settings
        }
    }

    private fun client(handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData) =
        createBackendHttpClient(MockEngine(handler), FakeSettings(), AppVersion("0.1.0"))

    private fun folders(handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData) =
        KtorFolderApi(client(handler))

    private fun topics(handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData) =
        KtorTopicApi(client(handler))

    private fun folderJson(
        id: String = folderId,
        name: String = "Lavoro",
    ) = """{"id":"$id","name":"$name","position":0,"topic_count":3,
        |"created_at":"2026-11-08T14:36:01.129Z","updated_at":"2026-11-08T14:36:02.000Z","extra":1}
        """.trimMargin()

    private fun topicJson(
        id: String = topicId,
        state: String? = "\"new\"",
    ) = """{"id":"$id","folder_id":"$folderId","title":"Rust","state":$state,
        |"created_at":"2026-11-08T14:40:12.003Z","updated_at":"2026-11-08T14:40:12.003+01:00"}
        """.trimMargin()

    private fun error(
        code: String,
        details: String = "null",
    ) = """{"error":{"code":"$code","message":"m","details":$details,"request_id":"r1"}}"""

    private fun <T> ApiResult<T>.success(): T = (this as ApiResult.Success).value

    private fun ApiResult<*>.failure(): ApiFailure = (this as ApiResult.Failure).failure

    @Test
    fun `folder is parsed and unknown fields ignored`() =
        runTest {
            var seen: HttpRequestData? = null
            val api =
                folders {
                    seen = it
                    respond(folderJson(), HttpStatusCode.OK, json)
                }

            val folder = api.getFolder(folderId).success()

            assertEquals("Lavoro", folder.name)
            assertEquals(3, folder.topicCount)
            assertEquals(Instant.parse("2026-11-08T14:36:01.129Z"), folder.createdAt)
            assertEquals("http://10.0.2.2:8080/api/v1/folders/$folderId", seen!!.url.toString())
            assertEquals("Bearer secret-token", seen.headers[HttpHeaders.Authorization])
        }

    @Test
    fun `topic parses state, offset dates`() =
        runTest {
            val topic = topics { respond(topicJson(), HttpStatusCode.OK, json) }.getTopic(topicId).success()

            assertEquals(TopicState.New, topic.state)
            assertEquals(folderId, topic.folderId)
            assertEquals(Instant.parse("2026-11-08T13:40:12.003Z"), topic.updatedAt)
        }

    @Test
    fun `topic language is read and a missing one is Italian`() =
        runTest {
            val en =
                topics {
                    respond(
                        topicJson().replace("\"state\"", "\"language\":\"en\",\"state\""),
                        HttpStatusCode.OK,
                        json,
                    )
                }
            assertEquals(TopicLanguage.En, en.getTopic(topicId).success().language)
            assertEquals(
                TopicLanguage.It,
                topics { respond(topicJson(), HttpStatusCode.OK, json) }.getTopic(topicId).success().language,
            )
            val fr =
                topics {
                    respond(topicJson().replace("\"state\"", "\"language\":\"fr\",\"state\""), HttpStatusCode.OK, json)
                }
            assertEquals(TopicLanguage.Unknown, fr.getTopic(topicId).success().language)
        }

    @Test
    fun `unknown and null topic state map to Unknown`() =
        runTest {
            val unknown = topics { respond(topicJson(state = "\"archived\""), HttpStatusCode.OK, json) }
            val nullState = topics { respond(topicJson(state = "null"), HttpStatusCode.OK, json) }

            assertEquals(TopicState.Unknown, unknown.getTopic(topicId).success().state)
            assertEquals(TopicState.Unknown, nullState.getTopic(topicId).success().state)
        }

    @Test
    fun `all known states are parsed`() {
        val expected =
            mapOf(
                "new" to TopicState.New,
                "interviewing" to TopicState.Interviewing,
                "mission_proposed" to TopicState.MissionProposed,
                "researching" to TopicState.Researching,
                "ready" to TopicState.Ready,
            )
        for ((wire, state) in expected) assertEquals(state, TopicState.fromWire(wire))
    }

    @Test
    fun `list sends cursor and limit and reads explicit null cursor`() =
        runTest {
            var seen: HttpRequestData? = null
            val api =
                folders {
                    seen = it
                    respond("""{"items":[${folderJson()}],"next_cursor":"abc"}""", HttpStatusCode.OK, json)
                }

            val page = api.listFolders(cursor = "prev", limit = 200).success()

            assertEquals("abc", page.nextCursor)
            assertEquals(1, page.items.size)
            assertEquals("prev", seen!!.url.parameters["cursor"])
            assertEquals("200", seen.url.parameters["limit"])

            val last = folders { respond("""{"items":[],"next_cursor":null}""", HttpStatusCode.OK, json) }
            assertNull(last.listFolders().success().nextCursor)
        }

    @Test
    fun `topic list uses the folder path`() =
        runTest {
            var seen: HttpRequestData? = null
            val api =
                topics {
                    seen = it
                    respond("""{"items":[${topicJson()}],"next_cursor":null}""", HttpStatusCode.OK, json)
                }

            assertEquals(
                1,
                api
                    .listTopics(folderId)
                    .success()
                    .items.size,
            )
            assertEquals("/api/v1/folders/$folderId/topics", seen!!.url.encodedPath)
        }

    @Test
    fun `create and patch send the documented bodies`() =
        runTest {
            val bodies = mutableListOf<Pair<HttpMethod, String>>()
            val api =
                topics {
                    bodies += it.method to it.body.toByteArray().decodeToString()
                    respond(topicJson(), HttpStatusCode.OK, json)
                }

            api.createTopic(folderId, "Rust")
            api.createTopic(folderId, "Rust", TopicLanguage.En)
            api.updateTopic(topicId, title = "Rust avanzato")
            api.updateTopic(topicId, folderId = folderId)
            api.updateTopic(topicId)

            assertEquals(HttpMethod.Post to """{"title":"Rust","language":"it"}""", bodies[0])
            assertEquals(HttpMethod.Post to """{"title":"Rust","language":"en"}""", bodies[1])
            assertEquals(HttpMethod.Patch to """{"title":"Rust avanzato"}""", bodies[2])
            assertEquals(HttpMethod.Patch to """{"folder_id":"$folderId"}""", bodies[3])
            assertEquals(HttpMethod.Patch to "{}", bodies[4])
        }

    @Test
    fun `create folder accepts 201 and delete accepts 204`() =
        runTest {
            val created = folders { respond(folderJson(), HttpStatusCode.Created, json) }.createFolder("Lavoro")
            assertEquals("Lavoro", created.success().name)

            val deleted = folders { respond("", HttpStatusCode.NoContent) }.deleteFolder(folderId)
            assertEquals(ApiResult.Success(Unit), deleted)
        }

    @Test
    fun `422 validation_failed carries field and issue details`() =
        runTest {
            val api =
                folders {
                    respond(
                        error(
                            "validation_failed",
                            """[{"field":"name","issue":"duplicate"},{"field":"name","issue":"too_long"}]""",
                        ),
                        HttpStatusCode.UnprocessableEntity,
                        json,
                    )
                }

            val failure = api.createFolder("Lavoro").failure() as ApiFailure.Validation

            assertEquals(2, failure.details.size)
            assertTrue(failure.hasIssue("name", "duplicate"))
            assertTrue(failure.hasIssue("name", "too_long"))
        }

    @Test
    fun `422 without details gives an empty list`() =
        runTest {
            val api = topics { respond(error("validation_failed"), HttpStatusCode.UnprocessableEntity, json) }

            val failure = api.createTopic(folderId, "x").failure() as ApiFailure.Validation

            assertTrue(failure.details.isEmpty())
        }

    @Test
    fun `400 invalid_id and 404 not_found and 401`() =
        runTest {
            val invalid = folders { respond(error("invalid_id"), HttpStatusCode.BadRequest, json) }
            val missing = folders { respond(error("not_found"), HttpStatusCode.NotFound, json) }
            val unauthorized = folders { respond(error("unauthorized"), HttpStatusCode.Unauthorized, json) }
            val bare404 = folders { respond("not json", HttpStatusCode.NotFound) }

            assertEquals(ApiFailure.InvalidId, invalid.getFolder("nope").failure())
            assertEquals(ApiFailure.NotFound, missing.getFolder(folderId).failure())
            assertEquals(ApiFailure.Unauthorized, unauthorized.listFolders().failure())
            assertEquals(ApiFailure.NotFound, bare404.deleteFolder(folderId).failure())
        }

    @Test
    fun `400 bad_request and 500 are distinguished`() =
        runTest {
            val bad = folders { respond(error("bad_request"), HttpStatusCode.BadRequest, json) }
            val boom = folders { respond(error("internal"), HttpStatusCode.InternalServerError, json) }

            assertEquals(ApiFailure.BadRequest("m"), bad.listFolders(limit = 9999).failure())
            val unexpected = boom.listFolders().failure() as ApiFailure.Unexpected
            assertEquals(500, unexpected.httpStatus)
            assertEquals("internal", unexpected.error?.code)
        }

    @Test
    fun `transport errors map to network failures`() =
        runTest {
            assertEquals(
                ApiFailure.Network(UnreachableReason.UnknownHost),
                folders { throw UnknownHostException("x") }.listFolders().failure(),
            )
            assertEquals(
                ApiFailure.Network(UnreachableReason.Other),
                folders { throw IOException("offline") }.listFolders().failure(),
            )
        }

    @Test
    fun `malformed success body is unexpected, not a crash`() =
        runTest {
            val failure = folders { respond("""{"id":1}""", HttpStatusCode.OK, json) }.getFolder(folderId).failure()

            assertEquals(ApiFailure.Unexpected(200), failure)
        }
}
