package com.teachme.sdui.inline

/** One piece of inline content (docs/contracts/sdui.md section 3). Pure data: styling is the renderer's job. */
sealed interface Inline {
    val bold: Boolean
    val italic: Boolean

    data class Text(
        val text: String,
        override val bold: Boolean = false,
        override val italic: Boolean = false,
    ) : Inline

    /** `` `code` ``: literal, nothing inside is interpreted. */
    data class Code(
        val text: String,
        override val bold: Boolean = false,
        override val italic: Boolean = false,
    ) : Inline

    /** `[label](https://...)`; only https links are kept as links. */
    data class Link(
        val label: String,
        val url: String,
        override val bold: Boolean = false,
        override val italic: Boolean = false,
    ) : Inline

    /** `[[key]]` or `[[key|text]]`: a glossary term. [text] is null when the glossary term is to be shown. */
    data class Term(
        val key: String,
        val text: String?,
        override val bold: Boolean = false,
        override val italic: Boolean = false,
    ) : Inline

    /** `[^key]`: a citation of `sources[key]`. */
    data class Cite(
        val key: String,
        override val bold: Boolean = false,
        override val italic: Boolean = false,
    ) : Inline
}

/**
 * An [Inline] node together with the interval it occupies in the markdown source: `[start, end)` in UTF-16 code
 * units, markers included. [charStarts]/[charEnds] are set for nodes whose displayed text is made of the parser's
 * own characters (text, code, link label): entry `k` is the source interval of displayed char `k`. They are null for
 * terms and citations, which the renderer replaces: those are atomic, the whole `[start, end)` stands for them.
 */
class SpannedInline internal constructor(
    val node: Inline,
    val start: Int,
    val end: Int,
    internal val charStarts: IntArray? = null,
    internal val charEnds: IntArray? = null,
)

private val KEY = Regex("[a-z0-9_]{1,40}")
private const val HTTPS = "https://"

/** No marker looks further than this for its closer: parsing stays linear in the text size. */
private const val MAX_SPAN = 2000

private fun String.indexOfBounded(
    ch: Char,
    from: Int,
    end: Int,
): Int {
    val limit = minOf(end, from + MAX_SPAN)
    var j = from
    while (j < limit) {
        if (this[j] == ch) return j
        j++
    }
    return -1
}

private fun String.indexOfBounded(
    needle: String,
    from: Int,
    end: Int,
): Int {
    val limit = minOf(end, from + MAX_SPAN) - needle.length
    var j = from
    while (j <= limit) {
        if (startsWith(needle, j)) return j
        j++
    }
    return -1
}

/**
 * The one inline markdown parser of the app: `**bold**`, `*italic*`, `` `code` ``, `[text](https://...)`,
 * `[^key]` (citation) and `[[key]]` / `[[key|text]]` (glossary term). It is tolerant by construction:
 * whatever does not match stays literal text, unclosed markers stay literal, and it never throws.
 * Code, terms and citations are leaves (no markup inside); bold and italic nest.
 */
object InlineMarkdown {
    fun parse(source: String): List<Inline> =
        merge(parseRange(source, 0, source.length, bold = false, italic = false, track = false)).map { it.node }

    /**
     * Same nodes as [parse], each with its source interval and per-char intervals (see [SpannedInline]). Only the
     * highlighter needs the per-char arrays: [parse] and [plainText] do not allocate them.
     */
    fun parseSpanned(source: String): List<SpannedInline> =
        merge(parseRange(source, 0, source.length, bold = false, italic = false, track = true))

    /** The text the learner reads, markup removed (for TalkBack descriptions and summaries). */
    fun plainText(
        source: String,
        termText: (String) -> String? = { null },
    ): String =
        parse(source).joinToString("") {
            when (it) {
                is Inline.Text -> it.text
                is Inline.Code -> it.text
                is Inline.Link -> it.label
                is Inline.Term -> it.text ?: termText(it.key) ?: it.key
                is Inline.Cite -> ""
            }
        }

