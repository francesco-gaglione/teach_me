package com.teachme.feature.mission

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.model.TopicMission
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h900dp-xxhdpi")
class MissionPdfLineUiTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val state =
        MissionUiState(
            load = MissionLoad.Ready,
            mission = TopicMission(PreviewData.confirmed1, null),
            prefsLoad = PreferencesLoad.Ready,
        )

    private fun show(
        pdfTitles: List<String>,
        onOpenResources: () -> Unit = {},
    ) {
        val actions =
            MissionTabActions({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, onOpenResources = onOpenResources)
        rule.setContent {
            TeachTheme { MissionTab("Rust", state, false, actions, pdfTitles = pdfTitles) }
        }
    }

    @Test
    fun `line under the mission appears with a ready document and opens the sources`() {
        var opened = 0
        show(listOf("Rust in Action")) { opened++ }
        rule.onNodeWithTag("mission-pdf-line").assertIsDisplayed()
        rule.onNodeWithText("Studi dal tuo PDF «Rust in Action».").assertIsDisplayed()
        rule.onNodeWithText("Vai ai documenti").performClick()
        assertEquals(1, opened)
    }

    @Test
    fun `line is absent without a ready document`() {
        show(emptyList())
        rule.onNodeWithTag("mission-pdf-line").assertDoesNotExist()
    }
}
