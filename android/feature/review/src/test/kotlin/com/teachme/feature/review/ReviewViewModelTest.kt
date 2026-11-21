package com.teachme.feature.review

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.LoadError
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewRating
import com.teachme.core.model.ReviewSyncer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class ReviewViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val repo = FakeReviewRepository()
    private val queue = MemoryReviewQueue()
    private val clock = FakeClock()

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private val models = mutableListOf<ReviewViewModel>()

    /**
     * The sync loop and its retry timer never end by themselves; a test that leaves the queue offline would make
     * `runTest` run virtual time forever, so every ViewModel is cleared when the test body is over.
     */
    private fun reviewTest(body: suspend TestScope.() -> Unit) =
        runTest(dispatcher) {
            try {
                body()
            } finally {
                models.forEach { it.viewModelScope.cancel() }
            }
        }

    private fun TestScope.vm(
        topicId: String? = null,
        topicTitle: String? = null,
        extra: Map<String, Any?> = emptyMap(),
    ): ReviewViewModel {
        val model =
            ReviewViewModel(
                repo,
                ReviewSyncer(
                    repo,
                    queue,
                    com.teachme.core.model
                        .ManualUserScopes(),
                ),
                clock,
                ReviewTimings(),
                SavedStateHandle(
                    buildMap {
                        if (topicId != null) put(ReviewArgs.TOPIC_ID, topicId)
                        if (topicTitle != null) put(ReviewArgs.TOPIC_TITLE, topicTitle)
                        putAll(extra)
                    },
                ),
            )
        models += model
        runCurrent()
        return model
    }

    private fun TestScope.session(n: Int): ReviewViewModel {
        repo.session = (1..n).map { card("c$it", count = it) }
        repo.dueCount = n
        return vm()
    }

    private fun ReviewViewModel.value() = state.value

    private fun TestScope.rateCurrent(
        vm: ReviewViewModel,
        rating: ReviewRating = ReviewRating.Good,
    ) {
        vm.reveal()
        vm.rate(rating)
        runCurrent()
    }

    @Test
    fun `loads a session of at most 20 cards and starts on the first front`() =
        reviewTest {
            repo.session = (1..30).map { card("c$it") }
            repo.dueCount = 30
            val vm = vm()
            assertEquals(listOf(20), repo.dueLimits)
            assertEquals(ReviewPhase.Card, vm.value().phase)
            assertEquals(20, vm.value().total)
            assertEquals("c1", vm.value().current?.id)
            assertFalse(vm.value().revealed)
        }

    @Test
    fun `reveal shows the back, a grade before it does nothing`() =
        reviewTest {
            val vm = session(3)
            vm.rate(ReviewRating.Good)
            runCurrent()
            assertEquals(0, vm.value().index)
            assertTrue(repo.rated.isEmpty())
            vm.reveal()
            assertTrue(vm.value().revealed)
        }

    @Test
    fun `a grade is optimistic, queued with the card version and sent`() =
        reviewTest {
            val vm = session(3)
            clock.now = 5_000L
            rateCurrent(vm, ReviewRating.Hard)
            // Already on the second front, before the backend could answer.
            assertEquals(1, vm.value().index)
            assertFalse(vm.value().revealed)
            assertEquals("c2", vm.value().current?.id)
            assertEquals(listOf(QueuedReview("c1", "hard", 5_000L, 1)), repo.rated)
            assertTrue(queue.pending().isEmpty())
            val notice = vm.value().notice as ReviewNotice.Voted
            assertEquals(ReviewRating.Hard, notice.rating)
            assertEquals(172_800L, notice.intervalSeconds)
        }

    @Test
    fun `the notice goes away after 5 seconds, longer when asked`() =
        reviewTest {
            val vm = session(3)
            rateCurrent(vm)
            advanceTimeBy(4_900)
            assertTrue(vm.value().notice != null)
            advanceTimeBy(200)
            assertNull(vm.value().notice)
            vm.undoVisibleMs = 12_000
            rateCurrent(vm)
            advanceTimeBy(11_000)
            assertTrue(vm.value().notice != null)
            advanceTimeBy(2_000)
            assertNull(vm.value().notice)
        }

    @Test
    fun `the last grade ends the session with the totals from the backend`() =
        reviewTest {
            val vm = session(2)
            rateCurrent(vm, ReviewRating.Again)
            repo.dueCount = 0
            repo.nextDueAt = Instant.parse("2026-11-11T09:00:00Z")
            rateCurrent(vm, ReviewRating.Good)
            assertEquals(ReviewPhase.Finished, vm.value().phase)
            assertEquals(listOf(20, 1), repo.dueLimits)
            val summary = vm.value().finishSummary()
            assertEquals(2, summary.reviewed)
            assertEquals(listOf("c1"), summary.again.map { it.card.id })
            assertEquals(600L, summary.again[0].intervalSeconds)
            assertEquals(Instant.parse("2026-11-11T09:00:00Z"), summary.nextDueAt)
            assertFalse(summary.hasBacklog)
        }

    @Test
    fun `with a backlog the end offers the next session and drops the list and the next date`() =
        reviewTest {
            val vm = session(1)
            rateCurrent(vm, ReviewRating.Again)
            // Not yet answered: nothing about the backlog is said.
            repo.dueCount = 34
            repo.nextDueAt = Instant.parse("2026-11-11T09:00:00Z")
            vm.start()
            runCurrent()
            rateCurrent(vm, ReviewRating.Again)
            val summary = vm.value().finishSummary()
            assertEquals(34, summary.residual)
            assertTrue(summary.hasBacklog)
            assertEquals(20, summary.nextSession)
            assertTrue(summary.again.isEmpty())
            assertNull(summary.nextDueAt)
        }

    @Test
    fun `cards still due net of the grades waiting in the queue`() =
        reviewTest {
            repo.rateResult = { NETWORK_DOWN }
            val vm = session(1)
            repo.dueCount = 5
            rateCurrent(vm)
            // The end waits up to 3 seconds for the queue, then counts with what is still in it.
            advanceTimeBy(3_100)
            assertEquals(ReviewPhase.Finished, vm.value().phase)
            // One of the five is the card just graded, still in the queue.
            assertEquals(4, vm.value().finish.residual)
        }

    @Test
    fun `without network the grade stays queued, the session goes on and it is retried with backoff`() =
        reviewTest {
            var online = false
            repo.rateResult = { if (online) ApiResult.Success(card(it.cardId, it.reviewCount + 1)) else NETWORK_DOWN }
            val vm = session(3)
            rateCurrent(vm)
            rateCurrent(vm)
            assertEquals(2, vm.value().index)
            // Both grades wait, in order; the session did not stop for them.
            assertEquals(listOf("c1", "c2"), queue.pending().map { it.cardId })
            assertEquals(2, vm.value().pendingCount)
            online = true
            advanceTimeBy(10_001)
            assertTrue(queue.pending().isEmpty())
            assertEquals(0, vm.value().pendingCount)
            assertTrue(vm.value().sentAnnouncement)
        }

    @Test
    fun `retries wait 10 seconds, then twice as long, up to 5 minutes`() =
        reviewTest {
            repo.rateResult = { NETWORK_DOWN }
            val vm = session(2)
            rateCurrent(vm)
            assertEquals(1, repo.rated.size)
            advanceTimeBy(9_900)
            assertEquals(1, repo.rated.size)
            advanceTimeBy(200) // t = 10 s
            assertEquals(2, repo.rated.size)
            advanceTimeBy(19_800)
            assertEquals(2, repo.rated.size)
            advanceTimeBy(200) // t = 30 s: 10 + 20
            assertEquals(3, repo.rated.size)
            advanceTimeBy(40_000) // + 40
            assertEquals(4, repo.rated.size)
            // 80, 160, then capped at 300 s.
            advanceTimeBy(80_000)
            advanceTimeBy(160_000)
            assertEquals(6, repo.rated.size)
            advanceTimeBy(299_000)
            assertEquals(6, repo.rated.size)
            advanceTimeBy(2_000)
            assertEquals(7, repo.rated.size)
        }

    @Test
    fun `back in the foreground or retry now sends the queue at once`() =
        reviewTest {
            var online = false
            repo.rateResult = { if (online) ApiResult.Success(card(it.cardId)) else NETWORK_DOWN }
            val vm = session(2)
            rateCurrent(vm)
            online = true
            vm.syncNow()
            runCurrent()
            assertTrue(queue.pending().isEmpty())
        }

    @Test
    fun `undo of a grade not yet sent only takes it out of the queue`() =
        reviewTest {
            repo.rateResult = { NETWORK_DOWN }
            val vm = session(3)
            rateCurrent(vm, ReviewRating.Easy)
            assertEquals(1, queue.pending().size)
            vm.undo()
            runCurrent()
            assertTrue(repo.undone.isEmpty())
            assertTrue(queue.pending().isEmpty())
            val s = vm.value()
            assertEquals(0, s.index)
            assertTrue("back on the back: the answer was seen", s.revealed)
            assertTrue(s.votes.isEmpty())
            assertNull(s.notice)
            assertFalse(vm.canUndo())
        }

    @Test
    fun `undo of a grade already sent asks the backend and takes its card`() =
        reviewTest {
            val vm = session(3)
            rateCurrent(vm)
            repo.undoResult = { ApiResult.Success(card(it, count = 1, front = "Aggiornata")) }
            vm.undo()
            runCurrent()
            assertEquals(listOf("c1"), repo.undone)
            val s = vm.value()
            assertEquals(0, s.index)
            assertTrue(s.revealed)
            assertEquals("Aggiornata", s.current?.front)
            assertTrue(s.votes.isEmpty())
        }

    @Test
    fun `undo from the end of the session goes back to the last card`() =
        reviewTest {
            val vm = session(1)
            rateCurrent(vm)
            assertEquals(ReviewPhase.Finished, vm.value().phase)
            vm.undo()
            runCurrent()
            assertEquals(ReviewPhase.Card, vm.value().phase)
            assertEquals("c1", vm.value().current?.id)
            assertTrue(vm.value().revealed)
        }

    @Test
    fun `nothing_to_undo, an unknown 409 and a gone card all read as undo gone`() =
        reviewTest {
            val vm = session(3)
            rateCurrent(vm)
            for (failure in listOf(ApiFailure.ReviewConflict.NothingToUndo, ApiFailure.Conflict)) {
                repo.undoResult = { ApiResult.Failure(failure) }
                vm.undo()
                runCurrent()
                assertEquals(failure.toString(), ReviewNotice.UndoGone, vm.value().notice)
                assertEquals(1, vm.value().index)
            }
        }

    @Test
    fun `the suspension notice stays 6 seconds and the other notices 4`() =
        reviewTest {
            val vm = session(3)
            vm.openSuspend()
            vm.confirmSuspend()
            runCurrent()
            assertEquals(ReviewNotice.Suspended, vm.value().notice)
            advanceTimeBy(5_900)
            assertEquals(ReviewNotice.Suspended, vm.value().notice)
            advanceTimeBy(200)
            assertNull(vm.value().notice)
        }

    @Test
    fun `only the last grade can be undone, and not after 10 minutes`() =
        reviewTest {
            val vm = session(3)
            rateCurrent(vm)
            clock.now += 11 * 60_000L
            assertFalse(vm.canUndo())
            vm.undo()
            runCurrent()
            assertTrue(repo.undone.isEmpty())
            assertEquals(ReviewNotice.UndoGone, vm.value().notice)
            assertEquals(1, vm.value().index)
        }

    @Test
    fun `a refused undo says so and the grade stays, no network says another thing`() =
        reviewTest {
            val vm = session(3)
            rateCurrent(vm)
            repo.undoResult =
                { ApiResult.Failure(ApiFailure.Network(com.teachme.core.model.UnreachableReason.Timeout)) }
            vm.undo()
            runCurrent()
            assertEquals(ReviewNotice.UndoOffline, vm.value().notice)
            assertEquals(1, vm.value().votes.size)
            repo.undoResult = { ApiResult.Failure(ApiFailure.ReviewConflict.UndoExpired) }
            vm.undo()
            runCurrent()
            assertEquals(ReviewNotice.UndoGone, vm.value().notice)
            assertEquals(1, vm.value().index)
            assertFalse(vm.canUndo())
        }

    @Test
    fun `a 409 on send drops the grade in silence and counts it for the end`() =
        reviewTest {
            repo.rateResult = { ApiResult.Failure(ApiFailure.ReviewConflict.AlreadyReviewed) }
            val vm = session(2)
            rateCurrent(vm)
            assertEquals(ReviewNotice.AlreadyChanged, vm.value().notice)
            assertEquals(1, vm.value().conflicts)
            assertTrue(vm.value().votes.isEmpty())
            assertTrue(queue.pending().isEmpty())
            repo.rateResult = { ApiResult.Success(card(it.cardId)) }
            rateCurrent(vm)
            val summary = vm.value().finishSummary()
            assertEquals(1, summary.reviewed)
            assertEquals(1, summary.conflicts)
        }

    @Test
    fun `a refused token keeps the grades and shows the panel until a send works`() =
        reviewTest {
            repo.rateResult = { ApiResult.Failure(ApiFailure.Unauthorized) }
            val vm = session(3)
            rateCurrent(vm)
            assertTrue(vm.value().unauthorized)
            assertEquals(1, queue.pending().size)
            // Not retried by itself.
            advanceTimeBy(600_000)
            assertEquals(1, repo.rated.size)
            repo.rateResult = { ApiResult.Success(card(it.cardId)) }
            vm.syncNow()
            runCurrent()
            assertFalse(vm.value().unauthorized)
            assertTrue(queue.pending().isEmpty())
        }

    @Test
    fun `suspending a card takes it out of the session and the total drops`() =
        reviewTest {
            val vm = session(3)
            vm.openSuspend()
            assertEquals(false, vm.value().suspendDialog?.submitting)
            vm.confirmSuspend()
            runCurrent()
            assertEquals(listOf("c1"), repo.suspended)
            assertNull(vm.value().suspendDialog)
            assertEquals(2, vm.value().total)
            assertEquals("c2", vm.value().current?.id)
            assertEquals(ReviewNotice.Suspended, vm.value().notice)
            // Not a grade: nothing was queued and the count of reviewed cards is untouched.
            assertTrue(repo.rated.isEmpty())
            assertEquals(0, vm.value().finishSummary().reviewed)
        }

    @Test
    fun `a failed suspend keeps the dialog open with the reason, and a missing card counts as done`() =
        reviewTest {
            val vm = session(3)
            repo.suspendResult = { NETWORK_DOWN }
            vm.openSuspend()
            vm.confirmSuspend()
            runCurrent()
            assertEquals(
                com.teachme.core.model.ActionFailure.Network,
                vm
                    .value()
                    .suspendDialog
                    ?.error
                    ?.action,
            )
            assertEquals(3, vm.value().total)
            vm.dismissSuspend()
            assertNull(vm.value().suspendDialog)
            repo.suspendResult = { ApiResult.Failure(ApiFailure.NotFound) }
            vm.openSuspend()
            vm.confirmSuspend()
            runCurrent()
            assertEquals(2, vm.value().total)
        }

    @Test
    fun `suspending the only card without a grade is the empty state, not a finished session`() =
        reviewTest {
            repo.nextDueAt = Instant.parse("2026-11-11T09:00:00Z")
            val vm = session(1)
            vm.openSuspend()
            vm.confirmSuspend()
            runCurrent()
            assertEquals(ReviewPhase.Empty, vm.value().phase)
            assertEquals(Instant.parse("2026-11-11T09:00:00Z"), vm.value().emptyNextDueAt)
        }

    @Test
    fun `suspending the last card after grading ends the session`() =
        reviewTest {
            val vm = session(2)
            rateCurrent(vm)
            vm.openSuspend()
            vm.confirmSuspend()
            runCurrent()
            assertEquals(ReviewPhase.Finished, vm.value().phase)
            assertEquals(1, vm.value().finishSummary().reviewed)
        }

    @Test
    fun `undo right after a grade the backend refused never reaches the backend`() =
        reviewTest {
            repo.rateResult = { ApiResult.Failure(ApiFailure.ReviewConflict.CardSuspended) }
            val vm = session(2)
            rateCurrent(vm)
            vm.undo()
            runCurrent()
            assertTrue(repo.undone.isEmpty())
            assertEquals(1, vm.value().index)
        }

    @Test
    fun `undo while that grade is being sent waits for the answer and then undoes it at the backend`() =
        reviewTest {
            var release = false
            repo.rateGate = { while (!release) kotlinx.coroutines.delay(100) }
            val vm = session(2)
            rateCurrent(vm)
            vm.undo()
            runCurrent()
            assertTrue(repo.undone.isEmpty())
            release = true
            advanceTimeBy(500)
            assertEquals(listOf("c1"), repo.undone)
            assertEquals(0, vm.value().index)
        }

    @Test
    fun `a screen closed during the request does not lose the answer`() =
        reviewTest {
            repo.rateGate = { kotlinx.coroutines.delay(5_000) }
            val vm = session(2)
            rateCurrent(vm)
            vm.viewModelScope.cancel()
            advanceTimeBy(6_000)
            assertEquals(1, repo.rated.size)
            assertTrue(queue.pending().isEmpty())
        }

    @Test
    fun `nothing due says when the next card comes, and no card at all is another state`() =
        reviewTest {
            repo.nextDueAt = Instant.parse("2026-11-11T09:00:00Z")
            val vm = vm()
            assertEquals(ReviewPhase.Empty, vm.value().phase)
            assertEquals(Instant.parse("2026-11-11T09:00:00Z"), vm.value().emptyNextDueAt)
        }

    @Test
    fun `a failed load gives the cause, retry loads again`() =
        reviewTest {
            repo.dueFailure = ApiFailure.Unauthorized
            val vm = vm()
            assertEquals(ReviewPhase.Failed, vm.value().phase)
            assertEquals(LoadError.Unauthorized, vm.value().loadError)
            repo.dueFailure = ApiFailure.Unexpected(500)
            vm.start()
            runCurrent()
            assertEquals(LoadError.Server, vm.value().loadError)
            repo.dueFailure = null
            repo.session = listOf(card("c1"))
            vm.start()
            runCurrent()
            assertEquals(ReviewPhase.Card, vm.value().phase)
        }

    @Test
    fun `a load that failed with a 401 is done again when the screen comes back after the sign-in`() =
        reviewTest {
            repo.dueFailure = ApiFailure.Unauthorized
            val vm = vm()
            assertEquals(LoadError.Unauthorized, vm.value().loadError)

            repo.dueFailure = null
            repo.session = listOf(card("c1"))
            vm.syncNow()
            runCurrent()

            assertEquals(ReviewPhase.Card, vm.value().phase)
        }

    @Test
    fun `a load that takes too long is a network error, after "still loading" at 8 seconds`() =
        reviewTest {
            repo.dueDelayMs = 60_000
            val vm = vm()
            assertFalse(vm.value().slowLoading)
            advanceTimeBy(8_100)
            assertTrue(vm.value().slowLoading)
            advanceTimeBy(2_500)
            assertEquals(ReviewPhase.Failed, vm.value().phase)
            assertEquals(LoadError.Network, vm.value().loadError)
        }

    @Test
    fun `a card whose grade is still queued is not shown again`() =
        reviewTest {
            queue.enqueue(QueuedReview("c1", "good", 1L, 1))
            repo.rateResult = { NETWORK_DOWN }
            repo.session = listOf(card("c1", 1), card("c2", 2))
            repo.dueCount = 2
            val vm = vm()
            // The load first waits up to 2 seconds for the queue to empty; it cannot, so it goes on.
            advanceTimeBy(2_100)
            assertEquals(listOf("c2"), vm.value().cards.map { it.id })
        }

    @Test
    fun `another session starts clean`() =
        reviewTest {
            val vm = session(1)
            rateCurrent(vm)
            repo.session = listOf(card("c9"))
            vm.start()
            runCurrent()
            val s = vm.value()
            assertEquals(ReviewPhase.Card, s.phase)
            assertEquals("c9", s.current?.id)
            assertTrue(s.votes.isEmpty())
            assertNull(s.notice)
        }

    // --- filtered session (review.md section 7.3) and the lesson action (section 4.6) ---------------

    private fun TestScope.topicSession(n: Int): ReviewViewModel {
        repo.session = (1..n).map { card("c$it", count = it) }
        repo.dueCount = n
        return vm("t1", "Rust")
    }

    @Test
    fun `a session without a filter sends no topic and has no topic title`() =
        reviewTest {
            val vm = session(2)
            assertEquals(listOf<String?>(null), repo.dueTopicIds)
            assertNull(vm.value().topicTitle)
        }

    @Test
    fun `the topic filter goes to every due call and the title is kept`() =
        reviewTest {
            val vm = topicSession(1)
            assertEquals("Rust", vm.value().topicTitle)
            repo.dueCount = 4
            rateCurrent(vm)
            runCurrent()
            assertEquals(ReviewPhase.Finished, vm.value().phase)
            val summary = vm.value().finishSummary()
            assertEquals("Rust", summary.topicTitle)
            assertEquals(4, summary.residual)
            // Load and end-of-session fetch both carry the filter.
            assertEquals(listOf<String?>("t1", "t1"), repo.dueTopicIds)
            // "Ripassa altre N" stays in the same filter.
            vm.start()
            runCurrent()
            assertEquals("t1", repo.dueTopicIds.last())
            assertEquals("Rust", vm.value().topicTitle)
        }

    @Test
    fun `nothing due in a filtered session keeps the title for the empty state`() =
        reviewTest {
            repo.dueCount = 0
            repo.nextDueAt = Instant.parse("2026-11-11T09:00:00Z")
            val vm = vm("t1", "Rust")
            assertEquals(ReviewPhase.Empty, vm.value().phase)
            assertEquals("Rust", vm.value().topicTitle)
            assertEquals(Instant.parse("2026-11-11T09:00:00Z"), vm.value().emptyNextDueAt)
        }

    @Test
    fun `a 404 on start is the topic gone panel, and review all restarts without the filter`() =
        reviewTest {
            repo.session = listOf(card("c1", count = 1))
            repo.dueCount = 1
            repo.dueFailureFor = { if (it == "t1") ApiFailure.NotFound else null }
            val vm = vm("t1", "Rust")
            assertEquals(ReviewPhase.TopicGone, vm.value().phase)
            assertNull(vm.value().topicTitle)
            assertNull(vm.value().loadError)
            vm.reviewAll()
            runCurrent()
            assertEquals(ReviewPhase.Card, vm.value().phase)
            assertNull(repo.dueTopicIds.last())
            assertNull(vm.value().topicTitle)
        }

    @Test
    fun `a 404 on the end of session fetch is silent`() =
        reviewTest {
            val vm = topicSession(1)
            repo.dueFailureFor = { ApiFailure.NotFound }
            rateCurrent(vm)
            runCurrent()
            assertEquals(ReviewPhase.Finished, vm.value().phase)
            assertNull(vm.value().finish.residual)
            assertNull(vm.value().finish.nextDueAt)
        }

    @Test
    fun `a 404 in the middle of the session does not interrupt it`() =
        reviewTest {
            val vm = topicSession(3)
            repo.dueFailureFor = { ApiFailure.NotFound }
            rateCurrent(vm)
            assertEquals(ReviewPhase.Card, vm.value().phase)
            assertEquals(1, vm.value().index)
        }

    @Test
    fun `coming back from the lesson leaves the same card on its back`() =
        reviewTest {
            val vm = session(3)
            rateCurrent(vm)
            vm.reveal()
            // The lesson opens over the review: the screen stops and starts again (ON_START syncs the queue).
            vm.syncNow()
            runCurrent()
            val s = vm.value()
            assertEquals(ReviewPhase.Card, s.phase)
            assertEquals(1, s.index)
            assertTrue(s.revealed)
            assertEquals(1, s.votes.size)
            assertEquals(1, repo.dueLimits.size)
        }

    @Test
    fun `a recreated view model goes back to the saved card on its back, without grades`() =
        reviewTest {
            repo.session = (1..3).map { card("c$it", count = it) }
            repo.dueCount = 3
            val vm =
                vm(
                    "t1",
                    "Rust",
                    mapOf(ReviewArgs.CARD_ID to "c2", ReviewArgs.REVEALED to true),
                )
            val s = vm.value()
            assertEquals(ReviewPhase.Card, s.phase)
            assertEquals(1, s.index)
            assertEquals("c2", s.current?.id)
            assertTrue(s.revealed)
            assertEquals("Rust", s.topicTitle)
            assertTrue(s.votes.isEmpty())
            assertTrue(repo.rated.isEmpty())
        }

    @Test
    fun `a saved card no longer due starts the session normally`() =
        reviewTest {
            repo.session = (1..3).map { card("c$it", count = it) }
            repo.dueCount = 3
            val vm = vm(extra = mapOf(ReviewArgs.CARD_ID to "gone", ReviewArgs.REVEALED to true))
            assertEquals(0, vm.value().index)
            assertEquals(false, vm.value().revealed)
            assertTrue(repo.rated.isEmpty())
        }

    @Test
    fun `a 404 after suspending the only card is the normal end, not nothing to review`() =
        reviewTest {
            val vm = topicSession(1)
            repo.dueFailureFor = { ApiFailure.NotFound }
            vm.openSuspend()
            vm.confirmSuspend()
            runCurrent()
            assertEquals(ReviewPhase.Finished, vm.value().phase)
            assertNull(vm.value().finish.residual)
            assertNull(vm.value().finish.nextDueAt)
        }
}
