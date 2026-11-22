package com.teachme.feature.lesson

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.Attempt
import com.teachme.core.model.AttemptQueue
import com.teachme.core.model.ExpectedUserScope
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.QueuedAttempt
import com.teachme.core.model.UserScopes
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

/** Most attempts in one request (lessons.md section 5). */
internal const val MAX_BATCH = 50

/** The backend's answer for one queued attempt. */
data class SavedAttempt(
    val queued: QueuedAttempt,
    val attempt: Attempt,
    /** The `Date` of the response that saved it (the server's clock, one-second precision), when it had one. */
    val serverDate: java.time.Instant? = null,
)

/** Why a flush stopped (lesson.md sections 7.2 and 8.1). */
enum class FlushStatus {
    /** The queue of the lesson is empty. */
    Done,

    /** No network, timeout or a server problem: what is left stays queued and is sent later. */
    Offline,

    /** `409`: the lesson is `completed` (for example from another device); the queue was emptied. */
    Closed,

    /** `404`: the lesson does not exist any more; the queue was emptied. */
    Gone,

    /** `401`: the session ended. The queue is kept intact (it belongs to its user) and not retried by itself. */
    Unauthorized,
}

data class FlushResult(
    val status: FlushStatus,
    /** Every attempt this flush tried to send, saved or not. */
    val attempted: List<QueuedAttempt>,
    val saved: List<SavedAttempt>,
    /** Refused for good (`422`) and dropped from the queue. */
    val discarded: List<QueuedAttempt>,
)

/**
 * Sends the persistent queue of attempts to the backend (lesson.md section 8.1). Idempotent: the backend keeps the
 * first attempt of every (block, attempt number) pair, so sending again never duplicates. Closed exercises go first, in
 * batches of up to [MAX_BATCH]; each `open_answer` travels alone because the Grader answers inside the request. One
 * flush at a time, so a slow evaluation never overtakes or doubles another request.
 */
