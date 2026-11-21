package com.teachme.feature.resources

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.DocumentSection
import com.teachme.core.model.DocumentStatus
import com.teachme.core.model.JobStatus
import com.teachme.core.model.MAX_DOCUMENTS_PER_TOPIC
import com.teachme.core.model.MAX_DOCUMENT_BYTES
import com.teachme.core.model.ResearchInfo
import com.teachme.core.model.TopicDocument
import com.teachme.core.model.TopicJob
import com.teachme.core.model.TopicLanguage
import java.util.Locale

// Pure rules of the "I tuoi documenti" section (docs/design/screens/resources.md section 13).

/** Why an upload did not start or did not finish (section 13.6). Nothing was created on the server. */
enum class UploadError {
    /** More than 50 MB: caught before sending, or answered with 413. */
    TooLarge,

    /** The server says it is not a PDF (415). */
    NotPdf,

    /** Network, timeout, 5xx or an unreadable file: "Riprova" reuses the same file. */
    Network,
}

/** What an upload answer means for the screen. */
internal sealed interface UploadFailureAction {
    data class Show(
        val error: UploadError,
    ) : UploadFailureAction

    /** 401: the session ended; the sign-in opens over the screen (login.md), the panel has nothing to say. */
    data object Silent : UploadFailureAction

    /** 409: the list changed (limit reached, or an import is running): show the truth, no panel. */
    data object Reload : UploadFailureAction

    /** 404 / invalid id: the Topic is gone, the Topic screen handles it. */
    data object TopicGone : UploadFailureAction
}

internal fun uploadFailureAction(failure: ApiFailure): UploadFailureAction =
    when (failure) {
        ApiFailure.Conflict -> UploadFailureAction.Reload
        ApiFailure.NotFound, ApiFailure.InvalidId -> UploadFailureAction.TopicGone
        ApiFailure.Unauthorized -> UploadFailureAction.Silent
        is ApiFailure.Unexpected ->
            UploadFailureAction.Show(
                when (failure.httpStatus) {
                    HTTP_PAYLOAD_TOO_LARGE -> UploadError.TooLarge
                    HTTP_UNSUPPORTED_MEDIA_TYPE -> UploadError.NotPdf
                    else -> UploadError.Network
                },
            )
        // Network, 400 and 422 (not documented for this call) are all "could not send it": try again.
        else -> UploadFailureAction.Show(UploadError.Network)
    }

private const val HTTP_PAYLOAD_TOO_LARGE = 413
private const val HTTP_UNSUPPORTED_MEDIA_TYPE = 415

/** True when the provider says the file is over the limit; an unknown size passes (the server checks too). */
internal fun isOverSizeLimit(sizeBytes: Long?): Boolean = sizeBytes != null && sizeBytes > MAX_DOCUMENT_BYTES

/** Whether "Carica un PDF" can be offered, and if not, which sentence takes its place (section 13.6). */
internal enum class UploadAvailability { Available, LimitReached, ImportBusy }

internal fun uploadAvailability(
    documents: List<TopicDocument>,
    uploading: Boolean,
): UploadAvailability =
    when {
        documents.size >= MAX_DOCUMENTS_PER_TOPIC -> UploadAvailability.LimitReached
        uploading || documents.any { it.status == DocumentStatus.Importing } -> UploadAvailability.ImportBusy
        else -> UploadAvailability.Available
    }

/** The line of an import in progress (section 13.4); there is never a percentage. */
internal sealed interface ImportPhase {
    data object Uploading : ImportPhase

    data object Queued : ImportPhase

    data object Reading : ImportPhase

    /** [done] of [total] pages; either may be missing. */
    data class Extracting(
        val done: Int?,
        val total: Int?,
    ) : ImportPhase

    data object Sectioning : ImportPhase

    data object Saving : ImportPhase

    data object Unknown : ImportPhase

    /** Phase changes are what a screen reader hears; the page numbers inside [Extracting] never are. */
    val announcedKey: String
        get() =
            when (this) {
                Uploading -> "uploading"
                Queued -> "queued"
                Reading -> "reading"
                is Extracting -> "extracting"
                Sectioning -> "sectioning"
                Saving -> "saving"
                Unknown -> "unknown"
            }
}

internal fun importPhase(job: TopicJob?): ImportPhase {
    if (job == null || !job.isActive) return ImportPhase.Unknown
    if (job.status == JobStatus.Queued) return ImportPhase.Queued
    val progress = job.progress
    return when (progress?.step) {
        "reading" -> ImportPhase.Reading
        "extracting" ->
            ImportPhase.Extracting(
                progress.done?.takeIf { it >= 0 },
                progress.total?.takeIf { it > 0 },
            )
        "sectioning" -> ImportPhase.Sectioning
        "saving" -> ImportPhase.Saving
        else -> ImportPhase.Unknown
    }
}

