package com.teachme.feature.review

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.DueReviews
import com.teachme.core.model.DueTopic
import com.teachme.core.model.IntervalsPreview
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.ReviewCardKind
import com.teachme.core.model.ReviewCardStatus
import com.teachme.core.model.ReviewQueue
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.ReviewState
import com.teachme.core.model.SuspendedReviews
import com.teachme.core.model.UnreachableReason
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import java.time.Instant

internal val NETWORK_DOWN = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout))

internal fun card(
    id: String,
    count: Int = 0,
    front: String = "Fronte $id",
) = ReviewCard(
    id = id,
    topicId = "t",
    topicTitle = "Rust",
    lessonId = "l",
    lessonNumber = 2,
    lessonTitle = "Result",
    kind = ReviewCardKind.Record,
    front = front,
    back = "Retro $id",
    status = ReviewCardStatus.Active,
    state = ReviewState.Review,
    due = null,
    lastReviewedAt = null,
    reviewCount = count,
    intervals = IntervalsPreview(600, 172_800, 432_000, 950_400),
)

internal class MemoryReviewQueue : ReviewQueue {
    private val items = MutableStateFlow<List<QueuedReview>>(emptyList())

    override suspend fun enqueue(review: QueuedReview) {
        items.value = items.value.filterNot { it.sameReview(review) } + review
    }

    override suspend fun pending() = items.value

    override suspend fun remove(review: QueuedReview): Boolean {
        val had = items.value.any { it.sameReview(review) }
        items.value = items.value.filterNot { it.sameReview(review) }
        return had
    }

    override fun count(): Flow<Int> = items.map { it.size }
}

/** Scriptable backend: every answer is a lambda a test can swap, every call is recorded. */
internal class FakeReviewRepository : ReviewRepository {
    var session: List<ReviewCard> = emptyList()
    var dueCount: Int = 0
    var nextDueAt: Instant? = null
    var dueDelayMs: Long = 0
    var dueFailure: ApiFailure? = null
    val dueLimits = mutableListOf<Int>()
    val dueTopicIds = mutableListOf<String?>()

    /** The failure for a given topic filter (null = none). Takes precedence over [dueFailure]. */
    var dueFailureFor: (String?) -> ApiFailure? = { null }
    var topicsDue: List<DueTopic> = emptyList()

    var rateResult: (QueuedReview) -> ApiResult<ReviewCard> = { ApiResult.Success(card(it.cardId, it.reviewCount + 1)) }
    var rateGate: (suspend () -> Unit)? = null
    val rated = mutableListOf<QueuedReview>()

    var undoResult: (String) -> ApiResult<ReviewCard> = { ApiResult.Success(card(it, 3)) }
    val undone = mutableListOf<String>()

    var suspendResult: (String) -> ApiResult<ReviewCard> = { ApiResult.Success(card(it)) }
    val suspended = mutableListOf<String>()

    var unsuspendResult: (String) -> ApiResult<ReviewCard> = { ApiResult.Success(card(it)) }
    val unsuspended = mutableListOf<String>()

    var suspendedPage: ApiResult<SuspendedReviews> = ApiResult.Success(SuspendedReviews(emptyList(), null, 0))
    val suspendedRequests = mutableListOf<Pair<Int, String?>>()

    override suspend fun due(
        limit: Int,
        topicId: String?,
    ): ApiResult<DueReviews> {
        dueLimits += limit
        dueTopicIds += topicId
        dueFailureFor(topicId)?.let { return ApiResult.Failure(it) }
        if (dueDelayMs > 0) delay(dueDelayMs)
        dueFailure?.let { return ApiResult.Failure(it) }
        return ApiResult.Success(DueReviews(session.take(limit), dueCount, 0, nextDueAt, topicsDue))
    }

    override suspend fun rate(review: QueuedReview): ApiResult<ReviewCard> {
        rated += review
        rateGate?.invoke()
        return rateResult(review)
    }

    override suspend fun undo(cardId: String): ApiResult<ReviewCard> {
        undone += cardId
        return undoResult(cardId)
    }

    override suspend fun suspend(cardId: String): ApiResult<ReviewCard> {
        suspended += cardId
        return suspendResult(cardId)
    }

    override suspend fun unsuspend(cardId: String): ApiResult<ReviewCard> {
        unsuspended += cardId
        return unsuspendResult(cardId)
    }

    override suspend fun suspended(
        limit: Int,
        cursor: String?,
    ): ApiResult<SuspendedReviews> {
        suspendedRequests += limit to cursor
        return suspendedPage
    }
}

internal class FakeClock(
    var now: Long = 1_000_000L,
) : AppClock {
    override fun nowMs() = now
}
