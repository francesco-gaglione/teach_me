package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.SettingsRepository
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class KtorComponentRequestsApiTest {
    private val json = headersOf(HttpHeaders.ContentType, "application/json")

    private class FakeSettings : SettingsRepository {
        private val state = MutableStateFlow(BackendSettings("http://100.64.0.1:8080", "secret-token"))
        override val settings: Flow<BackendSettings> = state

        override suspend fun current() = state.value

        override suspend fun update(settings: BackendSettings) {
            state.value = settings
        }
    }

    private fun api(
        status: HttpStatusCode,
        body: String,
        seen: MutableList<String> = mutableListOf(),
    ) = KtorComponentRequestsApi(
        createBackendHttpClient(
            MockEngine {
                seen += it.url.toString()
                respond(body, status, json)
            },
            FakeSettings(),
            AppVersion("0.1.0"),
        ),
    )

    @Test
    fun `parses a page, ignoring unknown fields and keeping the cursor`() =
        runBlocking {
            val seen = mutableListOf<String>()
            val body =
                """{"items":[{"wanted":"interactive_timeline","count":4,
                |"first_requested_at":"2026-11-09T15:02:11.420Z","last_requested_at":"2026-11-12T08:40:01.003Z",
                |"catalog_version":1,"extra":true,"examples":[{"purpose":"Ordine degli eventi",
                |"lesson_id":"l1","lesson_title":"Ownership e move","topic_title":"Rust",
                |"requested_at":"2026-11-12T08:40:01.003Z"}]}],"next_cursor":"abc"}
                """.trimMargin().replace("\n", "")
            val result = api(HttpStatusCode.OK, body, seen).list("c0")
            val page = (result as ApiResult.Success).value
            assertEquals("abc", page.nextCursor)
            val group = page.items.single()
            assertEquals("interactive_timeline", group.wanted)
            assertEquals(4, group.count)
            assertEquals("Ownership e move", group.examples.single().lessonTitle)
            assertTrue(group.firstRequestedAt!! < group.lastRequestedAt!!)
            assertTrue(seen.single().contains("/api/v1/component-requests") && seen.single().contains("cursor=c0"))
        }

    @Test
    fun `maps 401 to unauthorized`() =
        runBlocking {
            val result =
                api(
                    HttpStatusCode.Unauthorized,
                    """{"error":{"code":"unauthorized","message":"x"}}""",
                ).list(null)
            assertEquals(ApiFailure.Unauthorized, (result as ApiResult.Failure).failure)
        }
}
