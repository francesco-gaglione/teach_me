package com.teachme.feature.lesson

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.HighlightRecord
import com.teachme.core.model.InMemoryTopicTabInbox
import com.teachme.core.model.LessonView
import com.teachme.core.model.ManualUserScopes
import com.teachme.core.model.StoredHighlights
import com.teachme.sdui.highlight.Highlight
import com.teachme.sdui.highlight.HighlightColor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestResult
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** The highlight operations of the lesson screen: edits, the limit, undo, and the sending (highlight.md sections 5, 11). */
@OptIn(ExperimentalCoroutinesApi::class)
class HighlightsViewModelTest {
    private val backend = FakeLessons()
    private val queue = MemoryQueue()
    private val docs = FakeStore()
    private val highlightStore = MemoryHighlightStore()
    private val clock = ImmediateDispatchers()
    private val scopes = ManualUserScopes()

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private val created = mutableListOf<LessonViewModel>()

    private fun vmTest(body: suspend TestScope.() -> Unit): TestResult =
        runTest {
            try {
                body()
            } finally {
                created.forEach { it.viewModelScope.cancel() }
            }
        }

    private fun vm(key: String = "L1") =
        LessonViewModel(
            SavedStateHandle(mapOf("key" to key)),
            FakeSource(SMALL_LESSON),
            docs,
            FakeLogger(),
            clock,
            backend,
            AttemptSyncer(backend, queue, scopes),
            HighlightSyncer(backend, highlightStore, scopes),
            StrokeSyncer(backend, MemoryStrokeStore(), scopes),
            queue,
            InMemoryTopicTabInbox(),
        ).also { created += it }

    private suspend fun TestScope.opened(key: String = "L1"): LessonViewModel {
        val vm = vm(key)
        runCurrent()
        return vm
    }

    private fun ready(vm: LessonViewModel) = vm.state.value as LessonUiState.Ready

    private fun serverHas(vararg items: HighlightRecord) {
        backend.view = ApiResult.Success(LessonView(lesson(), SMALL_LESSON, emptyList(), null, items.toList()))
    }

    private fun rec(
        start: Int,
        color: String = "yellow",
    ) = HighlightRecord("p1", start, start + 5, color)

    private fun hl(
        start: Int,
        end: Int = start + 5,
        color: HighlightColor = HighlightColor.YELLOW,
        block: String = "p1",
    ) = Highlight(block, start, end, color)

    @Test
    fun `opens with the highlights of the backend and the remembered colour, an unknown colour is yellow`() =
        vmTest {
            serverHas(rec(0, "green"), rec(10, "ultraviolet"))
            highlightStore.color = "pink"
            val vm = opened()
            assertEquals(listOf(hl(0, color = HighlightColor.GREEN), hl(10)), ready(vm).highlights)
            assertEquals(HighlightColor.PINK, ready(vm).highlightColor)
            assertFalse(ready(vm).highlightSyncFailing)
            assertTrue(backend.highlightRequests.isEmpty())
        }

    @Test
    fun `without a highlights field in the lesson there are none`() =
        vmTest {
            val vm = opened()
            assertEquals(emptyList<Highlight>(), ready(vm).highlights)
            assertEquals(HighlightColor.YELLOW, ready(vm).highlightColor)
        }

    @Test
    fun `unsent changes win over the highlights of the server and are sent when the lesson opens`() =
        vmTest {
            serverHas(rec(0))
            highlightStore.entries["L1"] = StoredHighlights("L1", listOf(rec(20, "blue")), dirty = true)
            val vm = opened()
            assertEquals(listOf(hl(20, color = HighlightColor.BLUE)), ready(vm).highlights)
            assertEquals(listOf(listOf(rec(20, "blue"))), backend.highlightRequests)
            assertFalse(highlightStore.entries.getValue("L1").dirty)
        }

