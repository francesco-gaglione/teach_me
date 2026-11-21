package com.teachme.sdui.highlight

import android.graphics.Bitmap
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.CalloutVariant
import com.teachme.sdui.model.Paragraph
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/** Draws a highlighted paragraph and callout in both themes: checks the fill and the base line pixels, saves PNGs. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h640dp-xxhdpi")
class HighlightRenderTest {
    @get:Rule val rule = penRule()

    private val paragraph =
        "Quando sposti la variabile con `let b = a;` la prima non è più utilizzabile e il compilatore lo sa bene."
    private val callout =
        "Il valore passa all'altra variabile e la prima smette di valere, quindi non puoi più leggerla."

    private fun slice(
        text: String,
        from: String,
        to: String,
    ) = text.indexOf(from) to text.indexOf(to) + to.length

    private fun render(dark: Boolean) {
        val (ps, pe) = slice(paragraph, "sposti", "compilatore")
        val (cs, ce) = slice(callout, "passa", "prima")
        val host =
            FakeHighlightHost(
                listOf(
                    Highlight("p1", ps, pe, HighlightColor.YELLOW),
                    Highlight("c1", cs, ce, HighlightColor.GREEN),
                ),
            )
        rule.setLesson(
            testDocument(
                listOf(
                    Paragraph("p1", text = paragraph),
                    Callout("c1", variant = CalloutVariant.Tip, title = "Suggerimento", text = callout),
                ),
            ),
            host,
            dark = dark,
        )
        rule.waitForIdle()
        val bitmap = rule.onRoot().captureToImage().asAndroidBitmap()
        val dir = File("build/highlight-screenshots").apply { mkdirs() }
        File(dir, "highlight-${if (dark) "dark" else "light"}.png").outputStream().use {
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
        // The yellow of docs/design/tokens.json, per theme: fill and base line.
        val fill = if (dark) Color(0xFF5E4B00) else Color(0xFFFFE066)
        val line = if (dark) Color(0xFFE5C03A) else Color(0xFF8A6A00)
        // The yellow box of the first line of the paragraph: fill left of the first glyph, base line at the bottom.
        val p = rule.onNodeWithTextContaining("Quando").textOnScreen()
        val box = lineBoxes(p.layout, Interval(ps, pe), 2f * 3, 2f * 3).first()
        val x = (p.origin.x + box.left + 2f).toInt()
        val mid = (p.origin.y + (box.top + box.bottom) / 2).toInt()
        assertEquals(fill.toArgb(), bitmap.getPixel(x, mid))
        val baseY = (p.origin.y + box.bottom - 2f).toInt()
        val baseX = (p.origin.x + (box.left + box.right) / 2).toInt()
        assertEquals(line.toArgb(), bitmap.getPixel(baseX, baseY))
        // Outside the highlight (right of the last glyph of the paragraph) the page colour shows.
        assertNotEquals(fill.toArgb(), bitmap.getPixel(x - 12, mid))
    }

    @Test
    fun `light theme`() = render(dark = false)

    @Test
    fun `dark theme`() = render(dark = true)

    private fun androidx.compose.ui.test.junit4.AndroidComposeTestRule<*, *>.onNodeWithTextContaining(text: String) =
        onNodeWithText(text, substring = true)
}
