package com.teachme.core.model

import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.Serializable
import java.time.Instant

/** The four grades of a review (reviews.md section 4). The order is the order shown to the learner. */
enum class ReviewRating(
    val wire: String,
) {
    Again("again"),
    Hard("hard"),
    Good("good"),
    Easy("easy"),
    ;

    companion object {
        fun fromWire(value: String?): ReviewRating? = entries.firstOrNull { it.wire == value }
    }
}

/** `ReviewCard.kind`: where the card comes from. A value this app does not know is read as [Unknown] (generic card). */
enum class ReviewCardKind(
    val wire: String?,
) {
    Record("record"),
    Term("term"),
    Unknown(null),
    ;

    companion object {
        fun fromWire(value: String?): ReviewCardKind =
            entries.firstOrNull { it.wire != null && it.wire == value } ?: Unknown
    }
}

/** `ReviewCard.status`. A value this app does not know is read as [Active]. */
enum class ReviewCardStatus(
    val wire: String,
) {
    Active("active"),
    Suspended("suspended"),
    ;

    companion object {
        fun fromWire(value: String?): ReviewCardStatus = entries.firstOrNull { it.wire == value } ?: Active
    }
}

/**
 * `ReviewCard.suspension` (reviews.md section 2): why a card is suspended. Only [User] can be reactivated. A value this
 * app does not know is read as [Superseded] (not reactivatable): the safe reading.
 */
enum class SuspensionReason(
    val wire: String,
) {
    User("user"),
    Superseded("superseded"),
    ;

    companion object {
        /** Null in, null out (an active card); anything unknown is [Superseded]. */
        fun fromWire(value: String?): SuspensionReason? =
            value?.let { v -> entries.firstOrNull { it.wire == v } ?: Superseded }
    }
}

/** FSRS state of a card. A value this app does not know is read as [Review]. */
enum class ReviewState(
    val wire: String,
) {
    New("new"),
    Learning("learning"),
    Review("review"),
    Relearning("relearning"),
    ;

    companion object {
        fun fromWire(value: String?): ReviewState = entries.firstOrNull { it.wire == value } ?: Review
    }
}

/**
 * Seconds between the grade and the new due date, for each grade, as the backend would apply it now (no fuzz). A field
 * that is missing or not positive is null: the screen then writes no interval.
 */
data class IntervalsPreview(
    val again: Long?,
    val hard: Long?,
    val good: Long?,
    val easy: Long?,
) {
    fun forRating(rating: ReviewRating): Long? =
        when (rating) {
            ReviewRating.Again -> again
            ReviewRating.Hard -> hard
            ReviewRating.Good -> good
            ReviewRating.Easy -> easy
        }?.takeIf { it > 0 }
}

/** A flashcard (reviews.md section 2). [front] and [back] are inline markdown. */
data class ReviewCard(
    val id: String,
    val topicId: String,
    val topicTitle: String,
    val lessonId: String?,
    val lessonNumber: Int?,
    val lessonTitle: String?,
    val kind: ReviewCardKind,
    val front: String,
    val back: String,
    val status: ReviewCardStatus,
    /** Why the card is suspended; null when it is active. */
    val suspension: SuspensionReason? = null,
    /** When it was last suspended; null when it is active. */
    val suspendedAt: Instant? = null,
    /** True when the card tests an exercise the user has not passed yet (reviews.md section 1.3). */
    val toConsolidate: Boolean = false,
    val state: ReviewState,
    val due: Instant?,
    val lastReviewedAt: Instant?,
    /** Votes applied so far: also the version of the card for idempotent votes (reviews.md section 4). */
    val reviewCount: Int,
    /** Null when the card is suspended. */
    val intervals: IntervalsPreview?,
)

/** `GET /reviews/due` (reviews.md section 3.1). */
data class DueReviews(
    val items: List<ReviewCard>,
    val dueCount: Int,
    val estimatedMinutes: Int,
    /** First future due date among the active cards; null when there is none. */
    val nextDueAt: Instant?,
    /** Topics with cards due now, always over all the user's cards (ignores the topic filter). */
    val topics: List<DueTopic> = emptyList(),
)