    @Test
    fun `an edit is on disk at once and goes out once, 1,5 s after the last one`() =
        vmTest {
            val vm = opened()
            assertEquals(HighlightEdit.Applied, vm.addHighlight("p1", 0, 5))
            runCurrent()
            assertEquals(listOf(rec(0)), highlightStore.entries["L1"]?.items)
            assertTrue(highlightStore.entries.getValue("L1").dirty)
            advanceTimeBy(1_000)
            assertEquals(HighlightEdit.Applied, vm.addHighlight("p1", 10, 15, HighlightColor.BLUE))
            runCurrent()
            advanceTimeBy(1_400)
            assertTrue("the debounce restarted at the last edit", backend.highlightRequests.isEmpty())
            advanceTimeBy(200)
            assertEquals(listOf(listOf(rec(0), rec(10, "blue"))), backend.highlightRequests)
            assertFalse(highlightStore.entries.getValue("L1").dirty)
            assertFalse(ready(vm).highlightSyncFailing)
        }

    @Test
    fun `an edit that changes nothing is not saved and not sent`() =
        vmTest {
            val vm = opened()
            vm.addHighlight("p1", 0, 20)
            runCurrent()
            advanceTimeBy(2_000)
            assertEquals(1, backend.highlightRequests.size)
            // A stroke inside a highlight of the same colour, and an eraser over nothing.
            assertEquals(HighlightEdit.Unchanged, vm.addHighlight("p1", 5, 10))
            assertEquals(HighlightEdit.Unchanged, vm.eraseHighlight("p1", 30, 40))
            assertEquals(HighlightEdit.Unchanged, vm.eraseHighlight("p2", 0, 40))
            runCurrent()
            advanceTimeBy(5_000)
            assertEquals(1, backend.highlightRequests.size)
            assertFalse(highlightStore.entries.getValue("L1").dirty)
        }

    @Test
    fun `the colour of a stroke defaults to the current one, and picking one is remembered`() =
        vmTest {
            val vm = opened()
            vm.setHighlightColor(HighlightColor.GREEN)
            runCurrent()
            assertEquals("green", highlightStore.color)
            vm.addHighlight("p1", 0, 5)
            assertEquals(listOf(hl(0, color = HighlightColor.GREEN)), ready(vm).highlights)
            // Painting over it with another colour overwrites.
            vm.addHighlight("p1", 0, 5, HighlightColor.PINK)
            assertEquals(listOf(hl(0, color = HighlightColor.PINK)), ready(vm).highlights)
        }

    @Test
    fun `the eraser splits, and the limit of 500 stops what would grow past it`() =
        vmTest {
            serverHas(*(0 until 500).map { HighlightRecord("p1", it * 10, it * 10 + 5, "yellow") }.toTypedArray())
            val vm = opened()
            assertEquals(500, ready(vm).highlights.size)
            assertEquals(HighlightEdit.LimitReached, vm.addHighlight("p1", 6000, 6005))
            // A stroke that merges two of them makes the set smaller: it is allowed.
            assertEquals(HighlightEdit.Applied, vm.addHighlight("p1", 3, 12))
            assertEquals(499, ready(vm).highlights.size)
            assertEquals(HighlightEdit.Applied, vm.addHighlight("p1", 6000, 6005))
            assertEquals(HighlightEdit.LimitReached, vm.addHighlight("p1", 7000, 7005))
            // The eraser cutting one in two would make 501: refused too, and nothing changed.
            assertEquals(HighlightEdit.LimitReached, vm.eraseHighlight("p1", 21, 23))
            assertEquals(500, ready(vm).highlights.size)
            // Removing one makes room.
            assertEquals(1, vm.removeHighlightAt("p1", 30).size)
            assertEquals(HighlightEdit.Applied, vm.addHighlight("p1", 7000, 7005))
            assertEquals(500, ready(vm).highlights.size)
        }

    @Test
    fun `remove at a position returns what it took away and undo puts it back with its colour`() =
        vmTest {
            val vm = opened()
            vm.addHighlight("p1", 0, 10, HighlightColor.BLUE)
            vm.addHighlight("p1", 20, 30, HighlightColor.PINK)
            val removed = vm.removeHighlightAt("p1", 25)
            assertEquals(listOf(hl(20, 30, HighlightColor.PINK)), removed)
            assertEquals(listOf(hl(0, 10, HighlightColor.BLUE)), ready(vm).highlights)
            assertEquals(emptyList<Highlight>(), vm.removeHighlightAt("p1", 15))
            assertEquals(HighlightEdit.Applied, vm.restoreHighlights(removed))
            assertEquals(
                setOf(hl(0, 10, HighlightColor.BLUE), hl(20, 30, HighlightColor.PINK)),
                ready(vm).highlights.toSet(),
            )
            runCurrent()
            advanceTimeBy(2_000)
            assertEquals(2, backend.storedHighlights.size)
        }

