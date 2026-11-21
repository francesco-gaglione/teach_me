package com.teachme.sdui.highlight

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.CalloutVariant
import com.teachme.sdui.model.Paragraph
import com.teachme.sdui.render.DefaultBlockRegistry
import com.teachme.sdui.render.LocalSdui
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

/** A highlighted paragraph and callout on a real device, light and dark: saves a PNG per theme and checks the colours. */
class HighlightScreenshotInstrumentedTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val paragraph =
        "Quando sposti la variabile con `let b = a;` la prima non è più utilizzabile e il compilatore lo sa bene."
    private val callout =
        "Il valore passa all'altra variabile e la prima smette di valere, quindi non puoi più leggerla."

    private fun shoot(dark: Boolean) {
        val blocks =
            listOf(
                Paragraph("p1", text = paragraph),
                Callout("c1", variant = CalloutVariant.Tip, title = "Suggerimento", text = callout),
            )
        val env = deviceEnvironment(blocks)
        val host =
            StaticHighlights(
                listOf(
                    Highlight(
                        "p1",
                        paragraph.indexOf("sposti"),
                        paragraph.indexOf("compilatore") + 11,
                        HighlightColor.YELLOW,
                    ),
                    Highlight("c1", callout.indexOf("passa"), callout.indexOf("prima") + 5, HighlightColor.GREEN),
                ),
            )
        rule.setContent {
            TeachTheme(darkTheme = dark) {
                CompositionLocalProvider(LocalSdui provides env, LocalHighlightHost provides host) {
                    Column(Modifier.width(320.dp).background(TeachTheme.colors.bg)) {
                        blocks.forEach { DefaultBlockRegistry.Render(it) }
                    }
                }
            }
        }
        rule.waitForIdle()
        val shot =
            rule
                .onRoot()
                .captureToImage()
                .asAndroidBitmap()
                .copy(Bitmap.Config.ARGB_8888, false)
        File(rule.activity.getExternalFilesDir(null), "highlight-${if (dark) "dark" else "light"}.png")
            .outputStream()
            .use { shot.compress(Bitmap.CompressFormat.PNG, 100, it) }
        val fill = if (dark) Color(0xFF5E4B00) else Color(0xFFFFE066)
        val line = if (dark) Color(0xFFE5C03A) else Color(0xFF8A6A00)
        var fills = 0
        var lines = 0
        for (y in 0 until shot.height) {
            for (x in 0 until shot.width) {
                val px = shot.getPixel(x, y)
                if (px == fill.toArgb()) fills++
                if (px == line.toArgb()) lines++
            }
        }
        assertTrue("yellow fill drawn ($fills px)", fills > 2_000)
        assertTrue("yellow base line drawn ($lines px)", lines > 200)
    }

    @Test
    fun light() = shoot(false)

    @Test
    fun dark() = shoot(true)
}
