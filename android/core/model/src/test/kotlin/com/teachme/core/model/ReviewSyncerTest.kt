package com.teachme.core.model

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ReviewSyncerTest {
    private fun review(
        card: String = "c1",
        count: Int = 0,
        rating: String = "good",
        at: Long = 1L,
    ) = QueuedReview(card, rating, at, count)

    /** Answers every card with a scripted result and records the order of the requests. */
    private class FakeRepository : ReviewRepository {
        val calls = mutableListOf<QueuedReview>()
        val script = HashMap<String, ApiFailure>()
        var gate: (suspend () -> Unit)? = null

        override suspend fun due(
            limit: Int,
            topicId: String?,
        ): ApiResult<DueReviews> = error("not used")

        override suspend fun rate(review: QueuedReview): ApiResult<ReviewCard> {
            gate?.invoke()
            calls += review
            script[review.cardId]?.let { return ApiResult.Failure(it) }
            return ApiResult.Success(card(review.cardId, review.reviewCount + 1))
        }

        override suspend fun undo(cardId: String): ApiResult<ReviewCard> = error("not used")

        override suspend fun suspend(cardId: String): ApiResult<ReviewCard> = error("not used")

        override suspend fun unsuspend(cardId: String): ApiResult<ReviewCard> = error("not used")

        override suspend fun suspended(
            limit: Int,
            cursor: String?,
        ): ApiResult<SuspendedReviews> = error("not used")
    }

    /** In memory, so a test on virtual time never waits for a disk. */
    private class MemoryQueue : ReviewQueue {
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

    private companion object {
        fun card(
            id: String,
            count: Int,
        ) = ReviewCard(
            id = id,
            topicId = "t",
            topicTitle = "Rust",
            lessonId = null,
            lessonNumber = null,
            lessonTitle = null,
            kind = ReviewCardKind.Record,
            front = "f",
            back = "b",
            status = ReviewCardStatus.Active,
            state = ReviewState.Review,
            due = null,
            lastReviewedAt = null,
            reviewCount = count,
            intervals = null,
        )
    }

    @Test
    fun `flush sends oldest first with the card version and empties the queue`() =
        runTest {
            val repo = FakeRepository()
            val queue = MemoryQueue()
            val syncer = ReviewSyncer(repo, queue, ManualUserScopes())
            syncer.enqueue(review("c1", count = 4))
            syncer.enqueue(review("c2", count = 0))
            val result = syncer.flush()
            assertEquals(ReviewFlushStatus.Done, result.status)
            assertEquals(listOf("c1" to 4, "c2" to 0), repo.calls.map { it.cardId to it.reviewCount })
            assertEquals(listOf(5, 1), result.sent.map { it.card.reviewCount })
            assertTrue(queue.pending().isEmpty())
        }

    @Test
    fun `a 409 drops the grade in silence and the rest goes on`() =
        runTest {
            val repo = FakeRepository().apply { script["c1"] = ApiFailure.Conflict }
            val queue = MemoryQueue()
            val syncer = ReviewSyncer(repo, queue, ManualUserScopes())
            syncer.enqueue(review("c1"))
            syncer.enqueue(review("c2"))
            val result = syncer.flush()
            assertEquals(ReviewFlushStatus.Done, result.status)
            assertEquals(listOf("c1"), result.conflicts.map { it.cardId })
            assertEquals(listOf("c2"), result.sent.map { it.review.cardId })
            assertTrue(queue.pending().isEmpty())
        }

    @Test
    fun `422 and 404 drop the grade too, without counting it as a conflict`() =
        runTest {
            val repo =
                FakeRepository().apply {
                    script["c1"] = ApiFailure.Validation(emptyList())
                    script["c2"] = ApiFailure.NotFound
                }
            val queue = MemoryQueue()
            val syncer = ReviewSyncer(repo, queue, ManualUserScopes())
            syncer.enqueue(review("c1"))
            syncer.enqueue(review("c2"))
            val result = syncer.flush()
            assertEquals(listOf("c1", "c2"), result.discarded.map { it.cardId })
            assertTrue(result.conflicts.isEmpty())
            assertTrue(queue.pending().isEmpty())
        }

    @Test
    fun `a 401 stops the queue and keeps every grade`() =
        runTest {
            val repo = FakeRepository().apply { script["c1"] = ApiFailure.Unauthorized }
            val queue = MemoryQueue()
            val syncer = ReviewSyncer(repo, queue, ManualUserScopes())
            syncer.enqueue(review("c1"))
            syncer.enqueue(review("c2"))
            val result = syncer.flush()
            assertEquals(ReviewFlushStatus.Unauthorized, result.status)
            assertEquals(1, repo.calls.size)
            assertEquals(listOf("c1", "c2"), queue.pending().map { it.cardId })
            // After the token is fixed, the next flush resumes.
            repo.script.clear()
            assertEquals(ReviewFlushStatus.Done, syncer.flush().status)
            assertTrue(queue.pending().isEmpty())
        }

    @Test
    fun `without network the grades stay queued`() =
        runTest {
            val repo = FakeRepository().apply { script["c1"] = ApiFailure.Network(UnreachableReason.Timeout) }
            val queue = MemoryQueue()
            val syncer = ReviewSyncer(repo, queue, ManualUserScopes())
            syncer.enqueue(review("c1"))
            syncer.enqueue(review("c2"))
            assertEquals(ReviewFlushStatus.Offline, syncer.flush().status)
            assertEquals(2, queue.pending().size)
        }

    @Test
    fun `already_reviewed and card_suspended drop the grade as conflicts`() =
        runTest {
            val repo =
                FakeRepository().apply {
                    script["c1"] = ApiFailure.ReviewConflict.AlreadyReviewed
                    script["c2"] = ApiFailure.ReviewConflict.CardSuspended
                }
            val queue = MemoryQueue()
            val syncer = ReviewSyncer(repo, queue, ManualUserScopes())
            syncer.enqueue(review("c1"))
            syncer.enqueue(review("c2"))
            val result = syncer.flush()
            assertEquals(ReviewFlushStatus.Done, result.status)
            assertEquals(listOf("c1", "c2"), result.conflicts.map { it.cardId })
            assertTrue(queue.pending().isEmpty())
        }

    @Test
    fun `withdraw removes a grade not sent, and says what became of one that was`() =
        runTest {
            val repo = FakeRepository().apply { script["c2"] = ApiFailure.Conflict }
            val syncer = ReviewSyncer(repo, MemoryQueue(), ManualUserScopes())
            val a = review("c1")
            syncer.enqueue(a)
            assertEquals(ReviewSyncer.Withdrawn.FromQueue, syncer.withdraw(a))
            assertTrue(syncer.pending().isEmpty())
            syncer.enqueue(a)
            val refused = review("c2")
            syncer.enqueue(refused)
            syncer.flush()
            assertEquals(ReviewSyncer.Withdrawn.Applied, syncer.withdraw(a))
            // A grade the backend refused is never undone there.
            assertEquals(ReviewSyncer.Withdrawn.Dropped, syncer.withdraw(refused))
        }

    @Test
    fun `withdraw while the grade is being sent waits and reports the real outcome`() =
        runTest {
            val repo = FakeRepository().apply { gate = { delay(1_000) } }
            val syncer = ReviewSyncer(repo, MemoryQueue(), ManualUserScopes())
            val a = review("c1")
            syncer.enqueue(a)
            backgroundScope.launch { syncer.flush() }
            runCurrent()
            assertEquals(ReviewSyncer.Withdrawn.Applied, syncer.withdraw(a))
            assertEquals(1, repo.calls.size)
        }

    @Test
    fun `a flush cancelled during a request still records what the backend applied`() =
        runTest {
            val repo = FakeRepository().apply { gate = { delay(1_000) } }
            val queue = MemoryQueue()
            val syncer = ReviewSyncer(repo, queue, ManualUserScopes())
            val a = review("c1")
            syncer.enqueue(a)
            val job = launch { syncer.flush() }
            runCurrent()
            job.cancel()
            advanceUntilIdle()
            // The grade was sent and is gone from the queue: it will not be sent again and come back as a false 409.
            assertEquals(1, repo.calls.size)
            assertTrue(queue.pending().isEmpty())
            assertEquals(ReviewSyncer.Withdrawn.Applied, syncer.withdraw(a))
        }

    @Test
    fun `flushWithin gives up waiting but the flush finishes`() =
        runTest {
            val repo = FakeRepository().apply { gate = { delay(5_000) } }
            val syncer = ReviewSyncer(repo, MemoryQueue(), ManualUserScopes())
            syncer.enqueue(review("c1"))
            assertNull(syncer.flushWithin(backgroundScope, 1_000))
            delay(6_000)
            assertEquals(1, repo.calls.size)
            assertTrue(syncer.pending().isEmpty())
            assertEquals(ReviewFlushResult.Empty.status, syncer.flush().status)
        }

    // region data of a user never leaves with the session of another

    private class ScopedQueue(
        private val scopes: ManualUserScopes,
    ) : ReviewQueue {
        private val byUser = HashMap<UserScope, MutableStateFlow<List<QueuedReview>>>()

        private suspend fun flow() = byUser.getOrPut(scopes.current()!!) { MutableStateFlow(emptyList()) }

        suspend fun of(scope: UserScope) = byUser[scope]?.value.orEmpty()

        override suspend fun enqueue(review: QueuedReview) {
            val f = flow()
            f.value = f.value.filterNot { it.sameReview(review) } + review
        }

        override suspend fun pending() = flow().value

        override suspend fun remove(review: QueuedReview): Boolean {
            val f = flow()
            val had = f.value.any { it.sameReview(review) }
            f.value = f.value.filterNot { it.sameReview(review) }
            return had
        }

        override fun count(): Flow<Int> = MutableStateFlow(0)
    }

    private val userA = UserScope("user-a", "http://teach.test")
    private val userB = UserScope("user-b", "http://teach.test")

    @Test
    fun `a grade queued by user A is not sent with the session of B`() =
        runTest {
            val scopes = ManualUserScopes(userA)
            val queue = ScopedQueue(scopes)
            val repo = FakeRepository()
            val syncer = ReviewSyncer(repo, queue, scopes)
            syncer.enqueue(review("card-of-a"))

            // B signs in on the same tablet: A's queue is not B's.
            scopes.switchTo(userB)
            val result = syncer.flush()

            assertTrue(repo.calls.isEmpty())
            assertEquals(ReviewFlushStatus.Done, result.status)
            assertEquals(listOf("card-of-a"), queue.of(userA).map { it.cardId })
            assertTrue(queue.of(userB).isEmpty())

            // And when A is back, the grade goes out.
            scopes.switchTo(userA)
            syncer.flush()
            assertEquals(listOf("card-of-a"), repo.calls.map { it.cardId })
            assertTrue(queue.of(userA).isEmpty())
        }

    @Test
    fun `every request of a flush is tied to the user whose queue it read`() =
        runTest {
            val scopes = ManualUserScopes(userA)
            val seen = mutableListOf<UserScope?>()
            val repo =
                object : ReviewRepository {
                    override suspend fun due(
                        limit: Int,
                        topicId: String?,
                    ): ApiResult<DueReviews> = error("not used")

                    override suspend fun rate(review: QueuedReview): ApiResult<ReviewCard> {
                        seen += kotlin.coroutines.coroutineContext[ExpectedUserScope]?.scope
                        return ApiResult.Success(card(review.cardId, review.reviewCount + 1))
                    }

                    override suspend fun undo(cardId: String): ApiResult<ReviewCard> = error("not used")

                    override suspend fun suspend(cardId: String): ApiResult<ReviewCard> = error("not used")

                    override suspend fun unsuspend(cardId: String): ApiResult<ReviewCard> = error("not used")

                    override suspend fun suspended(
                        limit: Int,
                        cursor: String?,
                    ): ApiResult<SuspendedReviews> = error("not used")
                }
            val syncer = ReviewSyncer(repo, ScopedQueue(scopes), scopes)
            syncer.enqueue(review("c1"))
            syncer.enqueue(review("c2"))

            syncer.flush()

            assertEquals(listOf<UserScope?>(userA, userA), seen)
        }

    @Test
    fun `if the user changes while a request is out, the answer is not applied to the other queue`() =
        runTest {
            val scopes = ManualUserScopes(userA)
            val queue = ScopedQueue(scopes)
            val repo = FakeRepository()
            repo.gate = { scopes.switchTo(userB) }
            val syncer = ReviewSyncer(repo, queue, scopes)
            syncer.enqueue(review("c1"))
            scopes.switchTo(userA)

            val result = syncer.flush()

            assertEquals(ReviewFlushStatus.Offline, result.status)
            assertTrue(result.sent.isEmpty())
            // Nothing was removed from, or added to, anybody's queue by the late answer.
            assertEquals(listOf("c1"), queue.of(userA).map { it.cardId })
            assertTrue(queue.of(userB).isEmpty())
        }

    @Test
    fun `without a user there is nothing to send`() =
        runTest {
            val scopes = ManualUserScopes(null)
            val repo = FakeRepository()

            val result = ReviewSyncer(repo, MemoryQueue(), scopes).flush()

            assertEquals(ReviewFlushResult.Empty, result)
            assertTrue(repo.calls.isEmpty())
        }

    // endregion
}