    @Test
    fun `a failing send retries after 5, 15 and 60 s, shows the indicator only after 30 s, and clears it on success`() =
        vmTest {
            repeat(10) { backend.highlightFailures += NETWORK_DOWN }
            val vm = opened()
            vm.addHighlight("p1", 0, 5)
            runCurrent()
            advanceTimeBy(1_501)
            assertEquals(1, backend.highlightRequests.size)
            assertTrue(highlightStore.entries.getValue("L1").dirty)
            advanceTimeBy(5_000)
            assertEquals(2, backend.highlightRequests.size)
            advanceTimeBy(15_000)
            assertEquals(3, backend.highlightRequests.size)
            // 20 s after the first failure: the indicator waits for 30 s of failures.
            assertFalse(ready(vm).highlightSyncFailing)
            advanceTimeBy(10_000)
            assertTrue(ready(vm).highlightSyncFailing)
            // The network is back: the 60 s retry goes through and the indicator goes away.
            backend.highlightFailures.clear()
            advanceTimeBy(60_000)
            assertEquals(listOf(rec(0)), backend.storedHighlights)
            assertFalse(ready(vm).highlightSyncFailing)
            assertFalse(highlightStore.entries.getValue("L1").dirty)
        }

    @Test
    fun `after the last retry a resume tries again`() =
        vmTest {
            repeat(4) { backend.highlightFailures += NETWORK_DOWN }
            val vm = opened()
            vm.addHighlight("p1", 0, 5)
            runCurrent()
            advanceTimeBy(1_501 + 5_000 + 15_000 + 60_000 + 1)
            assertEquals(4, backend.highlightRequests.size)
            advanceTimeBy(600_000)
            assertEquals("nothing is retried by itself after the schedule", 4, backend.highlightRequests.size)
            vm.onResume()
            runCurrent()
            assertEquals(5, backend.highlightRequests.size)
            assertFalse(highlightStore.entries.getValue("L1").dirty)
            assertFalse(ready(vm).highlightSyncFailing)
        }

    @Test
    fun `sync now sends without waiting for the debounce`() =
        vmTest {
            val vm = opened()
            vm.addHighlight("p1", 0, 5)
            runCurrent()
            vm.syncHighlightsNow()
            runCurrent()
            assertEquals(1, backend.highlightRequests.size)
            advanceTimeBy(5_000)
            assertEquals(1, backend.highlightRequests.size)
        }

    @Test
    fun `an entry the backend refuses disappears from the screen and the rest is sent`() =
        vmTest {
            val vm = opened()
            vm.addHighlight("p1", 0, 5)
            vm.addHighlight("p1", 10, 15)
            runCurrent()
            backend.highlightFailures +=
                ApiFailure.Validation(listOf(ApiErrorDetail("highlights[0].block_id", "invalid")))
            advanceTimeBy(1_600)
            assertEquals(listOf(hl(10)), ready(vm).highlights)
            assertEquals(listOf(rec(10)), backend.storedHighlights)
            assertFalse(ready(vm).highlightSyncFailing)
        }

    @Test
    fun `a refused set that cannot be fixed is parked, with no loop, no indicator, and the marks stay`() =
        vmTest {
            val vm = opened()
            vm.addHighlight("p1", 0, 5)
            runCurrent()
            backend.highlightFailures += ApiFailure.Validation(listOf(ApiErrorDetail("highlights", "too_many")))
            advanceTimeBy(1_600)
            advanceTimeBy(600_000)
            assertEquals(1, backend.highlightRequests.size)
            assertEquals(listOf(hl(0)), ready(vm).highlights)
            assertFalse(ready(vm).highlightSyncFailing)
            assertTrue(highlightStore.entries.getValue("L1").parked)
            // The next edit unparks it and goes out.
            vm.addHighlight("p1", 10, 15)
            runCurrent()
            advanceTimeBy(1_600)
            assertEquals(2, backend.highlightRequests.size)
        }

