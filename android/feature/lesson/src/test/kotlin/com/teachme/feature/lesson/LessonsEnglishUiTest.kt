package com.teachme.feature.lesson

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
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
@Config(sdk = [34], qualifiers = "en-w360dp-h900dp-xxhdpi")
class LessonsEnglishUiTest {
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
    fun `english resources are used under the en locale`() {
        show(listOf("Rust in Action"))
        rule
            .onNodeWithText(
                "I'm also following your PDF “Rust in Action”: I choose which pages, starting from your mission.",
            ).assertIsDisplayed()
    }

    @Test
    fun `several ready documents use the generic english note`() {
        show(listOf("A", "B"))
        rule.onNodeWithText("I'm also following your PDFs.").assertIsDisplayed()
    }
}
