package com.teachme.core.network

import com.teachme.core.model.ApiResult
import com.teachme.core.model.DocumentUpload
import com.teachme.core.model.Page
import com.teachme.core.model.TopicDocument
import com.teachme.core.model.TopicJob
import com.teachme.core.model.UploadSource
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.plugins.timeout
import io.ktor.client.request.delete
import io.ktor.client.request.forms.InputProvider
import io.ktor.client.request.forms.MultiPartFormDataContent
import io.ktor.client.request.forms.formData
import io.ktor.client.request.get
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.Headers
import io.ktor.http.HttpHeaders
import io.ktor.http.path
import io.ktor.utils.io.streams.asInput
import javax.inject.Inject
import javax.inject.Singleton

/** Raw endpoints of `docs/contracts/documents.md` section 3 and `jobs.md` section 3. */
interface DocumentsApi {
    suspend fun list(topicId: String): ApiResult<List<TopicDocument>>

    suspend fun upload(
        topicId: String,
        source: UploadSource,
    ): ApiResult<DocumentUpload>

    suspend fun delete(documentId: String): ApiResult<Unit>

    suspend fun listJobs(topicId: String): ApiResult<Page<TopicJob>>
}

/** A 50 MB upload over a slow link takes minutes: the 30 s request timeout of the other calls would cut it. */
internal const val UPLOAD_REQUEST_TIMEOUT_MS = 30L * 60 * 1000

/** No bytes moving for this long means the connection is dead. */
internal const val UPLOAD_SOCKET_TIMEOUT_MS = 60_000L

@Singleton
class KtorDocumentsApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : DocumentsApi {
        override suspend fun list(topicId: String): ApiResult<List<TopicDocument>> =
            apiCall({ client.get { url { path("api", "v1", "topics", topicId, "documents") } } }) { response ->
                response.body<DocumentsListDto>().documents.map { it.toModel() }
            }

        override suspend fun upload(
            topicId: String,
            source: UploadSource,
        ): ApiResult<DocumentUpload> =
            apiCall({
                client.post {
                    url { path("api", "v1", "topics", topicId, "documents") }
                    setBody(multipartBody(source))
                    timeout {
                        requestTimeoutMillis = UPLOAD_REQUEST_TIMEOUT_MS
                        socketTimeoutMillis = UPLOAD_SOCKET_TIMEOUT_MS
                    }
                }
            }) { it.body<DocumentUploadDto>().toModel() }

        override suspend fun delete(documentId: String): ApiResult<Unit> =
            apiCall({ client.delete { url { path("api", "v1", "documents", documentId) } } }) { }

        override suspend fun listJobs(topicId: String): ApiResult<Page<TopicJob>> =
            apiCall({ client.get { url { path("api", "v1", "topics", topicId, "jobs") } } }) {
                it.body<PageDto<JobDto>>().toModel { dto -> dto.toModel() }
            }
    }

/**
 * The `file` part is an [InputProvider]: Ktor pulls the bytes from the stream while writing the request, so the file is
 * never in memory as a whole. The provider is called once per attempt and opens a fresh stream.
 */
internal fun multipartBody(source: UploadSource): MultiPartFormDataContent =
    MultiPartFormDataContent(
        formData {
            append(
                "file",
                InputProvider(source.sizeBytes) { source.open().asInput() },
                Headers.build {
                    append(HttpHeaders.ContentType, ContentType.Application.Pdf.toString())
                    append(HttpHeaders.ContentDisposition, "filename=\"${source.displayName.asHeaderFilename()}\"")
                },
            )
        },
    )

/** A file name safe inside a quoted header value: no quotes, no control characters, no path. */
internal fun String.asHeaderFilename(): String =
    substringAfterLast('/')
        .substringAfterLast('\\')
        .filter { it >= ' ' && it != '"' && it != '\u007f' }
        .ifBlank { "document.pdf" }
        .take(MAX_FILENAME_CHARS)

private const val MAX_FILENAME_CHARS = 255
