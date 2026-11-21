package com.teachme.feature.lesson

import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.lifecycle.SavedStateHandle
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.model.ApiResult
import com.teachme.core.model.InMemoryTopicTabInbox
import com.teachme.core.model.LessonView
import com.teachme.core.model.ManualUserScopes
import com.teachme.core.model.StrokeRecord
import com.teachme.sdui.highlight.HighlightColor
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkWidth
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

private const val TEXT = "Quando sposti la variabile, la prima non è più utilizzabile."
private const val NOTES_DOC =
    """{"catalog_version":1,"title":"Ownership","objective":"Capire il move.","estimated_minutes":5,
        "glossary":[],"sources":[],"blocks":[
        {"id":"h1","type":"heading","level":1,"text":"Ownership"},
        {"id":"p1","type":"paragraph","text":"$TEXT"},
        {"id":"q1","type":"quiz_single","question":"Quale parola resta valida dopo lo spostamento","options":[{"id":"a","text":"La prima","feedback":"f"},{"id":"b","text":"La seconda","feedback":"f"}],"correct":"b"}]}"""

private val FILLER = "Questo paragrafo è abbastanza lungo da occupare qualche riga sullo schermo del telefono. "
private val LONG_NOTES_DOC =
    """{"catalog_version":1,"title":"Lunga","objective":"Scorrere.","estimated_minutes":5,
        "glossary":[],"sources":[],"blocks":[{"id":"h1","type":"heading","level":1,"text":"Lunga"},""" +
        (1..14).joinToString(",") { """{"id":"p$it","type":"paragraph","text":"n$it; $FILLER"}""" } +
        "]}"

