package com.teachme.sdui.highlight

import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.text.TextLayoutResult
import com.teachme.core.designsystem.TeachColors
import com.teachme.core.designsystem.TeachHighlight

/** The `highlight*` colours of the design system for a [HighlightColor]. */
internal fun HighlightColor.fill(colors: TeachColors): Color =
    when (this) {
        HighlightColor.YELLOW -> colors.highlightYellow
        HighlightColor.GREEN -> colors.highlightGreen
        HighlightColor.BLUE -> colors.highlightBlue
        HighlightColor.PINK -> colors.highlightPink
    }

internal fun HighlightColor.line(colors: TeachColors): Color =
    when (this) {
        HighlightColor.YELLOW -> colors.highlightYellowLine
        HighlightColor.GREEN -> colors.highlightGreenLine
        HighlightColor.BLUE -> colors.highlightBlueLine
        HighlightColor.PINK -> colors.highlightPinkLine
    }

/** What a pen stroke shows while the tip is down: the words it would colour, or the ones the eraser touched. */
internal class StrokeUi(
    val erase: Boolean,
    val runs: List<Interval>,
    val color: HighlightColor,
)

/** The rectangle of one line fragment of a highlight, in pixels of the text node. */
internal class LineBox(
    val left: Float,
    val top: Float,
    val right: Float,
    val bottom: Float,
    val roundLeft: Boolean,
    val roundRight: Boolean,
)

internal class LayoutGeometry(
    private val layout: TextLayoutResult,
) : TextGeometry {
    override val lineCount: Int get() = layout.lineCount

    override fun lineTop(line: Int) = layout.getLineTop(line)

    override fun lineBottom(line: Int) = layout.getLineBottom(line)

    override fun lineStart(line: Int) = layout.getLineStart(line)

    override fun lineVisibleEnd(line: Int) = layout.getLineEnd(line, visibleEnd = true)

    override fun charLeft(char: Int) = layout.getBoundingBox(char).left

    override fun charRight(char: Int) = layout.getBoundingBox(char).right
}

/**
 * One box per line fragment of the displayed [run] (highlight.md section 3.1): the line box without [insetY] at the
 * top and bottom, from the first glyph to the last one, [insetX] wider each side and clipped to the text. A run that
 * goes on to the next line covers the lines in between margin to margin. Only the ends of the run are rounded.
 */
internal fun lineBoxes(
    layout: TextLayoutResult,
    run: Interval,
    insetX: Float,
    insetY: Float,
    roundStart: Boolean = true,
    roundEnd: Boolean = true,
): List<LineBox> {
    val length = layout.layoutInput.text.length
    if (run.isEmpty || run.start >= length) return emptyList()
    val width = layout.size.width.toFloat()
    val last = minOf(run.end, length) - 1
    val firstLine = layout.getLineForOffset(run.start)
    val lastLine = layout.getLineForOffset(last)
    val out = ArrayList<LineBox>()
    for (line in firstLine..lastLine) {
        val s = maxOf(run.start, layout.getLineStart(line))
        val e = minOf(run.end, layout.getLineEnd(line, visibleEnd = true))
        if (e <= s) continue
        val left = if (line == firstLine) layout.getBoundingBox(s).left else 0f
        val right =
            when {
                line == lastLine -> layout.getBoundingBox(e - 1).right
                line == firstLine -> layout.getBoundingBox(e - 1).right
                else -> width
            }
        out +=
            LineBox(
                left = (left - insetX).coerceAtLeast(0f),
                top = layout.getLineTop(line) + insetY,
                right = (right + insetX).coerceAtMost(width),
                bottom = layout.getLineBottom(line) - insetY,
                roundLeft = roundStart && line == firstLine,
                roundRight = roundEnd && line == lastLine,
            )
    }
    return out
}

private fun boxPath(
    box: LineBox,
    radius: Float,
): Path {
    val none = CornerRadius.Zero
    val round = CornerRadius(radius)
    return Path().apply {
        addRoundRect(
            RoundRect(
                left = box.left,
                top = box.top,
                right = box.right,
                bottom = box.bottom,
                topLeftCornerRadius = if (box.roundLeft) round else none,
                topRightCornerRadius = if (box.roundRight) round else none,
                bottomRightCornerRadius = if (box.roundRight) round else none,
                bottomLeftCornerRadius = if (box.roundLeft) round else none,
            ),
        )
    }
}

