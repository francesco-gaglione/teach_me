package com.teachme.sdui.highlight

import com.teachme.sdui.inline.Inline
import com.teachme.sdui.inline.SpannedInline

/** A half-open interval `[start, end)` of UTF-16 code units. Used for both the source and the displayed text. */
data class Interval(
    val start: Int,
    val end: Int,
) {
    val isEmpty: Boolean get() = end <= start
    val length: Int get() = end - start
}

/**
 * The link between the markdown source of a block's `text` and the text the learner reads (`lessons.md` section 10).
 * Every displayed char knows the source interval it comes from: its own char for plain text, code and link labels;
 * the whole `[[key]]`, `[^key]` or `[text](url)` for replaced (atomic) elements. Markers are in no interval, so a
 * source interval that only covers markers colours nothing. Pure and immutable.
 */
class SourceMap internal constructor(
    val source: String,
    val displayed: String,
    private val starts: IntArray,
    private val ends: IntArray,
    private val kinds: ByteArray,
    private val nodes: IntArray,
) {
    val displayLength: Int get() = displayed.length

    /**
     * The source interval to save for a displayed one (atomic elements and surrogate pairs taken whole), or null
     * when the interval covers no displayed char.
     */
    fun displayToSource(range: Interval): Interval? {
        val r = close(clamp(range)) ?: return null
        var s = Int.MAX_VALUE
        var e = Int.MIN_VALUE
        for (i in r.start until r.end) {
            s = minOf(s, starts[i])
            e = maxOf(e, ends[i])
        }
        return Interval(s, e)
    }

    /**
     * The displayed intervals to colour for a source interval: every char whose source overlaps it, merged into
     * runs, in order. Empty when it covers nothing (markers only, out of bounds). A partly covered atomic element
     * is coloured whole. Source starts and ends never decrease along the displayed text, so the chars that overlap
     * [range] are one contiguous block, found by binary search: redrawing costs O(log n) per highlight.
     */
    fun sourceToDisplay(range: Interval): List<Interval> {
        if (range.isEmpty) return emptyList()
        var lo = 0
        var hi = ends.size
        while (lo < hi) {
            val mid = (lo + hi) ushr 1
            if (ends[mid] > range.start) hi = mid else lo = mid + 1
        }
        var i = lo
        while (i < starts.size && starts[i] < range.end) i++
        return if (i > lo) listOf(Interval(lo, i)) else emptyList()
    }

    /**
     * The displayed chars that stand for one indivisible element of the source (inline code, a term, a citation, a
     * link shown as text): the whole element around [char]; for any other char, just [char] itself. The word
     * definition of the specification (blank to blank, `WordIndex`) uses it so such an element never splits.
     */
    fun atomicUnitAt(char: Int): Interval {
        if (char < 0 || char >= displayLength || kinds[char] == TEXT) return Interval(char, char + 1)
        var a = char
        while (a > 0 && nodes[a - 1] == nodes[char]) a--
        var b = char + 1
        while (b < displayLength && nodes[b] == nodes[char]) b++
        return Interval(a, b)
    }

    private fun clamp(range: Interval): Interval? {
        val a = maxOf(range.start, 0)
        val b = minOf(range.end, displayLength)
        return if (a < b) Interval(a, b) else null
    }

    /** Widens [range] so it never cuts an atomic element or a surrogate pair. */
    private fun close(range: Interval?): Interval? {
        if (range == null) return null
        var a = range.start
        var b = range.end
        while (a > 0 && starts[a - 1] == starts[a] && ends[a - 1] == ends[a]) a--
        while (b < displayLength && starts[b - 1] == starts[b] && ends[b - 1] == ends[b]) b++
        return Interval(a, b)
    }

    internal companion object {
        const val TEXT: Byte = 0
        const val UNIT: Byte = 1 // inline code, term, citation, shown link fallback: one atomic word
    }
}

/**
 * Builds a [SourceMap] next to the renderer: call [append] once per node, in order, with the length of the text the
 * renderer actually appended for it, then [build] with the final displayed text.
 */
class SourceMapBuilder(
    private val source: String,
) {
    private val starts = ArrayList<Int>()
    private val ends = ArrayList<Int>()
    private val kinds = ArrayList<Byte>()
    private val nodes = ArrayList<Int>()
    private var nodeCount = 0

    /**
     * [displayedLength] chars were appended for [span]. When they do not match the parser's own chars (a term, a
     * citation, a renamed link fallback) the node is atomic.
     */
    fun append(
        span: SpannedInline,
        displayedLength: Int,
    ) {
        var id = nodeCount++
        val charStarts = span.charStarts
        val charEnds = span.charEnds
        val perChar = charStarts != null && charEnds != null && charStarts.size == displayedLength
        val code = span.node is Inline.Code
        var prevStart = -1
        var prevEnd = -1
        var prevAtomic = false
        for (k in 0 until displayedLength) {
            var s = if (perChar) charStarts!![k] else span.start
            var e = if (perChar) charEnds!![k] else span.end
            var pair = false
            if (perChar && e - s == 1) {
                // A surrogate pair is one character: both halves share the interval of the pair.
                val c = source.getOrNull(s)
                if (c != null && Character.isLowSurrogate(c) && s > 0 && Character.isHighSurrogate(source[s - 1])) {
                    s -= 1
                    pair = true
                } else if (c != null &&
                    Character.isHighSurrogate(c) &&
                    s + 1 < source.length &&
                    Character.isLowSurrogate(source[s + 1])
                ) {
                    e = s + 2
                    pair = true
                }
            }
            // A char whose source is longer than itself (a term, a citation, a link shown as text, which the parser
            // may have merged into the text around it) is atomic: so is every char of inline code.
            val atomic = !perChar || code || (e - s > 1 && !pair)
            if (perChar && !code && (atomic != prevAtomic || s != prevStart || e != prevEnd)) id = nodeCount++
            prevStart = s
            prevEnd = e
            prevAtomic = atomic
            starts += s
            ends += e
            kinds += if (atomic) SourceMap.UNIT else SourceMap.TEXT
            nodes += id
        }
    }

    /** Null-safe: when [displayed] does not have the chars appended so far, the map is empty rather than wrong. */
    fun build(displayed: String): SourceMap {
        if (displayed.length !=
            starts.size
        ) {
            return SourceMap(source, "", IntArray(0), IntArray(0), ByteArray(0), IntArray(0))
        }
        return SourceMap(
            source,
            displayed,
            starts.toIntArray(),
            ends.toIntArray(),
            kinds.toByteArray(),
            nodes.toIntArray(),
        )
    }
}
