package com.teachme.sdui.diagram

import com.teachme.sdui.Fixtures
import com.teachme.sdui.model.Diagram
import com.teachme.sdui.model.DiagramEdge
import com.teachme.sdui.model.DiagramLayout
import com.teachme.sdui.model.DiagramNode
import com.teachme.sdui.model.LessonParser
import com.teachme.sdui.model.ParseResult
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs
import kotlin.math.ceil

/** Text of 7 dp per character, 20 dp lines (16 dp for labels): deterministic stand-in for the real measurer. */
private class FakeMeasure(
    private val diagram: Diagram,
    private val charWidth: Float = 7f,
) : DiagramMeasure {
    private fun lines(
        text: String,
        width: Float,
        char: Float = charWidth,
    ) = ceil(text.length * char / width.coerceAtLeast(char)).coerceAtLeast(1f)

    override fun nodeNaturalWidth(node: Int): Float {
        val n = diagram.nodes[node]
        return maxOf(n.label.length, n.detail?.length ?: 0) * charWidth + 24f
    }

    override fun nodeHeight(
        node: Int,
        width: Float,
    ): Float {
        val n = diagram.nodes[node]
        val inner = width - 24f
        val detail = n.detail?.let { 2f + lines(it, inner) * 16f } ?: 0f
        return maxOf(48f, 16f + lines(n.label, inner) * 20f + detail)
    }

    override fun labelNaturalWidth(edge: Int) =
        diagram.edges[edge]
            .label
            .orEmpty()
            .length * 6f + 8f

    override fun labelHeight(
        edge: Int,
        width: Float,
    ) = lines(diagram.edges[edge].label.orEmpty(), width - 8f, char = 6f) * 16f + 4f
}

class DiagramLayoutTest {
    private fun node(
        id: String,
        label: String = id,
        detail: String? = null,
    ) = DiagramNode(id, label, detail)

    private fun diagram(
        layout: String,
        nodes: List<DiagramNode>,
        edges: List<DiagramEdge> = emptyList(),
    ) = Diagram("d", null, layout, null, nodes, edges, "", "alt")

    private fun ids(vararg id: String) = id.map { node(it) }

    private fun plan(
        d: Diagram,
        width: Float,
        fontScale: Float = 1f,
        charWidth: Float = 7f,
    ): DiagramPlan? {
        val layout = DiagramLayout.fromWire(d.layout)!!
        return planDiagram(d, layout, width, fontScale, FakeMeasure(d, charWidth))
    }

    private val tablet = 568f
    private val phone = 296f

    private fun refs(vararg pairs: Pair<Int, Int>) = pairs.mapIndexed { i, (a, b) -> DiagramEdgeRef(i, a, b, null) }

    private fun rect(
        p: DiagramPlan,
        i: Int,
    ) = p.nodes[i].rect

    private fun DRect.overlaps(o: DRect) =
        x < o.right - 0.01f && o.x < right - 0.01f && y < o.bottom - 0.01f && o.y < bottom - 0.01f

    /** No node leaves the figure, none overlaps another, and the figure fits the column. */
    private fun assertSound(
        p: DiagramPlan,
        width: Float,
    ) {
        assertTrue("width ${p.width} > $width", p.width <= width + 0.01f)
        p.nodes.forEach {
            val r = it.rect
            assertTrue("node ${it.index} outside: $r in ${p.width}x${p.height}", r.x >= -0.01f && r.y >= -0.01f)
            assertTrue(
                "node ${it.index} outside: $r in ${p.width}x${p.height}",
                r.right <= p.width + 0.01f && r.bottom <= p.height + 0.01f,
            )
            assertTrue(
                "node ${it.index} too small: $r",
                r.w >= DiagramSpec.NODE_MIN_WIDTH - 0.01f && r.h >= DiagramSpec.NODE_MIN_HEIGHT - 0.01f,
            )
        }
        for (a in p.nodes) {
            for (b in p.nodes) {
                if (a.index <
                    b.index
                ) {
                    assertFalse("${a.index} overlaps ${b.index}", a.rect.overlaps(b.rect))
                }
            }
        }
        p.edges.forEach { e ->
            e.points.forEach {
                assertTrue(
                    "edge point $it outside",
                    it.x >= -0.01f && it.x <= p.width + 0.01f && it.y >= -0.01f && it.y <= p.height + 0.01f,
                )
            }
            // Orthogonal only: every segment is horizontal or vertical.
            e.points.zipWithNext().forEach { (a, b) ->
                assertTrue(
                    "diagonal $a -> $b",
                    abs(a.x - b.x) < 0.01f || abs(a.y - b.y) < 0.01f,
                )
            }
        }
        p.labels.forEach {
            assertTrue(
                "label outside: ${it.rect}",
                it.rect.x >= -0.01f && it.rect.right <= p.width + 0.01f && it.rect.y >= -0.01f,
            )
        }
        // Edge labels collide neither with each other nor with a node.
        for (a in p.labels) {
            p.nodes.forEach { n -> assertFalse("label ${a.rect} overlaps node ${n.rect}", a.rect.overlaps(n.rect)) }
            for (b in p.labels) {
                if (a.edge < b.edge) assertFalse("labels ${a.rect} and ${b.rect} overlap", a.rect.overlaps(b.rect))
            }
        }
    }

