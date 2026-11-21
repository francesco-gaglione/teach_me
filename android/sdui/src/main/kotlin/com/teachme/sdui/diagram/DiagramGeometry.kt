package com.teachme.sdui.diagram

import com.teachme.sdui.model.Diagram
import com.teachme.sdui.model.DiagramLayout

/*
 * Pure layout of the `diagram` block (docs/design/components/diagram.md). Everything here works in dp as Float and
 * knows nothing about Compose: text is measured through [DiagramMeasure], the result is a [DiagramPlan] the
 * renderer only has to draw. The sizes are the figures of the spec, which has no design token for them.
 */

/** Measures and gaps of the spec ("Nodo", "Freccia", the three layouts). */
internal object DiagramSpec {
    const val NODE_MIN_WIDTH = 104f
    const val NODE_MAX_WIDTH = 200f
    const val NODE_WIDE_CAP = 160f
    const val NODE_PADDING_X = 12f
    const val NODE_PADDING_Y = 8f
    const val NODE_MIN_HEIGHT = 48f
    const val DETAIL_GAP = 2f

    const val ARROW_HEAD = 8f
    const val ARROW_GAP = 2f

    const val LABEL_MAX_WIDTH = 88f
    const val LABEL_PADDING_X = 4f
    const val LABEL_PADDING_Y = 2f
    const val LABEL_OFFSET = 6f

    const val LANE = 12f
    const val MAX_LANES = 2
    const val LANE_EXIT = 8f
    const val LANE_ENTRY = 14f

    const val FLOW_STACK_GAP = 16f
    const val FLOW_GAP = 48f
    const val FLOW_GAP_LABEL = 96f
    const val FLOW_SIDE_GAP = 12f
    const val FLOW_LEVEL_GAP = 40f
    const val BUS_FIRST = 12f
    const val BUS_STEP = 8f

    const val TREE_SIBLING_GAP = 12f
    const val TREE_LEVEL_GAP = 40f
    const val TREE_STUB = 16f
    const val TREE_INDENT = 24f
    const val TREE_ELBOW = 16f
    const val TREE_ROW_GAP = 12f
    const val TREE_TRUNK = 8f

    const val CYCLE_GAP = 32f
    const val CYCLE_LANE = 16f
    const val BADGE = 20f
    const val ICON = 20f

    /** Above this font scale the preferred orientation is always the narrow one. */
    const val LARGE_FONT_SCALE = 1.3f
    const val EPSILON = 0.01f
}

internal data class DPoint(
    val x: Float,
    val y: Float,
)

internal data class DRect(
    val x: Float,
    val y: Float,
    val w: Float,
    val h: Float,
) {
    val right get() = x + w
    val bottom get() = y + h
    val cx get() = x + w / 2
    val cy get() = y + h / 2
}

internal data class PlacedNode(
    val index: Int,
    val rect: DRect,
    /** 1-based position in a `cycle`, null elsewhere. */
    val order: Int? = null,
)

/** A polyline between nodes; with [arrow] the last segment ends in an arrowhead pointing along it. */
internal data class PlacedEdge(
    val points: List<DPoint>,
    val arrow: Boolean,
)

internal data class PlacedLabel(
    val edge: Int,
    val rect: DRect,
)

internal enum class DiagramOrientation { Wide, Narrow }

internal data class DiagramPlan(
    val orientation: DiagramOrientation,
    val width: Float,
    val height: Float,
    val nodes: List<PlacedNode>,
    val edges: List<PlacedEdge>,
    val labels: List<PlacedLabel>,
    /** Where the `repeat` icon of a column `cycle` goes. */
    val returnIcon: DRect? = null,
)

/** Text measurement with the real font, font scale included. Sizes are outer sizes (padding included), in dp. */
internal interface DiagramMeasure {
    /** Widest edge label before wrapping: 88 dp, grown with the font scale so words are not broken. */
    val labelMaxWidth: Float get() = DiagramSpec.LABEL_MAX_WIDTH

    fun nodeNaturalWidth(node: Int): Float

    fun nodeHeight(
        node: Int,
        width: Float,
    ): Float

    fun labelNaturalWidth(edge: Int): Float

    fun labelHeight(
        edge: Int,
        width: Float,
    ): Float
}

internal class DiagramEdgeRef(
    val index: Int,
    val from: Int,
    val to: Int,
    label: String?,
) {
    val hasLabel = !label.isNullOrBlank()
}

/** Edges as node indices; null when an endpoint does not exist (the layouts then give up: text fallback). */
internal fun edgeRefs(diagram: Diagram): List<DiagramEdgeRef>? {
    val ids = diagram.nodes.map { it.id }
    return diagram.edges.mapIndexed { i, e ->
        val from = ids.indexOf(e.from)
        val to = ids.indexOf(e.to)
        if (from < 0 || to < 0) return null
        DiagramEdgeRef(i, from, to, e.label)
    }
}

/** Preferred orientation first ("Regola comune"): the narrow one above font scale 1.3. */
internal fun orientationOrder(fontScale: Float): List<DiagramOrientation> =
    if (fontScale > DiagramSpec.LARGE_FONT_SCALE) {
        listOf(DiagramOrientation.Narrow, DiagramOrientation.Wide)
    } else {
        listOf(DiagramOrientation.Wide, DiagramOrientation.Narrow)
    }

/**
 * Lays [diagram] out in [width] dp: tries the preferred orientation, then the other one, and returns null when
 * neither fits (the caller shows the text fallback). The plan never exceeds [width]: no scaling, no scrolling.
 */
internal fun planDiagram(
    diagram: Diagram,
    layout: DiagramLayout,
    width: Float,
    fontScale: Float,
    measure: DiagramMeasure,
): DiagramPlan? {
    val edges = edgeRefs(diagram) ?: return null
    for (orientation in orientationOrder(fontScale)) {
        val plan =
            when (layout) {
                DiagramLayout.Flow -> planFlow(diagram, edges, width, orientation, measure)
                DiagramLayout.Tree -> planTree(diagram, edges, width, orientation, measure)
                DiagramLayout.Cycle -> planCycle(diagram, width, orientation, measure)
            }
        if (plan != null && plan.width <= width + DiagramSpec.EPSILON) return plan
    }
    return null
}

/** Width of a node of [node]'s text, between the minimum and [cap]. */
internal fun nodeWidth(
    measure: DiagramMeasure,
    node: Int,
    cap: Float,
): Float = measure.nodeNaturalWidth(node).coerceIn(DiagramSpec.NODE_MIN_WIDTH, cap)

/** Label pill size for an edge, at most [maxWidth] wide. */
internal fun labelSize(
    measure: DiagramMeasure,
    edge: Int,
    maxWidth: Float,
): Pair<Float, Float> {
    val w = minOf(measure.labelNaturalWidth(edge), maxWidth)
    return w to measure.labelHeight(edge, w)
}
