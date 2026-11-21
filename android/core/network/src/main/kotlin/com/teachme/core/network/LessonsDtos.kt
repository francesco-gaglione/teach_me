package com.teachme.core.network

import com.teachme.core.model.Attempt
import com.teachme.core.model.AttemptGrade
import com.teachme.core.model.AttemptOutcome
import com.teachme.core.model.DocumentSource
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.HighlightRecord
import com.teachme.core.model.LearningRecord
import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonCompletion
import com.teachme.core.model.LessonInProgress
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LessonView
import com.teachme.core.model.QualityWarning
import com.teachme.core.model.QueuedAttempt
import com.teachme.core.model.RecordKind
import com.teachme.core.model.RecordStatus
import com.teachme.core.model.StrokeRecord
import com.teachme.core.model.TopicLanguage
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import java.time.Instant
import java.time.OffsetDateTime

private fun parseInstantOrNull(value: String?): Instant? =
    value?.let { runCatching { OffsetDateTime.parse(it).toInstant() }.getOrNull() }

@Serializable
internal data class LessonDto(
    val id: String,
    @SerialName("topic_id") val topicId: String,
    val number: Int = 0,
    val title: String = "",
    val objective: String = "",
    @SerialName("estimated_minutes") val estimatedMinutes: Int = 0,
    val status: String? = null,
    @SerialName("catalog_version") val catalogVersion: Int = 1,
    @SerialName("mission_version") val missionVersion: Int? = null,
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("completed_at") val completedAt: String? = null,
    /** Absent from a backend older than contract v1.8: no document sources. */
    @SerialName("document_sources") val documentSources: List<DocumentSourceDto> = emptyList(),
)

@Serializable
internal data class DocumentSourceDto(
    @SerialName("document_id") val documentId: String = "",
    val title: String = "",
    @SerialName("page_start") val pageStart: Int = 0,
    @SerialName("page_end") val pageEnd: Int = 0,
)

/** `LessonInProgress`: all the `Lesson` fields (flattened) plus the two below. */
@Serializable
internal data class LessonInProgressDto(
    val id: String,
    @SerialName("topic_id") val topicId: String,
    val number: Int = 0,
    val title: String = "",
    val objective: String = "",
    @SerialName("estimated_minutes") val estimatedMinutes: Int = 0,
    val status: String? = null,
    @SerialName("catalog_version") val catalogVersion: Int = 1,
    @SerialName("mission_version") val missionVersion: Int? = null,
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("completed_at") val completedAt: String? = null,
    @SerialName("document_sources") val documentSources: List<DocumentSourceDto> = emptyList(),
    @SerialName("topic_title") val topicTitle: String = "",
    @SerialName("last_activity_at") val lastActivityAt: String? = null,
)

@Serializable
internal data class AttemptGradeDto(
    val verdict: String? = null,
    val feedback: String = "",
    val misconception: String? = null,
)

@Serializable
internal data class AttemptDto(
    val id: String = "",
    @SerialName("block_id") val blockId: String,
    @SerialName("block_type") val blockType: String = "",
    @SerialName("attempt_number") val attemptNumber: Int = 1,
    val answer: JsonElement? = null,
    val outcome: String? = null,
    val grade: AttemptGradeDto? = null,
    @SerialName("answered_at") val answeredAt: String? = null,
)

@Serializable
internal data class LearningRecordDto(
    val id: String,
    val number: Int = 0,
    val kind: String? = null,
    val title: String = "",
    val body: String = "",
    val evidence: String = "",
    val status: String? = null,
    @SerialName("superseded_by") val supersededBy: String? = null,
    @SerialName("lesson_id") val lessonId: String? = null,
    @SerialName("created_at") val createdAt: String? = null,
)

@Serializable
internal data class GlossaryTermDto(
    val id: String = "",
    val key: String,
    val term: String,
    val definition: String = "",
    val avoid: List<String> = emptyList(),
    @SerialName("lesson_id") val lessonId: String? = null,
    @SerialName("created_at") val createdAt: String? = null,
    @SerialName("updated_at") val updatedAt: String? = null,
    /** Absent from a backend older than contract v1.5: same as null. */
    @SerialName("first_lesson_id") val firstLessonId: String? = null,
)

@Serializable
internal data class LessonCompletionDto(
    val job: JobDto? = null,
    val records: List<LearningRecordDto> = emptyList(),
    val terms: List<GlossaryTermDto> = emptyList(),
    @SerialName("review_card_count") val reviewCardCount: Int = 0,
)

@Serializable
internal data class LessonViewDto(
    val lesson: LessonDto,
    val document: JsonElement,
    val attempts: List<AttemptDto> = emptyList(),
    val completion: LessonCompletionDto? = null,
    /** Absent from a backend older than contract v1.2: no highlights. */
    val highlights: List<HighlightDto> = emptyList(),
    /** Absent from a backend older than contract v1.4: no notes. */
    val strokes: List<StrokeRecord> = emptyList(),
    /** Absent from a backend older than contract v1.7: no quality notes. */
    @SerialName("quality_warnings") val qualityWarnings: List<QualityWarningDto> = emptyList(),
    /** Absent from a backend older than contract v1.9: Italian. */
    val language: String? = null,
)

@Serializable
internal data class QualityWarningDto(
    val code: String = "",
    @SerialName("block_id") val blockId: String? = null,
    val message: String = "",
)