    // Pure helpers

    @Test
    fun `flow levels are the longest path from a source`() {
        // a -> b -> d, a -> c -> d, a -> d (skips two levels)
        val level = flowLevels(4, refs(0 to 1, 1 to 3, 0 to 2, 2 to 3, 0 to 3))
        assertEquals(listOf(0, 1, 1, 2), level.toList())
    }

    @Test
    fun `several sources all sit on level 0`() {
        val level = flowLevels(4, refs(0 to 2, 1 to 2, 2 to 3))
        assertEquals(listOf(0, 0, 1, 2), level.toList())
    }

    @Test
    fun `flow levels terminate on a cycle`() {
        assertEquals(3, flowLevels(3, refs(0 to 1, 1 to 2, 2 to 0)).size)
    }

    @Test
    fun `levels keep the order of nodes unless reordering removes crossings`() {
        // a->c, b->d: with nodes order (c, d) the edges cross when sources are (a, b) = (0, 1) and targets (d, c).
        val ordered = orderedLevels(intArrayOf(0, 0, 1, 1), refs(0 to 3, 1 to 2))
        assertEquals(listOf(listOf(0, 1), listOf(3, 2)), ordered)
        assertEquals(0, flowCrossings(ordered, refs(0 to 3, 1 to 2)))
        val untouched = orderedLevels(intArrayOf(0, 0, 1, 1), refs(0 to 2, 1 to 3))
        assertEquals(listOf(listOf(0, 1), listOf(2, 3)), untouched)
    }

    @Test
    fun `font scale above 1_3 prefers the narrow orientation`() {
        assertEquals(DiagramOrientation.Wide, orientationOrder(1f).first())
        assertEquals(DiagramOrientation.Wide, orientationOrder(1.3f).first())
        assertEquals(DiagramOrientation.Narrow, orientationOrder(1.31f).first())
        assertEquals(DiagramOrientation.Narrow, orientationOrder(2f).first())
    }

    @Test
    fun `tree children follow the order of edges and the root is found`() {
        val (root, kids) = treeChildren(4, refs(0 to 2, 0 to 1, 1 to 3))!!
        assertEquals(0, root)
        assertEquals(listOf(2, 1), kids[0].map { it.node })
        assertEquals(listOf(3), kids[1].map { it.node })
        assertNull(treeChildren(3, refs(0 to 1)))
    }

    // Flow

    private val chain = diagram("flow", ids("a", "b", "c"), listOf(DiagramEdge("a", "b"), DiagramEdge("b", "c")))

    @Test
    fun `a short flow is horizontal on a tablet with straight arrows`() {
        val p = plan(chain, tablet)!!
        assertEquals(DiagramOrientation.Wide, p.orientation)
        assertSound(p, tablet)
        assertTrue(rect(p, 0).right < rect(p, 1).x && rect(p, 1).right < rect(p, 2).x)
        assertEquals(rect(p, 0).cy, rect(p, 1).cy, 0.01f)
        p.edges.forEach { assertEquals(2, it.points.size) }
        assertEquals(DiagramSpec.FLOW_GAP, rect(p, 1).x - rect(p, 0).right, 0.01f)
    }

