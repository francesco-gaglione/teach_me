package com.teachme.sdui.highlight

import android.view.MotionEvent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import com.teachme.sdui.model.ChoiceOption
import com.teachme.sdui.model.Paragraph
import com.teachme.sdui.model.QuizSingle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** Pen, eraser and finger events dispatched to the window: what each one does to a paragraph (highlight.md 4, 7, 8, 9). */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h640dp-xxhdpi")
class PenInputTest {
    @get:Rule val rule = penRule()

    private val sentence = "Quando sposti la variabile, la prima non è più utilizzabile."
    private val quiz =
        QuizSingle(
            id = "q1",
            question = "Quale parola resta valida dopo lo spostamento",
            options = listOf(ChoiceOption("a", "La prima"), ChoiceOption("b", "La seconda")),
            correct = "b",
        )
    private val blocks = listOf(Paragraph("p1", text = sentence), quiz)

    private fun start(
        host: FakeHighlightHost = FakeHighlightHost(),
        scroll: androidx.compose.foundation.ScrollState? = null,
    ): PenInjector {
        rule.setLesson(testDocument(blocks), host, scroll = scroll)
        return PenInjector(rule as AndroidComposeTestRule<*, androidx.activity.ComponentActivity>)
    }

    private fun sourceOf(sub: String) = sentence.indexOf(sub).let { it to it + sub.length }

    @Test
    fun `a pen stroke over words highlights them whole, snapped to the words`() {
        val host = FakeHighlightHost()
        val pen = start(host)
        val text = rule.onNodeWithText(sentence).textOnScreen()
        // From the middle of "sposti" to the middle of "variabile,": the ends snap to the words.
        pen.gesture(
            listOf(text.centerOfWord("sposti"), text.centerOfWord("la variabile,"), text.centerOfWord("variabile,")),
        )
        val (s, e) = sourceOf("sposti la variabile,")
        assertEquals(listOf(Triple("p1", s, e)), host.added)
        assertTrue(host.erased.isEmpty())
    }

    @Test
    fun `the pen stroke does not scroll the page and the same gesture with a finger scrolls without highlighting`() {
        val scroll = androidx.compose.foundation.ScrollState(0)
        val host = FakeHighlightHost()
        val pen = start(host, scroll)
        val text = rule.onNodeWithText(sentence).textOnScreen()
        // The very same points for the pen and for the finger: a stroke over words that ends going up.
        val stroke =
            listOf(
                text.centerOfWord("sposti"),
                text.centerOfWord("variabile,"),
                text.centerOfWord("variabile,") + Offset(0f, -120f),
            )
        pen.gesture(stroke)
        assertEquals("the pen never scrolls inside a text block", 0, scroll.value)
        assertEquals(1, host.added.size)

        host.added.clear()
        pen.gesture(stroke, tool = MotionEvent.TOOL_TYPE_FINGER)
        assertTrue("the finger scrolls", scroll.value > 0)
        assertTrue("the finger never highlights", host.added.isEmpty())
    }

    @Test
    fun `with the pen writing ink the text leaves the tip alone and the finger is unchanged`() {
        val host = FakeHighlightHost()
        host.penWrites = true
        val pen = start(host)
        val text = rule.onNodeWithText(sentence).textOnScreen()
        val stroke =
            listOf(text.centerOfWord("sposti"), text.centerOfWord("la variabile,"), text.centerOfWord("variabile,"))
        pen.gesture(stroke)
        assertTrue("no highlight from the tip", host.added.isEmpty())
        host.penWrites = false
        pen.gesture(stroke)
        assertEquals("and back to the highlighter, it highlights", 1, host.added.size)
    }

    @Test
    fun `a stroke on the quiz does not highlight, on the paragraph beside it does`() {
        val host = FakeHighlightHost()
        val pen = start(host)
        val q = rule.onNodeWithText(quiz.question).textOnScreen()
        pen.gesture(listOf(q.centerOfWord("Quale"), q.centerOfWord("parola")))
        assertTrue(host.added.isEmpty() && host.erased.isEmpty())
        val p = rule.onNodeWithText(sentence).textOnScreen()
        pen.gesture(listOf(p.centerOfWord("Quando"), p.centerOfWord("sposti")))
        assertEquals(1, host.added.size)
    }

