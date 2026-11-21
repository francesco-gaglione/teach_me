package com.teachme.feature.lesson

import androidx.activity.ComponentActivity
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.InputModeManager
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.pressKey
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.model.DocumentSource
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.TopicState
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h900dp-xxhdpi")
class LessonsTabKeysUiTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()
    private var generated = 0
    private lateinit var inputMode: InputModeManager

    private fun show(state: LessonsUiState) {
        val actions = LessonsTabActions({ generated++ }, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})
        rule.setContent {
            inputMode = LocalInputModeManager.current
            TeachTheme {
                LessonsTab(TopicState.Ready, false, state, null, emptyList(), actions)
            }
        }
    }

    /** A tablet keyboard: clickable rows and buttons take the focus only in keyboard input mode. */
    @OptIn(ExperimentalComposeUiApi::class)
    private fun focusWithKeyboard(text: String) {
        rule.runOnIdle { inputMode.requestInputMode(InputMode.Keyboard) }
        rule.onNode(hasClickAction() and hasText(text)).performSemanticsAction(SemanticsActions.RequestFocus)
        rule.onNode(hasClickAction() and hasText(text)).assertIsFocused()
    }

    private val ready = LessonsUiState(load = LessonsLoad.Ready)

    @Test
    fun `pdf line present for a lesson with sources`() {
        val l =
            lesson(
                status = LessonStatus.Completed,
            ).copy(documentSources = listOf(DocumentSource("d", "Rust", 101, 118)))
        show(ready.copy(lessons = listOf(l)))
        rule.onNodeWithText("Dal tuo PDF: pp. 101-118").assertIsDisplayed()
    }

    @Test
    fun `pdf line absent without sources`() {
        show(ready.copy(lessons = listOf(lesson(status = LessonStatus.Completed))))
        rule.onNodeWithText("Dal tuo PDF", substring = true).assertDoesNotExist()
    }

    @Test
    fun `reading step shows its row`() {
        val j = job("lesson_generation", JobStatus.Running, step = "reading")
        show(ready.copy(job = j))
        rule.onNodeWithText("Leggo le fonti").assertIsDisplayed()
    }

    @Test
    fun `alt g generates when the card offers it`() {
        show(ready.copy(lessons = listOf(lesson(status = LessonStatus.Completed))))
        focusWithKeyboard("Genera lezione")
        rule.onRoot().performKeyInput {
            keyDown(Key.AltLeft)
            pressKey(Key.G)
            keyUp(Key.AltLeft)
        }
        assertEquals(1, generated)
    }

    @Test
    fun `alt g is ignored while the focus is on Riprendi`() {
        show(ready.copy(lessons = listOf(lesson(status = LessonStatus.InProgress))))
        focusWithKeyboard("Riprendi la lezione")
        rule.onRoot().performKeyInput {
            keyDown(Key.AltLeft)
            pressKey(Key.G)
            keyUp(Key.AltLeft)
        }
        assertEquals(0, generated)
    }

    @Test
    fun `alt g does nothing while preparing`() {
        show(ready.copy(job = job("lesson_generation", JobStatus.Running, step = "writing")))
        rule.onRoot().performKeyInput {
            keyDown(Key.AltLeft)
            pressKey(Key.G)
            keyUp(Key.AltLeft)
        }
        assertEquals(0, generated)
    }
}
