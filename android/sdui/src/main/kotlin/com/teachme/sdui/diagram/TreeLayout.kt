package com.teachme.sdui.diagram

import com.teachme.sdui.model.Diagram

/** A child and the edge that leads to it. */
internal class TreeChild(
    val node: Int,
    val edge: DiagramEdgeRef,
)

/** Children of each node in the order of `edges`, plus the root; null when the edges are not a tree. */
internal fun treeChildren(
    count: Int,
    edges: List<DiagramEdgeRef>,
): Pair<Int, List<List<TreeChild>>>? {
    val roots = (0 until count).filter { n -> edges.none { it.to == n } }
    if (roots.size != 1 || edges.size != count - 1) return null
    val children = List(count) { n -> edges.filter { it.from == n }.map { TreeChild(it.to, it) } }
    // Reachability guards the recursion against a malformed graph (a cycle detached from the root).
    val seen = HashSet<Int>()
    val pending = ArrayDeque(listOf(roots.first()))
    while (pending.isNotEmpty()) {
        val n = pending.removeFirst()
        if (seen.add(n)) pending += children[n].map { it.node }
    }
    return if (seen.size == count) roots.first() to children else null
}

internal fun planTree(
    diagram: Diagram,
    edges: List<DiagramEdgeRef>,
    width: Float,
    orientation: DiagramOrientation,
    measure: DiagramMeasure,
): DiagramPlan? {
    val (root, children) = treeChildren(diagram.nodes.size, edges) ?: return null
    return if (orientation == DiagramOrientation.Wide) {
        planTreeFromAbove(root, children, width, measure)
    } else {
        planTreeList(root, children, width, measure)
    }
}

private fun depths(
    root: Int,
    children: List<List<TreeChild>>,
): IntArray {
    val depth = IntArray(children.size)

    fun walk(n: Int) {
        for (c in children[n]) {
            depth[c.node] = depth[n] + 1
            walk(c.node)
        }
    }
    walk(root)
    return depth
}

private fun labelHeightOf(
    child: TreeChild,
    measure: DiagramMeasure,
): Float = if (child.edge.hasLabel) labelSize(measure, child.edge.index, measure.labelMaxWidth).second else 0f

/** Tree from above: one level per depth, the parent centered over its children, elbow connectors without arrows. */
private fun planTreeFromAbove(
    root: Int,
    children: List<List<TreeChild>>,
    width: Float,
    measure: DiagramMeasure,
): DiagramPlan? {
    val count = children.size
    val depth = depths(root, children)
    val levels = (depth.max() + 1)
    for (cap in listOf(DiagramSpec.NODE_WIDE_CAP, DiagramSpec.NODE_MIN_WIDTH)) {
        val w = FloatArray(count) { nodeWidth(measure, it, cap) }
        val h = FloatArray(count) { measure.nodeHeight(it, w[it]) }
        val span = FloatArray(count)

        fun measureSpan(n: Int): Float {
            val kids = children[n].map { measureSpan(it.node) }
            span[n] = maxOf(w[n], kids.sum() + DiagramSpec.TREE_SIBLING_GAP * (kids.size - 1).coerceAtLeast(0))
            return span[n]
        }
        measureSpan(root)
        val left = FloatArray(count)
        val centerX = FloatArray(count)

        fun place(
            n: Int,
            from: Float,
        ) {
            val kids = children[n]
            if (kids.isEmpty()) {
                centerX[n] = from + span[n] / 2
            } else {
                val total =
                    kids.sumOf { span[it.node].toDouble() }.toFloat() + DiagramSpec.TREE_SIBLING_GAP * (kids.size - 1)
                var x = from + (span[n] - total) / 2
                for (k in kids) {
                    place(k.node, x)
                    x += span[k.node] + DiagramSpec.TREE_SIBLING_GAP
                }
                centerX[n] = (centerX[kids.first().node] + centerX[kids.last().node]) / 2
            }
            left[n] = centerX[n] - w[n] / 2
        }
        place(root, 0f)
        val minX = left.indices.minOf { left[it] }
        val maxX = left.indices.maxOf { left[it] + w[it] }
        if (maxX - minX > width + DiagramSpec.EPSILON) continue

        val levelHeight = FloatArray(levels) { d -> (0 until count).filter { depth[it] == d }.maxOf { h[it] } }
        val levelTop = FloatArray(levels)
        for (d in 1 until levels) {
            val labelled =
                (0 until count)
                    .filter { depth[it] == d }
                    .maxOf { n -> children.flatten().filter { it.node == n }.maxOf { labelHeightOf(it, measure) } }
            val drop =
                maxOf(
                    DiagramSpec.TREE_LEVEL_GAP - DiagramSpec.TREE_STUB,
                    labelled + DiagramSpec.LABEL_PADDING_Y * 2,
                )
            levelTop[d] = levelTop[d - 1] + levelHeight[d - 1] + DiagramSpec.TREE_STUB + drop
        }
        val nodes = List(count) { PlacedNode(it, DRect(left[it] - minX, levelTop[depth[it]], w[it], h[it])) }
        val paths = ArrayList<Pair<Int, PlacedEdge>>()
        val labels = ArrayList<PlacedLabel>()
        val total = maxX - minX
        for (n in 0 until count) {
            for (c in children[n]) {
                val parent = nodes[n].rect
                val child = nodes[c.node].rect
                val bar = levelTop[depth[n]] + levelHeight[depth[n]] + DiagramSpec.TREE_STUB
                paths +=
                    c.edge.index to
                    PlacedEdge(
                        listOf(
                            DPoint(parent.cx, parent.bottom),
                            DPoint(parent.cx, bar),
                            DPoint(child.cx, bar),
                            DPoint(child.cx, child.y),
                        ),
                        arrow = false,
                    )
                if (c.edge.hasLabel) {
                    val (lw, lh) = labelSize(measure, c.edge.index, measure.labelMaxWidth)
                    var x = child.cx + DiagramSpec.LABEL_OFFSET
                    if (x + lw > total) x = child.cx - DiagramSpec.LABEL_OFFSET - lw
                    labels += PlacedLabel(c.edge.index, DRect(x, bar + (child.y - bar - lh) / 2, lw, lh))
                }
            }
        }
        return DiagramPlan(
            DiagramOrientation.Wide,
            total,
            levelTop[levels - 1] + levelHeight[levels - 1],
            nodes,
            paths.sortedBy { it.first }.map { it.second },
            labels,
        )
    }
    return null
}

