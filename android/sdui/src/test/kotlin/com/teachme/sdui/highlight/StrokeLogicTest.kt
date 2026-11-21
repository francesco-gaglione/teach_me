package com.teachme.sdui.highlight

import org.junit.Assert.assertEquals
import org.junit.Test

/** Words, erase runs and merging, without any UI (highlight.md 4.2, 5, 7). */
class StrokeLogicTest {
    private val text = "Quando sposti la variabile, il [[own]] passa. Fine"
    private val map = mapFor(text, terms = mapOf("own" to "valore preso"))
    private val index = WordIndex.of(map)
    private val shown = map.displayed

    /** The first whole word [s] (so "il" is not found inside "variabile"). */
    private fun word(s: String) =
        (" $s")
            .let {
                shown.indexOf(it).let { at ->
                    if (at <
                        0
                    ) {
                        shown.indexOf(s)
                    } else {
                        at + 1
                    }
                }
            }.let { Interval(it, it + s.length) }

    @Test
    fun `punctuation stays with its word and a term with spaces is one word`() {
        assertEquals(
            listOf("Quando", "sposti", "la", "variabile,", "il", "valore preso", "passa.", "Fine").map(::word),
            index.words,
        )
    }

    @Test
    fun `a blank belongs to no word`() {
        assertEquals(-1, index.wordAt(shown.indexOf(' ')))
        assertEquals(index.words.indexOf(word("la")), index.wordAt(shown.indexOf("la") + 1))
    }

    @Test
    fun `the span runs from the first to the last word hit, in reading order`() {
        val stroke = StrokeWords(index, shown)
        stroke.add(index.words.indexOf(word("variabile,")))
        stroke.add(index.words.indexOf(word("sposti")))
        stroke.add(-1)
        assertEquals(Interval(word("sposti").start, word("variabile,").end), stroke.span())
    }

    @Test
    fun `the eraser takes only the words passed over, with the blanks around them`() {
        val stroke = StrokeWords(index, shown)
        stroke.add(index.words.indexOf(word("la")))
        stroke.add(index.words.indexOf(word("il")))
        // "la" and "il" are not neighbours (variabile, is in between): two runs, each with its blanks.
        val la = word("la")
        val il = word("il")
        assertEquals(listOf(Interval(la.start - 1, la.end + 1), Interval(il.start - 1, il.end + 1)), stroke.eraseRuns())
    }

    @Test
    fun `neighbour words of the eraser are one run`() {
        val stroke = StrokeWords(index, shown)
        stroke.add(index.words.indexOf(word("sposti")))
        stroke.add(index.words.indexOf(word("la")))
        assertEquals(listOf(Interval(word("sposti").start - 1, word("la").end + 1)), stroke.eraseRuns())
    }

    @Test
    fun `a stroke next to a highlight of the same colour through blanks becomes one`() {
        val existing = listOf(word("Quando"))
        val merged = mergeAcrossBlanks(word("sposti"), existing, shown)
        assertEquals(Interval(word("Quando").start, word("sposti").end), merged)
        // A word in between keeps them apart.
        assertEquals(word("la"), mergeAcrossBlanks(word("la"), existing, shown))
    }

    @Test
    fun `a run splits around the cuts`() {
        val run = Interval(0, 20)
        assertEquals(
            listOf(Interval(0, 5) to false, Interval(5, 8) to true, Interval(8, 20) to false),
            splitRun(run, listOf(Interval(5, 8))),
        )
        assertEquals(listOf(Interval(0, 20) to true), splitRun(run, listOf(Interval(-3, 30))))
    }
}
