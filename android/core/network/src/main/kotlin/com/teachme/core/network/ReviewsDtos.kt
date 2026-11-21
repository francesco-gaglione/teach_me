package com.teachme.core.network

import com.teachme.core.model.DueReviews
import com.teachme.core.model.DueTopic
import com.teachme.core.model.IntervalsPreview
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.ReviewCardKind
import com.teachme.core.model.ReviewCardStatus
import com.teachme.core.model.ReviewState
import com.teachme.core.model.SuspendedReviews
import com.teachme.core.model.SuspensionReason
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import java.time.Instant
import java.time.OffsetDateTime
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter

private fun parseInstantOrNull(value: String?): Instant? =
    value?.let { runCatching { OffsetDateTime.parse(it).toInstant() }.getOrNull() }

/** RFC 3339, UTC, always three fractional digits (api-conventions section 5). */
private val MILLIS_UTC: DateTimeFormatter =
    DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'").withZone(ZoneOffset.UTC)

@Serializable
internal data class IntervalsPreviewDto(
    val again: Long? = null,
    val hard: Long? = null,
    val good: Long? = null,
    val easy: Long? = null,
)

@Serializable
internal data class ReviewCardDto(
    val id: String,
    @SerialName("topic_id") val topicId: String = "",
    @SerialName("topic_title") val topicTitle: String = "",
    @SerialName("lesson_id") val lessonId: String? = null,
    @SerialName("lesson_number") val lessonNumber: Int? = null,
    @SerialName("lesson_title") val lessonTitle: String? = null,
    val kind: String? = null,
    val front: String = "",
    val back: String = "",
    val status: String? = null,
    val suspension: String? = null,
    @SerialName("suspended_at") val suspendedAt: String? = null,
    // Absent from older backends: false.
    @SerialName("to_consolidate") val toConsolidate: Boolean = false,
    val state: String? = null,
    val due: String? = null,
    @SerialName("last_reviewed_at") val lastReviewedAt: String? = null,
    // No default: a card without its version cannot be graded safely, so the response is invalid.
    @SerialName("review_count") val reviewCount: Int,
    @SerialName("intervals_preview") val intervalsPreview: IntervalsPreviewDto? = null,
)

@Serializable
internal data class DueReviewsDto(
    val items: List<ReviewCardDto> = emptyList(),
    @SerialName("next_cursor") val nextCursor: String? = null,
    @SerialName("due_count") val dueCount: Int = 0,
    @SerialName("estimated_minutes") val estimatedMinutes: Int = 0,
    @SerialName("next_due_at") val nextDueAt: String? = null,
    val topics: List<DueTopicDto> = emptyList(),
)

@Serializable
internal data class DueTopicDto(
    @SerialName("topic_id") val topicId: String = "",
    @SerialName("topic_title") val topicTitle: String = "",
    @SerialName("due_count") val dueCount: Int = 0,
)

@Serializable
internal data class SuspendedReviewsDto(
    val items: List<ReviewCardDto> = emptyList(),
    @SerialName("next_cursor") val nextCursor: String? = null,
    @SerialName("suspended_count") val suspendedCount: Int = 0,
)

@Serializable
internal data class RateRequest(
    val rating: String,
    @SerialName("reviewed_at") val reviewedAt: String,
    @SerialName("review_count") val reviewCount: Int,
)

internal fun QueuedReview.toRequest() =
    RateRequest(rating, MILLIS_UTC.format(Instant.ofEpochMilli(reviewedAtMs)), reviewCount)

internal fun ReviewCardDto.toModel() =
    ReviewCard(
        id = id,
        topicId = topicId,
        topicTitle = topicTitle,
        lessonId = lessonId,
        lessonNumber = lessonNumber,
        lessonTitle = lessonTitle,
        kind = ReviewCardKind.fromWire(kind),
        front = front,
        back = back,
        status = ReviewCardStatus.fromWire(status),
        suspension = SuspensionReason.fromWire(suspension),
        suspendedAt = parseInstantOrNull(suspendedAt),
        toConsolidate = toConsolidate,
        state = ReviewState.fromWire(state),
        due = parseInstantOrNull(due),
        lastReviewedAt = parseInstantOrNull(lastReviewedAt),
        reviewCount = reviewCount,
        intervals = intervalsPreview?.let { IntervalsPreview(it.again, it.hard, it.good, it.easy) },
    )

internal fun DueReviewsDto.toModel() =
    DueReviews(
        items.map { it.toModel() },
        dueCount,
        estimatedMinutes,
        parseInstantOrNull(nextDueAt),
        topics.filter { it.topicId.isNotEmpty() }.map { DueTopic(it.topicId, it.topicTitle, it.dueCount) },
    )

internal fun SuspendedReviewsDto.toModel() = SuspendedReviews(items.map { it.toModel() }, nextCursor, suspendedCount)
