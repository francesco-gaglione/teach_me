package com.teachme.sdui.diagram

import com.teachme.sdui.model.Diagram
import kotlin.math.abs

/** Level of each node: length of the longest path from a source (sources are level 0). Cycle-safe. */
internal fun flowLevels(
    count: Int,
    edges: List<DiagramEdgeRef>,
): IntArray {
    val level = IntArray(count)
    repeat(count) {
        var changed = false
        for (e in edges) {
            if (level[e.from] + 1 > level[e.to] && level[e.from] + 1 < count) {
                level[e.to] = level[e.from] + 1
                changed = true
            }
        }
        if (!changed) return level
    }
    return level
}

/** Number of crossing pairs among the edges that join consecutive levels. */
internal fun flowCrossings(
    levels: List<List<Int>>,
    edges: List<DiagramEdgeRef>,
): Int {
    val level = HashMap<Int, Int>()
    val position = HashMap<Int, Int>()
    levels.forEachIndexed { l, nodes ->
        nodes.forEachIndexed { p, n ->
            level[n] = l
            position[n] = p
        }
    }
    val adjacent = edges.filter { level.getValue(it.to) - level.getValue(it.from) == 1 }
    var crossings = 0
    for (i in adjacent.indices) {
        for (j in i + 1 until adjacent.size) {
            val a = adjacent[i]
            val b = adjacent[j]
            if (level[a.from] != level[b.from]) continue
            val before = position.getValue(a.from) - position.getValue(b.from)
            val after = position.getValue(a.to) - position.getValue(b.to)
            if (before.toLong() * after < 0) crossings++
        }
    }
    return crossings
}

/**
 * Nodes per level, in the order of `nodes` and reordered (barycenter of the predecessors) only when that
 * strictly reduces the crossings.
 */
internal fun orderedLevels(
    level: IntArray,
    edges: List<DiagramEdgeRef>,
): List<List<Int>> {
    val depth = (level.maxOrNull() ?: 0) + 1
    val original = List(depth) { d -> level.indices.filter { level[it] == d } }
    val sorted = original.toMutableList()
    for (d in 1 until depth) {
        val position = HashMap<Int, Int>()
        sorted.forEach { nodes -> nodes.forEachIndexed { p, n -> position[n] = p } }
        sorted[d] =
            sorted[d].sortedBy { n ->
                edges
                    .filter { it.to == n }
                    .map { position.getValue(it.from) }
                    .average()
                    .let { if (it.isNaN()) 0.0 else it }
            }
    }
    return if (flowCrossings(sorted, edges) < flowCrossings(original, edges)) sorted else original
}

/** A node in the abstract frame: `main` runs along the levels, `cross` across them. */
private class Slot(
    val main0: Float,
    val mainLen: Float,
    val cross0: Float,
    val crossLen: Float,
) {
    val mainEnd get() = main0 + mainLen
    val crossCenter get() = cross0 + crossLen / 2
}

/** Abstract (main, cross) to (x, y): main is x for the wide orientation, y for the narrow one. */
internal class Axis(
    private val wide: Boolean,
) {
    fun point(
        main: Float,
        cross: Float,
    ) = if (wide) DPoint(main, cross) else DPoint(cross, main)

    fun rect(
        main: Float,
        cross: Float,
        mainLen: Float,
        crossLen: Float,
    ) = if (wide) DRect(main, cross, mainLen, crossLen) else DRect(cross, main, crossLen, mainLen)
}

private class Lane(
    val edge: DiagramEdgeRef,
    val extent: Float,
    val labelWidth: Float,
    val labelHeight: Float,
)

