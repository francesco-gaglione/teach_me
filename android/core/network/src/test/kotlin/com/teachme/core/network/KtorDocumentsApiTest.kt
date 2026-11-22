package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.DocumentStatus
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.UploadSource
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.toByteArray
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import io.ktor.http.headersOf
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.IOException
import java.io.InputStream

class KtorDocumentsApiTest {
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
        KtorDocumentsApi(createBackendHttpClient(MockEngine(handler), FakeSettings(), AppVersion("0.1.0")))

    private class BytesSource(
        private val bytes: ByteArray,
        override val displayName: String = "libro.pdf",
        override val sizeBytes: Long? = bytes.size.toLong(),
    ) : UploadSource {
        var opened = 0

        override fun open(): InputStream {
            opened++
            return ByteArrayInputStream(bytes)
        }
    }

    private fun documentJson(
        status: String = "ready",
        error: String = "null",
    ) = """{"id":"d1","topic_id":"$topicId","resource_id":"r1","filename":"rust-in-action.pdf",
        |"title":"Rust in Action","status":"$status","error":$error,"size_bytes":8421337,"page_count":456,
        |"language":"en","sections":[{"id":"s1","title":"1 Introducing Rust","level":1,"page_start":1,"page_end":24},
        |{"id":"s2","title":"1.1 Why Rust","level":2,"page_start":3,"page_end":9,"future":1}],
        |"created_at":"2026-11-09T18:02:11.420Z","future_field":true}
        """.trimMargin().replace("\n", "")

    private fun jobJson() =
        """{"id":"j1","topic_id":"$topicId","kind":"document_import","status":"queued","progress":null,
        |"error":null,"result":null,"attempt":0,"created_at":"2026-11-09T18:02:11.420Z",
        |"started_at":null,"finished_at":null}
        """.trimMargin().replace("\n", "")

    @Test
    fun `list maps documents with sections and tolerates unknown fields`() =
        runBlocking {
            var path = ""
            val result =
                api {
                    path = it.url.encodedPath
                    respond("""{"documents":[${documentJson()}]}""", HttpStatusCode.OK, json)
                }.list(topicId)
            assertEquals("/api/v1/topics/$topicId/documents", path)
            val document = (result as ApiResult.Success).value.single()
            assertEquals(DocumentStatus.Ready, document.status)
            assertEquals("Rust in Action", document.title)
            assertEquals(456, document.pageCount)
            assertEquals("en", document.language)
            assertEquals("r1", document.resourceId)
            assertEquals(2, document.sections.size)
            assertEquals(2, document.sections[1].level)
            assertEquals(3, document.sections[1].pageStart)
            assertEquals(9, document.sections[1].pageEnd)
            assertNull(document.error)
        }

    @Test
    fun `a failed document keeps its error code and an unknown status reads as importing`() =
        runBlocking {
            val failed = documentJson("failed", """{"code":"no_text","message":"Il PDF non contiene testo."}""")
            val weird = documentJson("paused").replace("\"id\":\"d1\"", "\"id\":\"d2\"")
            val result =
                api { respond("""{"documents":[$failed,$weird]}""", HttpStatusCode.OK, json) }.list(topicId)
            val documents = (result as ApiResult.Success).value
            assertEquals(DocumentStatus.Failed, documents[0].status)
            assertEquals("no_text", documents[0].error?.code)
            assertEquals(DocumentStatus.Importing, documents[1].status)
        }

    @Test
    fun `a document without optional fields does not break the list`() =
        runBlocking {
            val minimal = """{"id":"d9","filename":"x.pdf","title":"","status":"importing"}"""
            val result = api { respond("""{"documents":[$minimal]}""", HttpStatusCode.OK, json) }.list(topicId)
            val document = (result as ApiResult.Success).value.single()
            assertNull(document.pageCount)
            assertNull(document.resourceId)
            assertEquals(emptyList<Any>(), document.sections)
            assertEquals("x", document.title)
        }

    @Test
    fun `upload sends the file as the multipart field file and returns document and job`() =
        runBlocking {
            val bytes = "%PDF-1.7 hello".toByteArray() + ByteArray(2_000) { it.toByte() }
            val source = BytesSource(bytes)
            var request: HttpRequestData? = null
            var sent = ByteArray(0)
            val result =
                api {
                    request = it
                    sent = (it.body as OutgoingContent).toByteArray()
                    respond(
                        """{"document":${documentJson("importing")},"job":${jobJson()}}""",
                        HttpStatusCode.Accepted,
                        json,
                    )
                }.upload(topicId, source)
            val upload = (result as ApiResult.Success).value
            assertEquals(HttpMethod.Post, request!!.method)
            assertEquals("/api/v1/topics/$topicId/documents", request!!.url.encodedPath)
            assertEquals("Bearer secret-token", request!!.headers[HttpHeaders.Authorization])
            assertTrue(
                request!!
                    .body.contentType
                    .toString()
                    .startsWith("multipart/form-data"),
            )
            val text = String(sent, Charsets.ISO_8859_1)
            assertTrue(text.contains("name=\"file\""))
            assertTrue(text.contains("Content-Length: ${bytes.size}"))
            assertTrue(text.contains("filename=\"libro.pdf\""))
            assertTrue(text.contains("application/pdf"))
            // The bytes arrive unchanged in the body.
            assertTrue(text.contains(String(bytes, Charsets.ISO_8859_1)))
            assertEquals(DocumentStatus.Importing, upload.document.status)
            assertTrue(upload.job.isDocumentImport)
            assertEquals(1, source.opened)
        }

