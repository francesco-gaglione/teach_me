package com.teachme.feature.lesson

import androidx.lifecycle.SavedStateHandle
import com.teachme.core.model.InMemoryTopicTabInbox
import com.teachme.core.model.LessonStateStore
import com.teachme.sdui.logic.ExerciseReducers
import com.teachme.sdui.logic.LessonSavedState
import com.teachme.sdui.model.SduiEvent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File

@OptIn(ExperimentalCoroutinesApi::class)
class LessonViewModelTest {
    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun fixture(name: String): String {
        val dir =
            generateSequence(File(System.getProperty("user.dir").orEmpty()).absoluteFile) { it.parentFile }
                .map { File(it, "docs/contracts/fixtures") }
                .first { it.isDirectory }
        return File(dir, name).readText()
    }

    /** A sample lesson of a debug build: it comes from the fixture source and sends nothing to a backend. */
    private fun vm(
        text: String?,
        store: LessonStateStore = FakeStore(),
        logger: SduiEventLogger = FakeLogger(),
    ) = LessonViewModel(
        SavedStateHandle(mapOf("key" to "fixture/k.json")),
        FakeSource(text),
        store,
        logger,
        ImmediateDispatchers(),
        FakeLessons(),
        AttemptSyncer(
            FakeLessons(),
            MemoryQueue(),
            com.teachme.core.model
                .ManualUserScopes(),
        ),
        HighlightSyncer(
            FakeLessons(),
            MemoryHighlightStore(),
            com.teachme.core.model
                .ManualUserScopes(),
        ),
        StrokeSyncer(
            FakeLessons(),
            MemoryStrokeStore(),
            com.teachme.core.model
                .ManualUserScopes(),
        ),
        MemoryQueue(),
        InMemoryTopicTabInbox(),
    )

    private fun ready(vm: LessonViewModel) = vm.state.value as LessonUiState.Ready

    @Test
    fun `loads a fixture and starts on the introduction`() {
        val vm = vm(fixture("lesson-rust-ownership.json"))
        assertEquals(1, ready(vm).saved.revealedCount)
        assertEquals(0, ready(vm).currentIndex)
    }

    @Test
    fun `missing or broken documents give the right states`() {
        assertEquals(LessonUiState.LoadFailed, vm(null).state.value)
        assertEquals(LessonUiState.Unusable, vm("{not json").state.value)
    }

    @Test
    fun `a sample lesson sends nothing and completes locally`() {
        val vm = vm(fixture("lesson-rust-ownership.json"))
        assertEquals(null, ready(vm).lesson)
        assertEquals(0, ready(vm).queued)
        assertFalse(ready(vm).readOnly)
    }

    @Test
    fun `continue reveals the next section only when unlocked and completes on the last`() {
        val vm = vm(fixture("lesson-rust-ownership.json"))
        // The introduction of this lesson has no required exercise: continue works.
        vm.continueLesson()
        assertEquals(2, ready(vm).saved.revealedCount)
        assertEquals(1, ready(vm).justRevealed)
        // The next section needs an answer first.
        assertFalse(ready(vm).isUnlocked())
        vm.continueLesson()
        assertEquals(2, ready(vm).saved.revealedCount)
        val required =
            ready(vm)
                .sections.sections[1]
                .requiredExercises
                .first()
        vm.updateExercise(required.id) { it.copy(attempts = 1, checked = true) }
        assertTrue(ready(vm).isUnlocked())
        vm.continueLesson()
        assertEquals(3, ready(vm).saved.revealedCount)
        assertTrue(vm.isLocked(required.id))
        assertFalse(ready(vm).saved.completed)
    }

    @Test
    fun `state is written on flush and restored by a new view model`() {
        val store = FakeStore()
        val first = vm(fixture("lesson-rust-ownership.json"), store)
        first.continueLesson()
        first.updateExercise("q1") { ExerciseReducers.selectSingle(it, "b") }
        first.termOpened("ownership")
        first.saveScroll("q1", 77)
        first.flushState()
        assertTrue(store.saves >= 1)
        val second = vm(fixture("lesson-rust-ownership.json"), store)
        val saved = ready(second).saved
        assertEquals(2, saved.revealedCount)
        assertEquals(listOf("ownership"), saved.openedTerms)
        assertEquals("q1", saved.scrollBlockId)
        assertEquals(77, saved.scrollOffsetPx)
        assertEquals(LessonSavedState.decode(store.stored).exercises.keys, saved.exercises.keys)
    }

    @Test
    fun `saved state of blocks that vanished is dropped`() {
        val store =
            FakeStore(
                LessonSavedState(
                    revealedCount = 99,
                    exercises =
                        mapOf(
                            "gone" to
                                com.teachme.sdui.logic
                                    .ExerciseState(attempts = 1),
                        ),
                ).encode(),
            )
        val saved = ready(vm(fixture("lesson-rust-ownership.json"), store)).saved
        assertTrue(saved.exercises.isEmpty())
        assertTrue(saved.revealedCount < 99)
    }

    @Test
    fun `unknown block fixture logs the event and still opens`() {
        val logger = FakeLogger()
        val vm = vm(fixture("unknown-type.json"), logger = logger)
        assertTrue(vm.state.value is LessonUiState.Ready)
        assertTrue(logger.events.any { it is SduiEvent.UnknownBlockType && it.type == "timeline" })
    }

    @Test
    fun `a stored state of a different document is discarded`() {
        val store = FakeStore()
        val first = vm(fixture("lesson-rust-ownership.json"), store)
        first.continueLesson()
        first.flushState()
        assertEquals(2, ready(vm(fixture("lesson-rust-ownership.json"), store)).saved.revealedCount)
        // Same key, same block ids, different content: nothing carries over.
        val changed = fixture("lesson-rust-ownership.json").replaceFirst("Ownership e move", "Ownership, move e copy")
        val other = ready(vm(changed, store)).saved
        assertEquals(1, other.revealedCount)
        assertTrue(other.exercises.isEmpty())
        assertFalse(other.completed)
    }

    @Test
    fun `oversized documents are refused`() {
        val huge = "{\"blocks\":[" + "{\"id\":\"a\",\"type\":\"paragraph\",\"text\":\"x\"},".repeat(20_000) + "{}]}"
        assertEquals(LessonUiState.Unusable, vm(huge).state.value)
    }

    @Test
    fun `an unreadable stored state gives a fresh lesson, not a crash`() {
        val vm = vm(fixture("lesson-rust-ownership.json"), FailingStore())
        assertEquals(1, ready(vm).saved.revealedCount)
    }
}
