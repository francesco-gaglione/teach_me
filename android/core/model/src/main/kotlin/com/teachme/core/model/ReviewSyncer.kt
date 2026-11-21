package com.teachme.core.model

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/** Why a flush stopped (review.md section 8). */
enum class ReviewFlushStatus {
    /** The queue is empty. */
    Done,

    /** No network, timeout or a server problem: what is left stays queued and is sent later. */
    Offline,

    /** `401`: the session ended. The queue is kept intact (it belongs to its user) and not retried by itself. */
    Unauthorized,
}

/** A grade the backend applied, with the card as it is now. */
data class SentReview(
    val review: QueuedReview,
    val card: ReviewCard,
)

data class ReviewFlushResult(
    val status: ReviewFlushStatus,
    val sent: List<SentReview>,
    /** Dropped because the card changed (`409` `already_reviewed` or `card_suspended`): already graded, resent or suspended. */
    val conflicts: List<QueuedReview>,
    /** Dropped for another final reason (`404` the card is gone, `422`/`400` a value that can never work). */
    val discarded: List<QueuedReview>,
) {
    companion object {
        val Empty = ReviewFlushResult(ReviewFlushStatus.Done, emptyList(), emptyList(), emptyList())
    }
}

/**
 * Sends the persistent queue of grades to the backend, one request per grade, oldest first (review.md section 8).
 * Idempotent: every grade carries the `review_count` it was given on, so a grade the backend already applied answers `409`
 * and is dropped, never applied twice. One flush at a time. A flush belongs to the user whose queue it read: it only
 * sends with that user's session, and stops if the user changes under it (ADR 0008).
 */
class ReviewSyncer(
    private val repository: ReviewRepository,
    private val queue: ReviewQueue,
    private val scopes: UserScopes,
) {
    private val lock = Mutex()

    suspend fun enqueue(review: QueuedReview) = queue.enqueue(review)

    suspend fun pending(): List<QueuedReview> = queue.pending()

    fun pendingCount(): Flow<Int> = queue.count()

    /** What [withdraw] found. */
    enum class Withdrawn {
        /** It had not been sent: removed from the queue, nothing else to do. */
        FromQueue,

        /** The backend applied it: it must be undone there. */
        Applied,

        /** The backend refused it (409, 404, 422): there is nothing to undo, it never counted. */
        Dropped,
    }

    private val outcomes = HashMap<Pair<String, Int>, Boolean>()

    private fun record(
        review: QueuedReview,
        applied: Boolean,
    ) {
        if (outcomes.size > MAX_OUTCOMES) outcomes.clear()
        outcomes[review.cardId to review.reviewCount] = applied
    }

    /**
     * Takes [review] back out of the queue, or says what happened to it. Waits for a request in flight, so the answer is
     * final: undoing a grade the backend refused must never reach the backend (it would undo someone else's grade).
     */
    suspend fun withdraw(review: QueuedReview): Withdrawn =
        lock.withLock {
            when {
                queue.remove(review) -> Withdrawn.FromQueue
                outcomes[review.cardId to review.reviewCount] == false -> Withdrawn.Dropped
                else -> Withdrawn.Applied
            }
        }

    /** Empties the queue as far as the network allows. */
    suspend fun flush(): ReviewFlushResult =
        lock.withLock {
            val owner = scopes.current() ?: return@withLock ReviewFlushResult.Empty
            val sent = mutableListOf<SentReview>()
            val conflicts = mutableListOf<QueuedReview>()
            val discarded = mutableListOf<QueuedReview>()
            var status = ReviewFlushStatus.Done
            for (review in queue.pending()) {
                // A request that went out is recorded even if the caller goes away: a grade the backend applied but the
                // app forgot would come back as a false 409.
                withContext(NonCancellable + ExpectedUserScope(owner)) {
                    val result = repository.rate(review)
                    if (scopes.current() != owner) {
                        // The user changed while the request was out: the queue in front of us is not the one we read.
                        status = ReviewFlushStatus.Offline
                        return@withContext
                    }
                    when (result) {
                        is ApiResult.Success -> {
                            queue.remove(review)
                            record(review, true)
                            sent += SentReview(review, result.value)
                        }
                        is ApiResult.Failure ->
                            when (result.failure) {
                                // The specific `409` codes and an unknown one mean the same here: the card changed.
                                ApiFailure.Conflict, is ApiFailure.ReviewConflict -> {
                                    queue.remove(review)
                                    record(review, false)
                                    conflicts += review
                                }
                                ApiFailure.NotFound,
                                ApiFailure.InvalidId,
                                is ApiFailure.Validation,
                                is ApiFailure.BadRequest,
                                -> {
                                    queue.remove(review)
                                    record(review, false)
                                    discarded += review
                                }
                                ApiFailure.Unauthorized -> status = ReviewFlushStatus.Unauthorized
                                // No network, a server problem, a limit: the grade stays queued and goes out later.
                                is ApiFailure.Network,
                                is ApiFailure.Unexpected,
                                is ApiFailure.RateLimited,
                                ApiFailure.Unavailable,
                                ApiFailure.InvalidCredentials,
                                ApiFailure.LlmKeyMissing,
                                ApiFailure.LlmKeyRejected,
                                ApiFailure.LlmCreditExhausted,
                                -> status = ReviewFlushStatus.Offline
                            }
                    }
                }
                if (status != ReviewFlushStatus.Done) break
            }
            ReviewFlushResult(status, sent, conflicts, discarded)
        }

    /**
     * Flushes in [scope] and waits at most [timeoutMs]: Home and the end of a session wait a little so what they
     * show is up to date, but never for long. The flush itself is not cancelled by the timeout (a request that went
     * out must be recorded). Null when it did not finish in time.
     */
    suspend fun flushWithin(
        scope: CoroutineScope,
        timeoutMs: Long,
    ): ReviewFlushResult? {
        val job = scope.async { flush() }
        return withTimeoutOrNull(timeoutMs) { job.await() }
    }
}

private const val MAX_OUTCOMES = 500