/** Indented list: one node per row, a vertical connector on the left with an elbow into each node. */
private fun planTreeList(
    root: Int,
    children: List<List<TreeChild>>,
    width: Float,
    measure: DiagramMeasure,
): DiagramPlan? {
    val count = children.size
    val depth = depths(root, children)
    val order = ArrayList<Int>()

    fun walk(n: Int) {
        order += n
        children[n].forEach { walk(it.node) }
    }
    walk(root)
    val labelOf = HashMap<Int, TreeChild>()
    children.forEach { kids -> kids.forEach { labelOf[it.node] = it } }
    val rects = arrayOfNulls<DRect>(count)
    val labels = ArrayList<PlacedLabel>()
    var y = 0f
    for ((row, n) in order.withIndex()) {
        val x = DiagramSpec.TREE_INDENT * depth[n]
        val w = width - x
        if (w < DiagramSpec.NODE_MIN_WIDTH - DiagramSpec.EPSILON) return null
        val h = measure.nodeHeight(n, w)
        val incoming = labelOf[n]
        var gap = if (row == 0) 0f else DiagramSpec.TREE_ROW_GAP
        if (incoming != null && incoming.edge.hasLabel) {
            val trunk = DiagramSpec.TREE_INDENT * (depth[n] - 1) + DiagramSpec.TREE_TRUNK
            val (lw, lh) =
                labelSize(measure, incoming.edge.index, measure.labelMaxWidth)
            gap = maxOf(gap, lh + DiagramSpec.LABEL_PADDING_Y * 2)
            labels +=
                PlacedLabel(
                    incoming.edge.index,
                    DRect(
                        trunk + DiagramSpec.LABEL_OFFSET,
                        y + gap - lh - DiagramSpec.LABEL_PADDING_Y,
                        lw,
                        lh,
                    ),
                )
        }
        y += gap
        rects[n] = DRect(x, y, w, h)
        y += h
    }
    val placed = rects.map { it ?: return null }
    val paths = ArrayList<Pair<Int, PlacedEdge>>()
    for (n in 0 until count) {
        for (c in children[n]) {
            val parent = placed[n]
            val child = placed[c.node]
            val trunk = parent.x + DiagramSpec.TREE_TRUNK
            paths +=
                c.edge.index to
                PlacedEdge(
                    listOf(DPoint(trunk, parent.bottom), DPoint(trunk, child.cy), DPoint(child.x, child.cy)),
                    arrow = false,
                )
        }
    }
    return DiagramPlan(
        DiagramOrientation.Narrow,
        width,
        y,
        placed.mapIndexed { i, r -> PlacedNode(i, r) },
        paths.sortedBy { it.first }.map { it.second },
        labels,
    )
}
