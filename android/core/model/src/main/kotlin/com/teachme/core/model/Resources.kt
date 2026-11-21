package com.teachme.core.model

import java.time.Instant

/** Lifecycle of a background job (docs/contracts/jobs.md section 2). A value this app does not know is [Unknown]. */
enum class JobStatus { Queued, Running, Succeeded, Failed, Unknown }

/** `progress` of a job: [step] is a stable label chosen by the job, [message] an optional Italian fact. */
data class JobProgress(
    val step: String?,
    val message: String?,
    /** Position inside the step (for `lesson_generation` `writing`: the number of the document, 1..3). */
    val done: Int? = null,
    val total: Int? = null,
)

/** `error` of a failed job: [code] decides the text shown, [message] only goes under "Dettagli". */
data class JobError(
    val code: String,
    val message: String,
)

/**
 * `result` of a succeeded job; which fields are set depends on the kind (resources.md section 4 for `research`,
 * lessons.md sections 4 and 6 for `lesson_generation` and `lesson_completion`).
 */
data class JobResult(
    val resourceCount: Int? = null,
    val gapCount: Int? = null,
    val lessonId: String? = null,
    val lessonNumber: Int? = null,
    val recordCount: Int? = null,
    val termCount: Int? = null,
    val reviewCardCount: Int? = null,
    /** `document_import` (documents.md section 4). */
    val documentId: String? = null,
    val pageCount: Int? = null,
    val sectionCount: Int? = null,
)

/** A long-running operation of a Topic (jobs.md): research, lesson generation, lesson completion. */
data class TopicJob(
    val id: String,
    val topicId: String,
    val kind: String,
    val status: JobStatus,
    val progress: JobProgress?,
    val error: JobError?,
    val result: JobResult?,
    val attempt: Int,
    val createdAt: Instant,
    val startedAt: Instant?,
    val finishedAt: Instant?,
    /** Only `lesson_completion` jobs: the lesson being closed. */
    val lessonId: String? = null,
    /** Only `document_import` jobs: the document being read (jobs.md v1.2); null from an older backend. */
    val documentId: String? = null,
) {
    /** Queued or running: at most one such job per Topic and kind. */
    val isActive: Boolean get() = status == JobStatus.Queued || status == JobStatus.Running

    val isResearch: Boolean get() = kind == JOB_KIND_RESEARCH

    val isLessonGeneration: Boolean get() = kind == JOB_KIND_LESSON_GENERATION

    val isLessonCompletion: Boolean get() = kind == JOB_KIND_LESSON_COMPLETION

    val isDocumentImport: Boolean get() = kind == JOB_KIND_DOCUMENT_IMPORT
}

const val JOB_KIND_RESEARCH = "research"
const val JOB_KIND_LESSON_GENERATION = "lesson_generation"
const val JOB_KIND_LESSON_COMPLETION = "lesson_completion"
const val JOB_KIND_DOCUMENT_IMPORT = "document_import"

/**
 * Group of a source (resources.md section 2): high-confidence knowledge, where people learn, or a PDF the learner
 * uploaded ([Document], never shown among the web groups).
 */
enum class ResourceKind { Knowledge, Community, Document }

/** Type of a source; a wire value this app does not know is [Unknown] and is shown as "Fonte". */
enum class SourceType(
    val wire: String?,
) {
    Book("book"),
    Paper("paper"),
    Article("article"),
    Documentation("documentation"),
    Video("video"),
    Course("course"),
    Standard("standard"),
    Pdf("pdf"),
    Forum("forum"),
    SocialGroup("social_group"),
    LiveClass("live_class"),
    LocalGroup("local_group"),
    Other("other"),
    Unknown(null),
    ;

    companion object {
        fun fromWire(value: String?): SourceType =
            entries.firstOrNull { it.wire != null && it.wire == value } ?: Unknown
    }
}

data class Resource(
    val id: String,
    val kind: ResourceKind,
    val sourceType: SourceType,
    val title: String,
    /** Null for a source without a site (e.g. a live class). The app opens only `https` URLs. */
    val url: String?,
    val domain: String?,
    val language: String?,
    val annotation: String,
    val useFor: String,
    val createdAt: Instant?,
    /** Only [ResourceKind.Document]: the uploaded document this source comes from. */
    val documentId: String? = null,
)

/** What the research could not find (resources.md section 2). */
data class Gap(
    val area: String,
    val note: String,
)

/** Data of the last successful research. */
data class ResearchInfo(
    val jobId: String,
    val missionVersion: Int,
    val finishedAt: Instant?,
)

/** `GET /topics/{id}/resources`. [resources] holds the web sources only; uploaded PDFs are in [documentResources]. */
data class ResourcesView(
    val resources: List<Resource>,
    val gaps: List<Gap>,
    val communitiesSkipped: Boolean,
    val research: ResearchInfo?,
    /** The latest research job in any state, or null. */
    val job: TopicJob?,
    /** The `kind: document` sources: not counted and not listed with the web groups (resources.md section 13.2). */
    val documentResources: List<Resource> = emptyList(),
) {
    val knowledge: List<Resource> get() = resources.filter { it.kind == ResourceKind.Knowledge }
    val community: List<Resource> get() = resources.filter { it.kind == ResourceKind.Community }
}

/** Sources of a Topic and their research (docs/contracts/resources.md, jobs.md). No cache: every call goes out. */
interface ResourceRepository {
    suspend fun getResources(topicId: String): ApiResult<ResourcesView>

    /** `POST .../resources/research`: starts a new research; a 409 is [ApiFailure.Conflict]. */
    suspend fun startResearch(topicId: String): ApiResult<TopicJob>

    /** `POST .../jobs/{id}/retry`: puts a failed job back in the queue; a 409 is [ApiFailure.Conflict]. */
    suspend fun retryJob(
        topicId: String,
        jobId: String,
    ): ApiResult<TopicJob>
}

/** What the research looks like from outside the Fonti tab (resources.md section 7): a pointer, never details. */
enum class ResearchStatus { None, InProgress, Failed, Ready }

/**
 * Summary used by the card "Mission confermata", the context pane and the Mission tab.
 * [missionVersion] is the mission version of the sources shown, [resourceCount] their number.
 */
data class ResearchPointer(
    val status: ResearchStatus = ResearchStatus.None,
    val resourceCount: Int? = null,
    val missionVersion: Int? = null,
) {
    companion object {
        val None = ResearchPointer()
    }

    /** True when the sources were chosen for an older mission than [currentVersion]. */
    fun isStaleFor(currentVersion: Int?): Boolean =
        status == ResearchStatus.Ready &&
            currentVersion != null &&
            missionVersion != null &&
            missionVersion < currentVersion
}
