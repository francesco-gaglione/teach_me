package com.teachme.sdui.highlight

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.CalloutVariant
import com.teachme.sdui.model.LessonDocument
import com.teachme.sdui.model.Paragraph
import com.teachme.sdui.render.DefaultBlockRegistry
import com.teachme.sdui.render.LocalSdui
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import java.io.File

/**
 * On a real device: a block with inline code looks the same with the highlighter provided (and nothing highlighted) as
 * without it (highlight.md section 2). The highlighter draws the code background itself, so this compares the pixels.
 */
class CodeBackgroundInstrumentedTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val blocks =
        listOf(
            Paragraph(
                "p1",
                text =
                    "Quando sposti la variabile con `let b = a;` la prima non è più utilizzabile, perché `a` è spostata.",
            ),
            Callout(
                "c1",
                variant = CalloutVariant.Info,
                text = "Con `let mut contatore = 0;` il valore resta modificabile finché `contatore` è nello scope.",
            ),
        )

    private fun check(dark: Boolean) {
        val document = LessonDocument(1, "T", "O", 5, emptyList(), emptyList(), blocks)
        val env = deviceEnvironment(blocks)
        val height = 300.dp
        rule.setContent {
            TeachTheme(darkTheme = dark) {
                CompositionLocalProvider(LocalSdui provides env) {
                    Column(Modifier.width(320.dp).background(TeachTheme.colors.bg)) {
                        Column(Modifier.height(height)) { document.blocks.forEach { DefaultBlockRegistry.Render(it) } }
                        CompositionLocalProvider(LocalHighlightHost provides StaticHighlights()) {
                            Column(
                                Modifier.height(height),
                            ) { document.blocks.forEach { DefaultBlockRegistry.Render(it) } }
                        }
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
        val out = File(rule.activity.getExternalFilesDir(null), "code-${if (dark) "dark" else "light"}.png")
        out.outputStream().use { shot.compress(Bitmap.CompressFormat.PNG, 100, it) }
        val half = with(rule.density) { height.roundToPx() }
        var different = 0
        for (y in 0 until half) {
            for (x in 0 until shot.width) {
                if (shot.getPixel(x, y) !=
                    shot.getPixel(x, y + half)
                ) {
                    different++
                }
            }
        }
        assertEquals("pixels that differ (${out.absolutePath})", 0, different)
    }

    @Test
    fun light() = check(false)

    @Test
    fun dark() = check(true)
}
