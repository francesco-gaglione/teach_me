package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.JobStatus
import com.teachme.core.model.ResourceKind
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.SourceType
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class KtorResourcesApiTest {
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
        KtorResourcesApi(createBackendHttpClient(MockEngine(handler), FakeSettings(), AppVersion("0.1.0")))

    private fun jobJson(
        status: String = "running",
        progress: String = """{"step":"searching","message":"Ho cercato: axum","done":null,"total":null}""",
        error: String = "null",
    ) = """{"id":"job1","topic_id":"$topicId","kind":"research","status":"$status","progress":$progress,
        |"error":$error,"result":null,"attempt":1,"created_at":"2026-11-09T14:36:01.129Z",
        |"started_at":"2026-11-09T14:36:01.402Z","finished_at":null,"future_field":1}
        """.trimMargin().replace("\n", "")

    private fun resourceJson(
        id: String,
        kind: String = "knowledge",
        sourceType: String = "documentation",
        url: String? = "https://doc.rust-lang.org/book/",
    ) = """{"id":"$id","kind":"$kind","source_type":"$sourceType","title":"The Rust Book",
        |"url":${url?.let { "\"$it\"" }},"domain":"doc.rust-lang.org","language":"en",
        |"annotation":"Il libro ufficiale.","use_for":"capire ownership","created_at":"2026-11-09T14:40:12.003Z"}
        """.trimMargin().replace("\n", "")

    @Test
    fun `get resources maps sources gaps research and job`() =
        runBlocking {
            var path = ""
            val body =
                """{"resources":[${resourceJson("r1")},${resourceJson("r2", "community", "social_group", null)}],
                |"gaps":[{"area":"Migrazioni","note":"Nessuna guida."}],"communities_skipped":false,
                |"research":{"job_id":"job1","mission_version":3,"finished_at":"2026-11-09T14:40:12.003Z"},
                |"job":${jobJson("succeeded", "null")}}
                """.trimMargin().replace("\n", "")
            val result =
                api {
                    path = it.url.encodedPath
                    respond(body, HttpStatusCode.OK, json)
                }.getResources(topicId)
            assertEquals("/api/v1/topics/$topicId/resources", path)
            val view = (result as ApiResult.Success).value
            assertEquals(2, view.resources.size)
            assertEquals(ResourceKind.Knowledge, view.knowledge.single().kind)
            assertEquals(SourceType.Documentation, view.knowledge.single().sourceType)
            assertNull(view.community.single().url)
            assertEquals(SourceType.SocialGroup, view.community.single().sourceType)
            assertEquals("Migrazioni", view.gaps.single().area)
            assertEquals(3, view.research?.missionVersion)
            assertEquals(JobStatus.Succeeded, view.job?.status)
        }

    @Test
    fun `empty view without research or job`() =
        runBlocking {
            val result =
                api {
                    respond(
                        """{"resources":[],"gaps":[],"communities_skipped":true,"research":null,"job":null}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }.getResources(topicId)
            val view = (result as ApiResult.Success).value
            assertTrue(view.resources.isEmpty())
            assertTrue(view.communitiesSkipped)
            assertNull(view.research)
            assertNull(view.job)
        }

    @Test
    fun `unknown source type and status never crash and unknown kinds are read as knowledge`() =
        runBlocking {
            val result =
                api {
                    respond(
                        """{"resources":[${resourceJson("r1", sourceType = "hologram")},
                        |${resourceJson("r2", kind = "wisdom2")}],"gaps":[],"communities_skipped":false,
                        |"research":null,"job":${jobJson("paused")}}
                        """.trimMargin().replace("\n", ""),
                        HttpStatusCode.OK,
                        json,
                    )
                }.getResources(topicId)
            val view = (result as ApiResult.Success).value
            assertEquals(2, view.resources.size)
            assertEquals(SourceType.Unknown, view.resources.first().sourceType)
            assertEquals(ResourceKind.Knowledge, view.resources.last().kind)
            assertEquals(JobStatus.Unknown, view.job?.status)
        }

    @Test
    fun `document sources stay out of the web list and keep their document id`() =
        runBlocking {
            val pdf =
                """{"id":"r9","kind":"document","source_type":"pdf","title":"Rust in Action","url":null,
                |"domain":null,"language":"en","annotation":"PDF caricato da te, 456 pagine","use_for":"seguire il libro",
                |"document_id":"d1","created_at":"2026-11-09T14:40:12.003Z"}
                """.trimMargin().replace("\n", "")
            val result =
                api {
                    respond(
                        """{"resources":[$pdf,${resourceJson("r1")}],"gaps":[],"communities_skipped":false,
                        |"research":null,"job":null}
                        """.trimMargin().replace("\n", ""),
                        HttpStatusCode.OK,
                        json,
                    )
                }.getResources(topicId)
            val view = (result as ApiResult.Success).value
            assertEquals(listOf("r1"), view.resources.map { it.id })
            val document = view.documentResources.single()
            assertEquals(ResourceKind.Document, document.kind)
            assertEquals(SourceType.Pdf, document.sourceType)
            assertEquals("d1", document.documentId)
            assertNull(document.url)
            assertNull(document.domain)
        }

    @Test
    fun `a document import job result carries the document id and counts`() =
        runBlocking {
            val body =
                """{"id":"j1","topic_id":"$topicId","kind":"document_import","document_id":"d1","status":"succeeded",
                |"progress":{"step":"saving","message":null,"done":null,"total":null},"error":null,
                |"result":{"document_id":"d1","page_count":456,"section_count":31},"attempt":1,
                |"created_at":"2026-11-09T14:36:01.129Z","started_at":null,"finished_at":null}
                """.trimMargin().replace("\n", "")
            val job =
                (
                    api {
                        respond(
                            body,
                            HttpStatusCode.Accepted,
                            json,
                        )
                    }.startResearch(topicId) as ApiResult.Success
                ).value
            assertTrue(job.isDocumentImport)
            assertEquals("d1", job.documentId)
            assertEquals("d1", job.result?.documentId)
            assertEquals(456, job.result?.pageCount)
            assertEquals(31, job.result?.sectionCount)
        }

    @Test
    fun `start research posts and returns the queued job, 409 is a conflict`() =
        runBlocking {
            var method: HttpMethod? = null
            var path = ""
            val ok =
                api {
                    method = it.method
                    path = it.url.encodedPath
                    respond(jobJson("queued", "null"), HttpStatusCode.Accepted, json)
                }.startResearch(topicId)
            assertEquals(HttpMethod.Post, method)
            assertEquals("/api/v1/topics/$topicId/resources/research", path)
            assertEquals(JobStatus.Queued, (ok as ApiResult.Success).value.status)

            val conflict =
                api { respond("""{"error":{"code":"conflict","message":"x"}}""", HttpStatusCode.Conflict, json) }
                    .startResearch(topicId)
            assertEquals(ApiFailure.Conflict, (conflict as ApiResult.Failure).failure)
        }

    @Test
    fun `retry posts to the job and maps the failed error`() =
        runBlocking {
            var path = ""
            val ok =
                api {
                    path = it.url.encodedPath
                    respond(jobJson("queued", "null"), HttpStatusCode.Accepted, json)
                }.retryJob(topicId, "job1")
            assertEquals("/api/v1/topics/$topicId/jobs/job1/retry", path)
            assertEquals("job1", (ok as ApiResult.Success).value.id)

            val failed =
                api {
                    respond(
                        jobJson("failed", "null", """{"code":"timeout","message":"Troppo tempo."}"""),
                        HttpStatusCode.OK,
                        json,
                    )
                }.retryJob(topicId, "job1")
            assertEquals("timeout", (failed as ApiResult.Success).value.error?.code)

            val gone =
                api { respond("""{"error":{"code":"not_found","message":"x"}}""", HttpStatusCode.NotFound, json) }
                    .retryJob(topicId, "job1")
            assertEquals(ApiFailure.NotFound, (gone as ApiResult.Failure).failure)
        }

    private fun envelope(
        type: String,
        payload: String,
    ) = """{"type":"$type","topic_id":"$topicId","at":"2026-11-09T14:36:01.129Z","payload":$payload}"""

    @Test
    fun `job_updated event is decoded and a broken payload is skipped`() {
        val event = decodeChatEvent("job_updated", envelope("job_updated", """{"job":${jobJson()}}"""))
        val job = (event as ChatEvent.JobUpdated).job
        assertEquals("job1", job.id)
        assertEquals("searching", job.progress?.step)
        assertEquals(JobStatus.Running, job.status)

        assertNull(decodeChatEvent("job_updated", envelope("job_updated", """{"job":{"id":1}}""")))
    }
}
