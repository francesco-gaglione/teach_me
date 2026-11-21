package com.teachme.sdui.model

/** The layouts the app knows how to draw (docs/contracts/sdui.md section 10). */
enum class DiagramLayout(
    val wire: String,
) {
    Flow("flow"),
    Tree("tree"),
    Cycle("cycle"),
    ;

    companion object {
        fun fromWire(value: String): DiagramLayout? = entries.firstOrNull { it.wire == value }
    }
}

/** Why a diagram is shown as text (logged, never shown to the learner). */
enum class DiagramProblem {
    UnknownLayout,
    NodeCount,
    BlankNode,
    DuplicateNodeId,
    EdgeCount,
    UnknownEndpoint,
    SelfLoop,
    DuplicateEdge,
    FlowNeedsEdges,
    FlowHasCycle,
    FlowIsolatedNode,
    FlowNotConnected,
    TreeEdgeCount,
    TreeParents,
    TreeNotConnected,
    CycleTooSmall,
    CycleHasEdges,
}

sealed interface DiagramValidation {
    /** The structure fits its [layout]: the model is ready to be laid out and drawn. */
    data class Drawable(
        val diagram: Diagram,
        val layout: DiagramLayout,
    ) : DiagramValidation

    /** Not drawable: show the text fallback (`title`, `alt`, `caption`). */
    data class TextOnly(
        val problem: DiagramProblem,
    ) : DiagramValidation
}

private const val MIN_NODES = 2
private const val MAX_NODES = 8
private const val MAX_EDGES = 12
private const val MIN_CYCLE_NODES = 3

/**
 * Checks a diagram against the rules of sdui.md section 10 (the app does not assume the backend validator ran).
 * Pure: returns the diagram ready to draw, or [DiagramValidation.TextOnly] with the first problem found.
 */
fun validateDiagram(diagram: Diagram): DiagramValidation {
    val layout =
        DiagramLayout.fromWire(diagram.layout)
            ?: return DiagramValidation.TextOnly(DiagramProblem.UnknownLayout)
    val problem = commonProblem(diagram) ?: layoutProblem(diagram, layout)
    return if (problem == null) DiagramValidation.Drawable(diagram, layout) else DiagramValidation.TextOnly(problem)
}

private fun commonProblem(diagram: Diagram): DiagramProblem? {
    val ids = diagram.nodes.map { it.id }
    val pairs = diagram.edges.map { it.from to it.to }
    return when {
        diagram.nodes.size !in MIN_NODES..MAX_NODES -> DiagramProblem.NodeCount
        // The backend accepts any 1-40 character label (blank ones included) but an id of a-z, 0-9, _ and -.
        diagram.nodes.any { it.id.isBlank() || it.label.isEmpty() } -> DiagramProblem.BlankNode
        ids.size != ids.toSet().size -> DiagramProblem.DuplicateNodeId
        diagram.edges.size > MAX_EDGES -> DiagramProblem.EdgeCount
        pairs.any { (from, to) -> from !in ids || to !in ids } -> DiagramProblem.UnknownEndpoint
        pairs.any { (from, to) -> from == to } -> DiagramProblem.SelfLoop
        pairs.size != pairs.toSet().size -> DiagramProblem.DuplicateEdge
        else -> null
    }
}

private fun layoutProblem(
    diagram: Diagram,
    layout: DiagramLayout,
): DiagramProblem? {
    val ids = diagram.nodes.map { it.id }
    val edges = diagram.edges.map { it.from to it.to }
    return when (layout) {
        DiagramLayout.Flow -> flowProblem(ids, edges)
        DiagramLayout.Tree -> treeProblem(ids, edges)
        DiagramLayout.Cycle ->
            when {
                ids.size < MIN_CYCLE_NODES -> DiagramProblem.CycleTooSmall
                edges.isNotEmpty() -> DiagramProblem.CycleHasEdges
                else -> null
            }
    }
}

private fun flowProblem(
    ids: List<String>,
    edges: List<Pair<String, String>>,
): DiagramProblem? {
    val touched = edges.flatMap { it.toList() }.toSet()
    return when {
        edges.isEmpty() -> DiagramProblem.FlowNeedsEdges
        ids.any { it !in touched } -> DiagramProblem.FlowIsolatedNode
        hasDirectedCycle(ids, edges) -> DiagramProblem.FlowHasCycle
        !isConnected(ids, edges) -> DiagramProblem.FlowNotConnected
        else -> null
    }
}

private fun treeProblem(
    ids: List<String>,
    edges: List<Pair<String, String>>,
): DiagramProblem? {
    val parentCount = ids.associateWith { id -> edges.count { it.second == id } }
    val roots = ids.filter { parentCount.getValue(it) == 0 }
    if (edges.size != ids.size - 1) return DiagramProblem.TreeEdgeCount
    if (roots.size != 1 || parentCount.values.any { it > 1 }) return DiagramProblem.TreeParents
    val children = edges.groupBy({ it.first }, { it.second })
    val seen = mutableSetOf<String>()
    val pending = ArrayDeque(roots)
    while (pending.isNotEmpty()) {
        val node = pending.removeFirst()
        if (seen.add(node)) pending += children[node].orEmpty()
    }
    return if (seen.size == ids.size) null else DiagramProblem.TreeNotConnected
}

/** Kahn's algorithm: true when the directed graph has a cycle. */
private fun hasDirectedCycle(
    ids: List<String>,
    edges: List<Pair<String, String>>,
): Boolean {
    val indegree = ids.associateWithTo(mutableMapOf()) { id -> edges.count { it.second == id } }
    val pending = ArrayDeque(ids.filter { indegree.getValue(it) == 0 })
    var visited = 0
    while (pending.isNotEmpty()) {
        val node = pending.removeFirst()
        visited++
        for ((from, to) in edges) {
            if (from == node) {
                indegree[to] = indegree.getValue(to) - 1
                if (indegree.getValue(to) == 0) pending += to
            }
        }
    }
    return visited != ids.size
}

private fun isConnected(
    ids: List<String>,
    edges: List<Pair<String, String>>,
): Boolean {
    val seen = mutableSetOf<String>()
    val pending = ArrayDeque(listOf(ids.first()))
    while (pending.isNotEmpty()) {
        val node = pending.removeFirst()
        if (!seen.add(node)) continue
        edges.forEach { (a, b) ->
            if (a == node) pending += b
            if (b == node) pending += a
        }
    }
    return seen.size == ids.size
}
