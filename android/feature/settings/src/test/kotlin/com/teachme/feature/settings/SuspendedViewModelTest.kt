package com.teachme.feature.settings

import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.LoadError
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class SuspendedViewModelTest {
    private val reviews = FakeReviews()
    private val nowMs = Instant.parse("2026-11-16T10:00:00Z").toEpochMilli()
    private val created = mutableListOf<SuspendedViewModel>()

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun vm() = SuspendedViewModel(reviews, AppClock { nowMs }).also { created += it }

    private fun test(body: suspend TestScope.() -> Unit) =
        runTest {
            try {
                body()
            } finally {
                created.forEach { it.viewModelScope.cancel() }
            }
        }

    private val SuspendedViewModel.ids get() = state.value.rows?.map { it.card.id }

    @Test
    fun `it opens with the first page of 50 and the total`() =
        test {
            reviews.pages += page(listOf("a", "b"), next = "c1", count = 120)
            val vm = vm()
            assertNull(vm.state.value.rows)
            runCurrent()

            assertEquals(listOf("a", "b"), vm.ids)
            assertEquals(120, vm.state.value.count)
            assertEquals(listOf(50 to null), reviews.requests)
            assertFalse(vm.state.value.isEmpty)
        }

    @Test
    fun `no cards is the empty state`() =
        test {
            reviews.pages += page(emptyList())
            val vm = vm()
            runCurrent()
            assertTrue(vm.state.value.isEmpty)
        }

    @Test
    fun `scrolling to the end follows the cursor, once at a time, without repeating a card`() =
        test {
            reviews.pages += page(listOf("a", "b"), next = "c1", count = 4)
            reviews.pages += page(listOf("b", "c", "d"), next = null, count = 4)
            val vm = vm()
            runCurrent()

            vm.loadMore()
            vm.loadMore()
            assertTrue(vm.state.value.pageLoading)
            runCurrent()

            assertEquals(listOf("a", "b", "c", "d"), vm.ids)
            assertEquals(listOf(50 to null, 50 to "c1"), reviews.requests)
            assertNull(vm.state.value.nextCursor)
            // No cursor, no request.
            vm.loadMore()
            runCurrent()
            assertEquals(2, reviews.requests.size)
        }

    @Test
    fun `a failed page keeps the list and can be retried`() =
        test {
            reviews.pages += page(listOf("a"), next = "c1", count = 2)
            reviews.pages += NETWORK_DOWN
            reviews.pages += page(listOf("b"), count = 2)
            val vm = vm()
            runCurrent()

            vm.loadMore()
            runCurrent()
            assertTrue(vm.state.value.pageFailed)
            assertEquals(listOf("a"), vm.ids)
            assertFalse(vm.state.value.isEmpty)

            vm.loadMore()
            runCurrent()
            assertFalse(vm.state.value.pageFailed)
            assertEquals(listOf("a", "b"), vm.ids)
        }

    @Test
    fun `a network error on the first load shows the error and Riprova starts over`() =
        test {
            reviews.pages += NETWORK_DOWN
            reviews.pages += page(listOf("a"))
            val vm = vm()
            runCurrent()
            assertEquals(LoadError.Network, vm.state.value.loadError)
            assertNull(vm.state.value.rows)

            vm.refresh()
            assertNull(vm.state.value.loadError)
            runCurrent()
            assertEquals(listOf("a"), vm.ids)
        }

    @Test
    fun `a 5xx is the server error`() =
        test {
            reviews.pages += ApiResult.Failure(ApiFailure.Unexpected(500))
            val vm = vm()
            runCurrent()
            assertEquals(LoadError.Server, vm.state.value.loadError)
        }

    @Test
    fun `after 8 seconds it says it is still loading, after 10 it gives up`() =
        test {
            val never = CompletableDeferred<Unit>()
            reviews.pageGate = { never.await() }
            reviews.pages += page(listOf("a"))
            val vm = vm()
            runCurrent()
            assertFalse(vm.state.value.slowLoading)

            advanceTimeBy(SUSPENDED_SLOW_MS + 1)
            assertTrue(vm.state.value.slowLoading)
            assertNull(vm.state.value.loadError)

            advanceTimeBy(SUSPENDED_TIMEOUT_MS - SUSPENDED_SLOW_MS)
            runCurrent()
            assertEquals(LoadError.Network, vm.state.value.loadError)
            assertFalse(vm.state.value.slowLoading)
        }

    @Test
    fun `reactivating takes the row out, lowers the count and says when the card is due`() =
        test {
            reviews.pages += page(listOf("a", "b"), count = 2)
            val gate = CompletableDeferred<Unit>()
            reviews.unsuspendGate = { gate.await() }
            reviews.unsuspendResult =
                { ApiResult.Success(card(it, suspended = false, due = Instant.ofEpochMilli(nowMs + 3 * 86_400_000L))) }
            val vm = vm()
            runCurrent()

            vm.reactivate("a")
            runCurrent()
            assertEquals(
                RowPhase.Busy,
                vm.state.value.rows
                    ?.first { it.card.id == "a" }
                    ?.phase,
            )
            // A second tap on the same row, and a tap on another one, are independent.
            vm.reactivate("a")
            runCurrent()
            assertEquals(listOf("a"), reviews.unsuspended)

            gate.complete(Unit)
            runCurrent()
            assertEquals(listOf("b"), vm.ids)
            assertEquals(1, vm.state.value.count)
            assertEquals(SuspendedEffect.Reactivated("a", 3 * 86_400L), nextEffect(vm))
        }

    @Test
    fun `a card already due comes back with a non positive wait`() =
        test {
            reviews.pages += page(listOf("a", "b"))
            reviews.unsuspendResult =
                { ApiResult.Success(card(it, suspended = false, due = Instant.ofEpochMilli(nowMs - 1_000))) }
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()
            val effect = nextEffect(vm) as SuspendedEffect.Reactivated
            assertTrue(effect.dueInSeconds!! <= 0)
        }

    @Test
    fun `reactivating the last row of the last page leaves the empty state`() =
        test {
            reviews.pages += page(listOf("a"), count = 1)
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()
            assertTrue(vm.state.value.isEmpty)
            assertEquals(0, vm.state.value.count)
        }

    @Test
    fun `reactivating every loaded row while more pages exist loads the next page instead of showing empty`() =
        test {
            reviews.pages += page(listOf("a"), next = "c1", count = 2)
            reviews.pages += page(listOf("b"), count = 2)
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()
            assertEquals(listOf("b"), vm.ids)
            assertEquals(listOf(50 to null, 50 to "c1"), reviews.requests)
        }

    @Test
    fun `Annulla suspends the card again and its row returns on top`() =
        test {
            reviews.pages += page(listOf("a", "b", "c"), count = 3)
            val vm = vm()
            runCurrent()
            vm.reactivate("b")
            runCurrent()
            assertEquals(listOf("a", "c"), vm.ids)

            vm.undo("b")
            runCurrent()
            assertEquals(listOf("b", "a", "c"), vm.ids)
            assertEquals(3, vm.state.value.count)
            assertEquals(listOf("b"), reviews.suspendedAgain)
            // Nothing was reloaded.
            assertEquals(1, reviews.requests.size)
        }

    @Test
    fun `a failed Annulla keeps the card reactivated and says so`() =
        test {
            reviews.pages += page(listOf("a", "b"), count = 2)
            reviews.suspendResult = { NETWORK_DOWN }
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()
            nextEffect(vm)

            vm.undo("a")
            runCurrent()
            assertEquals(SuspendedEffect.UndoFailed, nextEffect(vm))
            assertEquals(listOf("b"), vm.ids)
            assertEquals(1, vm.state.value.count)
        }

    @Test
    fun `a superseded card leaves with a message and no undo`() =
        test {
            reviews.pages += page(listOf("a", "b"), count = 2)
            reviews.unsuspendResult = { ApiResult.Failure(ApiFailure.ReviewConflict.CardSuperseded) }
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()
            assertEquals(listOf("b"), vm.ids)
            assertEquals(1, vm.state.value.count)
            assertEquals(SuspendedEffect.Superseded, nextEffect(vm))
        }

    @Test
    fun `a card that is gone leaves in silence`() =
        test {
            reviews.pages += page(listOf("a", "b"), count = 2)
            reviews.unsuspendResult = { ApiResult.Failure(ApiFailure.NotFound) }
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()
            assertEquals(listOf("b"), vm.ids)
            assertEquals(1, vm.state.value.count)
            assertNull(nextEffect(vm))
        }

    @Test
    fun `a network error keeps the row suspended and Riprova tries again`() =
        test {
            reviews.pages += page(listOf("a", "b"), count = 2)
            reviews.unsuspendResult = { NETWORK_DOWN }
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()
            assertEquals(listOf("a", "b"), vm.ids)
            assertEquals(2, vm.state.value.count)
            assertEquals(
                RowPhase.Failed,
                vm.state.value.rows
                    ?.first()
                    ?.phase,
            )
            assertNull(nextEffect(vm))

            reviews.unsuspendResult = { ApiResult.Success(card(it, suspended = false)) }
            vm.reactivate("a")
            runCurrent()
            assertEquals(listOf("b"), vm.ids)
            assertEquals(listOf("a", "a"), reviews.unsuspended)
        }

    @Test
    fun `a 5xx is treated like a network error for the row`() =
        test {
            reviews.pages += page(listOf("a"), count = 1)
            reviews.unsuspendResult = { ApiResult.Failure(ApiFailure.Unexpected(500)) }
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()
            assertEquals(
                RowPhase.Failed,
                vm.state.value.rows
                    ?.first()
                    ?.phase,
            )
        }

    /** The next one-shot message, or null when there is none. */
    private suspend fun nextEffect(vm: SuspendedViewModel): SuspendedEffect? {
        while (true) {
            val effect = withTimeoutOrNull(1) { vm.effects.first() } ?: return null
            if (effect !is SuspendedEffect.MoveFocus) return effect
        }
    }

    /** The next focus move, skipping the toast messages. */
    private suspend fun nextFocus(vm: SuspendedViewModel): SuspendedEffect.MoveFocus? {
        while (true) {
            val effect = withTimeoutOrNull(1) { vm.effects.first() } ?: return null
            if (effect is SuspendedEffect.MoveFocus) return effect
        }
    }

    @Test
    fun `an undo that lands after the card is already listed does not count it twice`() =
        test {
            reviews.pages += page(listOf("a", "b"), count = 2)
            val vm = vm()
            runCurrent()

            // "a" never left (for example a late answer): undoing it must keep the count.
            vm.undo("a")
            runCurrent()
            assertEquals(listOf("a", "b"), vm.ids)
            assertEquals(2, vm.state.value.count)
        }

    @Test
    fun `an undo during the first load starts over instead of inventing a one row list`() =
        test {
            val gate = CompletableDeferred<Unit>()
            reviews.pageGate = { gate.await() }
            reviews.pages += page(listOf("a", "b"), count = 2)
            val vm = vm()
            runCurrent()
            assertNull(vm.state.value.rows)

            vm.undo("a")
            runCurrent()
            gate.complete(Unit)
            runCurrent()

            assertEquals(listOf("a", "b"), vm.ids)
            assertEquals(2, vm.state.value.count)
            // The first load, then the refresh after the undo.
            assertEquals(2, reviews.requests.size)
        }

    @Test
    fun `a page still on its way when the list is refreshed is not appended`() =
        test {
            reviews.pages += page(listOf("a"), next = "c1", count = 3)
            reviews.pages += page(listOf("x", "y"), next = "c9", count = 3)
            reviews.pages += page(listOf("stale"), count = 3)
            val vm = vm()
            runCurrent()

            // Only the page request (the second one) is held back.
            val gate = CompletableDeferred<Unit>()
            reviews.pageGate = { if (reviews.requests.size == 2) gate.await() }
            vm.loadMore()
            runCurrent()
            vm.refresh()
            runCurrent()
            assertEquals(listOf("x", "y"), vm.ids)

            gate.complete(Unit)
            runCurrent()
            assertEquals(listOf("x", "y"), vm.ids)
            assertEquals("c9", vm.state.value.nextCursor)
        }

    @Test
    fun `a page that lists a card reactivated in this session does not bring it back`() =
        test {
            reviews.pages += page(listOf("a", "b"), next = "c1", count = 3)
            reviews.pages += page(listOf("a", "c"), count = 3)
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()

            vm.loadMore()
            runCurrent()
            assertEquals(listOf("b", "c"), vm.ids)
        }

    @Test
    fun `an undo on a card that is gone says so`() =
        test {
            reviews.pages += page(listOf("a", "b"), count = 2)
            reviews.suspendResult = { ApiResult.Failure(ApiFailure.NotFound) }
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()
            nextEffect(vm)

            vm.undo("a")
            runCurrent()
            assertEquals(SuspendedEffect.UndoGone, nextEffect(vm))
            assertEquals(listOf("b"), vm.ids)
            assertEquals(1, vm.state.value.count)
        }

    @Test
    fun `an undo on a superseded card says so`() =
        test {
            reviews.pages += page(listOf("a", "b"), count = 2)
            reviews.suspendResult = { ApiResult.Failure(ApiFailure.ReviewConflict.CardSuperseded) }
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            runCurrent()
            nextEffect(vm)

            vm.undo("a")
            runCurrent()
            assertEquals(SuspendedEffect.UndoSuperseded, nextEffect(vm))
            assertEquals(listOf("b"), vm.ids)
        }

    @Test
    fun `reactivating, undoing and reactivating the same row again keeps the count right`() =
        test {
            reviews.pages += page(listOf("a", "b"), count = 2)
            val vm = vm()
            runCurrent()

            vm.reactivate("a")
            runCurrent()
            assertEquals(1, vm.state.value.count)
            vm.undo("a")
            runCurrent()
            assertEquals(listOf("a", "b"), vm.ids)
            assertEquals(2, vm.state.value.count)
            vm.reactivate("a")
            runCurrent()
            assertEquals(listOf("b"), vm.ids)
            assertEquals(1, vm.state.value.count)
            assertEquals(listOf("a", "a"), reviews.unsuspended)
        }

    @Test
    fun `two quick reactivations of different rows both leave and the count follows`() =
        test {
            reviews.pages += page(listOf("a", "b", "c"), count = 3)
            val vm = vm()
            runCurrent()
            vm.reactivate("a")
            vm.reactivate("b")
            runCurrent()
            assertEquals(listOf("c"), vm.ids)
            assertEquals(1, vm.state.value.count)
        }

    @Test
    fun `focus goes to the next row, or to the title when there is none`() =
        test {
            reviews.pages += page(listOf("a", "b"), count = 2)
            val vm = vm()
            runCurrent()

            vm.reactivate("a")
            runCurrent()
            assertEquals(SuspendedEffect.MoveFocus("b"), nextFocus(vm))
            vm.reactivate("b")
            runCurrent()
            assertEquals(SuspendedEffect.MoveFocus(null), nextFocus(vm))
        }
}
