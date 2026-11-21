package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AudioStatus
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
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class KtorAudioApiTest {
    private val json = headersOf(HttpHeaders.ContentType, "application/json")

    private class FakeSettings : SettingsRepository {
        private val state = MutableStateFlow(BackendSettings("http://10.0.2.2:8080", "secret-token"))
        override val settings: Flow<BackendSettings> = state

        override suspend fun current() = state.value

        override suspend fun update(settings: BackendSettings) {
            state.value = settings
        }
    }

    private fun api(
        body: String,
        status: HttpStatusCode = HttpStatusCode.OK,
        seen: MutableList<String> = mutableListOf(),
    ) = KtorAudioApi(
        createBackendHttpClient(
            MockEngine { request ->
                seen +=
                    "${request.method.value} ${request.url.encodedPath} ${request.headers[HttpHeaders.Authorization]}"
                respond(body, status, json)
            },
            FakeSettings(),
            AppVersion("0.1.0"),
        ),
    )

    @Test
    fun manifestMapsStatusesAndClips() =
        runBlocking {
            val seen = mutableListOf<String>()
            val body =
                """{"voice":"vivian","blocks":[
                |{"block_id":"p1","status":"ready","clip":{"url":"/audio/clips/a","voice":"vivian","duration_ms":41230}},
                |{"block_id":"c1","status":"pending","clip":{"url":"/audio/clips/b","voice":"serena","duration_ms":12040}},
                |{"block_id":"p2","status":"pending","clip":null},
                |{"block_id":"d1","status":"failed","clip":null},
                |{"block_id":"x","status":"something_new"}]}
                """.trimMargin()
            val result = api(body, seen = seen).getManifest("L1")
            assertEquals(listOf("GET /api/v1/lessons/L1/audio Bearer secret-token"), seen)
            val manifest = (result as ApiResult.Success).value
            assertEquals("vivian", manifest.voice)
            assertEquals(
                listOf(
                    AudioStatus.Ready,
                    AudioStatus.Pending,
                    AudioStatus.Pending,
                    AudioStatus.Failed,
                    AudioStatus.Pending,
                ),
                manifest.blocks.map { it.status },
            )
            assertEquals("serena", manifest.blocks[1].clip?.voice)
            assertEquals(41230L, manifest.blocks[0].clip?.durationMs)
            assertNull(manifest.blocks[2].clip)
            assertTrue(manifest.hasPending)
        }

    @Test
    fun voicesWhenFeatureIsOff() =
        runBlocking {
            val result = api("""{"available":false,"default_voice":null,"voices":[]}""").getVoices()
            val voices = (result as ApiResult.Success).value
            assertEquals(false, voices.available)
            assertTrue(voices.voices.isEmpty())
        }

    @Test
    fun voicesWhenOn() =
        runBlocking {
            val seen = mutableListOf<String>()
            val body =
                """{"available":true,"default_voice":"vivian","voices":[
                |{"id":"vivian","label":"Vivian","sample_url":"/audio/clips/a"},{"id":"serena","label":"Serena","sample_url":null}]}
                """.trimMargin()
            val voices = (api(body, seen = seen).getVoices() as ApiResult.Success).value
            assertEquals(listOf("GET /api/v1/tts/voices Bearer secret-token"), seen)
            assertTrue(voices.available)
            assertEquals("vivian", voices.defaultVoice)
            assertEquals(listOf("vivian", "serena"), voices.voices.map { it.id })
            assertEquals(listOf("/audio/clips/a", null), voices.voices.map { it.sampleUrl })
        }

    @Test
    fun notFoundIsAFailure() =
        runBlocking {
            val result =
                api(
                    """{"error":{"code":"not_found","message":"x"}}""",
                    HttpStatusCode.NotFound,
                ).getManifest("L1")
            assertTrue((result as ApiResult.Failure).failure is ApiFailure.NotFound)
        }
}