    @Test
    fun `an arrow stops 2 dp before the node it points to`() {
        val p = plan(chain, tablet)!!
        assertEquals(
            rect(p, 1).x - DiagramSpec.ARROW_GAP,
            p.edges[0]
                .points
                .last()
                .x,
            0.01f,
        )
        assertTrue(p.edges.all { it.arrow })
    }

    @Test
    fun `a labelled gap is 96 dp and the label sits above its arrow`() {
        val d = diagram("flow", ids("a", "b"), listOf(DiagramEdge("a", "b", "perche")))
        val p = plan(d, tablet)!!
        assertSound(p, tablet)
        assertEquals(DiagramSpec.FLOW_GAP_LABEL, rect(p, 1).x - rect(p, 0).right, 0.01f)
        val label = p.labels.single().rect
        assertTrue(
            label.bottom <=
                p.edges[0]
                    .points
                    .first()
                    .y,
        )
        assertTrue(label.x >= rect(p, 0).right && label.right <= rect(p, 1).x)
    }

    @Test
    fun `a branching flow stacks the nodes of a level and branches with elbows`() {
        val d =
            diagram(
                "flow",
                ids("a", "b", "c", "d"),
                listOf(DiagramEdge("a", "b"), DiagramEdge("b", "c"), DiagramEdge("b", "d")),
            )
        val p = plan(d, tablet)!!
        assertSound(p, tablet)
        assertEquals(DiagramOrientation.Wide, p.orientation)
        assertEquals(rect(p, 2).x, rect(p, 3).x, 0.01f)
        assertEquals(DiagramSpec.FLOW_STACK_GAP, rect(p, 3).y - rect(p, 2).bottom, 0.01f)
        assertEquals(4, p.edges[1].points.size)
        // b is centered between its two targets
        assertEquals((rect(p, 2).cy + rect(p, 3).cy) / 2, rect(p, 1).cy, 0.01f)
    }

    @Test
    fun `a flow too long for the width wraps its nodes at the minimum width before giving up`() {
        val d =
            diagram(
                "flow",
                listOf(
                    node("a", "Dati raccolti"),
                    node("b", "Confronto con l'ipotesi"),
                    node("c", "L'ipotesi regge"),
                    node("d", "L'ipotesi e smentita"),
                ),
                listOf(
                    DiagramEdge("a", "b"),
                    DiagramEdge("b", "c", "d'accordo"),
                    DiagramEdge("b", "d", "in disaccordo"),
                ),
            )
        val p = plan(d, tablet)!!
        assertEquals(DiagramOrientation.Wide, p.orientation)
        assertSound(p, tablet)
    }

    @Test
    fun `a flow switches to vertical when it does not fit horizontally`() {
        val long =
            diagram(
                "flow",
                listOf(
                    node("a", "Prima fase del lavoro"),
                    node("b", "Seconda fase del lavoro"),
                    node("c", "Terza fase del lavoro"),
                    node("d", "Quarta fase"),
                ),
                listOf(DiagramEdge("a", "b"), DiagramEdge("b", "c"), DiagramEdge("c", "d")),
            )
        val p = plan(long, phone)!!
        assertEquals(DiagramOrientation.Narrow, p.orientation)
        assertSound(p, phone)
        assertTrue(rect(p, 0).bottom < rect(p, 1).y && rect(p, 1).bottom < rect(p, 2).y)
        assertEquals(DiagramSpec.FLOW_LEVEL_GAP, rect(p, 1).y - rect(p, 0).bottom, 0.01f)
    }

    @Test
    fun `a large font forces the vertical flow even on a tablet`() {
        val p = plan(chain, tablet, fontScale = 2f)!!
        assertEquals(DiagramOrientation.Narrow, p.orientation)
        assertSound(p, tablet)
    }

    @Test
    fun `vertical siblings have the same width`() {
        val d = diagram("flow", ids("a", "b", "c"), listOf(DiagramEdge("a", "b"), DiagramEdge("a", "c")))
        val p = plan(d, tablet, fontScale = 2f)!!
        assertEquals(rect(p, 1).w, rect(p, 2).w, 0.01f)
        assertEquals(rect(p, 1).y, rect(p, 2).y, 0.01f)
        assertEquals(DiagramSpec.FLOW_SIDE_GAP, rect(p, 2).x - rect(p, 1).right, 0.01f)
    }

