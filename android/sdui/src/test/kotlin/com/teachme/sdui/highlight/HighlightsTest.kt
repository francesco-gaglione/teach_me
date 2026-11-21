package com.teachme.sdui.highlight

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test

class HighlightsTest {
    private val json = Json { ignoreUnknownKeys = true }

    private fun h(
        s: Int,
        e: Int,
        c: HighlightColor = HighlightColor.YELLOW,
        b: String = "p1",
    ) = Highlight(b, s, e, c)

    private val y = HighlightColor.YELLOW
    private val g = HighlightColor.GREEN

    @Test
    fun `contract json round trip`() {
        val parsed = json.decodeFromString<Highlight>("""{"block_id":"p3","start":12,"end":47,"color":"green"}""")
        assertEquals(Highlight("p3", 12, 47, g), parsed)
        assertEquals("""{"block_id":"p3","start":12,"end":47,"color":"green"}""", json.encodeToString(parsed))
        assertEquals(
            listOf(h(1, 2, y), h(3, 4, HighlightColor.BLUE), h(5, 6, HighlightColor.PINK)),
            json.decodeFromString<List<Highlight>>(
                """[{"block_id":"p1","start":1,"end":2,"color":"yellow"},
                   {"block_id":"p1","start":3,"end":4,"color":"blue"},
                   {"block_id":"p1","start":5,"end":6,"color":"pink","extra":1}]""",
            ),
        )
    }

    @Test
    fun `unknown colour is yellow`() {
        val parsed = json.decodeFromString<Highlight>("""{"block_id":"p3","start":1,"end":2,"color":"magenta"}""")
        assertEquals(y, parsed.color)
        assertEquals(y, HighlightColor.fromWire(null))
    }

    @Test
    fun `add on an empty block`() {
        assertEquals(listOf(h(2, 5)), Highlights.add(emptyList(), "p1", 2, 5, y))
    }

    @Test
    fun `add keeps disjoint and touching highlights of other colours`() {
        val start = listOf(h(0, 3, g), h(10, 12, g))
        assertEquals(listOf(h(0, 3, g), h(3, 6), h(10, 12, g)), Highlights.add(start, "p1", 3, 6, y))
    }

    @Test
    fun `add replaces the covered part and keeps the sides`() {
        val start = listOf(h(0, 10, g))
        assertEquals(listOf(h(0, 3, g), h(3, 6), h(6, 10, g)), Highlights.add(start, "p1", 3, 6, y))
        assertEquals(listOf(h(0, 4, g), h(4, 12)), Highlights.add(start, "p1", 4, 12, y))
    }

    @Test
    fun `add swallows highlights it fully covers`() {
        val start = listOf(h(2, 4, g), h(5, 7, g), h(20, 22, g))
        assertEquals(listOf(h(0, 10), h(20, 22, g)), Highlights.add(start, "p1", 0, 10, y))
    }

    @Test
    fun `same colour touching or overlapping becomes one`() {
        assertEquals(listOf(h(0, 6)), Highlights.add(listOf(h(0, 3)), "p1", 3, 6, y))
        assertEquals(listOf(h(0, 6)), Highlights.add(listOf(h(3, 6)), "p1", 0, 3, y))
        assertEquals(listOf(h(0, 8)), Highlights.add(listOf(h(0, 5)), "p1", 3, 8, y))
        assertEquals(listOf(h(0, 9)), Highlights.add(listOf(h(0, 3), h(6, 9)), "p1", 3, 6, y))
    }

    @Test
    fun `merging chains through a replaced middle highlight`() {
        val start = listOf(h(0, 5), h(5, 8, g), h(8, 10))
        assertEquals(listOf(h(0, 10)), Highlights.add(start, "p1", 4, 9, y))
    }

    @Test
    fun `adding inside a same colour highlight changes nothing`() {
        assertEquals(listOf(h(0, 10)), Highlights.add(listOf(h(0, 10)), "p1", 3, 6, y))
    }

