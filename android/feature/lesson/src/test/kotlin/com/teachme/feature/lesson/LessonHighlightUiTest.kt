package com.teachme.feature.lesson

import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.text.TextLayoutResult
import androidx.lifecycle.SavedStateHandle
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.model.ApiResult
import com.teachme.core.model.HighlightRecord
import com.teachme.core.model.InMemoryTopicTabInbox
import com.teachme.core.model.LessonView
import com.teachme.core.model.ManualUserScopes
import com.teachme.sdui.highlight.Highlight
import com.teachme.sdui.highlight.HighlightColor
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

private const val SENTENCE = "Quando sposti la variabile, la prima non è più utilizzabile."
private const val QUESTION = "Quale parola resta valida dopo lo spostamento"
private const val DOC =
    """{"catalog_version":1,"title":"Ownership","objective":"Capire il move.","estimated_minutes":5,
        "glossary":[],"sources":[],"blocks":[
        {"id":"h1","type":"heading","level":1,"text":"Ownership"},
        {"id":"p1","type":"paragraph","text":"$SENTENCE"},
        {"id":"q1","type":"quiz_single","question":"$QUESTION","options":[{"id":"a","text":"La prima","feedback":"f"},{"id":"b","text":"La seconda","feedback":"f"}],"correct":"b"}]}"""

