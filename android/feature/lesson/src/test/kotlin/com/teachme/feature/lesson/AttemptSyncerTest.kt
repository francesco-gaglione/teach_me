package com.teachme.feature.lesson

import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.AttemptOutcome
import com.teachme.core.model.QueuedAttempt
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AttemptSyncerTest {
    private val backend = FakeLessons()
    private val queue = MemoryQueue()
    private val syncer =
        AttemptSyncer(
            backend,
            queue,
            com.teachme.core.model
                .ManualUserScopes(),
        )

    private fun closed(
        block: String,
        number: Int = 1,
    ) = QueuedAttempt("L1", block, "quiz_single", number, buildJsonObject { put("option", "a") }, "correct", 1L)

    private fun open(block: String) =
        QueuedAttempt(
            "L1",
            block,
            "open_answer",
            1,
            buildJsonObject {
                put("text", "t")
            },
            null,
            1L,
        )

    @Test
    fun `closed exercises go in batches of 50 and each open answer travels alone, last`() =
        runTest {
            queue.enqueue(open("o1"))
            repeat(51) { queue.enqueue(closed("q$it")) }
            queue.enqueue(open("o2"))
            val result = syncer.flush("L1")
            assertEquals(FlushStatus.Done, result.status)
            assertEquals(listOf(50, 1, 1, 1), backend.requests.map { it.size })
            assertTrue(
                backend.requests
                    .take(2)
                    .flatten()
                    .none { it.isOpenAnswer },
            )
            assertEquals(listOf("o1", "o2"), backend.requests.drop(2).map { it.single().blockId })
            assertEquals(0, queue.size)
            assertEquals(53, result.saved.size)
        }

    @Test
    fun `an invalid attempt is found by its index, dropped, and the rest is sent`() =
        runTest {
            queue.enqueue(closed("q0"))
            queue.enqueue(closed("q1"))
            queue.enqueue(closed("q2"))
            backend.failures += ApiFailure.Validation(listOf(ApiErrorDetail("attempts[1].answer", "invalid")))
            val result = syncer.flush("L1")
            assertEquals(listOf("q1"), result.discarded.map { it.blockId })
            assertEquals(listOf("q0", "q2"), result.saved.map { it.queued.blockId })
            assertEquals(0, queue.size)
        }

    @Test
    fun `a 422 without an index is narrowed down one attempt at a time`() =
        runTest {
            queue.enqueue(closed("q0"))
            queue.enqueue(closed("bad"))
            queue.enqueue(closed("q2"))
            // The backend rejects any request that contains "bad", without saying where.
            val flaky =
                object : FakeLessonsDelegate(backend) {
                    override fun reject(batch: List<QueuedAttempt>) = batch.any { it.blockId == "bad" }
                }
            val result =
                AttemptSyncer(
                    flaky,
                    queue,
                    com.teachme.core.model
                        .ManualUserScopes(),
                ).flush("L1")
            assertEquals(listOf("bad"), result.discarded.map { it.blockId })
            assertEquals(setOf("q0", "q2"), result.saved.map { it.queued.blockId }.toSet())
            assertEquals(0, queue.size)
        }

    @Test
    fun `offline keeps everything, 409 and 404 empty the queue`() =
        runTest {
            queue.enqueue(closed("q0"))
            backend.failures += ApiFailure.Network(com.teachme.core.model.UnreachableReason.Timeout)
            assertEquals(FlushStatus.Offline, syncer.flush("L1").status)
            assertEquals(1, queue.size)
            backend.failures += ApiFailure.Unexpected(502)
            assertEquals(FlushStatus.Offline, syncer.flush("L1").status)
            backend.failures += ApiFailure.Conflict
            assertEquals(FlushStatus.Closed, syncer.flush("L1").status)
            assertEquals(0, queue.size)
            queue.enqueue(closed("q0"))
            backend.failures += ApiFailure.NotFound
            assertEquals(FlushStatus.Gone, syncer.flush("L1").status)
            assertEquals(0, queue.size)
        }

    @Test
    fun `the same pair queued twice is sent once and the backend keeps the first`() =
        runTest {
            queue.enqueue(closed("q0"))
            queue.enqueue(closed("q0").copy(outcome = "incorrect"))
            syncer.flush("L1")
            assertEquals(1, backend.requests.single().size)
            // Sent again after a lost answer: the backend returns what it stored.
            queue.enqueue(closed("q0").copy(outcome = "incorrect", answeredAtMs = 99L))
            val again = syncer.flush("L1")
            assertEquals(
                AttemptOutcome.Incorrect,
                again.saved
                    .single()
                    .attempt.outcome,
            )
            assertEquals(1, backend.store.size)
        }

    @Test
    fun `the saved attempts are reported batch by batch while the queue goes out`() =
        runTest {
            queue.enqueue(closed("q0"))
            queue.enqueue(open("o1"))
            val seen = mutableListOf<List<String>>()
            syncer.flush("L1") { seen += it.map { s -> s.queued.blockId } }
            assertEquals(listOf(listOf("q0"), listOf("o1")), seen)
        }

    // region Phase 11: data of a user never leaves with the session of another

    private val userA =
        com.teachme.core.model
            .UserScope("user-a", "http://teach.test")
    private val userB =
        com.teachme.core.model
            .UserScope("user-b", "http://teach.test")

    /** One [MemoryQueue] per user, picked by the current scope, like the DataStore one. */
    private class PerUserQueue(
        private val scopes: com.teachme.core.model.ManualUserScopes,
    ) : com.teachme.core.model.AttemptQueue {
        val byUser = HashMap<com.teachme.core.model.UserScope, MemoryQueue>()

        private suspend fun q() = byUser.getOrPut(scopes.current()!!) { MemoryQueue() }

        override suspend fun enqueue(attempt: QueuedAttempt) = q().enqueue(attempt)

        override suspend fun pending(lessonId: String) = q().pending(lessonId)

        override suspend fun lessonsWithPending() = q().lessonsWithPending()

        override suspend fun pendingTotal() = q().pendingTotal()

        override suspend fun remove(attempts: List<QueuedAttempt>) = q().remove(attempts)

        override suspend fun clear(lessonId: String) = q().clear(lessonId)

        override fun count(lessonId: String) = kotlinx.coroutines.flow.flowOf(0)
    }

    @Test
    fun `answers of user A are not sent with the session of B, and every request is tied to A`() =
        runTest {
            val scopes =
                com.teachme.core.model
                    .ManualUserScopes(userA)
            val perUser = PerUserQueue(scopes)
            var tiedTo: com.teachme.core.model.UserScope? = null
            val repo =
                object : com.teachme.core.model.LessonRepository by backend {
                    override suspend fun submitAttempts(
                        lessonId: String,
                        attempts: List<QueuedAttempt>,
                    ): com.teachme.core.model.ApiResult<List<com.teachme.core.model.Attempt>> {
                        tiedTo = kotlin.coroutines.coroutineContext[com.teachme.core.model.ExpectedUserScope]?.scope
                        return backend.submitAttempts(lessonId, attempts)
                    }
                }
            val syncer = AttemptSyncer(repo, perUser, scopes)
            syncer.enqueue(closed("q1"))

            scopes.switchTo(userB)
            syncer.flush("L1")
            syncer.flushAll()
            assertTrue(backend.requests.isEmpty())

            scopes.switchTo(userA)
            syncer.flush("L1")
            assertEquals(1, backend.requests.size)
            assertEquals(userA, tiedTo)
        }

    @Test
    fun `if the user changes while a batch is out, the queue of A stays as it was and B is untouched`() =
        runTest {
            val scopes =
                com.teachme.core.model
                    .ManualUserScopes(userA)
            val perUser = PerUserQueue(scopes)
            val repo =
                object : com.teachme.core.model.LessonRepository by backend {
                    override suspend fun submitAttempts(
                        lessonId: String,
                        attempts: List<QueuedAttempt>,
                    ): com.teachme.core.model.ApiResult<List<com.teachme.core.model.Attempt>> {
                        scopes.switchTo(userB)
                        return backend.submitAttempts(lessonId, attempts)
                    }
                }
            val syncer = AttemptSyncer(repo, perUser, scopes)
            syncer.enqueue(closed("q1"))

            val result = syncer.flush("L1")

            assertEquals(FlushStatus.Offline, result.status)
            assertTrue(result.saved.isEmpty())
            assertEquals(1, perUser.byUser.getValue(userA).size)
            assertTrue(perUser.byUser[userB]?.size ?: 0 == 0)
        }

    // endregion
}

/** Wraps [FakeLessons] to refuse a request by its content, like a backend that rejects a batch without an index. */
internal abstract class FakeLessonsDelegate(
    private val inner: FakeLessons,
) : com.teachme.core.model.LessonRepository by inner {
    abstract fun reject(batch: List<QueuedAttempt>): Boolean

    override suspend fun submitAttempts(
        lessonId: String,
        attempts: List<QueuedAttempt>,
    ) = if (reject(attempts)) {
        com.teachme.core.model.ApiResult
            .Failure(ApiFailure.Validation(emptyList()))
    } else {
        inner.submitAttempts(lessonId, attempts)
    }
}