@Serializable
internal data class HighlightDto(
    @SerialName("block_id") val blockId: String,
    val start: Int,
    val end: Int,
    val color: String,
)

@Serializable
internal data class PutHighlightsRequest(
    val highlights: List<HighlightDto>,
)

@Serializable
internal data class PutHighlightsResponse(
    val highlights: List<HighlightDto> = emptyList(),
)

@Serializable
internal data class PutStrokesRequest(
    val strokes: List<StrokeRecord>,
)

@Serializable
internal data class PutStrokesResponse(
    val strokes: List<StrokeRecord> = emptyList(),
)

internal fun HighlightDto.toModel() = HighlightRecord(blockId, start, end, color)

internal fun HighlightRecord.toDto() = HighlightDto(blockId, start, end, color)

@Serializable
internal data class AttemptItemRequest(
    @SerialName("block_id") val blockId: String,
    @SerialName("attempt_number") val attemptNumber: Int,
    val answer: JsonObject,
    val outcome: String? = null,
    @SerialName("answered_at") val answeredAt: String,
)

@Serializable
internal data class SubmitAttemptsRequest(
    val attempts: List<AttemptItemRequest>,
)

@Serializable
internal data class SubmitAttemptsResponse(
    val attempts: List<AttemptDto> = emptyList(),
)

internal fun QueuedAttempt.toRequest() =
    AttemptItemRequest(
        blockId = blockId,
        attemptNumber = attemptNumber,
        answer = answer,
        outcome = outcome,
        answeredAt = Instant.ofEpochMilli(answeredAtMs).toString(),
    )

internal fun LessonDto.toModel() =
    Lesson(
        id = id,
        topicId = topicId,
        number = number,
        title = title,
        objective = objective,
        estimatedMinutes = estimatedMinutes,
        status = LessonStatus.fromWire(status),
        catalogVersion = catalogVersion,
        missionVersion = missionVersion,
        createdAt = parseInstantOrNull(createdAt),
        completedAt = parseInstantOrNull(completedAt),
        documentSources = documentSources.map { it.toModel() },
    )

internal fun DocumentSourceDto.toModel() = DocumentSource(documentId, title, pageStart, pageEnd)

internal fun LessonInProgressDto.toModel() =
    LessonInProgress(
        lesson =
            Lesson(
                id = id,
                topicId = topicId,
                number = number,
                title = title,
                objective = objective,
                estimatedMinutes = estimatedMinutes,
                status = LessonStatus.fromWire(status),
                catalogVersion = catalogVersion,
                missionVersion = missionVersion,
                createdAt = parseInstantOrNull(createdAt),
                completedAt = parseInstantOrNull(completedAt),
                documentSources = documentSources.map { it.toModel() },
            ),
        topicTitle = topicTitle,
        lastActivityAt = parseInstantOrNull(lastActivityAt),
    )

internal fun AttemptDto.toModel() =
    Attempt(
        id = id,
        blockId = blockId,
        blockType = blockType,
        attemptNumber = attemptNumber,
        answer = answer as? JsonObject ?: JsonObject(emptyMap()),
        outcome = AttemptOutcome.fromWire(outcome),
        grade =
            grade?.let {
                AttemptGrade(AttemptOutcome.fromWire(it.verdict), it.feedback, it.misconception)
            },
        answeredAt = parseInstantOrNull(answeredAt),
    )

internal fun LearningRecordDto.toModel() =
    LearningRecord(
        id = id,
        number = number,
        kind =
            when (kind) {
                "demonstrated" -> RecordKind.Demonstrated
                "misconception_corrected" -> RecordKind.MisconceptionCorrected
                else -> RecordKind.Unknown
            },
        title = title,
        body = body,
        evidence = evidence,
        // An unknown status is read as active: better shown than hidden.
        status = if (status == "superseded") RecordStatus.Superseded else RecordStatus.Active,
        supersededBy = supersededBy,
        lessonId = lessonId,
        createdAt = parseInstantOrNull(createdAt),
    )

internal fun GlossaryTermDto.toModel() =
    GlossaryTerm(
        id = id,
        key = key,
        term = term,
        definition = definition,
        avoid = avoid,
        lessonId = lessonId,
        createdAt = parseInstantOrNull(createdAt),
        updatedAt = parseInstantOrNull(updatedAt),
        firstLessonId = firstLessonId,
    )

internal fun LessonViewDto.toModel() =
    LessonView(
        lesson = lesson.toModel(),
        documentJson = ApiJson.encodeToString(JsonElement.serializer(), document),
        attempts = attempts.map { it.toModel() },
        completion =
            completion?.let {
                LessonCompletion(
                    job = it.job?.toModel(),
                    records = it.records.map(LearningRecordDto::toModel),
                    terms = it.terms.map(GlossaryTermDto::toModel),
                    reviewCardCount = it.reviewCardCount,
                )
            },
        highlights = highlights.map(HighlightDto::toModel),
        strokes = strokes,
        language = TopicLanguage.fromWire(language),
        // A warning without text would be an empty entry in the note.
        qualityWarnings =
            qualityWarnings.filter { it.message.isNotBlank() }.map {
                QualityWarning(
                    it.code,
                    it.blockId?.takeIf(String::isNotEmpty),
                    it.message,
                )
            },
    )