/** The palette, the keyboard shortcuts, the limit toast and the screen reader side of the handwriting (stage 4). */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h640dp-xxhdpi")
class LessonPaletteUiTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val backend = FakeLessons()
    private val strokeStore = MemoryStrokeStore()
    private lateinit var vm: LessonViewModel
    private var clock = 0L

    private fun note(
        n: Int,
        block: String = "p1",
    ) = StrokeRecord("id-$n", block, "ink", "medium", listOf(100 + n, 200))

    private fun open(
        tool: PenTool = PenTool.PEN,
        notes: List<StrokeRecord> = emptyList(),
        doc: String = NOTES_DOC,
    ) {
        backend.view = ApiResult.Success(LessonView(lesson(), doc, emptyList(), null, emptyList(), notes))
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
                StrokeSyncer(backend, strokeStore, ManualUserScopes()),
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

    private fun bounds(text: String) = rule.onNodeWithText(text, substring = true).fetchSemanticsNode().boundsInRoot

    private fun send(
        action: Int,
        at: Offset,
        downTime: Long,
        time: Long,
    ) {
        val props =
            arrayOf(
                MotionEvent.PointerProperties().also {
                    it.id = 0
                    it.toolType = MotionEvent.TOOL_TYPE_STYLUS
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
        val e =
            MotionEvent.obtain(
                downTime,
                time,
                action,
                1,
                props,
                coords,
                0,
                0,
                1f,
                1f,
                0,
                0,
                InputDevice.SOURCE_STYLUS,
                0,
            )
        rule.runOnUiThread {
            val decor = rule.activity.window.decorView
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

    /** A wavy line across the paragraph, [dy] below its top: one pen stroke. */
    private fun write(dy: Float = 20f) {
        val b = bounds(TEXT)
        pen((0..12).map { Offset(b.left + 40f + it * 20f, b.top + dy + (if (it % 2 == 0) 0f else 12f)) })
    }

    private fun pen(line: List<Offset>) {
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        val down = clock
        send(MotionEvent.ACTION_DOWN, line.first(), down, clock)
        for (p in line.drop(1)) {
            clock += 16
            send(MotionEvent.ACTION_MOVE, p, down, clock)
        }
        clock += 16
        send(MotionEvent.ACTION_UP, line.last(), down, clock)
    }

    private fun finger(line: List<Offset>) {
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        val down = clock

        fun go(
            action: Int,
            at: Offset,
        ) {
            val props = arrayOf(MotionEvent.PointerProperties().also { it.id = 0 })
            props[0].toolType = MotionEvent.TOOL_TYPE_FINGER
            val coords =
                arrayOf(
                    MotionEvent.PointerCoords().also {
                        it.x = at.x
                        it.y = at.y
                        it.pressure = 1f
                    },
                )
            val e =
                MotionEvent.obtain(
                    down,
                    clock,
                    action,
                    1,
                    props,
                    coords,
                    0,
                    0,
                    1f,
                    1f,
                    0,
                    0,
                    InputDevice.SOURCE_TOUCHSCREEN,
                    0,
                )
            rule.runOnUiThread {
                rule.activity.window.decorView
                    .dispatchTouchEvent(e)
            }
            e.recycle()
            rule.waitForIdle()
        }
        go(MotionEvent.ACTION_DOWN, line.first())
        for (p in line.drop(1)) {
            clock += 16
            go(MotionEvent.ACTION_MOVE, p)
        }
        clock += 16
        go(MotionEvent.ACTION_UP, line.last())
    }

    private fun hover() {
        clock = maxOf(clock, SystemClock.uptimeMillis()) + 1000
        send(MotionEvent.ACTION_HOVER_MOVE, bounds("Capire il move.").center, clock, clock)
        rule.mainClock.advanceTimeBy(200)
    }

    private fun stateOf(text: String) =
        rule
            .onNode(hasText(text, substring = true))
            .fetchSemanticsNode()
            .config
            .getOrNull(SemanticsProperties.StateDescription)

    private fun stateOfDescription(description: String) =
        rule
            .onNodeWithContentDescription(description)
            .fetchSemanticsNode()
            .config
            .getOrNull(SemanticsProperties.StateDescription)

    private fun actionLabelsOf(text: String) =
        rule
            .onNode(hasText(text, substring = true))
            .fetchSemanticsNode()
            .config
            .getOrNull(SemanticsActions.CustomActions)
            .orEmpty()
            .map { it.label }

    // --- the palette ---------------------------------------------------------------------------------------------

    @Test
    fun `the tool selector changes the tool, shows its controls and remembers the choice`() {
        open(tool = PenTool.HIGHLIGHTER)
        hover()
        rule.onNodeWithContentDescription("Strumento").assertIsDisplayed()
        rule.onNodeWithContentDescription("Evidenziatore").assertIsSelected()
        rule.onNodeWithContentDescription("Colore dell'evidenziatore").assertIsDisplayed()
        rule.onNodeWithContentDescription("Annulla").assertDoesNotExist()

        rule.onNodeWithContentDescription("Penna").performClick()
        rule.waitForIdle()
        assertEquals(PenTool.PEN, ready.penTool)
        assertEquals("pen", strokeStore.tool)
        rule.onNodeWithContentDescription("Penna").assertIsSelected()
        rule.onNodeWithContentDescription("Colore dell'evidenziatore").assertDoesNotExist()
        rule.onNodeWithContentDescription("Annulla").assertExists()

        // The pen writes now, the highlighter does not.
        write()
        assertEquals(1, strokes.size)
        rule.onNodeWithContentDescription("Evidenziatore").performClick()
        rule.waitForIdle()
        assertEquals(PenTool.HIGHLIGHTER, ready.penTool)
        assertEquals("highlighter", strokeStore.tool)
        write(dy = 120f)
        assertEquals(1, strokes.size)
    }

    @Test
    fun `under 660 dp the pen has one style button whose popover holds inks and widths`() {
        open()
        hover()
        rule.onNodeWithContentDescription("Stile della penna").assertIsDisplayed()
        assertEquals("Inchiostro, medio", stateOfDescription("Stile della penna"))
        rule.onNodeWithContentDescription("Rosso").assertDoesNotExist()
        rule.onNodeWithContentDescription("Spesso").assertDoesNotExist()

        rule.onNodeWithContentDescription("Stile della penna").performClick()
        for (name in listOf("Inchiostro", "Blu", "Rosso", "Verde", "Sottile", "Medio", "Spesso")) {
            rule.onNodeWithContentDescription(name).assertExists()
        }
        rule.onNodeWithContentDescription("Inchiostro").assertIsSelected()
        rule.onNodeWithContentDescription("Rosso").performClick()
        rule.waitForIdle()
        assertEquals(InkColor.RED, ready.inkColor)
        assertEquals("red", strokeStore.color)
        // A choice does not close the popover: the width can be chosen too.
        rule.onNodeWithContentDescription("Spesso").performClick()
        rule.waitForIdle()
        assertEquals(InkWidth.THICK, ready.inkWidth)
        assertEquals("thick", strokeStore.width)
        assertEquals("Rosso, spesso", stateOfDescription("Stile della penna"))
        // The next stroke uses them (the popover closed by itself is not needed: the page is written under it).
        rule.onNodeWithContentDescription("Stile della penna").performClick()
        rule.onNodeWithContentDescription("Sottile").assertDoesNotExist()
        write()
        assertEquals(InkColor.RED, strokes.single().color)
        assertEquals(InkWidth.THICK, strokes.single().width)
    }

    @Test
    fun `the compact highlighter has a colour button whose popover closes on a choice`() {
        open(tool = PenTool.HIGHLIGHTER)
        hover()
        rule.onNodeWithContentDescription("Colore dell'evidenziatore").assertIsDisplayed()
        rule.onNodeWithContentDescription("Rosa").assertDoesNotExist()
        rule.onNodeWithContentDescription("Colore dell'evidenziatore").performClick()
        rule.onNodeWithContentDescription("Rosa").performClick()
        rule.waitForIdle()
        assertEquals(HighlightColor.PINK, ready.highlightColor)
        rule.onNodeWithContentDescription("Verde").assertDoesNotExist()
        rule
            .onNodeWithContentDescription("Colore dell'evidenziatore")
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "Rosa"))
    }

    @Test
    fun `the compact popover closes by itself after six seconds when nothing is chosen`() {
        open()
        hover()
        rule.onNodeWithContentDescription("Stile della penna").performClick()
        rule.onNodeWithContentDescription("Blu").assertExists()
        rule.mainClock.advanceTimeBy(6_500)
        rule.onNodeWithContentDescription("Blu").assertDoesNotExist()
    }

    @Test
    fun `a touch inside the popover restarts its six seconds`() {
        open()
        hover()
        rule.onNodeWithContentDescription("Stile della penna").performClick()
        rule.mainClock.advanceTimeBy(4_000)
        rule.onNodeWithContentDescription("Blu").performClick()
        rule.mainClock.advanceTimeBy(4_000)
        rule.onNodeWithContentDescription("Sottile").assertExists()
        rule.mainClock.advanceTimeBy(3_000)
        rule.onNodeWithContentDescription("Sottile").assertDoesNotExist()
    }

    // --- the eraser switch ---------------------------------------------------------------------------------------

    private fun eraserNode() = rule.onNodeWithContentDescription("Gomma")

    @Test
    fun `the eraser is a switch, off at first, named and stated for the screen reader, in both tools`() {
        open()
        hover()
        for (tool in listOf(PenTool.PEN, PenTool.HIGHLIGHTER)) {
            vm.setPenTool(tool)
            rule.waitForIdle()
            val config = eraserNode().fetchSemanticsNode().config
            assertEquals(Role.Switch, config[SemanticsProperties.Role])
            assertEquals("spenta", config[SemanticsProperties.StateDescription])
            eraserNode().assertIsOff()
        }
        eraserNode().performClick()
        rule.waitForIdle()
        assertTrue(ready.eraserMode)
        eraserNode().assertIsOn()
        assertEquals("attiva", eraserNode().fetchSemanticsNode().config[SemanticsProperties.StateDescription])
        rule.onNodeWithContentDescription("Gomma attiva: la punta cancella").assertExists()
        eraserNode().performClick()
        rule.waitForIdle()
        assertFalse(ready.eraserMode)
        rule.onNodeWithContentDescription("Gomma spenta").assertExists()
        // Said once and then gone.
        rule.mainClock.advanceTimeBy(3_500)
        rule.onNodeWithContentDescription("Gomma spenta").assertDoesNotExist()
    }

    @Test
    fun `choosing an ink, a width, a highlight colour or another tool turns the eraser off`() {
        open()
        hover()

        fun on() {
            vm.setEraserMode(true)
            rule.waitForIdle()
            assertTrue(ready.eraserMode)
        }
        on()
        rule.onNodeWithContentDescription("Stile della penna").performClick()
        rule.onNodeWithContentDescription("Blu").performClick()
        rule.waitForIdle()
        assertFalse(ready.eraserMode)
        eraserNode().assertIsOff()
        on()
        rule.onNodeWithContentDescription("Spesso").performClick()
        rule.waitForIdle()
        assertFalse(ready.eraserMode)
        on()
        rule.onNodeWithContentDescription("Evidenziatore").performClick()
        rule.waitForIdle()
        assertFalse(ready.eraserMode)
        on()
        rule.onNodeWithContentDescription("Colore dell'evidenziatore").performClick()
        rule.onNodeWithContentDescription("Verde").performClick()
        rule.waitForIdle()
        assertFalse(ready.eraserMode)
        // Undo and redo do not turn it off.
        vm.setPenTool(PenTool.PEN)
        on()
        write()
        rule.onNodeWithContentDescription("Annulla").performClick()
        rule.waitForIdle()
        assertTrue(ready.eraserMode)
    }

    @Test
    @Config(sdk = [34], qualifiers = "it-w659dp-h640dp-xxhdpi")
    fun `at 659 dp the palette is compact`() {
        open()
        hover()
        rule.onNodeWithContentDescription("Stile della penna").assertExists()
        rule.onNodeWithContentDescription("Rosso").assertDoesNotExist()
        eraserNode().assertExists()
    }

    @Test
    @Config(sdk = [34], qualifiers = "it-w660dp-h640dp-xxhdpi")
    fun `at 660 dp the palette is expanded, with the eraser before undo and redo`() {
        open()
        hover()
        rule.onNodeWithContentDescription("Stile della penna").assertDoesNotExist()
        rule.onNodeWithContentDescription("Rosso").assertExists()
        rule.onNodeWithContentDescription("Spesso").assertExists()
        val eraser = eraserNode().fetchSemanticsNode().boundsInRoot
        val undo = rule.onNodeWithContentDescription("Annulla").fetchSemanticsNode().boundsInRoot
        val thick = rule.onNodeWithContentDescription("Spesso").fetchSemanticsNode().boundsInRoot
        assertTrue(thick.right <= eraser.left)
        assertTrue(eraser.right <= undo.left)
        assertEquals(48f * rule.density.density, eraser.width, 1f)
        assertEquals(48f * rule.density.density, eraser.height, 1f)
    }

    @Test
    fun `undo and redo follow the stacks, from the buttons, and are said to the screen reader`() {
        open()
        hover()
        rule.onNodeWithContentDescription("Annulla").assertIsNotEnabled()
        rule.onNodeWithContentDescription("Ripeti").assertIsNotEnabled()
        write()
        rule.onNodeWithContentDescription("Annulla").assertIsEnabled()
        rule.onNodeWithContentDescription("Annulla").performClick()
        rule.waitForIdle()
        assertTrue(strokes.isEmpty())
        rule.onNodeWithContentDescription("Tratto annullato").assertExists()
        rule.onNodeWithContentDescription("Annulla").assertIsNotEnabled()
        rule.onNodeWithContentDescription("Ripeti").assertIsEnabled()
        rule.onNodeWithContentDescription("Ripeti").performClick()
        rule.waitForIdle()
        assertEquals(1, strokes.size)
        rule.onNodeWithContentDescription("Tratto ripristinato").assertExists()
        // Said once and then gone, so the same words can be said again.
        rule.mainClock.advanceTimeBy(3_500)
        rule.onNodeWithContentDescription("Tratto ripristinato").assertDoesNotExist()
    }

    @Test
    fun `the expanded form shows everything at once and the compact form does not`() {
        var tool by mutableStateOf(PenTool.PEN)
        var color by mutableStateOf(InkColor.BLUE)
        var width by mutableStateOf(InkWidth.THIN)
        var undone = 0
        var compact by mutableStateOf(false)
        var eraser by mutableStateOf(false)
        rule.setContent {
            TeachTheme {
                LessonPalette(
                    visible = true,
                    compact = compact,
                    tool = tool,
                    onTool = { tool = it },
                    highlightColor = HighlightColor.GREEN,
                    onHighlightColor = {},
                    inkColor = color,
                    onInkColor = { color = it },
                    inkWidth = width,
                    onInkWidth = { width = it },
                    canUndo = true,
                    canRedo = false,
                    onUndo = { undone++ },
                    onRedo = {},
                    eraserOn = eraser,
                    onEraser = { eraser = it },
                    onTouched = {},
                )
            }
        }
        for (name in listOf("Strumento", "Colore dell'inchiostro", "Spessore")) {
            rule.onNodeWithContentDescription(name).assertExists()
        }
        for (name in listOf("Inchiostro", "Blu", "Rosso", "Verde", "Sottile", "Medio", "Spesso")) {
            rule.onNodeWithContentDescription(name).assertExists()
        }
        rule.onNodeWithContentDescription("Blu").assertIsSelected()
        rule.onNodeWithContentDescription("Sottile").assertIsSelected()
        rule.onNodeWithContentDescription("Annulla").assertIsEnabled()
        rule.onNodeWithContentDescription("Ripeti").assertIsNotEnabled()
        rule.onNodeWithContentDescription("Gomma").assertExists()
        // The pill is wider than the test window: the actions are invoked, not touched.
        for (name in listOf("Verde", "Spesso", "Annulla")) {
            rule.onNodeWithContentDescription(name).performSemanticsAction(SemanticsActions.OnClick)
        }
        rule.waitForIdle()
        assertEquals(InkColor.GREEN, color)
        assertEquals(InkWidth.THICK, width)
        assertEquals(1, undone)
        // The highlighter shows its colours and nothing of the pen.
        tool = PenTool.HIGHLIGHTER
        rule.waitForIdle()
        rule.onNodeWithContentDescription("Colore dell'evidenziatore").assertExists()
        rule.onNodeWithContentDescription("Verde").assertExists()
        rule.onNodeWithContentDescription("Spesso").assertDoesNotExist()
        rule.onNodeWithContentDescription("Annulla").assertDoesNotExist()
        rule.onNodeWithContentDescription("Gomma").assertExists()
        // Narrow window: one style button.
        tool = PenTool.PEN
        compact = true
        rule.waitForIdle()
        rule.onNodeWithContentDescription("Rosso").assertDoesNotExist()
        assertEquals("Verde, spesso", stateOfDescription("Stile della penna"))
        rule.onNodeWithContentDescription("Gomma").performClick()
        assertTrue(eraser)
    }

    // --- the keyboard --------------------------------------------------------------------------------------------

    private fun key(
        code: Int,
        ctrl: Boolean = true,
        shift: Boolean = false,
        alt: Boolean = false,
        down: Boolean = true,
    ): InkShortcut? {
        val meta =
            (if (ctrl) android.view.KeyEvent.META_CTRL_ON else 0) or
                (if (shift) android.view.KeyEvent.META_SHIFT_ON else 0) or
                (if (alt) android.view.KeyEvent.META_ALT_ON else 0)
        val action = if (down) android.view.KeyEvent.ACTION_DOWN else android.view.KeyEvent.ACTION_UP
        val e = android.view.KeyEvent(0L, 0L, action, code, 0, meta)
        return inkShortcut(
            androidx.compose.ui.input.key
                .KeyEvent(e),
        )
    }

    @Test
    fun `Ctrl+Z undoes, Ctrl+Shift+Z and Ctrl+Y redo, and nothing else is a shortcut`() {
        assertEquals(InkShortcut.UNDO, key(android.view.KeyEvent.KEYCODE_Z))
        assertEquals(InkShortcut.REDO, key(android.view.KeyEvent.KEYCODE_Z, shift = true))
        assertEquals(InkShortcut.REDO, key(android.view.KeyEvent.KEYCODE_Y))
        assertNull(key(android.view.KeyEvent.KEYCODE_Z, ctrl = false))
        assertNull(key(android.view.KeyEvent.KEYCODE_Y, ctrl = false))
        assertNull(key(android.view.KeyEvent.KEYCODE_Z, down = false))
        assertNull(key(android.view.KeyEvent.KEYCODE_Z, alt = true))
        assertNull(key(android.view.KeyEvent.KEYCODE_X))
    }

    // --- a block that has scrolled away ----------------------------------------------------------------------------

    @Test
    fun `a note reaching into view from a block that scrolled off the screen is still drawn`() {
        open(doc = LONG_NOTES_DOC)
        vm.setInkColor(InkColor.RED)
        rule.waitForIdle()

        fun swipe() = finger((0..15).map { Offset(300f, 1500f - it * 10f) })

        fun top(n: Int) = bounds("n$n; Questo").top
        // The third paragraph comes up to the upper half of the screen, by finger.
        var swipes = 0
        while (top(3) > 700f && swipes++ < 30) swipe()
        assertTrue("the third paragraph is below the bar, at ${top(3)}", top(3) in 200f..700f)
        val p3 = bounds("n3; Questo")
        val rowHeight = top(4) - p3.top
        val x = p3.left + 100f
        // One long line down from the top of the third paragraph: its block is the third one all along.
        pen((0..14).map { Offset(x, p3.top + 10f + it * 50f) })
        assertEquals("p3", strokes.single().blockId)
        // The page goes on until the third paragraph is out of the list; the line is still there.
        while (top(4) - 48f >= 180f && swipes++ < 60) swipe()
        val fourth = top(4)
        val p3Top = fourth - rowHeight
        assertTrue("the third paragraph left the list: its top is at $p3Top", fourth - 48f < 180f)
        val tail = (p3Top + 10f + 12 * 50f).toInt()
        assertTrue("the line is on the screen at $tail", tail in 200..1800)
        val bitmap = rule.onRoot().captureToImage().asAndroidBitmap()
        assertEquals(0xFFB02E2E.toInt(), bitmap.getPixel(x.toInt(), tail))
    }

    // --- limit and queue -----------------------------------------------------------------------------------------

    @Test
    fun `at 2000 strokes the pen does not write and says so, and undo still works`() {
        open(notes = (0 until 2000).map { note(it) })
        assertEquals(2000, strokes.size)
        write()
        assertEquals(2000, strokes.size)
        rule.onNodeWithText("Hai raggiunto il limite di note a mano per questa lezione.").assertIsDisplayed()
        // The eraser and the undo are always possible.
        assertFalse(ready.canUndoInk)
    }

    private fun queue(
        failing: Boolean = false,
        tooLarge: Boolean = false,
        onRetry: () -> Unit = {},
    ) = rule.setContent {
        TeachTheme {
            QueueIndicator(
                queued = 0,
                onRetryNow = onRetry,
                notesFailing = failing,
                notesTooLarge = tooLarge,
            )
        }
    }

    @Test
    fun `the queue has a row for notes that are not saved, with Riprova ora`() {
        var retries = 0
        var failing by mutableStateOf(true)
        rule.setContent {
            TeachTheme {
                QueueIndicator(queued = 0, onRetryNow = { retries++ }, notesFailing = failing)
            }
        }
        rule.onNodeWithContentDescription("Note a mano da salvare").performClick()
        rule
            .onNodeWithText("Alcune note a mano non sono ancora salvate. Le invio appena torna la connessione.")
            .assertIsDisplayed()
        rule.onNodeWithText("Riprova ora").performClick()
        assertEquals(1, retries)
        failing = false
        rule.waitForIdle()
        rule.onNodeWithContentDescription("Note a mano da salvare").assertDoesNotExist()
        rule.onNodeWithContentDescription("Note a mano salvate.").assertExists()
    }

    @Test
    fun `a set that is too big has its own row at once, next to the highlights row`() {
        rule.setContent {
            TeachTheme {
                QueueIndicator(queued = 0, onRetryNow = {}, highlightsFailing = true, notesTooLarge = true)
            }
        }
        rule.onNodeWithContentDescription("Evidenziazioni da salvare").performClick()
        rule.onNodeWithText("Le note a mano di questa lezione sono troppe per essere salvate.").assertIsDisplayed()
        rule
            .onNodeWithText("Alcune evidenziazioni non sono ancora salvate. Le invio appena torna la connessione.")
            .assertIsDisplayed()
        rule.onNodeWithText("Riprova ora").assertIsDisplayed()
    }

    // --- screen reader -------------------------------------------------------------------------------------------

    @Test
    fun `a block with notes says how many, with its highlights, and the action needs the pen to have been seen`() {
        open(tool = PenTool.HIGHLIGHTER, notes = listOf(note(0)))
        assertEquals("1 nota a mano", stateOf(TEXT))
        // A device that never saw the pen cannot change notes: no action, as no palette.
        assertFalse("Elimina le note a mano del blocco" in actionLabelsOf(TEXT))
        vm.addHighlight("p1", 0, 6)
        rule.waitForIdle()
        assertEquals("1 evidenziazione, 1 nota a mano", stateOf("Quando"))
        // The pen shows up: the action is there.
        hover()
        assertTrue("Elimina le note a mano del blocco" in actionLabelsOf("Quando"))
    }

    @Test
    fun `the action takes the notes of the block away with a toast, and Annulla brings them back`() {
        open(notes = listOf(note(0), note(1), note(2, block = "q1")))
        hover()
        val action =
            rule
                .onNode(hasText(TEXT, substring = true))
                .fetchSemanticsNode()
                .config[SemanticsActions.CustomActions]
                .first { it.label == "Elimina le note a mano del blocco" }
        rule.runOnUiThread { action.action() }
        rule.waitForIdle()
        assertEquals(listOf("id-2"), strokes.map { it.id })
        rule.onNodeWithText("Note a mano eliminate").assertIsDisplayed()
        rule.onNodeWithText("Annulla").performClick()
        rule.waitForIdle()
        assertEquals(listOf("id-0", "id-1", "id-2"), strokes.map { it.id })
    }

    @Test
    fun `an exercise block also says its notes, once`() {
        open(notes = listOf(note(0, block = "q1"), note(1, block = "q1")))
        val withNotes =
            rule.onAllNodes(
                SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "2 note a mano"),
                useUnmergedTree = true,
            )
        withNotes.assertCountEquals(1)
        hover()
        val node = withNotes[0].fetchSemanticsNode()
        assertNotNull(node.config.getOrNull(SemanticsActions.CustomActions))
        assertNull(stateOf(TEXT))
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
