package com.teachme.core.network

import com.teachme.core.model.DocumentError
import com.teachme.core.model.DocumentSection
import com.teachme.core.model.DocumentStatus
import com.teachme.core.model.DocumentUpload
import com.teachme.core.model.TopicDocument
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import java.time.Instant
import java.time.OffsetDateTime

@Serializable
internal data class DocumentSectionDto(
    val id: String,
    val title: String = "",
    val level: Int = 1,
    @SerialName("page_start") val pageStart: Int = 1,
    @SerialName("page_end") val pageEnd: Int = 1,
)

@Serializable
internal data class DocumentDto(
    val id: String,
    @SerialName("topic_id") val topicId: String = "",
    @SerialName("resource_id") val resourceId: String? = null,
    val filename: String = "",
    val title: String = "",
    val status: String? = null,
    val error: JobErrorDto? = null,
    @SerialName("size_bytes") val sizeBytes: Long? = null,
    @SerialName("page_count") val pageCount: Int? = null,
    val language: String? = null,
    val sections: List<DocumentSectionDto> = emptyList(),
    @SerialName("created_at") val createdAt: String? = null,
)

@Serializable
internal data class DocumentsListDto(
    val documents: List<DocumentDto> = emptyList(),
)

@Serializable
internal data class DocumentUploadDto(
    val document: DocumentDto,
    val job: JobDto,
)

private fun parseInstantOrNull(value: String?): Instant? =
    value?.let { runCatching { OffsetDateTime.parse(it).toInstant() }.getOrNull() }

internal fun DocumentDto.toModel() =
    TopicDocument(
        id = id,
        topicId = topicId,
        resourceId = resourceId?.takeIf { it.isNotBlank() },
        filename = filename,
        // The title is required by the contract; an empty one falls back to the file name rather than a blank row.
        title = title.ifBlank { filename.removeSuffix(".pdf") },
        status = DocumentStatus.fromWire(status),
        error = error?.let { DocumentError(it.code, it.message) },
        sizeBytes = sizeBytes,
        pageCount = pageCount,
        language = language?.takeIf { it.isNotBlank() },
        sections = sections.map { DocumentSection(it.id, it.title, it.level.coerceIn(1, 3), it.pageStart, it.pageEnd) },
        createdAt = parseInstantOrNull(createdAt),
    )

internal fun DocumentUploadDto.toModel() = DocumentUpload(document.toModel(), job.toModel())