internal fun planFlow(
    diagram: Diagram,
    edges: List<DiagramEdgeRef>,
    width: Float,
    orientation: DiagramOrientation,
    measure: DiagramMeasure,
): DiagramPlan? {
    val count = diagram.nodes.size
    val level = flowLevels(count, edges)
    val levels = orderedLevels(level, edges)
    val skips = edges.filter { level[it.to] - level[it.from] > 1 }
    if (skips.size > DiagramSpec.MAX_LANES) return null
    val wide = orientation == DiagramOrientation.Wide
    val axis = Axis(wide)

    // Skip lanes: along the figure, outside the nodes (below when wide, to the right when narrow).
    val lanes =
        skips.map { e ->
            if (!e.hasLabel) {
                Lane(e, 0f, 0f, 0f)
            } else {
                val (w, h) = labelSize(measure, e.index, if (wide) LANE_LABEL_WIDE else measure.labelMaxWidth)
                Lane(e, if (wide) h else w, w, h)
            }
        }
    val laneExtra = lanes.sumOf { (DiagramSpec.LANE + it.extent).toDouble() }.toFloat()

    val slots = arrayOfNulls<Slot>(count)
    var stack: Map<Int, Float> = emptyMap()
    val (mainTotal, crossBody) =
        if (wide) {
            wideColumns(levels, edges, level, width, measure, slots) ?: return null
        } else {
            val rows = narrowRows(levels, edges, level, width - laneExtra, laneExtra, measure, slots) ?: return null
            stack = rows.stack
            rows.main to rows.cross
        }
    val placed = slots.map { it ?: return null }
    val crossTotal = crossBody + laneExtra
    val gapLabels = ArrayList<PlacedLabel>()
    val paths = ArrayList<Pair<Int, PlacedEdge>>()

    // Elbow edges of a gap leave their source on separate "bus" positions so branches stay readable.
    val busIndex = HashMap<Int, Int>()
    val perGap = edges.filter { level[it.to] - level[it.from] == 1 }.groupBy { level[it.from] }
    for (group in perGap.values) {
        group
            .filter { abs(placed[it.from].crossCenter - placed[it.to].crossCenter) >= DiagramSpec.EPSILON }
            .map { it.from }
            .distinct()
            .sortedBy { placed[it].crossCenter }
            .forEachIndexed { i, src -> busIndex[src] = i }
    }
    // Labels of a wide gap start after the last bus, so a pill never covers the vertical stretch of a branch.
    val labelFrom = HashMap<Int, Float>()
    for ((l, group) in perGap) {
        val first = placed[group.first().from].mainEnd
        val buses = group.mapNotNull { busIndex[it.from] }
        labelFrom[l] =
            if (buses.isEmpty()) first else first + DiagramSpec.BUS_FIRST + DiagramSpec.BUS_STEP * buses.max()
    }
    val labelsIntoTarget = HashMap<Int, Int>()
    for (e in edges) {
        val s = placed[e.from]
        val t = placed[e.to]
        val sc = s.crossCenter
        val tc = t.crossCenter
        val tip = t.main0 - DiagramSpec.ARROW_GAP
        if (level[e.to] - level[e.from] == 1) {
            val straight = abs(sc - tc) < DiagramSpec.EPSILON
            val bus =
                (s.mainEnd + DiagramSpec.BUS_FIRST + DiagramSpec.BUS_STEP * (busIndex[e.from] ?: 0))
                    .coerceAtMost(tip - DiagramSpec.ARROW_HEAD - DiagramSpec.ARROW_GAP)
            val points =
                if (straight) {
                    listOf(axis.point(s.mainEnd, sc), axis.point(tip, tc))
                } else {
                    listOf(
                        axis.point(s.mainEnd, sc),
                        axis.point(bus, sc),
                        axis.point(bus, tc),
                        axis.point(tip, tc),
                    )
                }
            paths += e.index to PlacedEdge(points, arrow = true)
            if (e.hasLabel) {
                val from = if (straight) s.mainEnd else bus
                gapLabels +=
                    if (wide) {
                        val slot = labelsIntoTarget.merge(e.to, 1, Int::plus)!! - 1
                        wideLabel(e, measure, labelFrom.getValue(level[e.from]), tip, tc, crossBody, slot)
                    } else {
                        narrowLabel(e, measure, from, tip, tc, sc, t.main0, crossTotal, stack[e.index] ?: 0f)
                    }
            }
        } else {
            val lane = lanes.first { it.edge === e }
            val laneCross = lanes.takeWhile { it !== lane }.sumOf { (DiagramSpec.LANE + it.extent).toDouble() }
            val laneC = crossBody + laneCross.toFloat() + DiagramSpec.LANE + lane.extent / 2
            val exit = s.mainEnd + DiagramSpec.LANE_EXIT
            val entry = t.main0 - DiagramSpec.LANE_ENTRY
            val points =
                listOf(
                    axis.point(s.mainEnd, sc),
                    axis.point(exit, sc),
                    axis.point(exit, laneC),
                    axis.point(entry, laneC),
                    axis.point(entry, tc),
                    axis.point(tip, tc),
                )
            paths += e.index to PlacedEdge(points, arrow = true)
            if (e.hasLabel) {
                val mid = (exit + entry) / 2
                val (w, h) = lane.labelWidth to lane.labelHeight
                val rect =
                    if (wide) {
                        DRect(
                            mid - w / 2,
                            laneC - h / 2,
                            w,
                            h,
                        )
                    } else {
                        DRect(laneC - w / 2, mid - h / 2, w, h)
                    }
                gapLabels += PlacedLabel(e.index, rect)
            }
        }
    }
    val nodes = placed.mapIndexed { i, s -> PlacedNode(i, axis.rect(s.main0, s.cross0, s.mainLen, s.crossLen)) }
    return DiagramPlan(
        orientation = orientation,
        width = if (wide) mainTotal else crossTotal,
        height = if (wide) crossTotal else mainTotal,
        nodes = nodes,
        edges = paths.sortedBy { it.first }.map { it.second },
        labels = gapLabels,
    )
}