/** One element of `DueReviews.topics` (reviews.md section 3.1). */
data class DueTopic(
    val topicId: String,
    val topicTitle: String,
    val dueCount: Int,
)

/** `GET /reviews/suspended` (reviews.md section 3.3): one page of the cards the user suspended by hand. */
data class SuspendedReviews(
    /** Most recently suspended first. */
    val items: List<ReviewCard>,
    val nextCursor: String?,
    /** Total of cards suspended by hand, not only this page. */
    val suspendedCount: Int,
)

/**
 * A grade waiting to be sent (reviews.md section 4), persisted in the local queue. The pair ([cardId], [reviewCount]) is
 * its identity: it is the version of the card the grade was given on, so sending it twice can never apply it twice.
 */
@Serializable
data class QueuedReview(
    val cardId: String,
    /** Wire value of a [ReviewRating]. */
    val rating: String,
    /** Epoch milliseconds of the moment the learner graded (the app's clock). */
    val reviewedAtMs: Long,
    val reviewCount: Int,
) {
    fun sameReview(other: QueuedReview): Boolean = cardId == other.cardId && reviewCount == other.reviewCount
}

/** Persistent FIFO queue of grades not yet registered by the backend. Survives restarts. */
interface ReviewQueue {
    /** Adds [review], replacing one of the same pair. */
    suspend fun enqueue(review: QueuedReview)

    /** All waiting grades, oldest first. */
    suspend fun pending(): List<QueuedReview>

    /** Removes exactly this grade (by pair); true when it was there. */
    suspend fun remove(review: QueuedReview): Boolean

    /** How many grades are waiting; emits on every change. */
    fun count(): Flow<Int>
}

/** Spaced repetition (docs/contracts/reviews.md). No cache: every call goes out. */
interface ReviewRepository {
    /** The cards due now, most overdue first, and the totals. [limit] is the size of the page. [topicId] limits to one topic ([ApiFailure.NotFound] if it is gone). */
    suspend fun due(
        limit: Int,
        topicId: String? = null,
    ): ApiResult<DueReviews>

    /**
     * `POST /reviews/{id}`: [ApiFailure.ReviewConflict.AlreadyReviewed] when the card changed since
     * [QueuedReview.reviewCount] was read, [ApiFailure.ReviewConflict.CardSuspended] when it is suspended.
     */
    suspend fun rate(review: QueuedReview): ApiResult<ReviewCard>

    /**
     * `POST /reviews/{id}/undo`: [ApiFailure.ReviewConflict.NothingToUndo] or [ApiFailure.ReviewConflict.UndoExpired]
     * when nothing can be undone any more.
     */
    suspend fun undo(cardId: String): ApiResult<ReviewCard>

    /** `POST /reviews/{id}/suspend` (idempotent). */
    suspend fun suspend(cardId: String): ApiResult<ReviewCard>

    /**
     * `POST /reviews/{id}/unsuspend` (idempotent). [ApiFailure.ReviewConflict.CardSuperseded] when the card was
     * superseded: it cannot come back. The "Annulla" of a reactivation is [suspend] again.
     */
    suspend fun unsuspend(cardId: String): ApiResult<ReviewCard>

    /** `GET /reviews/suspended`: cards suspended by hand, newest first; pass the previous page's `nextCursor`. */
    suspend fun suspended(
        limit: Int,
        cursor: String? = null,
    ): ApiResult<SuspendedReviews>
}

/** Cards in one session (review.md section 7). */
const val REVIEW_SESSION_SIZE = 20

/** Seconds the contract (and the screens) count per card. */
const val REVIEW_SECONDS_PER_CARD = 15

/** Minutes the Home card announces for [dueCount] cards: one session at most (home.md, "Ripasso di oggi"). */
fun sessionMinutes(dueCount: Int): Int {
    val cards = dueCount.coerceIn(0, REVIEW_SESSION_SIZE)
    return (cards * REVIEW_SECONDS_PER_CARD + 59) / 60
}

/** The app's wall clock, injectable so tests do not depend on the time of day. */
fun interface AppClock {
    fun nowMs(): Long
}