    @Test
    fun `when neither orientation fits the flow falls back to text`() {
        val d =
            diagram(
                "flow",
                ids("a", "b", "c", "d"),
                listOf(DiagramEdge("a", "b"), DiagramEdge("a", "c"), DiagramEdge("a", "d")),
            )
        // 3 side by side need 336 dp, the horizontal flow 256 dp: neither fits 240 dp
        assertNull(plan(d, 240f))
        assertNull(plan(d, 240f, fontScale = 2f))
        assertNotNull(plan(d, tablet))
    }

    private val skipTwo =
        diagram(
            "flow",
            ids("a", "b", "c", "d", "e"),
            listOf(
                DiagramEdge("a", "b"),
                DiagramEdge("b", "c"),
                DiagramEdge("c", "d"),
                DiagramEdge("a", "c"),
                DiagramEdge("b", "d", "salta"),
            ),
        )

    @Test
    fun `up to two level-skipping arrows run on lanes outside the nodes`() {
        val p = plan(skipTwo.copy(nodes = skipTwo.nodes.take(4)), tablet)!!
        assertSound(p, tablet)
        val lowest = p.nodes.maxOf { it.rect.bottom }
        val lanes = p.edges.filter { it.points.size == 6 }
        assertEquals(2, lanes.size)
        lanes.forEach { l -> assertTrue(l.points[2].y >= lowest + DiagramSpec.LANE - 0.01f) }
        assertTrue(p.height >= lowest + 2 * DiagramSpec.LANE)
    }

    @Test
    fun `a third level-skipping arrow sends the flow to the text fallback`() {
        val d =
            diagram(
                "flow",
                ids("a", "b", "c", "d", "e"),
                listOf(
                    DiagramEdge("a", "b"),
                    DiagramEdge("b", "c"),
                    DiagramEdge("c", "d"),
                    DiagramEdge("d", "e"),
                    DiagramEdge("a", "c"),
                    DiagramEdge("b", "d"),
                    DiagramEdge("c", "e"),
                ),
            )
        assertNull(plan(d, tablet))
        assertNull(plan(d, tablet, fontScale = 2f))
    }

    @Test
    fun `labels of two arrows entering the same node do not overlap`() {
        val d =
            diagram(
                "flow",
                ids("a", "b", "c"),
                listOf(DiagramEdge("a", "c", "si"), DiagramEdge("b", "c", "no")),
            )
        for ((w, fs) in listOf(tablet to 1f, phone to 1f, tablet to 2f)) {
            val p = plan(d, w, fs, charWidth = 7f * fs)!!
            assertSound(p, w)
            assertEquals(2, p.labels.size)
        }
        // On a tablet the pills sit right of both buses: no vertical stretch of a branch goes through them.
        val p = plan(d, tablet)!!
        assertEquals(DiagramOrientation.Wide, p.orientation)
        val busX =
            p.edges
                .flatMap { e ->
                    e.points
                        .drop(1)
                        .dropLast(1)
                        .map { it.x }
                }.max()
        p.labels.forEach { assertTrue("label ${it.rect} crosses the bus at $busX", it.rect.x > busX) }
    }

    @Test
    fun `an edge to a missing node cannot be laid out`() {
        val d = diagram("flow", ids("a", "b"), listOf(DiagramEdge("a", "zzz")))
        assertNull(plan(d, tablet))
    }

    // Tree

    private val tree3 =
        diagram(
            "tree",
            listOf(
                node("r", "Variabili"),
                node("a", "Indipendente", "la cambi tu"),
                node("b", "Dipendente", "la misuri"),
                node("c", "Di controllo"),
            ),
            listOf(DiagramEdge("r", "a"), DiagramEdge("r", "b"), DiagramEdge("r", "c")),
        )

    @Test
    fun `a tree on a tablet hangs from the root with the parent centered on its children`() {
        val p = plan(tree3, tablet)!!
        assertEquals(DiagramOrientation.Wide, p.orientation)
        assertSound(p, tablet)
        assertEquals((rect(p, 1).cx + rect(p, 3).cx) / 2, rect(p, 0).cx, 0.01f)
        assertTrue(rect(p, 1).x < rect(p, 2).x && rect(p, 2).x < rect(p, 3).x)
        assertEquals(DiagramSpec.TREE_SIBLING_GAP, rect(p, 2).x - rect(p, 1).right, 0.01f)
        assertTrue(p.edges.none { it.arrow })
        assertEquals(DiagramSpec.TREE_LEVEL_GAP, rect(p, 1).y - rect(p, 0).bottom, 0.01f)
    }

