package com.teachme.sdui.render

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import com.teachme.sdui.logic.Outcome
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Regression: the panel's content lambda used to bind to `trailing`, squeezing the title to one letter per line. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "it-w1280dp-h800dp-xxhdpi")
class OutcomePanelLayoutTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private fun check(outcome: Outcome) {
        rule.setContent {
            PreviewFrame(dark = false, states = emptyMap()) {
                OutcomePanel(outcome, "Spiegazione del risultato.", lang = null, secondAttempt = false)
            }
        }
        rule.mainClock.advanceTimeBy(1_000)
        rule.waitForIdle()
        val title = rule.onNodeWithText(outcomeTitleText(outcome), useUnmergedTree = true).fetchSemanticsNode()
        val body = rule.onNodeWithText("Spiegazione del risultato.", useUnmergedTree = true).fetchSemanticsNode()
        val density = rule.activity.resources.displayMetrics.density
        // One line of text is far below 40dp; one letter per line would be several times taller.
        assertTrue("title wraps: ${title.size}", title.size.height < 40 * density)
        assertTrue(
            "body not in the title column",
            kotlin.math.abs(body.boundsInRoot.left - title.boundsInRoot.left) < 1f,
        )
        assertTrue("body overlaps title", body.boundsInRoot.top >= title.boundsInRoot.bottom - 1f)
    }

    private fun outcomeTitleText(outcome: Outcome) =
        rule.activity.getString(
            when (outcome) {
                Outcome.Correct -> com.teachme.sdui.R.string.sdui_outcome_correct
                Outcome.Partial -> com.teachme.sdui.R.string.sdui_outcome_partial
                Outcome.Incorrect -> com.teachme.sdui.R.string.sdui_outcome_incorrect
            },
        )

    @Test fun `correct title stays on one line above the body`() = check(Outcome.Correct)

    @Test fun `incorrect title stays on one line above the body`() = check(Outcome.Incorrect)
}