    @Test
    fun `invalid intervals change nothing`() {
        val start = listOf(h(0, 3))
        assertEquals(start, Highlights.add(start, "p1", 5, 5, y))
        assertEquals(start, Highlights.add(start, "p1", 6, 5, y))
        assertEquals(start, Highlights.add(start, "p1", -1, 5, y))
        assertEquals(start, Highlights.remove(start, "p1", 5, 5))
    }

    @Test
    fun `other blocks are never touched`() {
        val start = listOf(h(0, 3, g, "p2"), h(1, 4, g, "p1"), h(0, 9, y, "p3"))
        val out = Highlights.add(start, "p1", 2, 6, y)
        assertEquals(listOf(h(0, 3, g, "p2"), h(1, 2, g, "p1"), h(2, 6, y, "p1"), h(0, 9, y, "p3")), out)
        assertEquals(start.filter { it.blockId != "p1" }, Highlights.remove(start, "p1", 0, 99))
    }

    @Test
    fun `remove splits a highlight in two`() {
        assertEquals(listOf(h(0, 3), h(6, 10)), Highlights.remove(listOf(h(0, 10)), "p1", 3, 6))
    }

    @Test
    fun `remove trims the edges and drops covered highlights`() {
        val start = listOf(h(0, 5), h(5, 8, g), h(8, 12))
        assertEquals(listOf(h(0, 4), h(10, 12)), Highlights.remove(start, "p1", 4, 10))
        assertEquals(emptyList<Highlight>(), Highlights.remove(listOf(h(2, 4)), "p1", 0, 9))
        assertEquals(listOf(h(2, 4)), Highlights.remove(listOf(h(2, 4)), "p1", 4, 9))
    }

    @Test
    fun `remove the whole highlight at a position`() {
        val start = listOf(h(0, 5), h(5, 8, g), h(10, 12))
        assertEquals(listOf(h(0, 5), h(10, 12)), Highlights.removeAt(start, "p1", 6))
        assertEquals(listOf(h(5, 8, g), h(10, 12)), Highlights.removeAt(start, "p1", 0))
        assertEquals(start, Highlights.removeAt(start, "p1", 8))
        assertEquals(start, Highlights.removeAt(start, "p2", 6))
        assertEquals(h(5, 8, g), Highlights.at(start, "p1", 5))
        assertEquals(null, Highlights.at(start, "p1", 9))
    }

    @Test
    fun `no overlaps whatever the sequence of operations`() {
        var set = emptyList<Highlight>()
        val colors = HighlightColor.entries
        var seed = 7L

        fun next(n: Int): Int {
            seed = (seed * 6364136223846793005L + 1442695040888963407L)
            return ((seed ushr 33) % n).toInt()
        }
        repeat(400) {
            val a = next(40)
            val b = a + 1 + next(15)
            set =
                if (next(3) == 0) {
                    Highlights.remove(set, "p1", a, b)
                } else {
                    Highlights.add(set, "p1", a, b, colors[next(4)])
                }
            val sorted = set.sortedBy { it.start }
            sorted.zipWithNext().forEach { (x, y2) -> assert(x.end <= y2.start) { "overlap $x $y2" } }
            assert(set.all { it.start < it.end })
        }
    }

    @Test
    fun `dirty input with overlaps is repaired by the next operation`() {
        val start = listOf(h(0, 6), h(4, 9, g))
        assertEquals(listOf(h(0, 6), h(6, 9, g), h(20, 22)), Highlights.add(start, "p1", 20, 22, y))
    }

    // --- marker-only fragments and tap by overlap ---

    private fun visible(
        all: List<Highlight>,
        m: SourceMap,
    ) = all.filter { m.sourceToDisplay(it.interval).isNotEmpty() }