private const val LANE_LABEL_WIDE = 160f

/** Narrow gap: the label sits beside the arrow into the target, under the bus line of its branch. */
private fun narrowLabel(
    e: DiagramEdgeRef,
    measure: DiagramMeasure,
    from: Float,
    tip: Float,
    targetCross: Float,
    sourceCross: Float,
    gapEnd: Float,
    crossTotal: Float,
    stack: Float,
): PlacedLabel {
    val (w, h) = labelSize(measure, e.index, measure.labelMaxWidth)
    val room = DiagramSpec.LABEL_PADDING_Y * 2
    val top = from + room
    val y = (top + stack).coerceAtMost(maxOf(top, gapEnd - room - h))
    val x = narrowLabelX(sourceCross, targetCross, w, crossTotal)
    return PlacedLabel(e.index, DRect(x, y, w, h))
}

/**
 * Wide gap: the label sits in the free stretch between the last bus and the arrow head, above the arrow into its
 * target, then below it, then further out, so labels of arrows that enter the same node never overlap.
 */
private fun wideLabel(
    e: DiagramEdgeRef,
    measure: DiagramMeasure,
    from: Float,
    tip: Float,
    targetCross: Float,
    crossBody: Float,
    slot: Int,
): PlacedLabel {
    val maxW = (tip - from - DiagramSpec.LABEL_OFFSET).coerceIn(MIN_LABEL_WIDTH, measure.labelMaxWidth)
    val (w, h) = labelSize(measure, e.index, maxW)
    val pad = DiagramSpec.LABEL_PADDING_Y
    val tops =
        (0..slot + 1).flatMap { k ->
            listOf(targetCross - pad - h - k * (h + pad), targetCross + pad + k * (h + pad))
        }
    val fitting = tops.filter { it >= 0f && it + h <= crossBody + 0.01f }
    val top = fitting.getOrElse(slot) { tops[slot.coerceAtMost(tops.lastIndex)].coerceAtLeast(0f) }
    val x = (from + tip) / 2 - w / 2
    return PlacedLabel(e.index, DRect(x, top, w, h))
}

private const val MIN_LABEL_WIDTH = 32f

/** Beside the arrow into the target, on the side it comes from; the other side when there is no room. */
private fun narrowLabelX(
    sourceCross: Float,
    targetCross: Float,
    w: Float,
    total: Float,
): Float {
    var x =
        if (sourceCross <= targetCross) {
            targetCross + DiagramSpec.LABEL_OFFSET
        } else {
            targetCross - DiagramSpec.LABEL_OFFSET - w
        }
    if (x + w > total) x = targetCross - DiagramSpec.LABEL_OFFSET - w
    if (x < 0f) x = targetCross + DiagramSpec.LABEL_OFFSET
    return x
}

/** Result of [narrowRows]: sizes plus, for labels that would collide, how far each is pushed down. */
private class NarrowResult(
    val main: Float,
    val cross: Float,
    val stack: Map<Int, Float>,
)

/** Wide flow: one column per level, nodes stacked; fills [slots] and returns (width, height) or null. */
private fun wideColumns(
    levels: List<List<Int>>,
    edges: List<DiagramEdgeRef>,
    level: IntArray,
    width: Float,
    measure: DiagramMeasure,
    slots: Array<Slot?>,
): Pair<Float, Float>? {
    val gaps =
        List((levels.size - 1).coerceAtLeast(0)) { l ->
            val labelled = edges.any { level[it.from] == l && level[it.to] == l + 1 && it.hasLabel }
            if (labelled) DiagramSpec.FLOW_GAP_LABEL else DiagramSpec.FLOW_GAP
        }
    // Natural width of the text first, then wrapped at the minimum width when the natural one does not fit.
    for (cap in listOf(DiagramSpec.NODE_WIDE_CAP, DiagramSpec.NODE_MIN_WIDTH)) {
        val columnWidth = levels.map { nodes -> nodes.maxOf { nodeWidth(measure, it, cap) } }
        val total = columnWidth.sum() + gaps.sum()
        if (total > width + DiagramSpec.EPSILON) continue
        val heights = HashMap<Int, Float>()
        levels.forEachIndexed { l, nodes -> nodes.forEach { heights[it] = measure.nodeHeight(it, columnWidth[l]) } }
        val columnHeight =
            levels.map { nodes ->
                nodes.sumOf { heights.getValue(it).toDouble() }.toFloat() +
                    DiagramSpec.FLOW_STACK_GAP * (nodes.size - 1)
            }
        val tallest = columnHeight.max()
        var main = 0f
        levels.forEachIndexed { l, nodes ->
            var cross = (tallest - columnHeight[l]) / 2
            for (n in nodes) {
                val h = heights.getValue(n)
                slots[n] = Slot(main, columnWidth[l], cross, h)
                cross += h + DiagramSpec.FLOW_STACK_GAP
            }
            main += columnWidth[l] + gaps.getOrElse(l) { 0f }
        }
        return total to tallest
    }
    return null
}

