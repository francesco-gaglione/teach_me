package com.teachme.sdui.diagram

import com.teachme.sdui.model.Diagram

/** Cycle: a two-row ring when wide, a column with a return lane on the right when narrow. */
internal fun planCycle(
    diagram: Diagram,
    width: Float,
    orientation: DiagramOrientation,
    measure: DiagramMeasure,
): DiagramPlan? =
    if (diagram.nodes.size < 2) {
        null
    } else if (orientation == DiagramOrientation.Wide) {
        planRing(diagram.nodes.size, width, measure)
    } else {
        planColumn(diagram.nodes.size, width, measure)
    }

private fun arrow(vararg points: DPoint) = PlacedEdge(points.toList(), arrow = true)

/** Top row left to right with ceil(n/2) nodes, bottom row right to left under the last ones, return on the left. */
private fun planRing(
    count: Int,
    width: Float,
    measure: DiagramMeasure,
): DiagramPlan? {
    val top = (count + 1) / 2
    val bottom = count / 2
    val gap = DiagramSpec.CYCLE_GAP
    val room = (width - gap * (top - 1)) / top
    val w = minOf((0 until count).maxOf { nodeWidth(measure, it, DiagramSpec.NODE_MAX_WIDTH) }, room)
    if (w < DiagramSpec.NODE_MIN_WIDTH - DiagramSpec.EPSILON) return null
    val h = (0 until count).maxOf { measure.nodeHeight(it, w) }
    val rowY = listOf(0f, h + gap)

    fun column(c: Int) = c * (w + gap)

    // Node i: top row for i < top, otherwise the bottom row, first node of the row under the last column.
    val rects =
        List(count) { i ->
            if (i < top) DRect(column(i), rowY[0], w, h) else DRect(column(top - 1 - (i - top)), rowY[1], w, h)
        }
    val tip = DiagramSpec.ARROW_GAP
    val edges = ArrayList<PlacedEdge>()
    for (i in 0 until count - 1) {
        val a = rects[i]
        val b = rects[i + 1]
        edges +=
            when {
                i + 1 < top -> arrow(DPoint(a.right, a.cy), DPoint(b.x - tip, b.cy))
                i + 1 == top -> arrow(DPoint(a.cx, a.bottom), DPoint(b.cx, b.y - tip))
                else -> arrow(DPoint(a.x, a.cy), DPoint(b.right + tip, b.cy))
            }
    }
    // Return: from the last node up to the first; an L when the last node is not under the first one.
    val last = rects.last()
    val first = rects.first()
    edges +=
        if (bottom > 0 && last.cx > first.cx + DiagramSpec.EPSILON) {
            arrow(DPoint(last.x, last.cy), DPoint(first.cx, last.cy), DPoint(first.cx, first.bottom + tip))
        } else {
            arrow(DPoint(last.cx, last.y), DPoint(first.cx, first.bottom + tip))
        }
    return DiagramPlan(
        DiagramOrientation.Wide,
        top * w + (top - 1) * gap,
        2 * h + gap,
        rects.mapIndexed { i, r -> PlacedNode(i, r, order = i + 1) },
        edges,
        emptyList(),
    )
}

/** Stacked nodes of width W - 32, down arrows, and a line on the right from the last node back to the first. */
private fun planColumn(
    count: Int,
    width: Float,
    measure: DiagramMeasure,
): DiagramPlan? {
    val w = width - DiagramSpec.CYCLE_GAP
    if (w < DiagramSpec.NODE_MIN_WIDTH - DiagramSpec.EPSILON) return null
    val rects = ArrayList<DRect>()
    var y = 0f
    for (i in 0 until count) {
        val h = measure.nodeHeight(i, w)
        rects += DRect(0f, y, w, h)
        y += h + DiagramSpec.CYCLE_GAP
    }
    val tip = DiagramSpec.ARROW_GAP
    val edges = ArrayList<PlacedEdge>()
    for (i in 0 until count - 1) {
        edges += arrow(DPoint(rects[i].cx, rects[i].bottom), DPoint(rects[i + 1].cx, rects[i + 1].y - tip))
    }
    val lane = w + DiagramSpec.CYCLE_LANE
    val first = rects.first()
    val last = rects.last()
    edges +=
        arrow(
            DPoint(last.right, last.cy),
            DPoint(lane, last.cy),
            DPoint(lane, first.cy),
            DPoint(first.right + tip, first.cy),
        )
    val icon = DiagramSpec.ICON
    return DiagramPlan(
        DiagramOrientation.Narrow,
        width,
        y - DiagramSpec.CYCLE_GAP,
        rects.mapIndexed { i, r -> PlacedNode(i, r, order = i + 1) },
        edges,
        emptyList(),
        returnIcon = DRect(lane - icon / 2, (first.cy + last.cy) / 2 - icon / 2, icon, icon),
    )
}