    @Test
    fun `erasing a word leaves no marker-only fragment`() {
        // "**foo** bar": the whole text highlighted, "foo" is source [2,5)
        val m = mapFor("**foo** bar")
        val all = listOf(h(0, 11))
        val range = Highlights.resolveRemove(all, "p1", 2, 5, m)
        assertEquals(Interval(0, 5), range)
        val after = Highlights.remove(all, "p1", 2, 5, m)
        assertEquals(listOf(h(5, 11)), after)
        assertEquals(after, visible(after, m))
        // applying the resolved range without the map gives the same set (what the host does)
        assertEquals(after, Highlights.remove(all, "p1", range.start, range.end))
    }

    @Test
    fun `erasing the last word of a link leaves no url fragment`() {
        val m = mapFor("a [bc](https://x.io) d")
        val all = listOf(h(0, 22))
        // erase "bc" (source [3,5)) and the right remainder is only "](https://x.io)" plus the rest " d"
        val after = Highlights.remove(all, "p1", 3, 5, m)
        assertEquals(after, visible(after, m))
        // erase everything after the label: the remainder before it keeps the text, nothing invisible is left
        val only = listOf(h(3, 20))
        val after2 = Highlights.remove(only, "p1", 3, 5, m)
        assertEquals(emptyList<Highlight>(), after2)
    }

    @Test
    fun `overwriting leaves no marker-only fragment and merges again`() {
        val m = mapFor("**foo** bar *baz*")
        val green = HighlightColor.GREEN
        val all = listOf(Highlight("p1", 0, 17, y))
        val range = Highlights.resolveAdd(all, "p1", 2, 5, green, m)
        assertEquals(Interval(0, 5), range)
        val after = Highlights.add(all, "p1", 2, 5, green, m)
        assertEquals(listOf(Highlight("p1", 0, 5, green), Highlight("p1", 5, 17, y)), after)
        assertEquals(after, visible(after, m))
        assertEquals(after, Highlights.add(all, "p1", range.start, range.end, green))
    }

    @Test
    fun `no invisible fragment whatever the edit`() {
        val src = "Il **gatto *nero*** salta [a](https://x.io) e `x` ok"
        val m = mapFor(src)
        val n = src.length
        for (a in 0 until n) {
            for (b in a + 1..n) {
                val start = m.displayToSource(m.sourceToDisplay(Interval(a, b)).firstOrNull() ?: continue) ?: continue
                val all = listOf(h(0, n), h(0, 0 + 1).copy(blockId = "other"))
                val removed = Highlights.remove(all, "p1", start.start, start.end, m)
                assertEquals(removed.filter { it.blockId == "p1" }, visible(removed, m).filter { it.blockId == "p1" })
                val added = Highlights.add(all, "p1", start.start, start.end, g, m)
                assertEquals(added, visible(added, m))
            }
        }
    }

    @Test
    fun `overlapping finds a highlight that starts in the middle of an atomic element`() {
        // "la [[k]] sale": the term is source [3,8); another client saved a highlight starting at 5
        val m = mapFor("la [[k]] sale", terms = mapOf("k" to "Chiave"))
        val all = listOf(h(5, 13))
        val tapped = m.displayToSource(Interval(4, 5))!! // a char inside "Chiave"
        assertEquals(Interval(3, 8), tapped)
        assertEquals(null, Highlights.at(all, "p1", tapped.start))
        assertEquals(all[0], Highlights.overlapping(all, "p1", tapped))
        // and what the drawing colours is exactly what can be tapped
        assertEquals(listOf(Interval(3, 14)), m.sourceToDisplay(all[0].interval))
    }

    @Test
    fun `overlapping ignores other blocks and touching intervals`() {
        val all = listOf(h(0, 5), h(10, 12, b = "p2"))
        assertEquals(null, Highlights.overlapping(all, "p1", Interval(5, 6)))
        assertEquals(null, Highlights.overlapping(all, "p1", Interval(10, 11)))
        assertEquals(all[0], Highlights.overlapping(all, "p1", Interval(4, 6)))
    }
}