/** The whole lesson screen with a real view model: pen events in, highlights, palette, undo and indicator out. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h640dp-xxhdpi")
class LessonHighlightUiTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val backend = FakeLessons()
    private lateinit var vm: LessonViewModel
    private var clock = 0L

    private fun open(vararg server: HighlightRecord) {
        backend.view = ApiResult.Success(LessonView(lesson(), DOC, emptyList(), null, server.toList()))
        val queue = MemoryQueue()
        vm =
            LessonViewModel(
                SavedStateHandle(mapOf("key" to "L1")),
                FakeSource(DOC),
                FakeStore(),
                FakeLogger(),
                ImmediateDispatchers(),
                backend,
                AttemptSyncer(backend, queue, ManualUserScopes()),
                HighlightSyncer(backend, MemoryHighlightStore(), ManualUserScopes()),
                StrokeSyncer(backend, MemoryStrokeStore(), ManualUserScopes()),
                queue,
                InMemoryTopicTabInbox(),
            )
        rule.setContent {
            TeachTheme { LessonRoute(onClose = {}, onAskAgent = null, viewModel = vm, audioViewModel = audioOff()) }
        }
        rule.waitUntil(5_000) { vm.state.value is LessonUiState.Ready }
        rule.waitForIdle()
    }

    private val highlights get() = (vm.state.value as LessonUiState.Ready).highlights

    private fun layoutOf(
        text: String,
        substring: Boolean = false,
    ): Pair<TextLayoutResult, Offset> {
        val node = rule.onNodeWithText(text, substring = substring).fetchSemanticsNode()
        val out = mutableListOf<TextLayoutResult>()
        node.config
            .getOrNull(SemanticsActions.GetTextLayoutResult)
            ?.action
            ?.invoke(out)
        return out.first() to node.boundsInWindow.topLeft
    }

    private fun centerOf(
        text: String,
        word: String,
        find: String = if (text == SENTENCE) "Quando" else text,
        substring: Boolean = text == SENTENCE,
    ): Offset {
        val (layout, origin) = layoutOf(find, substring)
        val at = text.indexOf(word)
        val a = layout.getBoundingBox(at)
        val b = layout.getBoundingBox(at + word.length - 1)
        return origin + Offset((a.left + b.right) / 2, a.center.y)
    }

    private fun send(
        action: Int,
        at: Offset,
        tool: Int,
        downTime: Long,
        time: Long,
    ) {
        val props =
            arrayOf(
                MotionEvent.PointerProperties().also {
                    it.id = 0
                    it.toolType = tool
                },
            )
        val coords =
            arrayOf(
                MotionEvent.PointerCoords().also {
                    it.x = at.x
                    it.y = at.y
                    it.pressure = 1f
                },
            )
        val source =
            if (tool ==
                MotionEvent.TOOL_TYPE_FINGER
            ) {
                InputDevice.SOURCE_TOUCHSCREEN
            } else {
                InputDevice.SOURCE_STYLUS
            }
        val e = MotionEvent.obtain(downTime, time, action, 1, props, coords, 0, 0, 1f, 1f, 0, 0, source, 0)
        rule.runOnUiThread {
            if (action == MotionEvent.ACTION_HOVER_MOVE) {
                rule.activity.window.decorView
                    .dispatchGenericMotionEvent(e)
            } else {
                rule.activity.window.decorView
                    .dispatchTouchEvent(e)
            }
        }
        e.recycle()
        rule.waitForIdle()
    }

    private fun gesture(
        points: List<Offset>,
        tool: Int = MotionEvent.TOOL_TYPE_STYLUS,
        stepMs: Long = 16,
    ) {
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        val down = clock
        send(MotionEvent.ACTION_DOWN, points.first(), tool, down, clock)
        for (p in points.drop(1)) {
            clock += stepMs
            send(MotionEvent.ACTION_MOVE, p, tool, down, clock)
        }
        clock += stepMs
        send(MotionEvent.ACTION_UP, points.last(), tool, down, clock)
    }

    private fun hover(at: Offset) {
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        send(MotionEvent.ACTION_HOVER_MOVE, at, MotionEvent.TOOL_TYPE_STYLUS, clock, clock)
    }

    private val palette get() = rule.onNodeWithContentDescription("Strumento")

    @Test
    fun `no palette before a pen event, then it appears and hides after six seconds`() {
        open()
        palette.assertDoesNotExist()
        // A finger on the same place is not a pen: still nothing (and the finger is not a highlight).
        gesture(listOf(centerOf(SENTENCE, "sposti")), tool = MotionEvent.TOOL_TYPE_FINGER, stepMs = 40)
        palette.assertDoesNotExist()
        // The finger does what it did before: no highlight, and a long drag over the words is not one either.
        gesture(
            listOf(centerOf(SENTENCE, "sposti"), centerOf(SENTENCE, "variabile,"), centerOf(SENTENCE, "variabile,")),
            tool = MotionEvent.TOOL_TYPE_FINGER,
        )
        assertTrue(highlights.isEmpty())
        palette.assertDoesNotExist()

        hover(centerOf(SENTENCE, "sposti"))
        rule.mainClock.advanceTimeBy(200)
        palette.assertIsDisplayed()
        rule.onNodeWithContentDescription("Colore dell'evidenziatore").assertIsDisplayed()

        rule.mainClock.advanceTimeBy(5_000)
        palette.assertIsDisplayed()
        rule.mainClock.advanceTimeBy(1_500)
        palette.assertDoesNotExist()
    }

    @Test
    fun `activity halfway through the wait restarts the six seconds`() {
        open()
        hover(centerOf(SENTENCE, "sposti"))
        rule.mainClock.advanceTimeBy(200)
        palette.assertIsDisplayed()
        // Activity at about 5 s: a single timeout around the whole loop would hide the palette at 6 s.
        rule.mainClock.advanceTimeBy(4_800)
        hover(centerOf(SENTENCE, "variabile,"))
        rule.mainClock.advanceTimeBy(5_000)
        palette.assertIsDisplayed()
        // Six seconds after the last activity (at 5 s), not after the first.
        rule.mainClock.advanceTimeBy(1_500)
        palette.assertDoesNotExist()
    }

    @Test
    fun `a color tapped in the palette is used by the next stroke`() {
        open()
        hover(centerOf(SENTENCE, "sposti"))
        rule.mainClock.advanceTimeBy(200)
        // The window is narrow: the colours are in the popover of the colour button.
        rule.onNodeWithContentDescription("Colore dell'evidenziatore").performClick()
        rule.onNodeWithContentDescription("Blu").performClick()
        rule.waitForIdle()
        gesture(listOf(centerOf(SENTENCE, "Quando"), centerOf(SENTENCE, "sposti")))
        val end = SENTENCE.indexOf("sposti") + "sposti".length
        assertEquals(listOf(Highlight("p1", 0, end, HighlightColor.BLUE)), highlights)
    }

    @Test
    fun `a pen stroke highlights the words, none on the quiz, and a tap removes it with undo`() {
        open()
        gesture(listOf(centerOf(QUESTION, "Quale"), centerOf(QUESTION, "parola")))
        assertTrue("the quiz is not highlightable", highlights.isEmpty())

        gesture(listOf(centerOf(SENTENCE, "sposti"), centerOf(SENTENCE, "variabile,")))
        val start = SENTENCE.indexOf("sposti")
        val end = SENTENCE.indexOf("variabile,") + "variabile,".length
        assertEquals(listOf(Highlight("p1", start, end, HighlightColor.YELLOW)), highlights)

        // Tap on the highlighted words (with the pen: the palette is up too, which is fine).
        gesture(listOf(centerOf(SENTENCE, "variabile,")), stepMs = 40)
        rule.onNodeWithText("Rimuovi").performClick()
        rule.waitForIdle()
        assertTrue(highlights.isEmpty())
        rule.onNodeWithText("Evidenziazione rimossa").assertIsDisplayed()

        rule.onNodeWithText("Annulla").performClick()
        rule.waitForIdle()
        assertEquals(listOf(Highlight("p1", start, end, HighlightColor.YELLOW)), highlights)
    }

    @Test
    fun `the eraser takes out only the words it passes over of a highlight of the server`() {
        val start = SENTENCE.indexOf("sposti")
        val end = SENTENCE.indexOf("variabile,") + "variabile,".length
        open(HighlightRecord("p1", start, end, "pink"))
        assertEquals(listOf(Highlight("p1", start, end, HighlightColor.PINK)), highlights)
        val la = centerOf(SENTENCE, "la")
        gesture(listOf(la, la + Offset(0f, 15f), la + Offset(0f, 35f)), tool = MotionEvent.TOOL_TYPE_ERASER)
        val before = SENTENCE.indexOf(" la ")
        assertEquals(
            listOf(
                Highlight("p1", start, before, HighlightColor.PINK),
                Highlight("p1", before + " la ".length, end, HighlightColor.PINK),
            ),
            highlights,
        )
    }

    @Test
    fun `with the eraser switch on the highlighter tip erases words and the finger does not`() {
        val start = SENTENCE.indexOf("sposti")
        val end = SENTENCE.indexOf("variabile,") + "variabile,".length
        open(HighlightRecord("p1", start, end, "pink"))
        vm.setEraserMode(true)
        rule.waitForIdle()
        val la = centerOf(SENTENCE, "la")
        gesture(listOf(la, la + Offset(0f, 15f), la + Offset(0f, 35f)), tool = MotionEvent.TOOL_TYPE_FINGER)
        assertEquals(listOf(Highlight("p1", start, end, HighlightColor.PINK)), highlights)
        gesture(listOf(la, la + Offset(0f, 15f), la + Offset(0f, 35f)))
        val before = SENTENCE.indexOf(" la ")
        assertEquals(
            listOf(
                Highlight("p1", start, before, HighlightColor.PINK),
                Highlight("p1", before + " la ".length, end, HighlightColor.PINK),
            ),
            highlights,
        )
        // No new highlight is written with the switch on (the pass over "Quando" would have started one at 0).
        gesture(listOf(centerOf(SENTENCE, "Quando"), centerOf(SENTENCE, "sposti")))
        assertTrue(highlights.none { it.start == 0 })
    }

    @Test
    fun `highlights of the server are drawn and a finger tap on them offers the removal`() {
        val start = SENTENCE.indexOf("prima")
        open(HighlightRecord("p1", start, start + "prima".length, "green"))
        assertEquals(1, highlights.size)
        palette.assertDoesNotExist()
        // The block tells TalkBack about the highlight, with its colour.
        val announced = "evidenziato in verde: prima, fine evidenziato."
        rule.onNodeWithText(announced, substring = true).assertExists()
        // Without a pen anywhere: the tap of a finger is enough.
        val at = centerOf(SENTENCE, "prima")
        gesture(listOf(at), tool = MotionEvent.TOOL_TYPE_FINGER, stepMs = 40)
        rule.onNodeWithText("Rimuovi").performClick()
        rule.waitForIdle()
        assertTrue(highlights.isEmpty())
        rule.onNodeWithText("Evidenziazione rimossa").assertIsDisplayed()
    }

    @Test
    fun `the indicator speaks of highlights that are not saved, offers a retry, and says when they are`() {
        var retries = 0
        val failing = androidx.compose.runtime.mutableStateOf(true)
        rule.setContent {
            TeachTheme { QueueIndicator(queued = 0, onRetryNow = { retries++ }, highlightsFailing = failing.value) }
        }
        rule.onNodeWithContentDescription("Evidenziazioni da salvare").performClick()
        rule
            .onNodeWithText(
                "Alcune evidenziazioni non sono ancora salvate. Le invio appena torna la connessione.",
            ).assertIsDisplayed()
        rule.onNodeWithText("Riprova ora").performClick()
        assertEquals(1, retries)
        failing.value = false
        rule.waitForIdle()
        rule.onNodeWithContentDescription("Evidenziazioni da salvare").assertDoesNotExist()
        rule.onNodeWithContentDescription("Evidenziazioni salvate.").assertExists()
    }

    /** The feature is off: no audio request beyond the switch, no control. */
    private fun audioOff() =
        LessonAudioViewModel(
            androidx.lifecycle.SavedStateHandle(mapOf("key" to "L1")),
            object : com.teachme.core.model.AudioRepository {
                override suspend fun getManifest(lessonId: String) =
                    com.teachme.core.model.ApiResult
                        .Success(
                            com.teachme.core.model
                                .AudioManifest(null, emptyList()),
                        )

                override suspend fun getVoices() =
                    com.teachme.core.model.ApiResult
                        .Success(
                            com.teachme.core.model
                                .TtsVoices(false, null, emptyList()),
                        )
            },
            { error("no player expected") },
            AudioTimings(),
            object : com.teachme.core.model.AudioPreferences {
                override val speed = kotlinx.coroutines.flow.flowOf(com.teachme.core.model.PlaybackSpeed.Default)

                override suspend fun setSpeed(speed: com.teachme.core.model.PlaybackSpeed) = Unit
            },
        )
}
