package com.teachme.feature.lesson

import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import androidx.lifecycle.SavedStateHandle
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.model.ApiResult
import com.teachme.core.model.InMemoryTopicTabInbox
import com.teachme.core.model.LessonView
import com.teachme.core.model.ManualUserScopes
import com.teachme.sdui.ink.BlockExtent
import com.teachme.sdui.ink.BlockGeometry
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkEdit
import com.teachme.sdui.ink.InkLimit
import com.teachme.sdui.ink.Stroke
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

private const val SENTENCE = "Quando sposti la variabile, la prima non è più utilizzabile."
private const val DOC =
    """{"catalog_version":1,"title":"Ownership","objective":"Capire il move.","estimated_minutes":5,
        "glossary":[],"sources":[],"blocks":[
        {"id":"h1","type":"heading","level":1,"text":"Ownership"},
        {"id":"p1","type":"paragraph","text":"$SENTENCE"},
        {"id":"q1","type":"quiz_single","question":"Quale parola resta valida dopo lo spostamento","options":[{"id":"a","text":"La prima","feedback":"f"},{"id":"b","text":"La seconda","feedback":"f"}],"correct":"b"}]}"""
private const val SHORT_DOC =
    """{"catalog_version":1,"title":"Breve","objective":"Capire.","estimated_minutes":5,
        "glossary":[],"sources":[],"blocks":[
        {"id":"h1","type":"heading","level":1,"text":"Breve"},
        {"id":"p1","type":"paragraph","text":"$SENTENCE"}]}"""
private val LONG_TEXT =
    "Questo paragrafo è abbastanza lungo da occupare qualche riga sullo schermo del telefono. "
        .repeat(
            3,
        )
private val LONG_DOC =
    """{"catalog_version":1,"title":"Lunga","objective":"Scorrere.","estimated_minutes":5,
        "glossary":[],"sources":[],"blocks":[{"id":"h1","type":"heading","level":1,"text":"Lunga"},""" +
        (1..14).joinToString(",") { """{"id":"p$it","type":"paragraph","text":"$it $LONG_TEXT"}""" } +
        "]}"

