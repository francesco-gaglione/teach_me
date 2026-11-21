package com.teachme.sdui.inline

import org.junit.Assert.assertEquals
import org.junit.Test

class InlineSpansTest {
    private fun spans(s: String) = InlineMarkdown.parseSpanned(s).map { Triple(it.node, it.start, it.end) }

    @Test
    fun `parse is the nodes of parseSpanned`() {
        val s = "a **b *c* d** [[k]] `x` [^s] [l](https://x.io) [u](ftp://z) *no"
        assertEquals(InlineMarkdown.parseSpanned(s).map { it.node }, InlineMarkdown.parse(s))
    }

    @Test
    fun `every node knows its source interval`() {
        val s = "a **b** [[k|t]] [^s] `c` [l](https://x.io)"
        assertEquals(
            listOf(
                Triple(Inline.Text("a "), 0, 2),
                Triple(Inline.Text("b", bold = true), 4, 5),
                Triple(Inline.Text(" "), 7, 8),
                Triple(Inline.Term("k", "t"), 8, 15),
                Triple(Inline.Text(" "), 15, 16),
                Triple(Inline.Cite("s"), 16, 20),
                Triple(Inline.Text(" "), 20, 21),
                Triple(Inline.Code("c"), 21, 24),
                Triple(Inline.Text(" "), 24, 25),
                Triple(Inline.Link("l", "https://x.io"), 25, 42),
            ),
            spans(s),
        )
    }

    @Test
    fun `unclosed markers stay in the text node`() {
        assertEquals(listOf(Triple(Inline.Text("a **b"), 0, 5)), spans("a **b"))
    }
}
