package com.teachme.sdui.highlight

/** The lines and glyph boxes of a laid out text, in pixels of the text node. Wraps `TextLayoutResult`. */
internal interface TextGeometry {
    val lineCount: Int

    fun lineTop(line: Int): Float

    fun lineBottom(line: Int): Float

    fun lineStart(line: Int): Int

    /** End (exclusive) of the displayed chars of the line, without the blank that was the wrap. */
    fun lineVisibleEnd(line: Int): Int

    fun charLeft(char: Int): Float

    fun charRight(char: Int): Float
}

/**
 * Turns the points of a pen stroke into words (highlight.md section 4.2): a point belongs to a line when it is in the
 * line band extended by [tolerancePx], and hits the word whose box contains it, a blank going to the nearest word.
 */
internal class WordHitTester(
    private val geometry: TextGeometry,
    private val text: String,
    private val words: WordIndex,
    private val tolerancePx: Float,
) {
    /** The index of the word hit by the point, or -1 when the point is out of every line band or the line is empty. */
    fun hitWord(
        x: Float,
        y: Float,
    ): Int {
        val line = lineAt(y)
        if (line < 0) return -1
        val char = charAt(line, x)
        if (char < 0) return -1
        return wordFor(char, x)
    }

    /** The line whose band (extended by the tolerance) holds [y]; the nearest one when two bands overlap. */
    fun lineAt(y: Float): Int {
        var best = -1
        var bestDistance = Float.MAX_VALUE
        for (line in 0 until geometry.lineCount) {
            val top = geometry.lineTop(line)
            val bottom = geometry.lineBottom(line)
            if (y < top - tolerancePx || y > bottom + tolerancePx) continue
            val distance =
                when {
                    y < top -> top - y
                    y > bottom -> y - bottom
                    else -> 0f
                }
            if (distance < bestDistance) {
                best = line
                bestDistance = distance
            }
        }
        return best
    }

    /** The displayed char of [line] under [x]: the first or the last one when the point is before or after the line. */
    fun charAt(
        line: Int,
        x: Float,
    ): Int {
        val first = geometry.lineStart(line)
        val end = geometry.lineVisibleEnd(line)
        if (end <= first) return -1
        if (x < geometry.charLeft(first)) return first
        for (i in first until end) {
            if (x < geometry.charRight(i)) return i
        }
        return end - 1
    }

    private fun wordFor(
        char: Int,
        x: Float,
    ): Int {
        val inside = words.wordAt(char)
        if (inside >= 0) return inside
        val before = words.wordBefore(char)
        val next = words.words.indexOfFirst { it.start > char }
        val center = (geometry.charLeft(char) + geometry.charRight(char)) / 2
        return when {
            before < 0 -> next
            next < 0 -> before
            x < center -> before
            else -> next
        }
    }
}

/** The words passed over by one stroke, and what they mean for a pen (one contiguous interval) or an eraser. */
internal class StrokeWords(
    private val words: WordIndex,
    private val text: String,
) {
    private val hit = sortedSetOf<Int>()

    val isEmpty: Boolean get() = hit.isEmpty()

    fun add(word: Int) {
        if (word >= 0) hit += word
    }

    /** From the first to the last word hit in reading order, the words in between included (section 4.2.4). */
    fun span(): Interval? {
        if (hit.isEmpty()) return null
        return Interval(words.words[hit.first()].start, words.words[hit.last()].end)
    }

    /** The words passed over, one interval each (what the eraser ghost shows). */
    fun hitRuns(): List<Interval> = hit.map { words.words[it] }

    /**
     * The eraser: only the words really passed over. Neighbours in reading order become one run, and the blanks
     * around each run are taken too, so no fragment is left that starts or ends with a space (section 7).
     */
    fun eraseRuns(): List<Interval> {
        val out = mutableListOf<Interval>()
        var runStart = -1
        var previous = -1

        fun close() {
            if (runStart < 0) return
            var s = words.words[runStart].start
            var e = words.words[previous].end
            while (s > 0 && text[s - 1].isWhitespace()) s--
            while (e < text.length && text[e].isWhitespace()) e++
            out += Interval(s, e)
        }
        for (w in hit) {
            if (runStart >= 0 && w == previous + 1) {
                previous = w
            } else {
                close()
                runStart = w
                previous = w
            }
        }
        close()
        return out
    }
}

/**
 * A new highlight of colour C next to one of the same colour with only blanks in between becomes one (section 5): the
 * result reaches the nearest same-colour [runs] that touch it through blanks, so the gap between words is filled too.
 */
internal fun mergeAcrossBlanks(
    range: Interval,
    runs: List<Interval>,
    text: String,
): Interval {
    var start = range.start
    var end = range.end
    var changed = true
    while (changed) {
        changed = false
        for (run in runs) {
            val before = run.end <= start && isBlankRange(text, Interval(run.end, start))
            val after = run.start >= end && isBlankRange(text, Interval(end, run.start))
            val overlaps = run.start < end && run.end > start
            if (before || after || overlaps) {
                val s = minOf(start, run.start)
                val e = maxOf(end, run.end)
                if (s != start || e != end) {
                    start = s
                    end = e
                    changed = true
                }
            }
        }
    }
    return Interval(start, end)
}