/** The whole lesson screen with a real view model: pen, finger, mouse and eraser events in, ink out. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h640dp-xxhdpi")
class LessonInkUiTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val backend = FakeLessons()
    private lateinit var vm: LessonViewModel
    private var clock = 0L

    private fun open(
        doc: String = DOC,
        tool: PenTool = PenTool.PEN,
    ) {
        backend.view = ApiResult.Success(LessonView(lesson(), doc, emptyList(), null, emptyList()))
        val queue = MemoryQueue()
        vm =
            LessonViewModel(
                SavedStateHandle(mapOf("key" to "L1")),
                FakeSource(doc),
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
        vm.setPenTool(tool)
        rule.waitForIdle()
    }

    private val ready get() = vm.state.value as LessonUiState.Ready
    private val strokes get() = ready.ink.set.strokes

    private fun bounds(
        text: String,
        substring: Boolean = false,
    ) = rule.onNodeWithText(text, substring = substring).fetchSemanticsNode().boundsInWindow

    private fun send(
        action: Int,
        at: Offset,
        tool: Int,
        downTime: Long,
        time: Long,
        buttons: Int = 0,
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
            when (tool) {
                MotionEvent.TOOL_TYPE_FINGER -> InputDevice.SOURCE_TOUCHSCREEN
                MotionEvent.TOOL_TYPE_MOUSE -> InputDevice.SOURCE_MOUSE
                else -> InputDevice.SOURCE_STYLUS
            }
        val e = MotionEvent.obtain(downTime, time, action, 1, props, coords, 0, buttons, 1f, 1f, 0, 0, source, 0)
        rule.runOnUiThread {
            rule.activity.window.decorView
                .dispatchTouchEvent(e)
        }
        e.recycle()
        rule.waitForIdle()
    }

    private fun gesture(
        points: List<Offset>,
        tool: Int = MotionEvent.TOOL_TYPE_STYLUS,
        buttons: Int = 0,
        end: Int = MotionEvent.ACTION_UP,
    ) {
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        val down = clock
        send(MotionEvent.ACTION_DOWN, points.first(), tool, down, clock, buttons)
        for (p in points.drop(1)) {
            clock += 16
            send(MotionEvent.ACTION_MOVE, p, tool, down, clock, buttons)
        }
        clock += 16
        send(end, points.last(), tool, down, clock, buttons)
    }

    /** A wavy line across the paragraph, [dx] to [dx] + 240 px from its left edge, starting [dy] below its top. */
    private fun scribble(
        dy: Float = 20f,
        dx: Float = 40f,
    ): List<Offset> {
        val b = bounds(SENTENCE, substring = true)
        return (0..12).map { Offset(b.left + dx + it * 20f, b.top + dy + (if (it % 2 == 0) 0f else 12f)) }
    }

    @Test
    fun `the tip writes a stroke on the block it started on, in block coordinates`() {
        open()
        val b = bounds(SENTENCE, substring = true)
        val line = scribble()
        gesture(line)
        assertEquals(1, strokes.size)
        val s = strokes.single()
        assertEquals("p1", s.blockId)
        assertEquals(InkColor.INK, s.color)
        assertTrue(s.id.length == 36)
        assertTrue("simplified and quantized: ${s.points.size / 2} points", s.pointCount in 2..13)
        // x and y are the offsets from the block corner over the column width, in ten-thousandths.
        val width = b.width
        assertEquals(40f / width * 10_000, s.points[0].toFloat(), 3f)
        assertEquals(20f / width * 10_000, s.points[1].toFloat(), 120f)
        assertTrue("the last x is the end of the line", s.points[s.points.size - 2] > s.points[0])
    }

    @Test
    fun `a tap with the tip is a one point stroke and the finger and the mouse never write`() {
        open()
        val b = bounds(SENTENCE, substring = true)
        val at = Offset(b.left + 60f, b.top + 20f)
        gesture(listOf(at), tool = MotionEvent.TOOL_TYPE_FINGER)
        gesture(scribble(), tool = MotionEvent.TOOL_TYPE_FINGER)
        gesture(scribble(), tool = MotionEvent.TOOL_TYPE_MOUSE)
        assertTrue(strokes.isEmpty())
        gesture(listOf(at))
        assertEquals(1, strokes.single().pointCount)
    }

    @Test
    fun `the highlighter tool leaves the pen to the highlights and writes nothing`() {
        open(tool = PenTool.HIGHLIGHTER)
        gesture(scribble())
        assertTrue(strokes.isEmpty())
    }

    @Test
    fun `the tip does not answer a quiz with the pen, the finger does`() {
        open()
        val option = bounds("La prima")
        gesture(listOf(option.center), tool = MotionEvent.TOOL_TYPE_STYLUS)
        assertTrue("the pen wrote a dot, not an answer", ready.exercise("q1").selection.isEmpty())
        assertEquals(1, strokes.size)
        gesture(listOf(option.center), tool = MotionEvent.TOOL_TYPE_FINGER)
        assertEquals(listOf("a"), ready.exercise("q1").selection)
    }

    @Test
    fun `the highlighter tool still answers with the tip`() {
        open(tool = PenTool.HIGHLIGHTER)
        gesture(listOf(bounds("La prima").center))
        assertEquals(listOf("a"), ready.exercise("q1").selection)
    }

    @Test
    fun `the tip never scrolls and the finger does`() {
        open(LONG_DOC)
        val before = bounds("1 Questo", substring = true).top
        gesture(listOf(Offset(300f, 1500f), Offset(300f, 1200f), Offset(300f, 900f), Offset(300f, 600f)))
        assertEquals("the pen wrote, the page stayed", before, bounds("1 Questo", substring = true).top, 0.5f)
        assertEquals(1, strokes.size)
        gesture(
            listOf(Offset(300f, 1500f), Offset(300f, 1450f), Offset(300f, 1400f), Offset(300f, 1350f)),
            tool = MotionEvent.TOOL_TYPE_FINGER,
        )
        assertTrue("the finger scrolled", bounds("1 Questo", substring = true).top < before - 20f)
        assertEquals(1, strokes.size)
    }

    @Test
    fun `a cancelled contact writes nothing`() {
        open()
        gesture(scribble(), end = MotionEvent.ACTION_CANCEL)
        assertTrue(strokes.isEmpty())
    }

    @Test
    fun `the eraser and the pen button take a whole stroke and a cancel changes nothing`() {
        open()
        gesture(scribble(dy = 20f))
        gesture(scribble(dy = 120f))
        assertEquals(2, strokes.size)
        val first = strokes[0].id
        val second = strokes[1].id
        // Cancelled: nothing changes.
        gesture(scribble(dy = 20f), tool = MotionEvent.TOOL_TYPE_ERASER, end = MotionEvent.ACTION_CANCEL)
        assertEquals(2, strokes.size)
        gesture(scribble(dy = 20f), tool = MotionEvent.TOOL_TYPE_ERASER)
        assertEquals(listOf(second), strokes.map { it.id })
        gesture(scribble(dy = 120f), buttons = MotionEvent.BUTTON_STYLUS_PRIMARY)
        assertTrue(strokes.isEmpty())
        assertNotEquals(first, second)
        // Undo (the palette is stage 4: the view model has it) puts the last pass back.
        vm.undoInk()
        assertEquals(listOf(second), strokes.map { it.id })
    }

    @Test
    fun `a stroke is drawn in its ink colour above the text`() {
        open()
        vm.setInkColor(InkColor.RED)
        rule.waitForIdle()
        val b = bounds(SENTENCE, substring = true)
        val y = b.top + 20f
        gesture(listOf(Offset(b.left + 40f, y), Offset(b.left + 300f, y)))
        rule.waitForIdle()
        val bitmap = rule.onRoot().captureToImage().asAndroidBitmap()
        val red = 0xFFB02E2E.toInt()
        assertEquals(red, bitmap.getPixel((b.left + 170f).toInt(), y.toInt()))
        // Nothing before the start of the line.
        assertNotEquals(red, bitmap.getPixel((b.left + 10f).toInt(), y.toInt()))
    }

    @Test
    fun `the eraser shows the touched stroke at the ghost opacity while the tip is down`() {
        open()
        vm.setInkColor(InkColor.RED)
        val b = bounds(SENTENCE, substring = true)
        val y = b.top + 20f
        gesture(listOf(Offset(b.left + 40f, y), Offset(b.left + 300f, y)))
        val px = (b.left + 170f).toInt()
        val full =
            rule
                .onRoot()
                .captureToImage()
                .asAndroidBitmap()
                .getPixel(px, y.toInt())
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        send(MotionEvent.ACTION_DOWN, Offset(b.left + 100f, y), MotionEvent.TOOL_TYPE_ERASER, clock, clock)
        send(MotionEvent.ACTION_MOVE, Offset(b.left + 110f, y), MotionEvent.TOOL_TYPE_ERASER, clock, clock + 16)
        val ghost =
            rule
                .onRoot()
                .captureToImage()
                .asAndroidBitmap()
                .getPixel(px, y.toInt())
        assertEquals(1, strokes.size)
        assertNotEquals("the stroke is paler under the eraser", full, ghost)
        send(MotionEvent.ACTION_UP, Offset(b.left + 110f, y), MotionEvent.TOOL_TYPE_ERASER, clock, clock + 32)
        assertTrue(strokes.isEmpty())
    }

    @Test
    fun `with the eraser switch on the tip erases whole strokes in one undo step and writes nothing`() {
        open()
        gesture(scribble(dy = 20f))
        gesture(scribble(dy = 120f))
        assertEquals(2, strokes.size)
        val first = strokes[0].id
        val second = strokes[1].id
        vm.setEraserMode(true)
        rule.waitForIdle()
        // A cancelled pass changes nothing; a pass over empty glass writes nothing.
        gesture(scribble(dy = 20f), end = MotionEvent.ACTION_CANCEL)
        assertEquals(2, strokes.size)
        gesture(scribble(dy = 300f))
        assertEquals(2, strokes.size)
        // One pass over both strokes is one operation.
        val b = bounds(SENTENCE, substring = true)
        gesture(listOf(Offset(b.left + 100f, b.top + 10f), Offset(b.left + 100f, b.top + 140f)))
        assertTrue(strokes.isEmpty())
        assertTrue("the switch stays on", ready.eraserMode)
        vm.undoInk()
        assertEquals(listOf(first, second), strokes.map { it.id })
    }

    @Test
    fun `with the eraser switch on the tip does not answer a quiz and the finger still does`() {
        open()
        vm.setEraserMode(true)
        rule.waitForIdle()
        val option = bounds("La prima")
        gesture(listOf(option.center))
        assertTrue(ready.exercise("q1").selection.isEmpty())
        assertTrue(strokes.isEmpty())
        gesture(listOf(option.center), tool = MotionEvent.TOOL_TYPE_FINGER)
        assertEquals(listOf("a"), ready.exercise("q1").selection)
    }

    @Test
    fun `with the eraser switch on the finger still scrolls and never erases`() {
        open(LONG_DOC)
        val line = bounds("1 Questo", substring = true)
        gesture(listOf(Offset(line.left + 40f, line.top + 20f), Offset(line.left + 300f, line.top + 20f)))
        assertEquals(1, strokes.size)
        vm.setEraserMode(true)
        rule.waitForIdle()
        val before = bounds("1 Questo", substring = true).top
        gesture(
            listOf(Offset(300f, 1500f), Offset(300f, 1450f), Offset(300f, 1400f), Offset(300f, 1350f)),
            tool = MotionEvent.TOOL_TYPE_FINGER,
        )
        assertTrue("the finger scrolled", bounds("1 Questo", substring = true).top < before - 20f)
        assertEquals(1, strokes.size)
    }

    private fun hover(
        at: Offset,
        action: Int = MotionEvent.ACTION_HOVER_MOVE,
        buttons: Int = 0,
    ) {
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        val props = arrayOf(MotionEvent.PointerProperties().also { it.toolType = MotionEvent.TOOL_TYPE_STYLUS })
        val coords =
            arrayOf(
                MotionEvent.PointerCoords().also {
                    it.x = at.x
                    it.y = at.y
                },
            )
        val e =
            MotionEvent.obtain(
                clock,
                clock,
                action,
                1,
                props,
                coords,
                0,
                buttons,
                1f,
                1f,
                0,
                0,
                InputDevice.SOURCE_STYLUS,
                0,
            )
        rule.runOnUiThread {
            rule.activity.window.decorView
                .dispatchGenericMotionEvent(e)
        }
        e.recycle()
        rule.waitForIdle()
    }

    @Test
    fun `the cursor ring follows the tip in hover and contact only with the switch on and no button`() {
        val surface = bare().surface
        hover(Offset(100f, 100f))
        assertEquals("no ring with the switch off", null, surface.ringAt)
        surface.eraserMode = true
        hover(Offset(120f, 130f))
        assertEquals(Offset(120f, 130f), surface.ringAt)
        val down = nextDownTime()
        send(MotionEvent.ACTION_DOWN, Offset(150f, 160f), MotionEvent.TOOL_TYPE_STYLUS, down, down)
        send(MotionEvent.ACTION_MOVE, Offset(160f, 170f), MotionEvent.TOOL_TYPE_STYLUS, down, down + 16)
        assertEquals(Offset(160f, 170f), surface.ringAt)
        send(MotionEvent.ACTION_UP, Offset(160f, 170f), MotionEvent.TOOL_TYPE_STYLUS, down, down + 32)
        hover(Offset(100f, 100f), buttons = MotionEvent.BUTTON_STYLUS_PRIMARY)
        assertEquals("no ring for the side button", null, surface.ringAt)
        hover(Offset(100f, 100f))
        hover(Offset(100f, 100f), action = MotionEvent.ACTION_HOVER_EXIT)
        assertEquals(null, surface.ringAt)
    }

    @Test
    fun `a pen without hover loses the ring when the tip lifts`() {
        val surface = bare().surface
        surface.eraserMode = true
        val down = nextDownTime()
        send(MotionEvent.ACTION_DOWN, Offset(150f, 160f), MotionEvent.TOOL_TYPE_STYLUS, down, down)
        assertEquals(Offset(150f, 160f), surface.ringAt)
        send(MotionEvent.ACTION_UP, Offset(150f, 160f), MotionEvent.TOOL_TYPE_STYLUS, down, down + 16)
        assertEquals("no hover follows the lift", null, surface.ringAt)
    }

    @Test
    fun `turning the switch off drops the ring so an old one does not come back`() {
        val surface = bare().surface
        surface.eraserMode = true
        hover(Offset(120f, 130f))
        assertEquals(Offset(120f, 130f), surface.ringAt)
        surface.eraserMode = false
        assertEquals(null, surface.ringAt)
        surface.eraserMode = true
        assertEquals("nothing until the tip moves again", null, surface.ringAt)
    }

    @Test
    fun `the ring is not shown where the tip does not erase`() {
        val surface = bare().surface
        surface.layout = {
            InkLayout(
                mapOf("b" to BlockGeometry(0f, 0f, 1000f)),
                listOf(BlockExtent("b", 0f, 1800f)),
                continueTop = 500f,
                continueBottom = 600f,
            )
        }
        surface.eraserMode = true
        hover(Offset(120f, 550f))
        assertEquals("the Continua row keeps the pen's normal job", null, surface.ringAt)
        hover(Offset(120f, 300f))
        assertEquals(Offset(120f, 300f), surface.ringAt)
        hover(Offset(120f, 560f))
        assertEquals(null, surface.ringAt)
    }

    @Test
    fun `the tip taps the continue bar and does not write there`() {
        open(SHORT_DOC)
        val bar = bounds("Completa lezione", substring = true)
        gesture(listOf(bar.center))
        assertTrue(strokes.isEmpty())
        val after = vm.state.value
        assertTrue("the tap reached the bar", after !is LessonUiState.Ready || after.completing || after.showSummary)
    }

    // --- the contact loop alone, on a bare surface: one block, a button under it ---

    private class Bare(
        val surface: InkSurface,
    ) {
        val written = mutableListOf<Stroke>()
        var limits = 0
        var clicks = 0
        var current by mutableStateOf(surface)
    }

    private fun bare(edit: (Stroke) -> InkEdit? = { null }): Bare {
        lateinit var holder: Bare
        val surface =
            InkSurface(
                write = {
                    holder.written += it
                    edit(it) ?: InkEdit.NoOp
                },
                erase = { InkEdit.NoOp },
            )
        holder = Bare(surface)
        surface.penWrites = true
        surface.layout = {
            InkLayout(mapOf("b" to BlockGeometry(0f, 0f, 1000f)), listOf(BlockExtent("b", 0f, 1800f)))
        }
        surface.onLimit = { holder.limits++ }
        rule.setContent {
            Box(Modifier.fillMaxSize().inkInput(holder.current)) {
                Box(
                    Modifier
                        .align(Alignment.BottomEnd)
                        .size(80.dp)
                        .testTag("target")
                        .clickable { holder.clicks++ },
                )
            }
        }
        rule.waitForIdle()
        return holder
    }

    private class Ptr(
        val id: Int,
        val tool: Int,
        val at: Offset,
    )

    private fun sendPointers(
        action: Int,
        downTime: Long,
        time: Long,
        vararg pointers: Ptr,
    ) {
        val props =
            Array(pointers.size) { i ->
                MotionEvent.PointerProperties().also {
                    it.id = pointers[i].id
                    it.toolType = pointers[i].tool
                }
            }
        val coords =
            Array(pointers.size) { i ->
                MotionEvent.PointerCoords().also {
                    it.x = pointers[i].at.x
                    it.y = pointers[i].at.y
                    it.pressure = 1f
                }
            }
        val source = InputDevice.SOURCE_STYLUS or InputDevice.SOURCE_TOUCHSCREEN
        val e =
            MotionEvent.obtain(downTime, time, action, pointers.size, props, coords, 0, 0, 1f, 1f, 0, 0, source, 0)
        rule.runOnUiThread {
            rule.activity.window.decorView
                .dispatchTouchEvent(e)
        }
        e.recycle()
        rule.waitForIdle()
    }

    private fun pen(
        at: Offset,
        id: Int = 0,
    ) = Ptr(id, MotionEvent.TOOL_TYPE_STYLUS, at)

    private fun nextDownTime(): Long {
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        return clock
    }

    @Test
    fun `a stroke whose pointer disappears is saved, not lost`() {
        val h = bare()
        val down = nextDownTime()
        sendPointers(MotionEvent.ACTION_DOWN, down, down, pen(Offset(100f, 100f)))
        sendPointers(MotionEvent.ACTION_MOVE, down, down + 16, pen(Offset(300f, 160f)))
        sendPointers(MotionEvent.ACTION_MOVE, down, down + 32, pen(Offset(500f, 100f)))
        assertTrue(h.written.isEmpty())
        // The contact loop restarts mid-stroke (its key changes): no UP and no cancel behind it, the stroke is kept.
        rule.runOnUiThread { h.current = InkSurface(write = { InkEdit.NoOp }, erase = { InkEdit.NoOp }) }
        rule.waitForIdle()
        assertEquals(1, h.written.size)
        assertTrue(h.written.single().pointCount >= 2)
        assertTrue(!h.surface.liveVisible)
    }

    @Test
    fun `the live stroke stops as soon as the pen leaves the area`() {
        val h = bare()
        val down = nextDownTime()
        sendPointers(MotionEvent.ACTION_DOWN, down, down, pen(Offset(100f, 100f)))
        sendPointers(MotionEvent.ACTION_MOVE, down, down + 16, pen(Offset(300f, 160f)))
        assertTrue(h.surface.liveVisible)
        sendPointers(MotionEvent.ACTION_MOVE, down, down + 32, pen(Offset(1500f, 160f)))
        assertEquals("the stroke is saved at once", 1, h.written.size)
        assertTrue("and the preview does not draw it a second time", !h.surface.liveVisible)
        sendPointers(MotionEvent.ACTION_UP, down, down + 48, pen(Offset(1500f, 160f)))
        assertEquals(1, h.written.size)
    }

    @Test
    fun `a finger resting in the area during a pen contact is consumed, alone it is not`() {
        val h = bare()
        val target =
            rule
                .onNodeWithTag("target")
                .fetchSemanticsNode()
                .boundsInWindow.center
        val finger = Ptr(1, MotionEvent.TOOL_TYPE_FINGER, target)
        val down = nextDownTime()
        val tip = pen(Offset(100f, 100f))
        sendPointers(MotionEvent.ACTION_DOWN, down, down, tip)
        val secondDown = MotionEvent.ACTION_POINTER_DOWN or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT)
        val secondUp = MotionEvent.ACTION_POINTER_UP or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT)
        sendPointers(secondDown, down, down + 16, tip, finger)
        sendPointers(secondUp, down, down + 32, tip, finger)
        sendPointers(MotionEvent.ACTION_UP, down, down + 48, tip)
        assertEquals("the palm did not press the button", 0, h.clicks)
        assertEquals(1, h.written.size)
        // No pen: the finger keeps its job.
        val t = nextDownTime()
        sendPointers(MotionEvent.ACTION_DOWN, t, t, finger)
        sendPointers(MotionEvent.ACTION_UP, t, t + 16, finger)
        assertEquals(1, h.clicks)
    }

    @Test
    fun `the limit is told once per contact however many parts are refused`() {
        val h = bare { InkEdit.Rejected(InkLimit.TOO_MANY_POINTS) }
        val down = nextDownTime()
        val points = (0 until 1700).map { Offset(50f + (it % 40) * 20f, 100f + (it % 2) * 10f) }
        sendPointers(MotionEvent.ACTION_DOWN, down, down, pen(points.first()))
        for ((i, p) in points.drop(1).withIndex()) sendPointers(MotionEvent.ACTION_MOVE, down, down + 16 + i, pen(p))
        sendPointers(MotionEvent.ACTION_UP, down, down + 5000L, pen(points.last()))
        assertTrue("the contact was split at 800 points: ${h.written.size}", h.written.size >= 3)
        assertEquals(1, h.limits)
        // A new contact tells again.
        val again = nextDownTime()
        sendPointers(MotionEvent.ACTION_DOWN, again, again, pen(Offset(100f, 100f)))
        sendPointers(MotionEvent.ACTION_UP, again, again + 16, pen(Offset(100f, 100f)))
        assertEquals(2, h.limits)
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