    private fun merge(nodes: List<SpannedInline>): List<SpannedInline> {
        val out = mutableListOf<SpannedInline>()
        for (span in nodes) {
            val node = span.node
            val last = out.lastOrNull()
            val lastNode = last?.node
            if (node is Inline.Text &&
                lastNode is Inline.Text &&
                lastNode.bold == node.bold &&
                lastNode.italic == node.italic
            ) {
                out[out.lastIndex] =
                    SpannedInline(
                        lastNode.copy(text = lastNode.text + node.text),
                        last.start,
                        span.end,
                        last.charStarts?.let { a -> span.charStarts?.let { b -> a + b } },
                        last.charEnds?.let { a -> span.charEnds?.let { b -> a + b } },
                    )
            } else if (!(node is Inline.Text && node.text.isEmpty())) {
                out += span
            }
        }
        return out
    }

    /** A node whose displayed chars map one to one, in order, to the source chars starting at [from]. */
    private fun contiguous(
        node: Inline,
        from: Int,
        length: Int,
        start: Int,
        end: Int,
        track: Boolean,
    ) = SpannedInline(
        node,
        start,
        end,
        if (track) IntArray(length) { from + it } else null,
        if (track) IntArray(length) { from + it + 1 } else null,
    )

    private fun parseRange(
        src: String,
        start: Int,
        end: Int,
        bold: Boolean,
        italic: Boolean,
        track: Boolean,
    ): List<SpannedInline> {
        val out = mutableListOf<SpannedInline>()
        val buf = StringBuilder()
        // Per-char source offsets are only collected when [track] is set (the highlighter needs them, nobody else).
        val bufSrc = if (track) ArrayList<Int>() else null
        var firstSrc = 0
        var lastSrc = 0

        fun put(
            ch: Char,
            at: Int,
        ) {
            if (buf.isEmpty()) firstSrc = at
            lastSrc = at
            buf.append(ch)
            bufSrc?.add(at)
        }

        fun flush() {
            if (buf.isNotEmpty()) {
                val starts = bufSrc?.let { l -> IntArray(l.size) { l[it] } }
                out +=
                    SpannedInline(
                        Inline.Text(buf.toString(), bold, italic),
                        firstSrc,
                        lastSrc + 1,
                        starts,
                        starts?.let { st -> IntArray(st.size) { st[it] + 1 } },
                    )
                buf.setLength(0)
                bufSrc?.clear()
            }
        }
        var i = start
        while (i < end) {
            val c = src[i]
            val next = i + 1 < end
            when {
                c == '`' -> {
                    val close = src.indexOfBounded('`', i + 1, end).takeIf { it >= i + 2 }
                    if (close != null) {
                        flush()
                        out +=
                            contiguous(
                                Inline.Code(src.substring(i + 1, close), bold, italic),
                                i + 1,
                                close - i - 1,
                                i,
                                close + 1,
                                track,
                            )
                        i = close + 1
                    } else {
                        put(c, i)
                        i++
                    }
                }
                c == '[' && next && src[i + 1] == '[' -> {
                    val node = parseTerm(src, i, end, bold, italic, track)
                    if (node != null) {
                        flush()
                        out += node.first
                        i = node.second
                    } else {
                        put(c, i)
                        i++
                    }
                }
                c == '[' && next && src[i + 1] == '^' -> {
                    val node = parseCite(src, i, end, bold, italic, track)
                    if (node != null) {
                        flush()
                        out += node.first
                        i = node.second
                    } else {
                        put(c, i)
                        i++
                    }
                }
                c == '[' -> {
                    val node = parseLink(src, i, end, bold, italic, track)
                    if (node != null) {
                        flush()
                        out += node.first
                        i = node.second
                    } else {
                        put(c, i)
                        i++
                    }
                }
                c == '*' -> {
                    val emphasis = parseEmphasis(src, i, end, bold, italic, track)
                    if (emphasis != null) {
                        flush()
                        out += emphasis.first
                        i = emphasis.second
                    } else {
                        put(c, i)
                        i++
                    }
                }
                else -> {
                    put(c, i)
                    i++
                }
            }
        }
        flush()
        return out
    }

