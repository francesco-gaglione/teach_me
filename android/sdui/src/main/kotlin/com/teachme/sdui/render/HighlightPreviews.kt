package com.teachme.sdui.render

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.sdui.highlight.Highlight
import com.teachme.sdui.highlight.HighlightColor
import com.teachme.sdui.highlight.HighlightHost
import com.teachme.sdui.highlight.LocalHighlightHost
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.CalloutVariant
import com.teachme.sdui.model.Paragraph

private const val PARAGRAPH =
    "In Rust ogni valore ha un **proprietario**. Quando lo sposti con `let b = a;` la prima variabile " +
        "non e piu valida e il compilatore ti impedisce di leggerla, anche se il valore esiste ancora."
private const val CALLOUT_TEXT = "`b = a` non copia: sposta. Dopo lo spostamento la prima variabile smette di valere."

private class PreviewHighlights(
    override val highlights: List<Highlight>,
) : HighlightHost {
    override val color = HighlightColor.YELLOW

    override fun add(
        blockId: String,
        start: Int,
        end: Int,
    ) = Unit

    override fun erase(
        blockId: String,
        start: Int,
        end: Int,
    ) = Unit

    override fun remove(
        blockId: String,
        position: Int,
    ) = Unit
}

private fun span(
    text: String,
    from: String,
    to: String,
) = text.indexOf(from) to text.indexOf(to) + to.length

@Composable
private fun HighlightSample(dark: Boolean) {
    val (ps, pe) = span(PARAGRAPH, "Quando lo sposti", "valida")
    val (ts, te) = span(PARAGRAPH, "leggerla", "esiste")
    val (cs, ce) = span(CALLOUT_TEXT, "sposta", "smette")
    val host =
        PreviewHighlights(
            listOf(
                Highlight("p", ps, pe, HighlightColor.YELLOW),
                Highlight("p", pe, te, HighlightColor.BLUE),
                Highlight("p", te, PARAGRAPH.length - 1, HighlightColor.PINK),
                Highlight("c1", cs, ce, HighlightColor.GREEN),
                Highlight("c2", 0, CALLOUT_TEXT.length - 1, HighlightColor.YELLOW),
            ),
        )
    PreviewFrame(dark) {
        CompositionLocalProvider(LocalHighlightHost provides host) {
            ParagraphBlock(Paragraph("p", text = PARAGRAPH))
            CalloutBlock(Callout("c1", variant = CalloutVariant.Tip, title = "Suggerimento", text = CALLOUT_TEXT))
            CalloutBlock(Callout("c2", variant = CalloutVariant.Warning, text = CALLOUT_TEXT))
            CalloutBlock(Callout("c3", variant = CalloutVariant.Info, text = "Senza evidenziazioni."))
        }
    }
}

@Preview(name = "Highlights light", showBackground = true, heightDp = 700)
@Composable
private fun HighlightsLight() = HighlightSample(false)

@Preview(name = "Highlights dark", showBackground = true, heightDp = 700)
@Composable
private fun HighlightsDark() = HighlightSample(true)

@Preview(name = "Highlights font 2.0", showBackground = true, heightDp = 1400, fontScale = 2f)
@Composable
private fun HighlightsLargeFont() = HighlightSample(false)
