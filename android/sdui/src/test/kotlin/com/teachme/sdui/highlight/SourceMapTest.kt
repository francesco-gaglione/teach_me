package com.teachme.sdui.highlight

import com.teachme.sdui.inline.Inline
import com.teachme.sdui.inline.InlineMarkdown
import com.teachme.sdui.inline.inlineCodeDisplay
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Builds the map the way the renderer does: terms by glossary, citations as thin space + `[n]`, unknown ones empty. */
internal fun mapFor(
    source: String,
    terms: Map<String, String> = emptyMap(),
    cites: Map<String, Int> = emptyMap(),
    /** Draw inline code as the renderer does (hair space padding). */
    padCode: Boolean = false,
): SourceMap {
    val builder = SourceMapBuilder(source)
    val shown = StringBuilder()
    for (span in InlineMarkdown.parseSpanned(source)) {
        val text =
            when (val n = span.node) {
                is Inline.Text -> n.text
                is Inline.Code -> if (padCode) inlineCodeDisplay(n.text) else n.text
                is Inline.Link -> n.label
                is Inline.Term -> n.text ?: terms[n.key] ?: n.key
                is Inline.Cite -> cites[n.key]?.let { " [$it]" } ?: ""
            }
        shown.append(text)
        builder.append(span, text.length)
    }
    return builder.build(shown.toString())
}

internal fun String.iv(sub: String): Interval {
    val i = indexOf(sub)
    check(i >= 0) { "'$sub' not in '$this'" }
    return Interval(i, i + sub.length)
}

class SourceMapTest {
    @Test
    fun `plain text is the identity`() {
        val m = mapFor("Ciao mondo")
        assertEquals(Interval(2, 6), m.displayToSource(Interval(2, 6)))
        assertEquals(listOf(Interval(2, 6)), m.sourceToDisplay(Interval(2, 6)))
    }

    @Test
    fun `nested bold and italic markers do not count`() {
        val src = "a **b *c* d** e"
        val m = mapFor(src)
        assertEquals("a b c d e", m.displayed)
        assertEquals(Interval(4, 8), m.displayToSource(Interval(2, 5)))
        assertEquals(listOf(Interval(4, 5)), m.sourceToDisplay(Interval(6, 9)))
        // a range that is only markers colours nothing
        assertEquals(emptyList<Interval>(), m.sourceToDisplay(Interval(8, 9)))
        assertEquals(emptyList<Interval>(), m.sourceToDisplay(Interval(11, 13)))
    }

    @Test
    fun `range that starts or ends in the middle of a marker`() {
        val src = "a **b** c"
        val m = mapFor(src)
        assertEquals(listOf(Interval(2, 3)), m.sourceToDisplay(Interval(3, 5)))
        assertEquals(listOf(Interval(2, 3)), m.sourceToDisplay(Interval(4, 6)))
        assertEquals(listOf(Interval(1, 2)), m.sourceToDisplay(Interval(1, 3)))
        assertEquals(emptyList<Interval>(), m.sourceToDisplay(Interval(5, 7)))
    }

    @Test
    fun `link label maps char by char and the url part counts as marker`() {
        val src = "vedi [docs](https://x.io/a) ora"
        val m = mapFor(src)
        assertEquals("vedi docs ora", m.displayed)
        val label = src.iv("docs")
        assertEquals(label, m.displayToSource(Interval(5, 9)))
        assertEquals(emptyList<Interval>(), m.sourceToDisplay(Interval(src.indexOf("]("), src.indexOf(")") + 1)))
        assertEquals(listOf(Interval(6, 8)), m.sourceToDisplay(Interval(label.start + 1, label.end - 1)))
    }

    @Test
    fun `non https link is shown with its url and is atomic`() {
        val src = "x [a](ftp://b) y"
        val m = mapFor(src)
        assertEquals("x a (ftp://b) y", m.displayed)
        assertEquals(src.iv("[a](ftp://b)"), m.displayToSource(Interval(3, 4)))
        assertEquals(listOf(Interval(2, 13)), m.sourceToDisplay(Interval(3, 4)))
    }

    @Test
    fun `term with display text is atomic`() {
        val src = "il [[ent|entropia]] sale"
        val m = mapFor(src)
        assertEquals("il entropia sale", m.displayed)
        assertEquals(src.iv("[[ent|entropia]]"), m.displayToSource(Interval(5, 6)))
        // range inside the markers, or covering only part of the element: the element in full
        assertEquals(listOf(Interval(3, 11)), m.sourceToDisplay(Interval(4, 6)))
        assertEquals(listOf(Interval(3, 11)), m.sourceToDisplay(Interval(13, 14)))
    }

    @Test
    fun `term replaced by the glossary entry`() {
        val src = "il [[ent]] sale"
        val m = mapFor(src, terms = mapOf("ent" to "Entropia"))
        assertEquals("il Entropia sale", m.displayed)
        assertEquals(src.iv("[[ent]]"), m.displayToSource(Interval(3, 4)))
        assertEquals(listOf(Interval(3, 11)), m.sourceToDisplay(Interval(3, 5)))
    }