/** Why a document failed, from the `error.code` stored in the `Document` (section 13.5). */
enum class DocumentFailure(
    val code: String?,
    val retryable: Boolean,
) {
    NoText("no_text", false),
    Encrypted("encrypted_pdf", false),
    TooManyPages("too_many_pages", false),
    InvalidPdf("invalid_pdf", false),

    /** Any other code (`timeout`, `internal`...): not the file's fault, and a retry may work. */
    Other(null, true),
    ;

    companion object {
        fun fromCode(code: String?): DocumentFailure =
            entries.firstOrNull { it.code != null && it.code == code } ?: Other
    }
}

/**
 * The import job that belongs to [document]: the one that names it (`document_id`, jobs.md v1.2), else, from a backend
 * that does not send it, [latest] when it can only be this document's (nothing else is known).
 */
internal fun jobFor(
    document: TopicDocument,
    byDocument: Map<String, TopicJob>,
    latest: TopicJob?,
): TopicJob? = byDocument[document.id] ?: latest?.takeIf { it.documentId == null }

/**
 * The job to retry for a failed document, or null. A job that names the document is enough; without `document_id` the
 * job is attributed only when it can only be this one: the document is the newest, the job failed, and the error
 * codes agree.
 */
internal fun retryableJob(
    document: TopicDocument,
    documents: List<TopicDocument>,
    job: TopicJob?,
): TopicJob? {
    if (document.status != DocumentStatus.Failed) return null
    if (!DocumentFailure.fromCode(document.error?.code).retryable) return null
    val failed = job?.takeIf { it.status == JobStatus.Failed } ?: return null
    if (failed.documentId != null) return failed.takeIf { it.documentId == document.id }
    val newest = documents.maxByOrNull { it.createdAt ?: java.time.Instant.EPOCH }
    if (newest?.id != document.id) return null
    val jobCode = failed.error?.code
    if (document.error?.code != null && jobCode != null && jobCode != document.error?.code) return null
    return failed
}

/**
 * The documents to name in the banner "Ho letto il tuo PDF" (section 13.8): ready, created after the last research
 * finished, and not dismissed. Empty when nothing should be proposed: no research yet (the first one includes the
 * documents), a research in progress, or an unknown research date (nothing to compare with).
 */
internal fun documentsNewerThanResearch(
    documents: List<TopicDocument>,
    research: ResearchInfo?,
    researchActive: Boolean,
    dismissed: Set<String>,
): List<TopicDocument> {
    val finished = research?.finishedAt ?: return emptyList()
    if (researchActive) return emptyList()
    return documents.filter {
        it.status == DocumentStatus.Ready && it.id !in dismissed && it.createdAt?.isAfter(finished) == true
    }
}

/** "pp. 1-24" parts: one page is "p. 5". Null when the entry is an automatic block "Pagine 1-10" (title says it). */
internal data class PageRange(
    val start: Int,
    val end: Int,
) {
    val single: Boolean get() = start >= end
}

internal fun DocumentSection.pageRangeOrNull(): PageRange? =
    if (title.trimStart().startsWith("Pagine", ignoreCase = true)) null else PageRange(pageStart, pageEnd)

/** An index is shown only when it has something to say (section 13.4). */
internal fun TopicDocument.hasIndex(): Boolean = sections.size >= 2

/** The language to name in the meta line: none when it is the Topic's own, nor for unknown or invalid codes. */
internal fun languageNameOrNull(
    code: String?,
    topicLanguage: TopicLanguage,
    display: Locale = Locale.getDefault(),
): String? {
    val tag = code?.trim()?.takeIf { it.isNotEmpty() } ?: return null
    val locale = Locale.forLanguageTag(tag)
    if (locale.language.isEmpty() || locale.language == topicLanguage.wire) return null
    val name = locale.getDisplayLanguage(display)
    return name.takeIf { it.isNotBlank() && !it.equals(locale.language, ignoreCase = true) }
}

/** The title of a document: the real one once it is ready, the file name while it is read or if it failed. */
internal fun TopicDocument.listTitle(): String =
    if (status == DocumentStatus.Ready) title.ifBlank { filename } else filename.ifBlank { title }

/** Whether the file name adds something to the title (section 13.4: shown only when it differs). */
internal fun TopicDocument.showsFilename(): Boolean =
    filename.isNotBlank() && !filename.removeSuffix(".pdf").removeSuffix(".PDF").equals(title, ignoreCase = true)
