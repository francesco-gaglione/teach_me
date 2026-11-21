package com.teachme.sdui.highlight

import org.junit.Assert.assertEquals
import org.junit.Test

/** The one definition of a word (highlight.md 4.2.1): blank to blank, punctuation included, atomic elements whole. */
class WordUnitsTest {
    private fun words(
        source: String,
        terms: Map<String, String> = emptyMap(),
        cites: Map<String, Int> = emptyMap(),
        padCode: Boolean = false,
    ): List<String> {
        val m = mapFor(source, terms, cites, padCode)
        return WordIndex.of(m).words.map { m.displayed.substring(it.start, it.end) }
    }

    @Test
    fun `a word runs from blank to blank with its punctuation`() {
        assertEquals(listOf("il", "gatto,", "l'albero", "auto-esame."), words("il gatto, l'albero auto-esame."))
    }

    @Test
    fun `the padding of inline code does not split the word it touches`() {
        val h = "\u200A"
        assertEquals(listOf("(${h}x$h)"), words("(`x`)", padCode = true))
        assertEquals(listOf("foo${h}bar$h"), words("foo`bar`", padCode = true))
    }

    @Test
    fun `markup inside a word does not split it`() {
        assertEquals(listOf("il", "gatto", "nero"), words("il ga**tt**o nero"))
    }

    @Test
    fun `inline code is one word even with blanks inside`() {
        assertEquals(listOf("usa", "foo bar()", "ora"), words("usa `foo bar()` ora"))
        assertEquals(listOf("abc d", "x"), words("ab`c d` x"))
    }

    @Test
    fun `a term is one word and takes the word it is glued to`() {
        assertEquals(listOf("la", "due parole", "sale"), words("la [[k|due parole]] sale"))
        assertEquals(listOf("la", "pre-due parole"), words("la pre-[[k|due parole]]"))
    }

    @Test
    fun `a citation takes the word it is glued to`() {
        val w = words("vero[^s1] dopo", cites = mapOf("s1" to 1))
        assertEquals(2, w.size)
        assertEquals("dopo", w[1])
        assertEquals("vero", w[0].take(4))
    }

    @Test
    fun `a link shown as text stays one word even merged with the text around it`() {
        assertEquals(listOf("x", "a (ftp://b)", "y"), words("x [a](ftp://b) y"))
    }

    @Test
    fun `non BMP chars are never cut`() {
        assertEquals(listOf("a", "𝐀bc", "😀", "d"), words("a 𝐀bc 😀 d"))
    }

    @Test
    fun `atomicUnitAt gives the whole element or just the char`() {
        val m = mapFor("usa `foo bar()` ora")
        assertEquals(Interval(4, 13), m.atomicUnitAt(6))
        assertEquals(Interval(1, 2), m.atomicUnitAt(1))
        assertEquals(Interval(-1, 0), m.atomicUnitAt(-1))
    }
}
