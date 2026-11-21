package com.teachme.core.designsystem

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import com.teachme.core.designsystem.components.DialogErrorPanel
import com.teachme.core.designsystem.components.TeachInputDialog
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class InputDialogTest {
    @get:Rule
    val rule = createAndroidComposeRule<ComponentActivity>()

    private fun show(
        onConfirm: (String) -> Unit = {},
        error: DialogErrorPanel? = null,
        serverError: String? = null,
    ) = rule.setContent {
        TeachTheme {
            TeachInputDialog(
                title = "Nuova cartella",
                label = "Nome della cartella",
                confirmLabel = "Crea",
                loadingLabel = "Creo…",
                dismissLabel = "Annulla",
                maxLength = 10,
                emptyText = "Dai un nome alla cartella.",
                tooLongText = { "Al massimo 10 caratteri: ora sono $it." },
                onConfirm = onConfirm,
                onDismiss = {},
                serverFieldError = serverError,
                error = error,
            )
        }
    }

    @Test
    fun typingAndSubmittingSendsTheTrimmedText() {
        var sent: String? = null
        show(onConfirm = { sent = it })
        rule.onNodeWithText("Nome della cartella").assertIsDisplayed()
        rule
            .onNode(
                androidx.compose.ui.test
                    .hasSetTextAction(),
            ).performTextInput("  Lavoro ")
        rule.onNodeWithText("Crea").performClick()
        assertEquals("Lavoro", sent)
    }

    @Test
    fun emptyIsOnlyAnErrorOnSubmit() {
        var sent: String? = null
        show(onConfirm = { sent = it })
        rule.onNodeWithText("Dai un nome alla cartella.").assertDoesNotExist()
        rule.onNodeWithText("Crea").performClick()
        rule.onNodeWithText("Dai un nome alla cartella.").assertIsDisplayed()
        assertEquals(null, sent)
    }

    @Test
    fun tooLongIsLiveAndBlocksSubmit() {
        var sent: String? = null
        show(onConfirm = { sent = it })
        rule
            .onNode(
                androidx.compose.ui.test
                    .hasSetTextAction(),
            ).performTextInput("abcdefghijkl")
        rule.onNodeWithText("Al massimo 10 caratteri: ora sono 12.").assertIsDisplayed()
        rule.onNodeWithText("12/10").assertIsDisplayed()
        rule.onNodeWithText("Crea").performClick()
        assertEquals(null, sent)
        rule
            .onNode(
                androidx.compose.ui.test
                    .hasSetTextAction(),
            ).performTextClearance()
        rule
            .onNode(
                androidx.compose.ui.test
                    .hasSetTextAction(),
            ).performTextInput("abc")
        rule.onNodeWithText("Al massimo 10 caratteri: ora sono 12.").assertDoesNotExist()
    }

    @Test
    fun serverAndPanelErrorsAreShown() {
        show(
            serverError = "Hai già una cartella con questo nome.",
            error = DialogErrorPanel("Non riesco a raggiungere il backend", "La cartella non è stata creata."),
        )
        rule.onNodeWithText("Hai già una cartella con questo nome.").assertIsDisplayed()
        rule.onNodeWithText("Non riesco a raggiungere il backend").assertIsDisplayed()
    }

    @Test
    fun typedTextSurvivesStateRestoration() {
        val tester = StateRestorationTester(rule)
        tester.setContent {
            TeachTheme {
                TeachInputDialog(
                    "Nuova cartella",
                    "Nome",
                    "Crea",
                    "Creo…",
                    "Annulla",
                    10,
                    "vuoto",
                    { "lungo $it" },
                    {},
                    {},
                )
            }
        }
        rule.onNode(hasSetTextAction()).performTextInput("Lavoro")
        // Same as a rotation: the saved state is written and the composition is rebuilt from it.
        tester.emulateSavedInstanceStateRestore()
        rule.onNodeWithText("Lavoro").assertIsDisplayed()
    }
}