/** Splits [run] into the parts outside and inside of [cuts] (sorted or not), in order. */
internal fun splitRun(
    run: Interval,
    cuts: List<Interval>,
): List<Pair<Interval, Boolean>> {
    val out = ArrayList<Pair<Interval, Boolean>>()
    var at = run.start
    for (cut in cuts.sortedBy { it.start }) {
        val s = maxOf(cut.start, run.start)
        val e = minOf(cut.end, run.end)
        if (e <= s || e <= at) continue
        if (s > at) out += Interval(at, s) to false
        out += Interval(maxOf(s, at), e) to true
        at = e
    }
    if (at < run.end) out += Interval(at, run.end) to false
    return out
}

/** The layer of a text block: the highlights, the stroke in progress and the selected highlight. */
internal class HighlightPaint(
    val layout: TextLayoutResult,
    val map: SourceMap,
    val colors: TeachColors,
    val own: List<Highlight>,
    val stroke: StrokeUi?,
    val selected: Highlight?,
)

internal fun DrawScope.drawHighlightLayer(paint: HighlightPaint) {
    val layout = paint.layout
    if (layout.layoutInput.text.length != paint.map.displayLength) return
    val insetX = TeachHighlight.boxInsetX.toPx()
    val insetY = TeachHighlight.boxInsetY.toPx()
    val radius = TeachHighlight.cornerRadius.toPx()
    val lineWidth = TeachHighlight.lineWidth.toPx()
    val gap = (TeachHighlight.gapBetweenColors / 2).toPx()

    val ghosts =
        paint.stroke
            ?.takeIf { it.erase }
            ?.runs
            .orEmpty()
    for (h in paint.own) {
        val runs = paint.map.sourceToDisplay(h.interval)
        if (runs.isEmpty()) continue
        val fill = h.color.fill(paint.colors)
        val baseline = h.color.line(paint.colors)
        val touchesBefore = paint.own.any { it.color != h.color && it.end == h.start }
        val touchesAfter = paint.own.any { it.color != h.color && it.start == h.end }
        for (run in runs) {
            for ((piece, ghost) in splitRun(run, ghosts)) {
                val boxes =
                    lineBoxes(
                        layout,
                        piece,
                        insetX,
                        insetY,
                        roundStart = piece.start == runs.first().start,
                        roundEnd = piece.end == runs.last().end,
                    )
                for ((i, raw) in boxes.withIndex()) {
                    val box =
                        LineBox(
                            raw.left + if (touchesBefore && i == 0 && raw.roundLeft) gap else 0f,
                            raw.top,
                            raw.right - if (touchesAfter && i == boxes.lastIndex && raw.roundRight) gap else 0f,
                            raw.bottom,
                            raw.roundLeft,
                            raw.roundRight,
                        )
                    val path = boxPath(box, radius)
                    if (ghost) {
                        drawPath(path, fill, alpha = TeachHighlight.eraseGhostAlpha)
                    } else {
                        drawPath(path, fill)
                        clipPath(path) {
                            drawRect(
                                baseline,
                                Offset(box.left, box.bottom - lineWidth),
                                Size(box.right - box.left, lineWidth),
                            )
                        }
                    }
                }
            }
        }
    }

    val stroke = paint.stroke
    if (stroke != null && !stroke.erase) {
        for (run in stroke.runs) {
            for (box in lineBoxes(layout, run, insetX, insetY)) {
                drawPath(boxPath(box, radius), stroke.color.fill(paint.colors), alpha = TeachHighlight.previewAlpha)
            }
        }
    }

    val selected = paint.selected
    if (selected != null) {
        val runs = paint.map.sourceToDisplay(selected.interval)
        for (run in runs) {
            for (box in lineBoxes(layout, run, insetX, insetY)) {
                drawPath(
                    boxPath(box, radius),
                    paint.colors.focus,
                    style = Stroke(TeachHighlight.popoverFocusStroke.toPx()),
                )
            }
        }
    }
}
