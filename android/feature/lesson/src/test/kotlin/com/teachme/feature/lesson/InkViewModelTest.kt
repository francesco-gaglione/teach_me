package com.teachme.feature.lesson

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.InMemoryTopicTabInbox
import com.teachme.core.model.LessonView
import com.teachme.core.model.ManualUserScopes
import com.teachme.core.model.StoredStrokes
import com.teachme.core.model.StrokeRecord
import com.teachme.sdui.highlight.HighlightColor
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkEdit
import com.teachme.sdui.ink.InkLimit
import com.teachme.sdui.ink.InkWidth
import com.teachme.sdui.ink.Stroke
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

/** The handwritten-note operations of the lesson screen: edits, undo, and the sending (handwriting.md section 9). */
@OptIn(ExperimentalCoroutinesApi::class)
class InkViewModelTest {
    private val backend = FakeLessons()
    private val queue = MemoryQueue()
    private val docs = FakeStore()
    private val strokeStore = MemoryStrokeStore()
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
            HighlightSyncer(backend, MemoryHighlightStore(), scopes),
            StrokeSyncer(backend, strokeStore, scopes),
            queue,
            InMemoryTopicTabInbox(),
        ).also { created += it }

    private suspend fun TestScope.opened(key: String = "L1"): LessonViewModel {
        val vm = vm(key)
        runCurrent()
        return vm
    }

    private fun ready(vm: LessonViewModel) = vm.state.value as LessonUiState.Ready

    private fun strokes(vm: LessonViewModel) = ready(vm).ink.set.strokes

    private fun serverHas(vararg items: StrokeRecord) {
        backend.view =
            ApiResult.Success(LessonView(lesson(), SMALL_LESSON, emptyList(), null, emptyList(), items.toList()))
    }

    private fun rec(
        n: Int,
        color: String = "ink",
    ) = StrokeRecord("id-$n", "p1", color, "medium", listOf(n, n))

    private fun stroke(
        n: Int,
        color: InkColor = InkColor.INK,
    ) = Stroke("id-$n", "p1", color, InkWidth.MEDIUM, listOf(n, n))

    @Test
    fun `opens with the strokes of the backend in order, an unknown colour is ink, and the default pen choices`() =
        vmTest {
            serverHas(rec(1, "red"), rec(0, "ultraviolet"))
            val vm = opened()
            assertEquals(listOf(stroke(1, InkColor.RED), stroke(0)), strokes(vm))
            assertEquals(PenTool.HIGHLIGHTER, ready(vm).penTool)
            assertEquals(InkColor.INK, ready(vm).inkColor)
            assertEquals(InkWidth.MEDIUM, ready(vm).inkWidth)
            assertFalse(ready(vm).canUndoInk)
            assertFalse(ready(vm).inkSyncFailing)
            assertTrue(backend.strokeRequests.isEmpty())
        }

    @Test
    fun `without a strokes field in the lesson there are none`() =
        vmTest {
            val vm = opened()
            assertEquals(emptyList<Stroke>(), strokes(vm))
        }

    @Test
    fun `unsent notes win over the strokes of the server and are sent when the lesson opens`() =
        vmTest {
            serverHas(rec(0))
            strokeStore.entries["L1"] = StoredStrokes("L1", listOf(rec(5, "blue")), dirty = true)
            val vm = opened()
            assertEquals(listOf(stroke(5, InkColor.BLUE)), strokes(vm))
            assertEquals(listOf(listOf(rec(5, "blue"))), backend.strokeRequests)
            assertFalse(strokeStore.entries.getValue("L1").dirty)
        }

    @Test
    fun `a written stroke is on disk at once and goes out once, 4 s after the last edit`() =
        vmTest {
            val vm = opened()
            assertTrue(vm.writeStroke(stroke(0)) is InkEdit.Applied)
            runCurrent()
            assertEquals(listOf(rec(0)), strokeStore.entries["L1"]?.items)
            assertTrue(strokeStore.entries.getValue("L1").dirty)
            advanceTimeBy(3_000)
            vm.writeStroke(stroke(1))
            runCurrent()
            advanceTimeBy(3_900)
            assertTrue("the debounce restarted at the last edit", backend.strokeRequests.isEmpty())
            advanceTimeBy(200)
            assertEquals(listOf(listOf(rec(0), rec(1))), backend.strokeRequests)
            assertFalse(strokeStore.entries.getValue("L1").dirty)
            assertFalse(ready(vm).inkSyncFailing)
        }

    @Test
    fun `an eraser pass, undo and redo change the set, enable the buttons, and are sent`() =
        vmTest {
            val vm = opened()
            vm.writeStroke(stroke(0))
            vm.writeStroke(stroke(1))
            vm.writeStroke(stroke(2))
            assertTrue(ready(vm).canUndoInk)
            assertFalse(ready(vm).canRedoInk)
            assertTrue(vm.eraseStrokes(listOf("id-0", "id-2")) is InkEdit.Applied)
            assertEquals(listOf(stroke(1)), strokes(vm))
            assertTrue(vm.undoInk() is InkEdit.Applied)
            // Back at the original positions.
            assertEquals(listOf(stroke(0), stroke(1), stroke(2)), strokes(vm))
            assertTrue(ready(vm).canRedoInk)
            assertTrue(vm.redoInk() is InkEdit.Applied)
            assertEquals(listOf(stroke(1)), strokes(vm))
            runCurrent()
            advanceTimeBy(4_100)
            assertEquals(listOf(rec(1)), backend.storedStrokes)
        }

    @Test
    fun `edits that change nothing are not saved and not sent`() =
        vmTest {
            val vm = opened()
            assertEquals(InkEdit.NoOp, vm.eraseStrokes(listOf("nothing")))
            assertEquals(InkEdit.NoOp, vm.eraseStrokes(emptyList()))
            assertEquals(InkEdit.NoOp, vm.undoInk())
            assertEquals(InkEdit.NoOp, vm.redoInk())
            runCurrent()
            advanceTimeBy(10_000)
            assertTrue(backend.strokeRequests.isEmpty())
            assertTrue(strokeStore.dirtyLessons().isEmpty())
        }

    @Test
    fun `a stroke over a limit is rejected and leaves the set, the history and the disk as they were`() =
        vmTest {
            val vm = opened()
            val long = Stroke("big", "p1", InkColor.INK, InkWidth.MEDIUM, List(2002) { it })
            assertEquals(InkEdit.Rejected(InkLimit.TOO_MANY_POINTS_IN_STROKE), vm.writeStroke(long))
            runCurrent()
            assertEquals(emptyList<Stroke>(), strokes(vm))
            assertFalse(ready(vm).canUndoInk)
            assertTrue(strokeStore.dirtyLessons().isEmpty())
        }

    @Test
    fun `the history belongs to the screen session, a new session starts with empty stacks`() =
        vmTest {
            val first = opened()
            first.writeStroke(stroke(0))
            runCurrent()
            assertTrue(ready(first).canUndoInk)
            val second = opened()
            assertEquals(listOf(stroke(0)), strokes(second))
            assertFalse(ready(second).canUndoInk)
        }

    @Test
    fun `the eraser switch starts off, is not saved and goes off when the tool or the colour is chosen`() =
        vmTest {
            val vm = opened()
            assertFalse(ready(vm).eraserMode)
            vm.setPenTool(PenTool.PEN)

            fun on() {
                vm.setEraserMode(true)
                assertTrue(ready(vm).eraserMode)
            }
            on()
            vm.setPenTool(PenTool.PEN)
            assertTrue("the same tool again changes nothing", ready(vm).eraserMode)
            vm.setPenTool(PenTool.HIGHLIGHTER)
            assertFalse(ready(vm).eraserMode)
            on()
            vm.setHighlightColor(HighlightColor.PINK)
            assertFalse(ready(vm).eraserMode)
            vm.setPenTool(PenTool.PEN)
            on()
            vm.setInkColor(InkColor.RED)
            assertFalse(ready(vm).eraserMode)
            on()
            vm.setInkWidth(InkWidth.THICK)
            assertFalse(ready(vm).eraserMode)
            // Undo and redo keep it on; switching it off by hand works.
            vm.writeStroke(stroke(0))
            on()
            vm.undoInk()
            vm.redoInk()
            assertTrue(ready(vm).eraserMode)
            vm.setEraserMode(false)
            assertFalse(ready(vm).eraserMode)
            runCurrent()
            // Nothing about it is persisted: a lesson opened again starts with it off.
            on()
            assertFalse(ready(opened()).eraserMode)
        }

    @Test
    fun `the pen choices are changed on the screen and remembered on the device`() =
        vmTest {
            strokeStore.tool = "pen"
            strokeStore.color = "green"
            strokeStore.width = "thin"
            val vm = opened()
            assertEquals(PenTool.PEN, ready(vm).penTool)
            assertEquals(InkColor.GREEN, ready(vm).inkColor)
            assertEquals(InkWidth.THIN, ready(vm).inkWidth)
            vm.setPenTool(PenTool.HIGHLIGHTER)
            vm.setInkColor(InkColor.RED)
            vm.setInkWidth(InkWidth.THICK)
            runCurrent()
            assertEquals(PenTool.HIGHLIGHTER, ready(vm).penTool)
            assertEquals(InkColor.RED, ready(vm).inkColor)
            assertEquals(InkWidth.THICK, ready(vm).inkWidth)
            assertEquals("highlighter", strokeStore.tool)
            assertEquals("red", strokeStore.color)
            assertEquals("thick", strokeStore.width)
            // Choosing does not touch the notes.
            runCurrent()
            advanceTimeBy(10_000)
            assertTrue(backend.strokeRequests.isEmpty())
        }

    @Test
    fun `a failing send retries after 5, 15 and 60 s, shows the indicator only after 30 s, and clears it on success`() =
        vmTest {
            repeat(10) { backend.strokeFailures += NETWORK_DOWN }
            val vm = opened()
            vm.writeStroke(stroke(0))
            runCurrent()
            advanceTimeBy(4_001)
            assertEquals(1, backend.strokeRequests.size)
            assertTrue(strokeStore.entries.getValue("L1").dirty)
            advanceTimeBy(5_000)
            assertEquals(2, backend.strokeRequests.size)
            advanceTimeBy(15_000)
            assertEquals(3, backend.strokeRequests.size)
            assertFalse(ready(vm).inkSyncFailing)
            advanceTimeBy(10_000)
            assertTrue(ready(vm).inkSyncFailing)
            backend.strokeFailures.clear()
            advanceTimeBy(60_000)
            assertEquals(listOf(rec(0)), backend.storedStrokes)
            assertFalse(ready(vm).inkSyncFailing)
            assertFalse(strokeStore.entries.getValue("L1").dirty)
        }

    @Test
    fun `after the last retry a resume tries again`() =
        vmTest {
            repeat(4) { backend.strokeFailures += NETWORK_DOWN }
            val vm = opened()
            vm.writeStroke(stroke(0))
            runCurrent()
            advanceTimeBy(4_001 + 5_000 + 15_000 + 60_000 + 1)
            assertEquals(4, backend.strokeRequests.size)
            advanceTimeBy(600_000)
            assertEquals(4, backend.strokeRequests.size)
            vm.onResume()
            runCurrent()
            assertEquals(5, backend.strokeRequests.size)
            assertFalse(strokeStore.entries.getValue("L1").dirty)
            assertFalse(ready(vm).inkSyncFailing)
        }

    @Test
    fun `sync now sends without waiting for the debounce`() =
        vmTest {
            val vm = opened()
            vm.writeStroke(stroke(0))
            runCurrent()
            vm.syncInkNow()
            runCurrent()
            assertEquals(1, backend.strokeRequests.size)
            advanceTimeBy(10_000)
            assertEquals(1, backend.strokeRequests.size)
        }

    @Test
    fun `a stroke the backend refuses disappears from the screen and from undo, and the rest is sent`() =
        vmTest {
            val vm = opened()
            vm.writeStroke(stroke(0))
            vm.writeStroke(stroke(1))
            runCurrent()
            backend.strokeFailures +=
                ApiFailure.Validation(listOf(ApiErrorDetail("strokes[0].block_id", "invalid")))
            advanceTimeBy(4_100)
            assertEquals(listOf(stroke(1)), strokes(vm))
            assertEquals(listOf(rec(1)), backend.storedStrokes)
            // Undo takes the remaining one away and cannot bring the refused one back.
            assertTrue(vm.undoInk() is InkEdit.Applied)
            assertEquals(emptyList<Stroke>(), strokes(vm))
            assertFalse(ready(vm).canUndoInk)
            assertFalse(ready(vm).inkSyncFailing)
        }

    @Test
    fun `a set that cannot be fixed or is too big is parked, with no loop, no indicator, and the notes stay`() =
        vmTest {
            val vm = opened()
            vm.writeStroke(stroke(0))
            runCurrent()
            backend.strokeFailures += ApiFailure.Unexpected(413)
            advanceTimeBy(4_100)
            advanceTimeBy(600_000)
            assertEquals(1, backend.strokeRequests.size)
            assertEquals(listOf(stroke(0)), strokes(vm))
            assertFalse(ready(vm).inkSyncFailing)
            assertTrue(strokeStore.entries.getValue("L1").parked)
            // The next edit unparks it and goes out.
            vm.writeStroke(stroke(1))
            runCurrent()
            advanceTimeBy(4_100)
            assertEquals(2, backend.strokeRequests.size)
        }

    @Test
    fun `a parked set is not sent again by a pause or a close, only by Riprova ora`() =
        vmTest {
            val vm = opened()
            vm.writeStroke(stroke(0))
            runCurrent()
            backend.strokeFailures += ApiFailure.Validation(listOf(ApiErrorDetail("strokes", "too_many")))
            advanceTimeBy(4_100)
            assertEquals(1, backend.strokeRequests.size)
            assertTrue(strokeStore.entries.getValue("L1").parked)
            vm.syncInkNow()
            runCurrent()
            vm.syncInkNow()
            runCurrent()
            assertEquals(1, backend.strokeRequests.size)
            vm.retryQueueNow()
            runCurrent()
            assertEquals(2, backend.strokeRequests.size)
        }

    @Test
    fun `404 while sending says the lesson is gone`() =
        vmTest {
            val vm = opened()
            vm.writeStroke(stroke(0))
            runCurrent()
            backend.strokeFailures += ApiFailure.NotFound
            advanceTimeBy(4_100)
            assertEquals(LessonUiState.Gone, vm.state.value)
        }

    @Test
    fun `opened from the copy on the device it shows what the device holds, then the backend's set once it answers`() =
        vmTest {
            val view = LessonView(lesson(), SMALL_LESSON, emptyList(), null, emptyList(), listOf(rec(0)))
            docs.save("doc/L1", CachedLesson.of(view).encode())
            strokeStore.entries["L1"] = StoredStrokes("L1", listOf(rec(40)), dirty = false)
            backend.view = ApiResult.Failure(NETWORK_DOWN)
            val vm = opened()
            assertEquals(listOf(stroke(40)), strokes(vm))
            backend.view = ApiResult.Success(view)
            vm.onResume()
            runCurrent()
            assertEquals(listOf(stroke(0)), strokes(vm))
        }

    @Test
    fun `opened from the copy with unsent notes the backend does not replace them`() =
        vmTest {
            val view = LessonView(lesson(), SMALL_LESSON, emptyList(), null, emptyList(), listOf(rec(0)))
            docs.save("doc/L1", CachedLesson.of(view).encode())
            strokeStore.entries["L1"] = StoredStrokes("L1", listOf(rec(40)), dirty = true)
            backend.view = ApiResult.Failure(NETWORK_DOWN)
            repeat(3) { backend.strokeFailures += NETWORK_DOWN }
            val vm = opened()
            backend.view = ApiResult.Success(view)
            vm.onResume()
            runCurrent()
            assertEquals(listOf(stroke(40)), strokes(vm))
            assertTrue(strokeStore.entries.getValue("L1").dirty)
        }

    @Test
    fun `a sample lesson takes edits but keeps and sends nothing`() =
        vmTest {
            val vm = opened("fixture/k.json")
            assertTrue(vm.writeStroke(stroke(0)) is InkEdit.Applied)
            runCurrent()
            advanceTimeBy(10_000)
            assertEquals(listOf(stroke(0)), strokes(vm))
            assertTrue(backend.strokeRequests.isEmpty())
            assertTrue(strokeStore.entries.isEmpty())
        }

    @Test
    fun `edits before the lesson is ready do nothing`() =
        vmTest {
            val vm = vm()
            assertEquals(InkEdit.NoOp, vm.writeStroke(stroke(0)))
            assertEquals(InkEdit.NoOp, vm.undoInk())
        }

    @Test
    fun `a failing save does not end the persisting, the next edit is saved and sent`() =
        vmTest {
            val vm = opened()
            strokeStore.failNextUpdate = true
            vm.writeStroke(stroke(0))
            runCurrent()
            advanceTimeBy(4_100)
            assertEquals(0, backend.strokeRequests.size)
            vm.writeStroke(stroke(1))
            runCurrent()
            advanceTimeBy(4_100)
            assertEquals(listOf(listOf(rec(0), rec(1))), backend.strokeRequests)
        }

    @Test
    fun `a full disk keeps the notes in memory, shows the row, and the retries save and send them when it is back`() =
        vmTest {
            val vm = opened()
            strokeStore.diskFailing = true
            vm.writeStroke(stroke(0))
            runCurrent()
            advanceTimeBy(40_000)
            assertEquals(1, strokes(vm).size)
            assertTrue(backend.strokeRequests.isEmpty())
            assertTrue(ready(vm).inkSyncFailing)
            strokeStore.diskFailing = false
            advanceTimeBy(60_000)
            assertEquals(listOf(rec(0)), backend.storedStrokes)
            assertFalse(ready(vm).inkSyncFailing)
            assertFalse(strokeStore.entries.getValue("L1").dirty)
        }

    @Test
    fun `after a full disk the next stroke saves the whole set again`() =
        vmTest {
            val vm = opened()
            strokeStore.diskFailing = true
            vm.writeStroke(stroke(0))
            runCurrent()
            strokeStore.diskFailing = false
            vm.writeStroke(stroke(1))
            runCurrent()
            assertEquals(listOf(rec(0), rec(1)), strokeStore.entries["L1"]?.items)
            advanceTimeBy(4_100)
            assertEquals(listOf(rec(0), rec(1)), backend.storedStrokes)
        }

    @Test
    fun `a 413 is told to the queue until a send gets through, other refusals are not`() =
        vmTest {
            val vm = opened()
            vm.writeStroke(stroke(0))
            runCurrent()
            backend.strokeFailures += ApiFailure.Unexpected(413)
            advanceTimeBy(4_100)
            assertTrue(ready(vm).inkTooLarge)
            assertFalse(ready(vm).inkSyncFailing)
            // A pause does not send the parked set again and does not make the row disappear.
            vm.syncInkNow()
            runCurrent()
            assertTrue(ready(vm).inkTooLarge)
            // The next edit goes out and the row goes away.
            vm.writeStroke(stroke(1))
            runCurrent()
            advanceTimeBy(4_100)
            assertFalse(ready(vm).inkTooLarge)
        }

    @Test
    fun `a refusal without the 413 does not say that the notes are too big`() =
        vmTest {
            val vm = opened()
            vm.writeStroke(stroke(0))
            runCurrent()
            backend.strokeFailures += ApiFailure.Validation(listOf(ApiErrorDetail("strokes", "too_many")))
            advanceTimeBy(4_100)
            assertFalse(ready(vm).inkTooLarge)
        }

    @Test
    fun `the notes of a block are cleared as one pass that can be taken back, once, and only while it is the last`() =
        vmTest {
            val vm = opened()
            vm.writeStroke(stroke(0))
            vm.writeStroke(stroke(1).copy(blockId = "other"))
            vm.writeStroke(stroke(2))
            val pass = vm.clearBlockNotes("p1")!!
            assertEquals(listOf("id-1"), strokes(vm).map { it.id })
            assertEquals(null, vm.clearBlockNotes("p1"))
            assertTrue(vm.undoNotesClear(pass))
            assertEquals(listOf("id-0", "id-1", "id-2"), strokes(vm).map { it.id })
            // Another operation in between: the pass is no longer the last one and nothing else is undone.
            val again = vm.clearBlockNotes("p1")!!
            vm.writeStroke(stroke(3))
            assertFalse(vm.undoNotesClear(again))
            assertEquals(listOf("id-1", "id-3"), strokes(vm).map { it.id })
        }
}