    @Test
    fun `a stroke starting outside the text is not a highlight`() {
        val host = FakeHighlightHost()
        val pen = start(host)
        val p = rule.onNodeWithText(sentence).textOnScreen()
        // Starts well to the left of the text node: nothing is highlighted.
        pen.gesture(listOf(Offset(-1f, p.origin.y + 10f), p.centerOfWord("Quando"), p.centerOfWord("sposti")))
        assertTrue(host.added.isEmpty())
    }

    @Test
    fun `the eraser removes only the words it passes over and the pen button erases too`() {
        val (s, e) = sourceOf("sposti la variabile,")
        val host = FakeHighlightHost()
        val pen = start(host)
        val p = rule.onNodeWithText(sentence).textOnScreen()
        host.highlights = listOf(Highlight("p1", s, e, HighlightColor.YELLOW))
        val la = p.centerOfWord("la")
        pen.gesture(listOf(la, la + Offset(0f, 15f), la + Offset(0f, 35f)), tool = MotionEvent.TOOL_TYPE_ERASER)
        val (ls, le) = sourceOf(" la ")
        // The word and the blanks around it go, so no fragment is left that starts or ends with a space.
        assertEquals(listOf(Triple("p1", ls, le)), host.erased)
        assertEquals(
            listOf(Interval(s, ls), Interval(le, e)),
            host.highlights.map { it.interval },
        )

        host.erased.clear()
        val v = p.centerOfWord("variabile,")
        pen.gesture(
            listOf(v, v + Offset(0f, 15f), v + Offset(0f, 35f)),
            tool = MotionEvent.TOOL_TYPE_STYLUS,
            buttons = MotionEvent.BUTTON_STYLUS_PRIMARY,
        )
        assertEquals(1, host.erased.size)
        assertTrue(host.added.isEmpty())
    }

    @Test
    fun `a tap on a highlight asks to remove it, with the finger or the pen`() {
        val (s, e) = sourceOf("sposti la variabile,")
        val host = FakeHighlightHost()
        val pen = start(host)
        val p = rule.onNodeWithText(sentence).textOnScreen()
        host.highlights = listOf(Highlight("p1", s, e, HighlightColor.GREEN))
        rule.onNodeWithText("Rimuovi").assertDoesNotExistCompat()
        pen.gesture(listOf(p.centerOfWord("variabile")), tool = MotionEvent.TOOL_TYPE_FINGER, stepMs = 50)
        rule.onNodeWithText("Rimuovi").performClick()
        assertEquals(listOf("p1" to s), host.removed)
        assertTrue(host.highlights.isEmpty())

        // The pen taps too (and a tap on text without highlight does nothing).
        host.highlights = listOf(Highlight("p1", s, e, HighlightColor.GREEN))
        host.removed.clear()
        pen.gesture(listOf(p.centerOfWord("prima")), stepMs = 50)
        rule.onNodeWithText("Rimuovi").assertDoesNotExistCompat()
        pen.gesture(listOf(p.centerOfWord("sposti")), stepMs = 50)
        rule.onNodeWithText("Rimuovi").performClick()
        assertEquals(listOf("p1" to s), host.removed)
    }

    @Test
    fun `a pen held still on the text is nothing, and a cancelled stroke leaves nothing`() {
        val host = FakeHighlightHost()
        val pen = start(host)
        val p = rule.onNodeWithText(sentence).textOnScreen()
        pen.gesture(listOf(p.centerOfWord("sposti")), holdMs = 900)
        assertEquals("held still: added", emptyList<Any>(), host.added)
        assertEquals("held still: erased", emptyList<Any>(), host.erased)
        assertEquals("held still: removed", emptyList<Any>(), host.removed)
        pen.gesture(
            listOf(p.centerOfWord("sposti"), p.centerOfWord("la"), p.centerOfWord("variabile,")),
            cancel = true,
        )
        assertEquals("cancelled", emptyList<Any>(), host.added)
    }

    private fun androidx.compose.ui.test.SemanticsNodeInteraction.assertDoesNotExistCompat() = assertDoesNotExist()
}
