package com.teachme.core.designsystem

import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performTextInput
import com.teachme.core.designsystem.components.TeachTextField
import com.teachme.core.designsystem.components.rememberTextFieldValue
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class TextFieldTest {
    @get:Rule
    val rule = createAndroidComposeRule<ComponentActivity>()

    private var stored by mutableStateOf("")
    private var version by mutableIntStateOf(0)
    private val seen = mutableListOf<String>()

    private fun show() =
        rule.setContent {
            TeachTheme {
                // `stored` plays the ViewModel: it is only read to seed or reset the field.
                val field = rememberTextFieldValue(stored, version)
                TeachTextField(
                    value = field.value,
                    onValueChange = {
                        field.value = it
                        seen += it.text
                    },
                    label = "URL",
                )
            }
        }

    @Test
    fun rapidTypingKeepsEveryCharacter() {
        show()
        val text = "http://10.0.2.2:8080"
        text.forEach { rule.onNode(hasSetTextAction()).performTextInput(it.toString()) }
        rule.onNode(hasSetTextAction()).assertTextContains(text)
        assertEquals(text, seen.last())
    }

    @Test
    fun aCallerResetReplacesWhatWasTypedDeterministically() {
        show()
        rule.onNode(hasSetTextAction()).performTextInput("typed")
        stored = "http://loaded:8080"
        version++
        rule.waitForIdle()
        rule.onNode(hasSetTextAction()).assertTextContains("http://loaded:8080")
    }

    @Test
    fun aChangeWithoutAResetKeyNeverOverwritesTyping() {
        show()
        rule.onNode(hasSetTextAction()).performTextInput("abc")
        stored = "something else"
        rule.waitForIdle()
        rule.onNode(hasSetTextAction()).assertTextContains("abc")
    }
}
