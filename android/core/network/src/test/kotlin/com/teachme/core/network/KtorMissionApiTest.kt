package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.MessagePart
import com.teachme.core.model.MissionStatus
import com.teachme.core.model.PreferenceScope
import com.teachme.core.model.SettingsRepository
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
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class KtorMissionApiTest {
    private val json = headersOf(HttpHeaders.ContentType, "application/json")
    private val topicId = "01933b4e-9a1b-7c3d-9e1f-2a3b4c5d6e70"

    private class FakeSettings : SettingsRepository {
        private val state = MutableStateFlow(BackendSettings("http://10.0.2.2:8080", "secret-token"))
        override val settings: Flow<BackendSettings> = state

        override suspend fun current() = state.value

        override suspend fun update(settings: BackendSettings) {
            state.value = settings
        }
    }

    private fun api(handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData) =
        KtorMissionApi(createBackendHttpClient(MockEngine(handler), FakeSettings(), AppVersion("0.1.0")))

    private fun missionJson(
        version: Int = 2,
        status: String = "draft",
        confirmedAt: String = "null",
        startingPoint: String? = null,
    ) = """{"topic_id":"$topicId","version":$version,"status":"$status","why":"Per lavoro",${startingPoint ?: ""}
        |"success_criteria":["Leggo un servizio axum","Scrivo test"],"constraints":["30 minuti"],"out_of_scope":[],
        |"created_at":"2026-11-08T14:36:01.129Z","confirmed_at":$confirmedAt,"superseded_at":null,"extra":true}
        """.trimMargin().replace("\n", "")

    @Test
    fun `get mission maps current and draft`() =
        runBlocking {
            var path = ""
            val result =
                api {
                    path = it.url.encodedPath
                    respond(
                        """{"current":${missionJson(
                            1,
                            "confirmed",
                            "\"2026-11-03T10:00:00Z\"",
                        )},"draft":${missionJson()}}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }.getMission(topicId)
            assertEquals("/api/v1/topics/$topicId/mission", path)
            val value = (result as ApiResult.Success).value
            assertEquals(MissionStatus.Confirmed, value.current?.status)
            assertEquals(2, value.draft?.version)
            assertEquals(listOf("Leggo un servizio axum", "Scrivo test"), value.draft?.successCriteria)
            assertTrue(value.current?.confirmedAt != null)
        }

    @Test
    fun `empty mission has no current and no draft`() =
        runBlocking {
            val result =
                api {
                    respond(
                        """{"current":null,"draft":null}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }.getMission(topicId)
            val value = (result as ApiResult.Success).value
            assertNull(value.current)
            assertNull(value.draft)
        }

    @Test
    fun `confirm posts the version and 409 is a conflict`() =
        runBlocking {
            var body = ""
            var method: HttpMethod? = null
            val ok =
                api {
                    body = it.body.toByteArray().decodeToString()
                    method = it.method
                    respond("""{"current":${missionJson(2, "confirmed")},"draft":null}""", HttpStatusCode.OK, json)
                }.confirm(topicId, 2)
            assertEquals(HttpMethod.Post, method)
            assertEquals("""{"version":2}""", body)
            assertNull((ok as ApiResult.Success).value.draft)

            val conflict =
                api {
                    respond(
                        """{"error":{"code":"conflict","message":"x"}}""",
                        HttpStatusCode.Conflict,
                        json,
                    )
                }.confirm(topicId, 2)
            assertEquals(ApiFailure.Conflict, (conflict as ApiResult.Failure).failure)
        }

    @Test
    fun `versions are a page`() =
        runBlocking {
            val result =
                api {
                    respond(
                        """{"items":[${missionJson(
                            2,
                            "superseded",
                        )},${missionJson(1, "confirmed")}],"next_cursor":"c"}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }.listVersions(topicId)
            val page = (result as ApiResult.Success).value
            assertEquals(listOf(2, 1), page.items.map { it.version })
            assertEquals("c", page.nextCursor)
        }

    @Test
    fun `preferences send the topic filter and skip unknown scopes`() =
        runBlocking {
            var query = ""
            val result =
                api {
                    query = it.url.encodedQuery
                    respond(
                        """{"items":[
                        |{"id":"p1","scope":"topic","topic_id":"$topicId","text":"Python","created_at":"2026-11-08T14:36:01Z"},
                        |{"id":"p2","scope":"global","topic_id":null,"text":"Niente sport","created_at":"2026-11-08T14:37:01Z"},
                        |{"id":"p3","scope":"galaxy","topic_id":null,"text":"?","created_at":"2026-11-08T14:37:01Z"}
                        |],"next_cursor":null}
                        """.trimMargin().replace("\n", ""),
                        HttpStatusCode.OK,
                        json,
                    )
                }.listPreferences(topicId)
            assertEquals("topic_id=$topicId", query)
            val items = (result as ApiResult.Success).value.items
            assertEquals(listOf("p1", "p2"), items.map { it.id })
            assertEquals(PreferenceScope.Global, items[1].scope)
        }

    @Test
    fun `delete preference maps 204 and 404`() =
        runBlocking {
            var method: HttpMethod? = null
            val ok =
                api {
                    method = it.method
                    respond("", HttpStatusCode.NoContent)
                }.deletePreference("p1")
            assertEquals(HttpMethod.Delete, method)
            assertTrue(ok is ApiResult.Success)
            val gone =
                api { respond("""{"error":{"code":"not_found","message":""}}""", HttpStatusCode.NotFound, json) }
                    .deletePreference("p1")
            assertEquals(ApiFailure.NotFound, (gone as ApiResult.Failure).failure)
        }

    private fun parts(raw: String): List<MessagePart> =
        Json.parseToJsonElement(raw).let { (it as kotlinx.serialization.json.JsonArray).map(JsonElement::toPart) }

    @Test
    fun `mission proposal part is decoded and broken ones are unknown`() {
        val result =
            parts(
                """[{"type":"text","text":"ok"},
                |{"type":"mission_proposal","mission":${missionJson()},"replaces_version":1},
                |{"type":"mission_proposal","mission":{"nope":1}},
                |{"type":"hologram"}]
                """.trimMargin().replace("\n", ""),
            )
        val proposal = result[1] as MessagePart.MissionProposal
        assertEquals(2, proposal.mission.version)
        assertEquals(1, proposal.replacesVersion)
        assertEquals(MessagePart.Unknown, result[2])
        assertEquals(MessagePart.Unknown, result[3])
    }

    private fun startingPointOf(raw: String): String? =
        (parts("[{\"type\":\"mission_proposal\",\"mission\":$raw}]").single() as MessagePart.MissionProposal)
            .mission.startingPoint

    @Test
    fun `starting point is read when present, null, absent or blank`() {
        assertEquals(
            "Programmo in Kotlin",
            startingPointOf(missionJson(startingPoint = "\"starting_point\":\"Programmo in Kotlin\",")),
        )
        assertNull(startingPointOf(missionJson(startingPoint = "\"starting_point\":null,")))
        // Snapshots saved before contract v1.2 do not have the key at all.
        assertNull(startingPointOf(missionJson()))
        assertNull(startingPointOf(missionJson(startingPoint = "\"starting_point\":\"  \\n \",")))
    }

    @Test
    fun `starting point is read from the REST mission`() =
        runBlocking {
            val result =
                api {
                    respond(
                        """{"current":${missionJson(
                            1,
                            "confirmed",
                            "\"2026-11-03T10:00:00Z\"",
                            "\"starting_point\":\"Parto da zero\",",
                        )},"draft":${missionJson()}}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }.getMission(topicId)
            val value = (result as ApiResult.Success).value
            assertEquals("Parto da zero", value.current?.startingPoint)
            assertNull(value.draft?.startingPoint)
        }

    @Test
    fun `proposal without replaces_version has null`() {
        val proposal =
            parts("""[{"type":"mission_proposal","mission":${missionJson()},"replaces_version":null}]""").single()
        assertNull((proposal as MessagePart.MissionProposal).replacesVersion)
    }

    private fun envelope(
        type: String,
        payload: String,
    ) = """{"type":"$type","topic_id":"$topicId","at":"2026-11-08T14:36:01.129Z","payload":$payload}"""

    @Test
    fun `mission and preference events are decoded`() {
        val proposed =
            decodeChatEvent("mission_proposed", envelope("mission_proposed", """{"mission":${missionJson()}}"""))
        assertEquals(2, (proposed as ChatEvent.MissionProposed).mission.version)
        val confirmed =
            decodeChatEvent(
                "mission_confirmed",
                envelope("mission_confirmed", """{"mission":${missionJson(2, "confirmed")}}"""),
            )
        assertEquals(MissionStatus.Confirmed, (confirmed as ChatEvent.MissionConfirmed).mission.status)
        val saved =
            decodeChatEvent(
                "preference_saved",
                envelope(
                    "preference_saved",
                    """{"preference":{"id":"p1","scope":"global","topic_id":null,"text":"x","created_at":"2026-11-08T14:36:01Z"}}""",
                ),
            )
        assertEquals("p1", (saved as ChatEvent.PreferenceSaved).preference.id)
        val deleted =
            decodeChatEvent(
                "preference_deleted",
                envelope("preference_deleted", """{"preference_id":"p1","scope":"topic","topic_id":"$topicId"}"""),
            )
        assertEquals(
            ChatEvent.PreferenceDeleted("p1", PreferenceScope.Topic, topicId),
            deleted,
        )
    }

    @Test
    fun `mission events carry the starting point, null when absent`() {
        val with =
            decodeChatEvent(
                "mission_confirmed",
                envelope(
                    "mission_confirmed",
                    """{"mission":${missionJson(
                        2,
                        "confirmed",
                        startingPoint = "\"starting_point\":\"Parto da zero\",",
                    )}}""",
                ),
            )
        assertEquals("Parto da zero", (with as ChatEvent.MissionConfirmed).mission.startingPoint)
        val without =
            decodeChatEvent("mission_proposed", envelope("mission_proposed", """{"mission":${missionJson()}}"""))
        assertNull((without as ChatEvent.MissionProposed).mission.startingPoint)
    }

    @Test
    fun `malformed or unknown mission events are ignored`() {
        assertNull(decodeChatEvent("mission_proposed", envelope("mission_proposed", """{"mission":{"x":1}}""")))
        assertNull(decodeChatEvent("mission_archived", envelope("mission_archived", "{}")))
        assertNull(
            decodeChatEvent(
                "preference_deleted",
                envelope("preference_deleted", """{"preference_id":"p","scope":"?"}"""),
            ),
        )
    }

    @Test
    fun `message with a proposal part keeps its status`() {
        val dto =
            ApiJson.decodeFromString<ChatMessageDto>(
                """{"id":"m1","topic_id":"$topicId","role":"assistant","status":"complete",
                |"parts":[{"type":"text","text":"Ecco"},{"type":"mission_proposal","mission":${missionJson()},"replaces_version":null}],
                |"created_at":"2026-11-08T14:36:01.129Z","updated_at":"2026-11-08T14:36:04.877Z"}
                """.trimMargin().replace("\n", ""),
            )
        val message = dto.toModel()
        assertEquals(ChatMessageStatus.Complete, message.status)
        assertEquals(2, message.missionProposal?.mission?.version)
    }
}