/** Narrow flow: one row per level, nodes side by side with equal width; null when a row does not fit. */
private fun narrowRows(
    levels: List<List<Int>>,
    edges: List<DiagramEdgeRef>,
    level: IntArray,
    width: Float,
    laneExtra: Float,
    measure: DiagramMeasure,
    slots: Array<Slot?>,
): NarrowResult? {
    val rowWidth =
        levels.map { nodes ->
            val count = nodes.size
            val room = (width - DiagramSpec.FLOW_SIDE_GAP * (count - 1)) / count
            minOf(nodes.maxOf { nodeWidth(measure, it, DiagramSpec.NODE_MAX_WIDTH) }, room)
        }
    if (rowWidth.any { it < DiagramSpec.NODE_MIN_WIDTH - DiagramSpec.EPSILON }) return null
    val rowCross =
        levels.mapIndexed { l, nodes ->
            rowWidth[l] * nodes.size +
                DiagramSpec.FLOW_SIDE_GAP * (nodes.size - 1)
        }
    val body = rowCross.max()
    val rowHeight = levels.mapIndexed { l, nodes -> nodes.maxOf { measure.nodeHeight(it, rowWidth[l]) } }
    val center = HashMap<Int, Float>()
    levels.forEachIndexed { l, nodes ->
        nodes.forEachIndexed { i, n ->
            center[n] = (body - rowCross[l]) / 2 + i * (rowWidth[l] + DiagramSpec.FLOW_SIDE_GAP) + rowWidth[l] / 2
        }
    }
    val stack = HashMap<Int, Float>()
    var main = 0f
    levels.forEachIndexed { l, nodes ->
        var cross = (body - rowCross[l]) / 2
        for (n in nodes) {
            slots[n] = Slot(main, rowHeight[l], cross, rowWidth[l])
            cross += rowWidth[l] + DiagramSpec.FLOW_SIDE_GAP
        }
        val labelled = edges.filter { level[it.from] == l && level[it.to] == l + 1 && it.hasLabel }
        // Labels of a gap are pushed down when two would overlap sideways: the gap grows to hold the stack.
        val placed = ArrayList<Triple<Float, Float, Float>>() // x, right, bottom offset
        var stackHeight = 0f
        for (e in labelled.sortedBy { center.getValue(it.to) }) {
            val (w, h) = labelSize(measure, e.index, measure.labelMaxWidth)
            val x = narrowLabelX(center.getValue(e.from), center.getValue(e.to), w, body + laneExtra)
            val offset =
                placed
                    .filter { it.first < x + w && x < it.second }
                    .maxOfOrNull { it.third + DiagramSpec.LABEL_PADDING_Y } ?: 0f
            stack[e.index] = offset
            placed += Triple(x, x + w, offset + h)
            stackHeight = maxOf(stackHeight, offset + h)
        }
        val labelHeight = stackHeight
        // The label sits under the horizontal bus of its branch: room for the farthest bus plus the label.
        val sources =
            edges
                .filter { level[it.from] == l && level[it.to] == l + 1 }
                .map { it.from }
                .distinct()
                .size
        val bus = DiagramSpec.BUS_FIRST + DiagramSpec.BUS_STEP * (sources - 1).coerceAtLeast(0)
        val needed = if (labelled.isEmpty()) 0f else bus + labelHeight + DiagramSpec.LABEL_PADDING_Y * 4
        val gap = maxOf(DiagramSpec.FLOW_LEVEL_GAP, needed)
        main += rowHeight[l] + if (l < levels.size - 1) gap else 0f
    }
    return NarrowResult(main, body, stack)
}
