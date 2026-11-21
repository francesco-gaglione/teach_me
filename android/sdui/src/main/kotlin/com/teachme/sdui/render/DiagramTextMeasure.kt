package com.teachme.sdui.render

import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import com.teachme.sdui.diagram.DiagramMeasure
import com.teachme.sdui.diagram.DiagramSpec
import com.teachme.sdui.model.Diagram
import kotlin.math.ceil
import kotlin.math.floor

/** Measures node and edge-label text with the real text styles (font scale comes in through [density]). */
internal class DiagramTextMeasure(
    private val diagram: Diagram,
    private val measurer: TextMeasurer,
    private val density: Density,
    private val labelStyle: TextStyle,
    private val detailStyle: TextStyle,
    private val edgeStyle: TextStyle,
) : DiagramMeasure {
    override val labelMaxWidth: Float = DiagramSpec.LABEL_MAX_WIDTH * maxOf(1f, density.fontScale)

    private fun dp(px: Int): Float = px / density.density

    private fun naturalWidth(
        text: String,
        style: TextStyle,
    ): Float = dp(measurer.measure(text, style).size.width)

    private fun textHeight(
        text: String,
        style: TextStyle,
        width: Float,
    ): Float {
        val px = floor(width * density.density).toInt().coerceAtLeast(1)
        return dp(measurer.measure(text, style, constraints = Constraints(maxWidth = px)).size.height)
    }

    override fun nodeNaturalWidth(node: Int): Float {
        val n = diagram.nodes[node]
        val text =
            maxOf(
                naturalWidth(n.label, labelStyle),
                n.detail?.takeIf { it.isNotBlank() }?.let { naturalWidth(it, detailStyle) } ?: 0f,
            )
        return ceil(text) + 2 * DiagramSpec.NODE_PADDING_X
    }

    override fun nodeHeight(
        node: Int,
        width: Float,
    ): Float {
        val n = diagram.nodes[node]
        val inner = width - 2 * DiagramSpec.NODE_PADDING_X
        var h = textHeight(n.label, labelStyle, inner)
        n.detail?.takeIf { it.isNotBlank() }?.let { h += DiagramSpec.DETAIL_GAP + textHeight(it, detailStyle, inner) }
        return maxOf(DiagramSpec.NODE_MIN_HEIGHT, h + 2 * DiagramSpec.NODE_PADDING_Y)
    }

    private fun labelText(edge: Int) = diagram.edges[edge].label.orEmpty()

    override fun labelNaturalWidth(edge: Int): Float =
        ceil(naturalWidth(labelText(edge), edgeStyle)) + 2 * DiagramSpec.LABEL_PADDING_X

    override fun labelHeight(
        edge: Int,
        width: Float,
    ): Float =
        textHeight(labelText(edge), edgeStyle, width - 2 * DiagramSpec.LABEL_PADDING_X) +
            2 * DiagramSpec.LABEL_PADDING_Y
}