    private fun parseTerm(
        src: String,
        i: Int,
        end: Int,
        bold: Boolean,
        italic: Boolean,
        track: Boolean,
    ): Pair<SpannedInline, Int>? {
        val close = src.indexOfBounded("]]", i + 2, end).takeIf { it >= 0 } ?: return null
        val body = src.substring(i + 2, close)
        val key = body.substringBefore('|')
        val text = if ('|' in body) body.substringAfter('|') else null
        if (!KEY.matches(key) || text != null && (text.isEmpty() || '[' in text || ']' in text)) return null
        return SpannedInline(Inline.Term(key, text, bold, italic), i, close + 2) to close + 2
    }

    private fun parseCite(
        src: String,
        i: Int,
        end: Int,
        bold: Boolean,
        italic: Boolean,
        track: Boolean,
    ): Pair<SpannedInline, Int>? {
        val close = src.indexOfBounded(']', i + 2, end).takeIf { it >= 0 } ?: return null
        val key = src.substring(i + 2, close)
        return if (KEY.matches(key)) SpannedInline(Inline.Cite(key, bold, italic), i, close + 1) to close + 1 else null
    }

    private fun parseLink(
        src: String,
        i: Int,
        end: Int,
        bold: Boolean,
        italic: Boolean,
        track: Boolean,
    ): Pair<SpannedInline, Int>? {
        val closeLabel = src.indexOfBounded(']', i + 1, end).takeIf { it >= 0 } ?: return null
        if (closeLabel + 1 >= end || src[closeLabel + 1] != '(') return null
        val closeUrl = src.indexOfBounded(')', closeLabel + 2, end).takeIf { it >= 0 } ?: return null
        val label = src.substring(i + 1, closeLabel)
        val url = src.substring(closeLabel + 2, closeUrl).trim()
        if (label.isEmpty() || url.isEmpty() || url.any { it.isWhitespace() }) return null
        val node =
            if (url.startsWith(HTTPS, ignoreCase = true) && url.length > HTTPS.length) {
                contiguous(Inline.Link(label, url, bold, italic), i + 1, label.length, i, closeUrl + 1, track)
            } else {
                // Not https: no link, the URL is shown in plain sight. The shown text is not in the source: atomic.
                val shown = "$label ($url)"
                SpannedInline(
                    Inline.Text(shown, bold, italic),
                    i,
                    closeUrl + 1,
                    if (track) IntArray(shown.length) { i } else null,
                    if (track) IntArray(shown.length) { closeUrl + 1 } else null,
                )
            }
        return node to closeUrl + 1
    }

    private fun parseEmphasis(
        src: String,
        i: Int,
        end: Int,
        bold: Boolean,
        italic: Boolean,
        track: Boolean,
    ): Pair<List<SpannedInline>, Int>? {
        val double = i + 1 < end && src[i + 1] == '*'
        val size = if (double) 2 else 1
        val contentStart = i + size
        if (contentStart >= end || src[contentStart].isWhitespace() || src[contentStart] == '*') return null
        val closer = findCloser(src, contentStart, end, size) ?: return null
        val inner =
            parseRange(
                src,
                contentStart,
                closer,
                bold = bold || double,
                italic = italic || !double,
                track = track,
            )
        return inner to closer + size
    }

    /** Index where the closing `*` / `**` starts, skipping code spans, terms and citations. */
    private fun findCloser(
        src: String,
        from: Int,
        end: Int,
        size: Int,
    ): Int? {
        var j = from
        val limit = minOf(end, from + MAX_SPAN)
        while (j < limit) {
            val c = src[j]
            when {
                c == '`' -> {
                    val close = src.indexOfBounded('`', j + 1, end).takeIf { it >= j + 2 }
                    j = if (close != null) close + 1 else j + 1
                }
                c == '[' && j + 1 < end && (src[j + 1] == '[' || src[j + 1] == '^') -> {
                    val close = src.indexOfBounded(']', j + 1, end).takeIf { it >= 0 }
                    j = if (close != null) close + 1 else j + 1
                }
                c == '*' -> {
                    val run = runLength(src, j, end)
                    val closes = !src[j - 1].isWhitespace() && (size == 1 && run == 1 || size == 2 && run >= 2)
                    if (closes) return if (size == 2) j + run - 2 else j
                    j += run
                }
                else -> j++
            }
        }
        return null
    }

    private fun runLength(
        src: String,
        from: Int,
        end: Int,
    ): Int {
        var j = from
        while (j < end && src[j] == '*') j++
        return j - from
    }
}
