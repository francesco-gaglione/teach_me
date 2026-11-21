package com.teachme.core.network

import com.teachme.core.model.Gap
import com.teachme.core.model.JobError
import com.teachme.core.model.JobProgress
import com.teachme.core.model.JobResult
import com.teachme.core.model.JobStatus
import com.teachme.core.model.ResearchInfo
import com.teachme.core.model.Resource
import com.teachme.core.model.ResourceKind
import com.teachme.core.model.ResourcesView
import com.teachme.core.model.SourceType
import com.teachme.core.model.TopicJob
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import java.time.Instant
import java.time.OffsetDateTime

@Serializable
internal data class JobProgressDto(
    val step: String? = null,
    val message: String? = null,
    val done: Int? = null,
    val total: Int? = null,
)

@Serializable
internal data class JobErrorDto(
    val code: String,
    val message: String = "",
)

@Serializable
internal data class JobResultDto(
    @SerialName("resource_count") val resourceCount: Int? = null,
    @SerialName("gap_count") val gapCount: Int? = null,
    @SerialName("lesson_id") val lessonId: String? = null,
    @SerialName("lesson_number") val lessonNumber: Int? = null,
    @SerialName("record_count") val recordCount: Int? = null,
    @SerialName("term_count") val termCount: Int? = null,
    @SerialName("review_card_count") val reviewCardCount: Int? = null,
    @SerialName("document_id") val documentId: String? = null,
    @SerialName("page_count") val pageCount: Int? = null,
    @SerialName("section_count") val sectionCount: Int? = null,
)

@Serializable
internal data class JobDto(
    val id: String,
    @SerialName("topic_id") val topicId: String,
    @SerialName("lesson_id") val lessonId: String? = null,
    @SerialName("document_id") val documentId: String? = null,
    val kind: String,
    val status: String? = null,
    val progress: JobProgressDto? = null,
    val error: JobErrorDto? = null,
    val result: JobResultDto? = null,
    val attempt: Int = 0,
    @SerialName("created_at") val createdAt: String,
    @SerialName("started_at") val startedAt: String? = null,
    @SerialName("finished_at") val finishedAt: String? = null,
)

@Serializable
internal data class ResourceDto(
    val id: String,
    val kind: String? = null,
    @SerialName("source_type") val sourceType: String? = null,
    val title: String,
    val url: String? = null,
    val domain: String? = null,
    val language: String? = null,
    val annotation: String = "",
    @SerialName("use_for") val useFor: String = "",
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("document_id") val documentId: String? = null,
)

@Serializable
internal data class GapDto(
    val area: String,
    val note: String = "",
)

@Serializable
internal data class ResearchInfoDto(
    @SerialName("job_id") val jobId: String,
    @SerialName("mission_version") val missionVersion: Int,
    @SerialName("finished_at") val finishedAt: String? = null,
)

@Serializable
internal data class ResourcesViewDto(
    val resources: List<ResourceDto> = emptyList(),
    val gaps: List<GapDto> = emptyList(),
    @SerialName("communities_skipped") val communitiesSkipped: Boolean = false,
    val research: ResearchInfoDto? = null,
    val job: JobDto? = null,
)

private fun parseInstant(value: String): Instant = OffsetDateTime.parse(value).toInstant()

/** An unknown status is [JobStatus.Unknown]: the job is shown as neither running nor finished, never a crash. */
internal fun JobDto.toModel() =
    TopicJob(
        id = id,
        topicId = topicId,
        kind = kind,
        status =
            when (status) {
                "queued" -> JobStatus.Queued
                "running" -> JobStatus.Running
                "succeeded" -> JobStatus.Succeeded
                "failed" -> JobStatus.Failed
                else -> JobStatus.Unknown
            },
        progress = progress?.let { JobProgress(it.step, it.message, it.done, it.total) },
        error = error?.let { JobError(it.code, it.message) },
        result =
            result?.let {
                JobResult(
                    resourceCount = it.resourceCount,
                    gapCount = it.gapCount,
                    lessonId = it.lessonId,
                    lessonNumber = it.lessonNumber,
                    recordCount = it.recordCount,
                    termCount = it.termCount,
                    reviewCardCount = it.reviewCardCount,
                    documentId = it.documentId,
                    pageCount = it.pageCount,
                    sectionCount = it.sectionCount,
                )
            },
        attempt = attempt,
        createdAt = parseInstant(createdAt),
        startedAt = startedAt?.let(::parseInstant),
        finishedAt = finishedAt?.let(::parseInstant),
        lessonId = lessonId,
        documentId = documentId?.takeIf { it.isNotBlank() },
    )

/** A `kind` this app does not know is read as `knowledge` (resources.md section 2), never dropped or a crash. */
internal fun ResourceDto.toModel(): Resource {
    val group =
        when (kind) {
            "community" -> ResourceKind.Community
            "document" -> ResourceKind.Document
            else -> ResourceKind.Knowledge
        }
    return Resource(
        id = id,
        kind = group,
        sourceType = SourceType.fromWire(sourceType),
        title = title,
        url = url?.takeIf { it.isNotBlank() },
        domain = domain?.takeIf { it.isNotBlank() },
        language = language,
        annotation = annotation,
        useFor = useFor,
        createdAt = createdAt?.let(::parseInstant),
        documentId = documentId?.takeIf { group == ResourceKind.Document },
    )
}

internal fun ResourcesViewDto.toModel() =
    ResourcesView(
        resources = resources.map { it.toModel() }.filter { it.kind != ResourceKind.Document },
        gaps = gaps.map { Gap(it.area, it.note) },
        communitiesSkipped = communitiesSkipped,
        research = research?.let { ResearchInfo(it.jobId, it.missionVersion, it.finishedAt?.let(::parseInstant)) },
        job = job?.toModel(),
        documentResources = resources.map { it.toModel() }.filter { it.kind == ResourceKind.Document },
    )
