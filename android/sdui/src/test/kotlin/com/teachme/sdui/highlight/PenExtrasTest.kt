package com.teachme.sdui.highlight

import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.text.selection.rememberSelectionState
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import com.teachme.core.designsystem.TeachTheme
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.CalloutVariant
import com.teachme.sdui.model.GlossaryEntry
import com.teachme.sdui.model.Paragraph
import com.teachme.sdui.render.ExerciseStates
import com.teachme.sdui.render.LocalSdui
import com.teachme.sdui.render.SduiEnvironment
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** The rest of the pen contract: callouts, terms under the pen, and the system selection (highlight.md 3.3, 4.1, 9). */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h640dp-xxhdpi", shadows = [ShadowNoMagnifier::class])
class PenExtrasTest {
    @get:Rule val rule = penRule()

    private fun injector() = PenInjector(rule as AndroidComposeTestRule<*, ComponentActivity>)

    @Test
    fun `a stroke on the body of a callout highlights, on its title it does not`() {
        val body = "Il valore passa all'altra variabile e la prima smette di valere."
        val host = FakeHighlightHost()
        rule.setLesson(
            testDocument(
                listOf(Callout("c1", variant = CalloutVariant.Warning, title = "Attenzione al move", text = body)),
            ),
            host,
        )
        val pen = injector()
        val title = rule.onNodeWithText("Attenzione al move", useUnmergedTree = true).textOnScreen()
        pen.gesture(listOf(title.centerOfWord("Attenzione"), title.centerOfWord("move")))
        assertTrue(host.added.isEmpty())
        val text = rule.onNodeWithText(body, useUnmergedTree = true).textOnScreen()
        pen.gesture(listOf(text.centerOfWord("passa"), text.centerOfWord("altra")))
        val s = body.indexOf("passa")
        assertEquals(listOf(Triple("c1", s, body.indexOf("altra") + "altra".length)), host.added)
    }

    @Test
    fun `the pen taps a term like a finger and a tap on a term inside a highlight is not a removal`() {
        val source = "Il [[ownership]] sposta il valore verso un'altra variabile."
        val host = FakeHighlightHost()
        val sdui = FakeSduiHost()
        rule.setLesson(
            testDocument(
                listOf(Paragraph("p1", text = source)),
                listOf(GlossaryEntry("ownership", "ownership", "Chi possiede il valore")),
            ),
            host,
            sdui,
        )
        val pen = injector()
        val text = rule.onNodeWithText("Il ownership sposta il valore verso un'altra variabile.").textOnScreen()
        pen.gesture(listOf(text.centerOfWord("ownership")), stepMs = 40)
        assertEquals(listOf("ownership"), sdui.openedTerms)
        pen.gesture(listOf(text.centerOfWord("ownership")), tool = MotionEvent.TOOL_TYPE_FINGER, stepMs = 40)
        assertEquals(listOf("ownership", "ownership"), sdui.openedTerms)

        // The whole term is one word: a stroke over a part of it highlights all of its source.
        pen.gesture(listOf(text.centerOfWord("ownership") + Offset(-20f, 0f), text.centerOfWord("sposta")))
        val s = source.indexOf("[[ownership]]")
        assertEquals(listOf(Triple("p1", s, source.indexOf("sposta") + "sposta".length)), host.added)
        rule.onNodeWithText("Rimuovi").assertDoesNotExist()
        // On the highlight, a tap on the term opens the term (not the removal), a tap on the plain word asks to remove.
        pen.gesture(listOf(text.centerOfWord("ownership")), stepMs = 40)
        assertEquals(3, sdui.openedTerms.size)
        rule.onNodeWithText("Rimuovi").assertDoesNotExist()
        pen.gesture(listOf(text.centerOfWord("sposta")), tool = MotionEvent.TOOL_TYPE_FINGER, stepMs = 40)
        rule.onNodeWithText("Rimuovi").performClick()
        assertEquals(1, host.removed.size)
        assertTrue(host.highlights.isEmpty())
    }

    @Test
    fun `without the highlighter a held stylus does start a selection (so the previous test proves something)`() {
        val source = "Quando sposti la variabile, la prima non è più utilizzabile."
        lateinit var selection: androidx.compose.foundation.text.selection.SelectionState
        rule.setContent {
            selection = rememberSelectionState()
            TeachTheme {
                CompositionLocalProvider(LocalSdui provides plainEnv()) {
                    SelectionContainer(selection) {
                        InlineText(markdown = source, style = TeachTheme.typography.reading.body)
                    }
                }
            }
        }
        val text = rule.onNodeWithText(source).textOnScreen()
        injector().gesture(listOf(text.centerOfWord("variabile,")), holdMs = 1200)
        assertTrue(selection.selectedTexts.isNotEmpty())
    }

    private fun plainEnv() =
        SduiEnvironment(
            testDocument(emptyList()),
            FakeSduiHost(),
            object : ExerciseStates {
                override fun get(blockId: String) = ExerciseState()

                override fun update(
                    blockId: String,
                    transform: (ExerciseState) -> ExerciseState,
                ) = Unit

                override fun isLocked(blockId: String) = false
            },
        )

    @Test
    fun `the pen never starts a selection, the finger does`() {
        val host = FakeHighlightHost()
        val source = "Quando sposti la variabile, la prima non è più utilizzabile."
        lateinit var selection: androidx.compose.foundation.text.selection.SelectionState
        val states =
            object : ExerciseStates {
                override fun get(blockId: String) = ExerciseState()

                override fun update(
                    blockId: String,
                    transform: (ExerciseState) -> ExerciseState,
                ) = Unit

                override fun isLocked(blockId: String) = false
            }
        val env = SduiEnvironment(testDocument(emptyList()), FakeSduiHost(), states)
        rule.setContent {
            selection = rememberSelectionState()
            TeachTheme {
                CompositionLocalProvider(LocalSdui provides env, LocalHighlightHost provides host) {
                    SelectionContainer(selection) {
                        InlineText(
                            markdown = source,
                            style = TeachTheme.typography.reading.body,
                            highlight = HighlightTarget("p1") { selection.clear() },
                        )
                    }
                }
            }
        }
        val pen = injector()
        val text = rule.onNodeWithText(source).textOnScreen()
        val at = text.centerOfWord("variabile,")

        pen.gesture(listOf(at), holdMs = 1200)
        assertTrue("the pen held down selects nothing", selection.selectedTexts.isEmpty())
        assertTrue(host.added.isEmpty())

        // Control: the finger still selects with a long press, as it does today.
        pen.gesture(listOf(at), tool = MotionEvent.TOOL_TYPE_FINGER, holdMs = 1200)
        assertTrue("the finger long press selects (control)", selection.selectedTexts.isNotEmpty())
        selection.clear()
        assertTrue(selection.selectedTexts.isEmpty())

        // Control: the mouse still selects by dragging, as it does today.
        pen.gesture(
            listOf(text.centerOfWord("sposti"), text.centerOfWord("sposti") + Offset(40f, 0f), at),
            tool = MotionEvent.TOOL_TYPE_MOUSE,
            buttons = MotionEvent.BUTTON_PRIMARY,
        )
        assertTrue("the mouse drag selects (control)", selection.selectedTexts.isNotEmpty())

        // A pen touching the text clears the selection.
        pen.gesture(listOf(at), stepMs = 40)
        assertTrue(selection.selectedTexts.isEmpty())
    }
}
