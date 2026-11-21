package com.teachme.sdui.render

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.model.ChoiceOption
import com.teachme.sdui.model.ExerciseCode
import com.teachme.sdui.model.Flashcard
import com.teachme.sdui.model.OpenAnswer
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The four exercise cards draw their `code` above the question as one TalkBack node. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "it-w360dp-h900dp-xxhdpi")
class ExerciseCodeRenderTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val code = ExerciseCode("rust", "let r = &s;\nprintln!(\"{}\", r);\ns.push('!');")
    private val options = listOf(ChoiceOption("a", "Prima"), ChoiceOption("b", "Seconda"))
    private val description = "Codice rust, 3 righe"

    private fun show(
        states: Map<String, ExerciseState> = emptyMap(),
        content: @androidx.compose.runtime.Composable () -> Unit,
    ) = rule.setContent { PreviewFrame(dark = false, states = states, content = content) }

    @Test
    fun `quiz single shows the code above the question`() {
        show {
            QuizSingleBlock(
                QuizSingle("q", code = code, question = "Domanda unica?", options = options, correct = "a"),
            )
        }
        rule.onAllNodesWithContentDescription(description).assertCountEquals(1)
        val codeTop =
            rule
                .onAllNodesWithContentDescription(description)[0]
                .fetchSemanticsNode()
                .boundsInRoot.top
        val questionTop =
            rule
                .onNodeWithText("Domanda unica?")
                .fetchSemanticsNode()
                .boundsInRoot.top
        assertTrue(codeTop < questionTop)
    }

    @Test
    fun `quiz multi, open answer and flashcard show the code`() {
        show {
            QuizMultiBlock(QuizMulti("m", code = code, question = "Molte?", options = options, correct = listOf("a")))
            OpenAnswerBlock(OpenAnswer("o", code = code, question = "Aperta?"))
            FlashcardBlock(Flashcard("f", code = code, front = "Fronte", back = "Retro"))
        }
        rule.onAllNodesWithContentDescription(description).assertCountEquals(3)
    }

    @Test
    fun `flashcard keeps the code on the back`() {
        show(mapOf("f" to ExerciseState(revealed = true))) {
            FlashcardBlock(Flashcard("f", code = code, front = "Fronte", back = "Retro"))
        }
        rule.onAllNodesWithContentDescription(description).assertCountEquals(1)
        rule.onNodeWithText("Retro").assertIsDisplayed()
    }

    @Test
    fun `no code, no node`() {
        show { QuizSingleBlock(QuizSingle("q", question = "Senza?", options = options, correct = "a")) }
        rule.onAllNodesWithContentDescription(description).assertCountEquals(0)
    }

    @Test
    fun `the code stays after the answer is checked`() {
        val checked = ExerciseState(selection = listOf("a"), attempts = 1, checked = true)
        show(mapOf("q" to checked)) {
            QuizSingleBlock(QuizSingle("q", code = code, question = "Domanda?", options = options, correct = "a"))
        }
        rule.onAllNodesWithContentDescription(description).assertCountEquals(1)
    }
}