@Singleton
class AttemptSyncer
    @Inject
    constructor(
        private val lessons: LessonRepository,
        private val queue: AttemptQueue,
        private val scopes: UserScopes,
    ) {
        private val lock = Mutex()

        suspend fun enqueue(attempt: QueuedAttempt) = queue.enqueue(attempt)

        suspend fun hasPending(lessonId: String): Boolean = queue.pending(lessonId).isNotEmpty()

        suspend fun lessonsWithPending(): List<String> = queue.lessonsWithPending()

        /**
         * Sends the queues of every lesson that has attempts waiting (app start, back in the foreground), so answers do not
         * depend on the lesson being on screen. Stops at the first refused token.
         */
        suspend fun flushAll() {
            for (id in queue.lessonsWithPending()) {
                if (flush(id).status == FlushStatus.Unauthorized) return
            }
        }

        /** Empties the queue of [lessonId] as far as the network allows; [onSaved] sees every batch as soon as it is saved. */
        suspend fun flush(
            lessonId: String,
            onSaved: suspend (List<SavedAttempt>) -> Unit = {},
        ): FlushResult =
            lock.withLock {
                // A flush belongs to the user whose queue it reads (ADR 0008): it only goes out with that user's session.
                val owner =
                    scopes.current()
                        ?: return@withLock FlushResult(FlushStatus.Done, emptyList(), emptyList(), emptyList())
                val attempted = mutableListOf<QueuedAttempt>()
                val saved = mutableListOf<SavedAttempt>()
                val discarded = mutableListOf<QueuedAttempt>()
                var singles = false
                var status = FlushStatus.Done
                while (true) {
                    if (scopes.current() != owner) {
                        status = FlushStatus.Offline
                        break
                    }
                    val pending = queue.pending(lessonId)
                    if (pending.isEmpty()) break
                    val batch = nextBatch(pending, singles)
                    attempted += batch.filter { b -> attempted.none { it.sameAttempt(b) } }
                    val response = withContext(ExpectedUserScope(owner)) { lessons.submitAttempts(lessonId, batch) }
                    if (scopes.current() != owner) {
                        // The user changed while the request was out: the queue in front of us is not the one we read.
                        status = FlushStatus.Offline
                        break
                    }
                    val failure =
                        when (val result = response) {
                            is ApiResult.Success -> {
                                queue.remove(batch)
                                val pairs = pair(batch, result.value, result.serverDate)
                                saved += pairs
                                if (pairs.isNotEmpty()) onSaved(pairs)
                                null
                            }
                            is ApiResult.Failure -> result.failure
                        } ?: continue
                    when (failure) {
                        ApiFailure.Conflict -> {
                            queue.clear(lessonId)
                            status = FlushStatus.Closed
                        }
                        ApiFailure.NotFound -> {
                            queue.clear(lessonId)
                            status = FlushStatus.Gone
                        }
                        is ApiFailure.Validation, is ApiFailure.BadRequest, ApiFailure.InvalidId -> {
                            // One invalid attempt rejects the whole request: find it, drop it, send the rest.
                            val index =
                                (failure as? ApiFailure.Validation)?.let(::badIndex)?.takeIf {
                                    it in
                                        batch.indices
                                }
                            val bad =
                                if (index != null) {
                                    listOf(batch[index])
                                } else if (batch.size == 1) {
                                    batch
                                } else {
                                    null
                                }
                            if (bad == null) {
                                singles = true
                            } else {
                                queue.remove(bad)
                                discarded += bad
                            }
                            continue
                        }
                        ApiFailure.Unauthorized -> status = FlushStatus.Unauthorized
                        // No network, a server problem, a limit: what is left stays queued and is sent later. The key
                        // codes cannot come from this call (users.md section 3): an open answer waits for its grade.
                        is ApiFailure.Network,
                        is ApiFailure.Unexpected,
                        is ApiFailure.RateLimited,
                        ApiFailure.Unavailable,
                        ApiFailure.InvalidCredentials,
                        ApiFailure.LlmKeyMissing,
                        ApiFailure.LlmKeyRejected,
                        ApiFailure.LlmCreditExhausted,
                        is ApiFailure.LlmBudgetExceeded,
                        ApiFailure.StorageQuotaExceeded,
                        is ApiFailure.ReviewConflict,
                        -> status = FlushStatus.Offline
                    }
                    break
                }
                FlushResult(status, attempted, saved, discarded)
            }

        private fun nextBatch(
            pending: List<QueuedAttempt>,
            singles: Boolean,
        ): List<QueuedAttempt> {
            val closed = pending.filter { !it.isOpenAnswer }
            return when {
                closed.isNotEmpty() -> closed.take(if (singles) 1 else MAX_BATCH)
                else -> listOf(pending.first())
            }
        }

        /** The response lists the attempts as saved, in the order sent; a mismatch falls back to block and number. */
        private fun pair(
            batch: List<QueuedAttempt>,
            attempts: List<Attempt>,
            serverDate: java.time.Instant?,
        ): List<SavedAttempt> =
            batch.mapIndexedNotNull { i, queued ->
                val attempt =
                    attempts.getOrNull(i)?.takeIf { it.blockId == queued.blockId }
                        ?: attempts.firstOrNull {
                            it.blockId == queued.blockId &&
                                it.attemptNumber == queued.attemptNumber
                        }
                attempt?.let { SavedAttempt(queued, it, serverDate) }
            }

        /** `attempts[3].answer` -> 3. */
        private fun badIndex(failure: ApiFailure.Validation): Int? =
            failure.details.firstNotNullOfOrNull { d ->
                d.field
                    ?.let(INDEX::find)
                    ?.groupValues
                    ?.get(1)
                    ?.toIntOrNull()
            }

        private companion object {
            val INDEX = Regex("""attempts\[(\d+)]""")
        }
    }