    @Test
    fun `an upload read again opens a fresh stream`() =
        runBlocking {
            val source = BytesSource("%PDF-".toByteArray())
            val client =
                api {
                    (it.body as OutgoingContent).toByteArray()
                    respond(
                        """{"document":${documentJson("importing")},"job":${jobJson()}}""",
                        HttpStatusCode.Accepted,
                        json,
                    )
                }
            client.upload(topicId, source)
            client.upload(topicId, source)
            assertEquals(2, source.opened)
        }

    @Test
    fun `the file name is reduced to a safe header value`() {
        assertEquals("libro.pdf", "/storage/emulated/0/Download/libro.pdf".asHeaderFilename())
        assertEquals("a b.pdf", "a \"b\".pdf".asHeaderFilename())
        assertEquals("document.pdf", "\"\"".asHeaderFilename())
        assertFalse("x\r\ny.pdf".asHeaderFilename().contains('\n'))
    }

    @Test
    fun `upload errors map to failures the screen can tell apart`() =
        runBlocking {
            suspend fun failure(
                status: HttpStatusCode,
                code: String,
            ): ApiFailure {
                val result =
                    api { respond("""{"error":{"code":"$code","message":"x"}}""", status, json) }
                        .upload(topicId, BytesSource("%PDF-".toByteArray()))
                return (result as ApiResult.Failure).failure
            }
            assertEquals(ApiFailure.Conflict, failure(HttpStatusCode.Conflict, "conflict"))
            assertEquals(
                413,
                (failure(HttpStatusCode.PayloadTooLarge, "payload_too_large") as ApiFailure.Unexpected).httpStatus,
            )
            assertEquals(
                415,
                (
                    failure(
                        HttpStatusCode.UnsupportedMediaType,
                        "unsupported_media_type",
                    ) as ApiFailure.Unexpected
                ).httpStatus,
            )
            assertEquals(ApiFailure.NotFound, failure(HttpStatusCode.NotFound, "not_found"))
            assertEquals(
                503,
                (failure(HttpStatusCode.ServiceUnavailable, "internal") as ApiFailure.Unexpected).httpStatus,
            )
        }

    @Test
    fun `a file that cannot be read is a failure, not a crash`() =
        runBlocking {
            val broken =
                object : UploadSource {
                    override val displayName = "gone.pdf"
                    override val sizeBytes: Long? = 10

                    override fun open(): InputStream = throw IOException("file vanished")
                }
            val result = api { respond("""{}""", HttpStatusCode.Accepted, json) }.upload(topicId, broken)
            assertTrue(result is ApiResult.Failure)
        }

    @Test
    fun `the connection lost while the body goes out is a network failure and the same source can be sent again`() =
        runBlocking {
            val source = BytesSource("%PDF-1.7 hello".toByteArray() + ByteArray(2_000))
            var attempts = 0
            val client =
                api {
                    attempts++
                    // The body is read (the upload is under way), then the link drops before any answer.
                    (it.body as OutgoingContent).toByteArray()
                    if (attempts == 1) throw IOException("connection reset")
                    respond(
                        """{"document":${documentJson("importing")},"job":${jobJson()}}""",
                        HttpStatusCode.Accepted,
                        json,
                    )
                }

            val lost = client.upload(topicId, source)
            assertTrue(lost is ApiResult.Failure)
            assertTrue((lost as ApiResult.Failure).failure is ApiFailure.Network)

            // Retry: a fresh stream of the same file, now it goes through.
            assertTrue(client.upload(topicId, source) is ApiResult.Success)
            assertEquals(2, source.opened)
        }

    @Test
    fun `the file stream failing halfway through is a failure, not a crash or a hang`() =
        runBlocking {
            val flaky =
                object : UploadSource {
                    override val displayName = "libro.pdf"
                    override val sizeBytes: Long? = 4_000

                    override fun open(): InputStream =
                        object : InputStream() {
                            private var n = 0

                            override fun read(): Int = if (n++ < 100) 1 else throw IOException("read failed")
                        }
                }
            val result = api { respond("""{}""", HttpStatusCode.Accepted, json) }.upload(topicId, flaky)
            assertTrue(result is ApiResult.Failure)
        }

    @Test
    fun `delete sends DELETE and 409 is a conflict`() =
        runBlocking {
            var method: HttpMethod? = null
            var path = ""
            val ok =
                api {
                    method = it.method
                    path = it.url.encodedPath
                    respond("", HttpStatusCode.NoContent)
                }.delete("d1")
            assertTrue(ok is ApiResult.Success)
            assertEquals(HttpMethod.Delete, method)
            assertEquals("/api/v1/documents/d1", path)
            val conflict =
                api {
                    respond(
                        """{"error":{"code":"conflict","message":"x"}}""",
                        HttpStatusCode.Conflict,
                        json,
                    )
                }.delete("d1")
            assertEquals(ApiFailure.Conflict, (conflict as ApiResult.Failure).failure)
        }

    @Test
    fun `listJobs reads the first page of the Topic jobs`() =
        runBlocking {
            var path = ""
            val result =
                api {
                    path = it.url.encodedPath
                    respond("""{"items":[${jobJson()}],"next_cursor":null}""", HttpStatusCode.OK, json)
                }.listJobs(topicId)
            assertEquals("/api/v1/topics/$topicId/jobs", path)
            assertTrue(
                (result as ApiResult.Success)
                    .value.items
                    .single()
                    .isDocumentImport,
            )
        }
}
