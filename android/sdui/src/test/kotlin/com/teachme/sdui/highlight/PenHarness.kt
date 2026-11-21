package com.teachme.sdui.highlight

import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import android.view.View
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.model.Block
import com.teachme.sdui.model.GlossaryEntry
import com.teachme.sdui.model.LessonDocument
import com.teachme.sdui.render.DefaultBlockRegistry
import com.teachme.sdui.render.ExerciseStates
import com.teachme.sdui.render.LessonLinkTarget
import com.teachme.sdui.render.LocalSdui
import com.teachme.sdui.render.SduiEnvironment
import com.teachme.sdui.render.SduiHost

/** A [HighlightHost] that applies the pure operations to a list in Compose state and remembers every call. */
class FakeHighlightHost(
    initial: List<Highlight> = emptyList(),
    color: HighlightColor = HighlightColor.YELLOW,
) : HighlightHost {
    override var highlights: List<Highlight> by mutableStateOf(initial)
    override var color: HighlightColor by mutableStateOf(color)
    override var penWrites: Boolean by mutableStateOf(false)
    val added = mutableListOf<Triple<String, Int, Int>>()
    val erased = mutableListOf<Triple<String, Int, Int>>()
    val removed = mutableListOf<Pair<String, Int>>()

    override fun add(
        blockId: String,
        start: Int,
        end: Int,
    ) {
        added += Triple(blockId, start, end)
        highlights = Highlights.add(highlights, blockId, start, end, color)
    }

    override fun erase(
        blockId: String,
        start: Int,
        end: Int,
    ) {
        erased += Triple(blockId, start, end)
        highlights = Highlights.remove(highlights, blockId, start, end)
    }

    override fun remove(
        blockId: String,
        position: Int,
    ) {
        removed += blockId to position
        highlights = Highlights.removeAt(highlights, blockId, position)
    }
}

class FakeSduiHost : SduiHost {
    val openedTerms = mutableListOf<String>()

    override fun openTerm(key: String) {
        openedTerms += key
    }

    override fun openSource(key: String) = Unit

    override fun openUrl(url: String) = Unit

    override fun askAgent(promptHint: String) = Unit

    override fun openLesson(lessonId: String) = Unit

    override fun lessonLinkTarget(lessonId: String): LessonLinkTarget? = null

    override fun copyText(text: String) = Unit
}

fun testDocument(
    blocks: List<Block>,
    glossary: List<GlossaryEntry> = emptyList(),
) = LessonDocument(
    catalogVersion = 1,
    title = "T",
    objective = "O",
    estimatedMinutes = 5,
    glossary = glossary,
    sources = emptyList(),
    blocks = blocks,
)

/** Stylus, eraser and finger events dispatched to the real window, like the hardware would. */
class PenInjector(
    private val rule: AndroidComposeTestRule<*, ComponentActivity>,
) {
    private var downTime = 0L
    private var clock = 0L

    private val decor: View get() = rule.activity.window.decorView

    private fun event(
        action: Int,
        at: Offset,
        tool: Int,
        buttons: Int,
        time: Long,
    ): MotionEvent {
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
            when (tool) {
                MotionEvent.TOOL_TYPE_FINGER -> InputDevice.SOURCE_TOUCHSCREEN
                MotionEvent.TOOL_TYPE_MOUSE -> InputDevice.SOURCE_MOUSE
                else -> InputDevice.SOURCE_STYLUS
            }
        return MotionEvent.obtain(downTime, time, action, 1, props, coords, 0, buttons, 1f, 1f, 0, 0, source, 0)
    }

    private fun send(
        action: Int,
        at: Offset,
        tool: Int,
        buttons: Int,
        advance: Long,
    ) {
        clock += advance
        if (action == MotionEvent.ACTION_DOWN) downTime = clock
        val e = event(action, at, tool, buttons, clock)
        rule.runOnUiThread {
            if (action ==
                MotionEvent.ACTION_HOVER_MOVE
            ) {
                decor.dispatchGenericMotionEvent(e)
            } else {
                decor.dispatchTouchEvent(e)
            }
        }
        e.recycle()
        rule.waitForIdle()
    }

    /** A gesture: down at the first point, a move per further point (every [stepMs]), then up (or cancel). */
    fun gesture(
        points: List<Offset>,
        tool: Int = MotionEvent.TOOL_TYPE_STYLUS,
        buttons: Int = 0,
        stepMs: Long = 16,
        holdMs: Long = 0,
        cancel: Boolean = false,
    ) {
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        send(MotionEvent.ACTION_DOWN, points.first(), tool, buttons, 0)
        if (holdMs > 0) {
            rule.mainClock.advanceTimeBy(holdMs)
            clock += holdMs
        }
        for (p in points.drop(1)) send(MotionEvent.ACTION_MOVE, p, tool, buttons, stepMs)
        send(if (cancel) MotionEvent.ACTION_CANCEL else MotionEvent.ACTION_UP, points.last(), tool, buttons, stepMs)
    }

    fun hover(at: Offset) {
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        send(MotionEvent.ACTION_HOVER_MOVE, at, MotionEvent.TOOL_TYPE_STYLUS, 0, 0)
    }
}

/** The laid out text of the node showing [text], and where it is in the window. */
class TextOnScreen(
    val layout: TextLayoutResult,
    val origin: Offset,
) {
    /** Window position of the middle of the displayed char [index]. */
    fun centerOf(index: Int): Offset {
        val box = layout.getBoundingBox(index)
        return origin + box.center
    }

    /** Window position of the middle of the word [word] (first occurrence of [word] in the displayed text). */
    fun centerOfWord(word: String): Offset {
        val at =
            layout.layoutInput.text.text
                .indexOf(word)
        check(at >= 0) { "'$word' not on screen" }
        val a = layout.getBoundingBox(at)
        val b = layout.getBoundingBox(at + word.length - 1)
        return origin + Offset((a.left + b.right) / 2, a.center.y)
    }
}

fun SemanticsNodeInteraction.textOnScreen(): TextOnScreen {
    val node = fetchSemanticsNode()
    val results = mutableListOf<TextLayoutResult>()
    node.config
        .getOrNull(SemanticsActions.GetTextLayoutResult)
        ?.action
        ?.invoke(results)
    return TextOnScreen(results.first(), node.boundsInWindow.topLeft)
}

/** A column of blocks like the lesson screen, in a scroll container, with the highlighter [host] provided. */
fun AndroidComposeTestRule<*, ComponentActivity>.setLesson(
    document: LessonDocument,
    host: HighlightHost,
    sdui: SduiHost = FakeSduiHost(),
    scroll: androidx.compose.foundation.ScrollState? = null,
    dark: Boolean = false,
) {
    val states =
        object : ExerciseStates {
            override fun get(blockId: String) = ExerciseState()

            override fun update(
                blockId: String,
                transform: (ExerciseState) -> ExerciseState,
            ) = Unit

            override fun isLocked(blockId: String) = false
        }
    val env = SduiEnvironment(document, sdui, states)
    setContent {
        val state = scroll ?: rememberScrollState()
        TeachTheme(darkTheme = dark) {
            CompositionLocalProvider(LocalSdui provides env, LocalHighlightHost provides host) {
                Column(
                    Modifier
                        .width(320.dp)
                        .height(480.dp)
                        .background(TeachTheme.colors.bg)
                        .verticalScroll(state),
                ) {
                    document.blocks.forEach { block ->
                        Box { DefaultBlockRegistry.Render(block) }
                    }
                    Spacer(Modifier.height(1200.dp))
                }
            }
        }
    }
}

fun penRule() = createAndroidComposeRule<ComponentActivity>()
