package com.teachme.feature.lesson

import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.sdui.ink.BlockGeometry
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkEdit
import com.teachme.sdui.ink.InkWidth
import com.teachme.sdui.ink.Stroke
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** Draws saved strokes in both themes and at two block widths: checks the colour of the ink and the scale. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "it-w360dp-h640dp-xxhdpi")
class InkRenderTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    // A line along the block's top edge from x = 0.2 to x = 0.8 of the width, one point in the middle of the block.
    private val line = Stroke("s1", "p1", InkColor.RED, InkWidth.THICK, listOf(2_000, 1_000, 8_000, 1_000))
    private val dot = Stroke("s2", "p1", InkColor.BLUE, InkWidth.THICK, listOf(5_000, 5_000))

    private fun draw(
        dark: Boolean,
        blockWidthPx: Int,
        strokes: List<Stroke> = listOf(line, dot),
    ): android.graphics.Bitmap {
        val surface = InkSurface({ InkEdit.NoOp }, { InkEdit.NoOp })
        surface.strokes = strokes
        surface.layout =
            { InkLayout(mapOf("p1" to BlockGeometry(left = 30f, top = 40f, width = blockWidthPx.toFloat()))) }
        rule.setContent {
            val density = LocalDensity.current
            TeachTheme(darkTheme = dark) {
                Box(Modifier.size(with(density) { 1000.toDp() }, 400.dp).background(TeachTheme.colors.bg)) {
                    InkLayers(surface)
                }
            }
        }
        rule.waitForIdle()
        return rule.onRoot().captureToImage().asAndroidBitmap()
    }

    private fun Int.argb(c: Color) = assertEquals(c.toArgb(), this)

    @Test
    fun `light theme draws the ink roles with the light values`() {
        val bitmap = draw(dark = false, blockWidthPx = 600)
        // y = 40 + 0.1 * 600 = 100: the line from x = 30 + 120 to 30 + 480.
        bitmap.getPixel(300, 100).argb(Color(0xFFB02E2E))
        // The dot: centre at (30 + 300, 40 + 300).
        bitmap.getPixel(330, 340).argb(Color(0xFF1F4FB0))
        assertNotEquals(Color(0xFFB02E2E).toArgb(), bitmap.getPixel(100, 100))
    }

    @Test
    fun `dark theme draws the same roles with the dark values`() {
        val bitmap = draw(dark = true, blockWidthPx = 600)
        bitmap.getPixel(300, 100).argb(Color(0xFFFF9E9E))
        bitmap.getPixel(330, 340).argb(Color(0xFF8FB4FF))
    }

    @Test
    fun `the same stroke at a narrower block is smaller in shape and as thick as before`() {
        // At 300 px the line is x 120..?: from 30 + 60 to 30 + 240 at y = 40 + 30 = 70, and the dot is at (180, 190).
        val bitmap = draw(dark = false, blockWidthPx = 300)
        val red = Color(0xFFB02E2E)
        bitmap.getPixel(200, 70).argb(red)
        assertNotEquals("the wide line is not here any more", red.toArgb(), bitmap.getPixel(400, 100))
        bitmap.getPixel(180, 190).argb(Color(0xFF1F4FB0))
        // 4 dp at 3 px per dp = 12 px thick, whatever the scale: 4 px above the centre is ink, 9 px is not.
        bitmap.getPixel(200, 70 - 4).argb(red)
        assertNotEquals(red.toArgb(), bitmap.getPixel(200, 70 - 9))
    }

    @Test
    fun `a wide block keeps the same line thickness`() {
        val bitmap = draw(dark = false, blockWidthPx = 600)
        val red = Color(0xFFB02E2E)
        bitmap.getPixel(400, 100).argb(red)
        bitmap.getPixel(300, 100 - 4).argb(red)
        assertNotEquals(red.toArgb(), bitmap.getPixel(300, 100 - 9))
    }

    @Test
    fun `a stroke outside the visible area is not drawn`() {
        val surface = InkSurface({ InkEdit.NoOp }, { InkEdit.NoOp })
        surface.strokes =
            listOf(line, Stroke("far", "p2", InkColor.RED, InkWidth.THICK, listOf(2_000, 1_000, 8_000, 1_000)))
        surface.layout =
            {
                InkLayout(
                    mapOf(
                        "p1" to BlockGeometry(left = 30f, top = 40f, width = 600f),
                        "p2" to BlockGeometry(left = 30f, top = 5_000f, width = 600f),
                    ),
                )
            }
        rule.setContent {
            val density = LocalDensity.current
            TeachTheme(darkTheme = false) {
                Box(Modifier.size(with(density) { 1000.toDp() }, 400.dp)) { InkLayers(surface) }
            }
        }
        rule.waitForIdle()
        rule.onRoot().captureToImage() // forces a frame to be drawn
        assertEquals(1, surface.drawnCount)
    }

    @Test
    fun `the path cache does not rebuild after a refresh with equal points`() {
        val cache = InkPathCache()
        val first = cache.pathOf(line, 600f)
        val refreshed = line.copy(points = line.points.toList())
        assertEquals(true, cache.pathOf(refreshed, 600f) === first)
        assertEquals(true, cache.pathOf(refreshed, 600f) === first)
        assertEquals(1, cache.builds)
        cache.pathOf(refreshed, 300f)
        assertEquals(2, cache.builds)
    }
}