    @Test
    fun `an elbow connector leaves the parent 16 dp down and drops straight into the child`() {
        val p = plan(tree3, tablet)!!
        val (a, b, c, d) = p.edges[0].points
        assertEquals(rect(p, 0).cx, a.x, 0.01f)
        assertEquals(rect(p, 0).bottom, a.y, 0.01f)
        assertEquals(rect(p, 0).bottom + DiagramSpec.TREE_STUB, b.y, 0.01f)
        assertEquals(b.y, c.y, 0.01f)
        assertEquals(rect(p, 1).cx, d.x, 0.01f)
        assertEquals(rect(p, 1).y, d.y, 0.01f)
    }

    @Test
    fun `a tree too wide for the phone becomes an indented list`() {
        val p = plan(tree3, phone)!!
        assertEquals(DiagramOrientation.Narrow, p.orientation)
        assertSound(p, phone)
        assertEquals(phone, p.width, 0.01f)
        // one node per row, in depth-first order, children indented by 24 dp
        assertEquals(0f, rect(p, 0).x, 0.01f)
        (1..3).forEach { assertEquals(DiagramSpec.TREE_INDENT, rect(p, it).x, 0.01f) }
        assertEquals(DiagramSpec.TREE_ROW_GAP, rect(p, 2).y - rect(p, 1).bottom, 0.01f)
        assertEquals(phone - DiagramSpec.TREE_INDENT, rect(p, 1).w, 0.01f)
    }

    @Test
    fun `an indented list is always drawable up to depth 7 and respects the column`() {
        val nodes = (0..7).map { node("n$it", "Livello $it") }
        val edges = (0..6).map { DiagramEdge("n$it", "n${it + 1}") }
        val d = diagram("tree", nodes, edges)
        val p = plan(d, phone, fontScale = 2f)!!
        assertSound(p, phone)
        assertEquals(DiagramOrientation.Narrow, p.orientation)
        assertEquals(DiagramSpec.TREE_INDENT * 7, rect(p, 7).x, 0.01f)
    }

    @Test
    fun `an indented list connects each node with a trunk and an elbow of 16 dp`() {
        val p = plan(tree3, phone)!!
        val (start, corner, end) = p.edges[0].points
        assertEquals(DiagramSpec.TREE_TRUNK, start.x, 0.01f)
        assertEquals(rect(p, 0).bottom, start.y, 0.01f)
        assertEquals(start.x, corner.x, 0.01f)
        assertEquals(rect(p, 1).cy, corner.y, 0.01f)
        assertEquals(rect(p, 1).x, end.x, 0.01f)
        assertEquals(DiagramSpec.TREE_ELBOW, end.x - corner.x, 0.01f)
    }

    @Test
    fun `a deeper tree keeps subtrees apart`() {
        val d =
            diagram(
                "tree",
                ids("r", "a", "b", "a1", "a2", "b1"),
                listOf(
                    DiagramEdge("r", "a"),
                    DiagramEdge("r", "b"),
                    DiagramEdge("a", "a1"),
                    DiagramEdge("a", "a2"),
                    DiagramEdge("b", "b1"),
                ),
            )
        val p = plan(d, tablet)!!
        assertSound(p, tablet)
        assertTrue(rect(p, 4).right < rect(p, 5).x)
    }

    @Test
    fun `tree edge labels sit beside the drop into the child and enlarge the level gap when tall`() {
        val d =
            diagram(
                "tree",
                ids("r", "a", "b"),
                listOf(DiagramEdge("r", "a", "una frase molto lunga per andare a capo"), DiagramEdge("r", "b")),
            )
        val p = plan(d, tablet)!!
        assertSound(p, tablet)
        val label = p.labels.single().rect
        val child = rect(p, 1)
        assertTrue(label.x >= child.cx || label.right <= child.cx)
        assertTrue(label.y >= rect(p, 0).bottom && label.bottom <= child.y)
    }

    // Cycle

    private fun cycle(n: Int) = diagram("cycle", (1..n).map { node("n$it", "Tappa $it") })