    @Test
    fun `unknown term is shown as its key and stays atomic`() {
        val src = "il [[zzz]] sale"
        val m = mapFor(src)
        assertEquals("il zzz sale", m.displayed)
        assertEquals(src.iv("[[zzz]]"), m.displayToSource(Interval(4, 5)))
    }

    @Test
    fun `citation is a thin space plus number and atomic`() {
        val src = "vero[^s1] dopo"
        val m = mapFor(src, cites = mapOf("s1" to 1))
        assertEquals("vero [1] dopo", m.displayed)
        assertEquals(src.iv("[^s1]"), m.displayToSource(Interval(4, 5)))
        assertEquals(src.iv("[^s1]"), m.displayToSource(Interval(6, 7)))
        assertEquals(listOf(Interval(4, 8)), m.sourceToDisplay(Interval(5, 7)))
        // the word before it is not touched by the citation's source
        assertEquals(listOf(Interval(0, 4)), m.sourceToDisplay(Interval(0, 4)))
    }

    @Test
    fun `unknown citation shows nothing and highlights nothing`() {
        val src = "vero[^zz] dopo"
        val m = mapFor(src)
        assertEquals("vero dopo", m.displayed)
        assertEquals(emptyList<Interval>(), m.sourceToDisplay(src.iv("[^zz]")))
        assertEquals(Interval(0, 4), m.displayToSource(Interval(0, 4)))
    }

    @Test
    fun `unclosed markers stay literal and map to themselves`() {
        val src = "a **b `c [d [[e [^f"
        val m = mapFor(src)
        assertEquals(src, m.displayed)
        assertEquals(Interval(2, 4), m.displayToSource(Interval(2, 4)))
        assertEquals(listOf(Interval(2, 4)), m.sourceToDisplay(Interval(2, 4)))
    }

    @Test
    fun `code is mapped inside its backticks`() {
        val src = "usa `foo bar` ora"
        val m = mapFor(src)
        assertEquals("usa foo bar ora", m.displayed)
        assertEquals(src.iv("foo"), m.displayToSource(Interval(4, 7)))
        assertEquals(emptyList<Interval>(), m.sourceToDisplay(Interval(4, 5)))
    }

    @Test
    fun `chars outside the BMP are one unit`() {
        val src = "x 😀 **y**"
        val m = mapFor(src)
        assertEquals("x 😀 y", m.displayed)
        // a displayed interval that cuts the pair is widened to the whole pair
        assertEquals(Interval(2, 4), m.displayToSource(Interval(2, 3)))
        assertEquals(Interval(2, 4), m.displayToSource(Interval(3, 4)))
        // a source interval on the second half colours both halves
        assertEquals(listOf(Interval(2, 4)), m.sourceToDisplay(Interval(3, 4)))
        assertEquals(listOf(Interval(5, 6)), m.sourceToDisplay(Interval(7, 8)))
        assertEquals(Interval(7, 8), m.displayToSource(Interval(5, 6)))
    }

    @Test
    fun `empty and out of range intervals`() {
        val m = mapFor("abc")
        assertNull(m.displayToSource(Interval(2, 2)))
        assertNull(m.displayToSource(Interval(5, 9)))
        assertEquals(Interval(1, 3), m.displayToSource(Interval(1, 99)))
        assertEquals(emptyList<Interval>(), m.sourceToDisplay(Interval(2, 2)))
        assertEquals(emptyList<Interval>(), m.sourceToDisplay(Interval(10, 20)))
    }

    @Test
    fun `a map built with the wrong length is empty instead of wrong`() {
        val b = SourceMapBuilder("abc")
        b.append(InlineMarkdown.parseSpanned("abc").single(), 3)
        val m = b.build("ab")
        assertEquals(0, m.displayLength)
        assertNull(m.displayToSource(Interval(0, 1)))
    }

    private val rich =
        "Il **gatto *nero*** salta [[ent|l'albero]], vedi [docs](https://x.io) e `a b`[^s1] " +
            "😀 `x`è [[k]] [^zz] [a](ftp://b) *aperto"

    @Test
    fun `convert save and convert back colours the same chars`() {
        val m = mapFor(rich, terms = mapOf("k" to "Chiave"), cites = mapOf("s1" to 1))
        val words = WordIndex.of(m).words
        assertTrue(words.size > 10)
        for (a in words.indices) {
            for (b in a until words.size) {
                val span = Interval(words[a].start, words[b].end)
                val source = m.displayToSource(span)
                assertNotNull(source)
                assertEquals("words $a..$b", listOf(span), m.sourceToDisplay(source!!))
            }
        }
    }

    @Test
    fun `source intervals always give runs inside the text`() {
        val m = mapFor(rich, terms = mapOf("k" to "Chiave"), cites = mapOf("s1" to 1))
        for (a in 0..rich.length) {
            for (b in a..rich.length) {
                for (r in m.sourceToDisplay(Interval(a, b))) {
                    assertTrue(r.start >= 0 && r.end <= m.displayLength && r.start < r.end)
                }
            }
        }
    }
}
