package com.teachme.sdui.render

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.unit.Density
import com.teachme.sdui.model.Blank
import com.teachme.sdui.model.FillBlank
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Regression: inline blanks (48dp slot) used to be drawn over the previous text line (fixed 28sp line height).
 * Hard line breaks are used because Robolectric does not wrap text at the container width.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "it-w700dp-h800dp-xxhdpi")
class FillBlankLineSpacingTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val block =
        FillBlank(
            "f1",
            text =
                "Un altro flusso parte con numero iniziale 2000 e viaggia in segmenti da 500 byte.\n" +
                    "Il primo ha seq 2000, il secondo ha seq {{a}} e il terzo ha seq {{b}},\n" +
                    "mentre il quarto ha seq {{c}} e il quinto ha seq {{d}}.",
            blanks = listOf("a", "b", "c", "d").map { Blank(it, listOf("x")) },
        )

    private fun check(fontScale: Float) {
        val base = rule.activity.resources.displayMetrics.density
        rule.setContent {
            CompositionLocalProvider(LocalDensity provides Density(base, fontScale)) {
                PreviewFrame(dark = false) { FillBlankBlock(block) }
            }
        }
        rule.waitForIdle()
        val node =
            rule
                .onNode(hasText("Un altro flusso", substring = true), useUnmergedTree = true)
                .fetchSemanticsNode()
        val results = mutableListOf<TextLayoutResult>()
        node.config
            .getOrNull(SemanticsActions.GetTextLayoutResult)
            ?.action
            ?.invoke(results)
        val layout = results.single()
        val rects = layout.placeholderRects.filterNotNull()
        assertTrue("expected 4 placeholders, got ${rects.size}", rects.size == 4)
        assertTrue("expected several lines, got ${layout.lineCount}", layout.lineCount > 1)
        rects.forEach { r ->
            val line = layout.getLineForVerticalPosition((r.top + r.bottom) / 2)
            val top = layout.getLineTop(line)
            val bottom = layout.getLineBottom(line)
            assertTrue("blank $r pokes above line $line [$top, $bottom] (scale $fontScale)", r.top >= top - 1f)
            assertTrue("blank $r pokes below line $line [$top, $bottom] (scale $fontScale)", r.bottom <= bottom + 1f)
        }
    }

    @Test fun `blanks stay inside their line at default font scale`() = check(1f)

    @Test fun `blanks stay inside their line at 150 percent`() = check(1.5f)

    @Test fun `blanks stay inside their line at 200 percent`() = check(2f)
}
