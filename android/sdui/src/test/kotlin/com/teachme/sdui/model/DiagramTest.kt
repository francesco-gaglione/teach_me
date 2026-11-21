package com.teachme.sdui.model

import com.teachme.sdui.Fixtures
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DiagramTest {
    private fun node(id: String) = DiagramNode(id, id.uppercase())

    private fun edge(
        from: String,
        to: String,
    ) = DiagramEdge(from, to)

    private fun diagram(
        layout: String,
        nodes: List<String>,
        edges: List<DiagramEdge> = emptyList(),
        alt: String = "alt",
    ) = Diagram("d", null, layout, null, nodes.map(::node), edges, "caption", alt)

    private fun problem(d: Diagram) = (validateDiagram(d) as? DiagramValidation.TextOnly)?.problem

    private fun block(json: String): Block {
        val doc = """{"blocks":[{"id":"p","type":"paragraph","text":"x"},$json]}"""
        return (LessonParser.parse(doc) as ParseResult.Usable).document.blocks[1]
    }

    // Parser

    @Test
    fun `the fixture diagrams parse and are drawable`() {
        val doc = (LessonParser.parse(Fixtures.text("lesson-scientific-method.json")) as ParseResult.Usable).document
        val diagrams = doc.blocks.filterIsInstance<Diagram>()
        assertEquals(setOf("cycle", "tree", "flow"), diagrams.map { it.layout }.toSet())
        assertEquals(3, diagrams.size)
        diagrams.forEach { assertTrue(it.id, validateDiagram(it) is DiagramValidation.Drawable) }
    }

    @Test
    fun `a valid diagram keeps all its fields`() {
        val d =
            block(
                """{"id":"d","type":"diagram","layout":"flow","title":"T","lang":"en",
                  |"nodes":[{"id":"a","label":"A","detail":"da"},{"id":"b","label":"B"}],
                  |"edges":[{"from":"a","to":"b","label":"go"}],"caption":"**c**","alt":"a to b"}
                """.trimMargin(),
            ) as Diagram
        assertEquals("flow", d.layout)
        assertEquals("T", d.title)
        assertEquals("en", d.lang)
        assertEquals(listOf(DiagramNode("a", "A", "da"), DiagramNode("b", "B")), d.nodes)
        assertEquals(listOf(DiagramEdge("a", "b", "go")), d.edges)
        assertEquals("a to b", d.alt)
        assertTrue(validateDiagram(d) is DiagramValidation.Drawable)
    }

    @Test
    fun `an unknown layout parses and falls back to text`() {
        val d =
            block(
                """{"id":"d","type":"diagram","layout":"matrix","nodes":[{"id":"a","label":"A"},{"id":"b","label":"B"}],
                  |"caption":"c","alt":"words"}
                """.trimMargin(),
            ) as Diagram
        assertEquals(DiagramProblem.UnknownLayout, problem(d))
    }

    @Test
    fun `a diagram with an invalid structure still parses as a diagram`() {
        val doc = (LessonParser.parse(Fixtures.text("invalid/diagram_structure.json")) as ParseResult.Usable)
        val diagrams = doc.document.blocks.filterIsInstance<Diagram>()
        assertTrue(diagrams.isNotEmpty())
        assertTrue(diagrams.any { validateDiagram(it) is DiagramValidation.TextOnly })
        assertTrue(doc.events.any { it is SduiEvent.MalformedBlock && it.detail?.startsWith("diagram:") == true })
    }

    @Test
    fun `a blank or missing alt makes the block unavailable, not a text fallback`() {
        val nodes = """"nodes":[{"id":"a","label":"A"},{"id":"b","label":"B"},{"id":"c","label":"C"}]"""
        val variants =
            listOf(
                """{"id":"d","type":"diagram","layout":"cycle",$nodes,"caption":"c","alt":""}""",
                """{"id":"d","type":"diagram","layout":"cycle",$nodes,"caption":"c","alt":"   "}""",
                """{"id":"d","type":"diagram","layout":"cycle",$nodes,"caption":"c"}""",
            )
        for (json in variants) {
            val result = block(json)
            assertTrue(json, result is Unavailable && result.type == "diagram")
        }
    }

    @Test
    fun `malformed node fields do not crash the parser`() {
        val d =
            block(
                """{"id":"d","type":"diagram","layout":"flow","nodes":[{"id":"a"},{"label":"B"}],"edges":[{"from":"a"}],
                  |"caption":"c","alt":"words"}
                """.trimMargin(),
            )
        assertTrue(d is Diagram)
        assertEquals(DiagramProblem.BlankNode, problem(d as Diagram))
        val wrongType = block("""{"id":"d","type":"diagram","layout":"flow","nodes":"nope","alt":"x"}""")
        assertTrue(wrongType is Unavailable && wrongType.reason == UnavailableReason.Malformed)
    }

    // Structure

    @Test
    fun `flow rules`() {
        val ok = diagram("flow", listOf("a", "b", "c"), listOf(edge("a", "b"), edge("a", "c")))
        assertTrue(validateDiagram(ok) is DiagramValidation.Drawable)
        assertEquals(DiagramProblem.FlowNeedsEdges, problem(diagram("flow", listOf("a", "b"))))
        assertEquals(
            DiagramProblem.FlowHasCycle,
            problem(diagram("flow", listOf("a", "b", "c"), listOf(edge("a", "b"), edge("b", "c"), edge("c", "a")))),
        )
        assertEquals(
            DiagramProblem.FlowIsolatedNode,
            problem(diagram("flow", listOf("a", "b", "c"), listOf(edge("a", "b")))),
        )
        assertEquals(
            DiagramProblem.FlowNotConnected,
            problem(diagram("flow", listOf("a", "b", "c", "d"), listOf(edge("a", "b"), edge("c", "d")))),
        )
    }

    @Test
    fun `tree rules`() {
        val ok = diagram("tree", listOf("r", "a", "b"), listOf(edge("r", "a"), edge("r", "b")))
        assertTrue(validateDiagram(ok) is DiagramValidation.Drawable)
        assertEquals(
            DiagramProblem.TreeEdgeCount,
            problem(diagram("tree", listOf("r", "a", "b"), listOf(edge("r", "a")))),
        )
        // Two roots, one of them with two children: right edge count, wrong shape.
        assertEquals(
            DiagramProblem.TreeParents,
            problem(diagram("tree", listOf("r", "s", "a"), listOf(edge("r", "a"), edge("s", "a")))),
        )
        // A cycle detached from the root: every node has one parent except the root, all edges counted.
        val detached = listOf(edge("r", "a"), edge("b", "c"), edge("c", "b"))
        assertEquals(DiagramProblem.TreeNotConnected, problem(diagram("tree", listOf("r", "a", "b", "c"), detached)))
    }

    @Test
    fun `cycle rules`() {
        assertTrue(validateDiagram(diagram("cycle", listOf("a", "b", "c"))) is DiagramValidation.Drawable)
        assertEquals(DiagramProblem.CycleTooSmall, problem(diagram("cycle", listOf("a", "b"))))
        assertEquals(
            DiagramProblem.CycleHasEdges,
            problem(diagram("cycle", listOf("a", "b", "c"), listOf(edge("a", "b")))),
        )
    }

    @Test
    fun `edge references and common limits`() {
        assertEquals(
            DiagramProblem.UnknownEndpoint,
            problem(diagram("flow", listOf("a", "b"), listOf(edge("a", "zzz")))),
        )
        assertEquals(DiagramProblem.SelfLoop, problem(diagram("flow", listOf("a", "b"), listOf(edge("a", "a")))))
        assertEquals(
            DiagramProblem.DuplicateEdge,
            problem(diagram("flow", listOf("a", "b"), listOf(edge("a", "b"), edge("a", "b")))),
        )
        assertEquals(DiagramProblem.NodeCount, problem(diagram("cycle", listOf("a"))))
        assertEquals(DiagramProblem.NodeCount, problem(diagram("cycle", ('a'..'i').map { it.toString() })))
        assertEquals(DiagramProblem.DuplicateNodeId, problem(diagram("cycle", listOf("a", "a", "b"))))
        assertEquals(DiagramProblem.BlankNode, problem(diagram("cycle", listOf("a", "b", " "))))
    }

    @Test
    fun `a whitespace-only label is accepted like the backend does, an empty or blank id is not`() {
        val spaced =
            diagram("cycle", listOf("a", "b", "c")).let { d ->
                d.copy(nodes = d.nodes.mapIndexed { i, n -> if (i == 2) n.copy(label = "   ") else n })
            }
        assertNull(problem(spaced))
        val noId = spaced.copy(nodes = spaced.nodes.mapIndexed { i, n -> if (i == 2) n.copy(id = " ") else n })
        assertEquals(DiagramProblem.BlankNode, problem(noId))
    }
}