    @Test
    fun `a ring of four has two rows and the return goes straight up on the left`() {
        val p = plan(cycle(4), tablet)!!
        assertEquals(DiagramOrientation.Wide, p.orientation)
        assertSound(p, tablet)
        assertEquals(rect(p, 0).y, rect(p, 1).y, 0.01f)
        assertTrue(rect(p, 0).x < rect(p, 1).x)
        // bottom row runs right to left, the first bottom node is under the last top node
        assertEquals(rect(p, 1).x, rect(p, 2).x, 0.01f)
        assertEquals(rect(p, 0).x, rect(p, 3).x, 0.01f)
        assertEquals(DiagramSpec.CYCLE_GAP, rect(p, 1).x - rect(p, 0).right, 0.01f)
        assertEquals(4, p.edges.size)
        val back = p.edges.last()
        assertEquals(2, back.points.size)
        assertEquals(rect(p, 0).cx, back.points.last().x, 0.01f)
        assertTrue(back.points.last().y < back.points.first().y)
        assertEquals((1..4).toList(), p.nodes.map { it.order })
    }

    @Test
    fun `a ring of three has two nodes on top, one below the second, and an L return`() {
        val p = plan(cycle(3), tablet)!!
        assertSound(p, tablet)
        assertEquals(rect(p, 1).x, rect(p, 2).x, 0.01f)
        assertTrue(rect(p, 2).y > rect(p, 1).y)
        val back = p.edges.last()
        assertEquals(3, back.points.size)
        assertEquals(rect(p, 2).cy, back.points[1].y, 0.01f)
        assertEquals(rect(p, 0).cx, back.points[1].x, 0.01f)
    }

    @Test
    fun `a ring of eight fits four per row on a tablet but not on a phone`() {
        val p = plan(cycle(8), tablet)!!
        assertEquals(DiagramOrientation.Wide, p.orientation)
        assertSound(p, tablet)
        val phonePlan = plan(cycle(8), phone)!!
        assertEquals(DiagramOrientation.Narrow, phonePlan.orientation)
        assertSound(phonePlan, phone)
    }

    @Test
    fun `a cycle that does not fit as a ring is a column with a return lane and the repeat icon on it`() {
        val p = plan(cycle(6), phone)!!
        assertEquals(DiagramOrientation.Narrow, p.orientation)
        assertSound(p, phone)
        assertEquals(phone - 32f, rect(p, 0).w, 0.01f)
        assertEquals(DiagramSpec.CYCLE_GAP, rect(p, 1).y - rect(p, 0).bottom, 0.01f)
        val ret = p.edges.last()
        assertEquals(4, ret.points.size)
        val lane = ret.points[1].x
        assertTrue(lane > rect(p, 0).right)
        val icon = p.returnIcon!!
        assertEquals(lane, icon.cx, 0.01f)
        assertEquals(DiagramSpec.ICON, icon.w, 0.01f)
        assertTrue(icon.cy > rect(p, 0).cy && icon.cy < rect(p, 5).cy)
        assertEquals(rect(p, 0).right + DiagramSpec.ARROW_GAP, ret.points.last().x, 0.01f)
    }

    @Test
    fun `a cycle with a large font is a column on a tablet`() {
        val p = plan(cycle(5), tablet, fontScale = 1.5f)!!
        assertEquals(DiagramOrientation.Narrow, p.orientation)
        assertSound(p, tablet)
    }

    @Test
    fun `a cycle is never null for any readable width and font`() {
        for (n in 3..8) {
            for (w in listOf(296f, 360f, 568f)) {
                for (fs in listOf(1f, 1.3f, 2f)) {
                    val p = plan(cycle(n), w, fs, charWidth = 7f * fs)
                    assertNotNull("n=$n w=$w fs=$fs", p)
                    assertSound(p!!, w)
                }
            }
        }
    }

    // Fixture

    @Test
    fun `the fixture diagrams are laid out on tablet and phone and in a large font`() {
        val doc = (LessonParser.parse(Fixtures.text("lesson-scientific-method.json")) as ParseResult.Usable).document
        for (d in doc.blocks.filterIsInstance<Diagram>()) {
            for ((w, fs) in listOf(tablet to 1f, phone to 1f, tablet to 2f, phone to 2f)) {
                val p = plan(d, w, fs, charWidth = 7f * fs)
                if (p != null) assertSound(p, w)
            }
            assertNotNull("${d.id} on a tablet", plan(d, tablet))
        }
    }
}
