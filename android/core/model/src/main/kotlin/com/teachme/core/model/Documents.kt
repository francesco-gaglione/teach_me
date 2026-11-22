package com.teachme.core.model

import java.io.InputStream
import java.time.Instant

/** The server limits of an upload (documents.md section 3): 5 documents per Topic, 50 MB each. */
const val MAX_DOCUMENTS_PER_TOPIC = 5
const val MAX_DOCUMENT_BYTES: Long = 50L * 1024 * 1024

/** State of an uploaded document; a value this app does not know is read as [Importing] (documents.md section 2). */
enum class DocumentStatus {
    Importing,
    Ready,
    Failed,
    ;

    companion object {
        fun fromWire(value: String?): DocumentStatus =
            when (value) {
                "ready" -> Ready
                "failed" -> Failed
                else -> Importing
            }
    }
}

/** One entry of the table of contents. Pages are numbered from 1, as in the file. */
data class DocumentSection(
    val id: String,
    val title: String,
    val level: Int,
    val pageStart: Int,
    val pageEnd: Int,
)

/** Why an import failed (`documents.md` section 4): [code] decides the text shown, [message] is not shown. */
data class DocumentError(
    val code: String,
    val message: String,
)

/** A PDF the learner uploaded to a Topic (`documents.md` section 2). */
data class TopicDocument(
    val id: String,
    val topicId: String,
    val resourceId: String?,
    val filename: String,
    val title: String,
    val status: DocumentStatus,
    val error: DocumentError?,
    val sizeBytes: Long?,
    val pageCount: Int?,
    val language: String?,
    val sections: List<DocumentSection>,
    val createdAt: Instant?,
)

/** The `202` answer of an upload: the document and its `document_import` job. */
data class DocumentUpload(
    val document: TopicDocument,
    val job: TopicJob,
)

/**
 * Where the bytes of a file to upload come from. The file is read from [open] while it is sent, in blocks: it is never
 * held in memory as a whole. [open] may be called again (a retry reads the same source from the start).
 */
interface UploadSource {
    val displayName: String

    /** Size in bytes when the provider knows it. */
    val sizeBytes: Long?

    fun open(): InputStream

    /**
     * A small stable reference (a `content://` URI) from which an [UploadSourceRestorer] can rebuild this source after
     * the process died, or null when the source cannot be rebuilt. Never file bytes.
     */
    val reference: String? get() = null
}

/** Rebuilds an [UploadSource] from [UploadSource.reference] and its display name; null when it cannot be read. */
fun interface UploadSourceRestorer {
    fun restore(
        reference: String,
        displayName: String,
    ): UploadSource?

    /** The reference is forgotten for good: give back the persisted read permission, if there is one. */
    fun release(reference: String) = Unit
}

/** Uploaded documents of a Topic (docs/contracts/documents.md). No cache: every call goes out. */
interface DocumentRepository {
    suspend fun list(topicId: String): ApiResult<List<TopicDocument>>

    /**
     * `POST /topics/{id}/documents`, `multipart/form-data` with the field `file`. Failures: 409 is
     * [ApiFailure.Conflict], 413 and 415 are [ApiFailure.Unexpected] with that `httpStatus`.
     */
    suspend fun upload(
        topicId: String,
        source: UploadSource,
    ): ApiResult<DocumentUpload>

    /** `DELETE /documents/{id}`: a 409 is [ApiFailure.Conflict] (the import is still running). */
    suspend fun delete(documentId: String): ApiResult<Unit>

    /** The `document_import` jobs of the Topic, newest first; `GET /topics/{id}/jobs`, first page. */
    suspend fun importJobs(topicId: String): ApiResult<List<TopicJob>>

    /** `POST .../jobs/{id}/retry`. */
    suspend fun retryJob(
        topicId: String,
        jobId: String,
    ): ApiResult<TopicJob>
}

/** "Non ora" on the banner "Ho letto il tuo PDF" (resources.md section 13.8), kept on the device per document. */
interface DocumentBannerStore {
    suspend fun dismissed(): Set<String>

    suspend fun dismiss(documentIds: Collection<String>)
}
