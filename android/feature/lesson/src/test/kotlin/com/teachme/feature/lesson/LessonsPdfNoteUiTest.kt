package com.teachme.feature.lesson

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.model.TopicState
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h900dp-xxhdpi")
class LessonsPdfNoteUiTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private fun show(pdfTitles: List<String>) {
        val actions = LessonsTabActions({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})
        rule.setContent {
            TeachTheme {
                LessonsTab(
                    topicState = TopicState.Ready,
                    researchFailed = false,
                    state = LessonsUiState(load = LessonsLoad.Ready),
                    missionWhy = null,
                    recentRecords = emptyList(),
                    actions = actions,
                    pdfTitles = pdfTitles,
                )
            }
        }
    }

    @Test
    fun `one ready document names the pdf in the card`() {
        show(listOf("Rust in Action"))
        rule
            .onNodeWithText(
                "Seguo anche il tuo PDF «Rust in Action»: scelgo io quali pagine, partendo dalla tua mission.",
            ).assertIsDisplayed()
    }

    @Test
    fun `several ready documents use the generic note`() {
        show(listOf("A", "B"))
        rule.onNodeWithText("Seguo anche i tuoi PDF.").assertIsDisplayed()
    }

    @Test
    fun `no ready document no note`() {
        show(emptyList())
        rule.onNodeWithTag("lessons-card-pdf").assertDoesNotExist()
    }
}
