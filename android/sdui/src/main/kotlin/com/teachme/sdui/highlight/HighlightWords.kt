package com.teachme.sdui.highlight

/**
 * The words of a displayed text, as the pen sees them (highlight.md section 4.2): a run of displayed chars between
 * blanks, so punctuation stuck to a word is part of it. Glossary terms and citations are one word whatever they
 * contain, and take the word they are glued to. A thin space (the one before a citation) does not split.
 */
internal class WordIndex(
    val words: List<Interval>,
) {
    /** Index of the word that contains the displayed char [char], or -1 for a blank. */
    fun wordAt(char: Int): Int {
        var lo = 0
        var hi = words.size - 1
        while (lo <= hi) {
            val mid = (lo + hi) ushr 1
            val w = words[mid]
            when {
                char < w.start -> hi = mid - 1
                char >= w.end -> lo = mid + 1
                else -> return mid
            }
        }
        return -1
    }

    /** The word before the blank [char], or -1. */
    fun wordBefore(char: Int): Int {
        var found = -1
        for (i in words.indices) {
            if (words[i].end <= char) found = i else break
        }
        return found
    }

    companion object {
        private const val THIN_SPACE = ' '

        /** Padding around inline code (see `inlineCodeDisplay`): part of the word it touches, like [THIN_SPACE]. */
        private const val HAIR_SPACE = '\u200A'

        private fun isGap(c: Char) = c.isWhitespace() && c != THIN_SPACE && c != HAIR_SPACE

        fun of(map: SourceMap): WordIndex {
            val text = map.displayed
            val n = text.length
            val out = ArrayList<Interval>()
            var i = 0
            while (i < n) {
                if (isGap(text[i])) {
                    i++
                    continue
                }
                var start = i
                var end = i
                while (end < n && !isGap(text[end])) {
                    val unit = map.atomicUnitAt(end)
                    start = minOf(start, unit.start)
                    end = maxOf(end + 1, unit.end)
                }
                out += Interval(start, end)
                i = end
            }
            return WordIndex(out)
        }
    }
}

/** Whether every char of [range] in [text] is a blank. */
internal fun isBlankRange(
    text: String,
    range: Interval,
): Boolean = (range.start until range.end).all { text[it].isWhitespace() }