    @Test
    fun `404 while sending says the lesson is gone`() =
        vmTest {
            val vm = opened()
            vm.addHighlight("p1", 0, 5)
            runCurrent()
            backend.highlightFailures += ApiFailure.NotFound
            advanceTimeBy(1_600)
            assertEquals(LessonUiState.Gone, vm.state.value)
        }

    @Test
    fun `opened from the copy on the device it shows what the device holds, then the backend's set once it answers`() =
        vmTest {
            val view = LessonView(lesson(), SMALL_LESSON, emptyList(), null, listOf(rec(0)))
            docs.save("doc/L1", CachedLesson.of(view).encode())
            highlightStore.entries["L1"] = StoredHighlights("L1", listOf(rec(40)), dirty = false)
            backend.view = ApiResult.Failure(NETWORK_DOWN)
            val vm = opened()
            assertEquals(listOf(hl(40)), ready(vm).highlights)
            backend.view = ApiResult.Success(view)
            vm.onResume()
            runCurrent()
            assertEquals(listOf(hl(0)), ready(vm).highlights)
        }

    @Test
    fun `opened from the copy with unsent changes the backend does not replace them`() =
        vmTest {
            val view = LessonView(lesson(), SMALL_LESSON, emptyList(), null, listOf(rec(0)))
            docs.save("doc/L1", CachedLesson.of(view).encode())
            highlightStore.entries["L1"] = StoredHighlights("L1", listOf(rec(40)), dirty = true)
            backend.view = ApiResult.Failure(NETWORK_DOWN)
            repeat(3) { backend.highlightFailures += NETWORK_DOWN }
            val vm = opened()
            backend.view = ApiResult.Success(view)
            vm.onResume()
            runCurrent()
            assertEquals(listOf(hl(40)), ready(vm).highlights)
            assertTrue(highlightStore.entries.getValue("L1").dirty)
        }

    @Test
    fun `a sample lesson takes edits but keeps and sends nothing`() =
        vmTest {
            val vm = opened("fixture/k.json")
            assertEquals(HighlightEdit.Applied, vm.addHighlight("p1", 0, 5))
            runCurrent()
            advanceTimeBy(5_000)
            assertEquals(listOf(hl(0)), ready(vm).highlights)
            assertTrue(backend.highlightRequests.isEmpty())
            assertTrue(highlightStore.entries.isEmpty())
        }

    @Test
    fun `edits before the lesson is ready do nothing`() =
        vmTest {
            val vm = vm()
            assertEquals(HighlightEdit.Unchanged, vm.addHighlight("p1", 0, 5))
            assertEquals(emptyList<Highlight>(), vm.removeHighlightAt("p1", 0))
        }

    @Test
    fun `a parked set is not sent again by a pause or a close, only by Riprova ora`() =
        vmTest {
            val vm = opened()
            vm.addHighlight("p1", 0, 5)
            runCurrent()
            backend.highlightFailures += ApiFailure.Validation(listOf(ApiErrorDetail("highlights", "too_many")))
            advanceTimeBy(1_600)
            assertEquals(1, backend.highlightRequests.size)
            assertTrue(highlightStore.entries.getValue("L1").parked)
            // ON_PAUSE and the disposal of the screen.
            vm.syncHighlightsNow()
            runCurrent()
            vm.syncHighlightsNow()
            runCurrent()
            assertEquals(1, backend.highlightRequests.size)
            vm.retryQueueNow()
            runCurrent()
            assertEquals(2, backend.highlightRequests.size)
        }

    @Test
    fun `a failing save does not end the persisting, the next edit is saved and sent`() =
        vmTest {
            val vm = opened()
            highlightStore.failNextUpdate = true
            vm.addHighlight("p1", 0, 5)
            runCurrent()
            advanceTimeBy(1_600)
            assertEquals(0, backend.highlightRequests.size)
            vm.addHighlight("p1", 10, 15)
            runCurrent()
            advanceTimeBy(1_600)
            assertEquals(listOf(listOf(rec(0), rec(10))), backend.highlightRequests)
        }
}
