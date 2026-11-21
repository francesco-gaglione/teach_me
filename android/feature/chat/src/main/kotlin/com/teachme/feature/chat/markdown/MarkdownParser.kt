package com.teachme.feature.chat.markdown

import java.net.URI

/**
 * Tolerant parser of the markdown subset of chat.md section 5: paragraphs, bold, italic, inline code,
 * fenced code, bullet/numbered lists (2 levels), `https` links, rules, headings (as bold paragraphs) and
 * tables (as code). Anything else stays literal text. It never throws: incomplete input (a stream that
 * stopped mid-marker) leaves the unmatched marker as plain characters.
 *
 * Hand written on purpose, see the module report: the subset is small, the rules differ from CommonMark
 * in several places (no autolinks, literal `>` quotes, https-only links, unclosed fences are already
 * code), and a library would have to be bent to all of them.
 */
object MarkdownParser {
    fun parse(source: String): List<MdBlock> = BlockParser(source).parse()
}

/** Backtick fences cannot have a backtick in the info string (CommonMark): "```a``` text" is not a fence. */
private val FENCE_BACKTICK = Regex("""^\s*(`{3,})([^`]*)$""")
private val FENCE_TILDE = Regex("""^\s*(~{3,})(.*)$""")
private const val MAX_RULE_LENGTH = 100
private val RULE = Regex("""^ {0,3}([-*_])([ \t]*\1){2,}[ \t]*$""")
private val HEADING = Regex("""^ {0,3}#{1,4}[ \t]+(\S.*)$""")
private val LIST_ITEM = Regex("""^([ \t]*)([-*+]|\d{1,9}[.)])[ \t]+(.*)$""")

private class ItemBuilder(
    val level: Int,
    val ordinal: Int?,
    val lines: MutableList<String>,
)

private class BlockParser(
    source: String,
) {
    private val lines = source.replace("\r\n", "\n").replace('\r', '\n').split('\n')
    private val blocks = mutableListOf<MdBlock>()
    private val paragraph = mutableListOf<String>()

    private val items = mutableListOf<MdListItem>()
    private var current: ItemBuilder? = null
    private val indents = mutableListOf<Int>()
    private val lastOrdinal = mutableMapOf<Int, Int>()
    private var blankSinceItem = false

    fun parse(): List<MdBlock> {
        var i = 0
        while (i < lines.size) {
            val line = lines[i]
            val trimmed = line.trim()
            val fence = FENCE_BACKTICK.matchEntire(line) ?: FENCE_TILDE.matchEntire(line)
            when {
                fence != null -> i = readFence(i, fence)
                trimmed.isEmpty() -> {
                    flushParagraph()
                    if (current != null) blankSinceItem = true
                    i++
                }
                trimmed.startsWith("|") -> i = readTable(i)
                trimmed.length <= MAX_RULE_LENGTH && RULE.matches(line) -> {
                    flushAll()
                    blocks += MdBlock.Rule
                    i++
                }
                HEADING.matches(line) -> {
                    flushAll()
                    blocks += MdBlock.Heading(parseInlines(HEADING.matchEntire(line)!!.groupValues[1].trim()))
                    i++
                }
                LIST_ITEM.matches(line) -> {
                    startItem(LIST_ITEM.matchEntire(line)!!)
                    i++
                }
                current != null && !blankSinceItem -> {
                    current!!.lines += line
                    i++
                }
                else -> {
                    flushList()
                    paragraph += line
                    i++
                }
            }
        }
        flushAll()
        return blocks
    }

    private fun readFence(
        start: Int,
        open: MatchResult,
    ): Int {
        flushAll()
        val marker = open.groupValues[1]
        val language =
            open.groupValues[2]
                .trim()
                .substringBefore(' ')
                .ifEmpty { null }
        val indent = lines[start].length - lines[start].trimStart().length
        val body = mutableListOf<String>()
        var i = start + 1
        var closed = false
        while (i < lines.size) {
            val candidate = lines[i].trim()
            if (candidate.length >= marker.length && candidate.all { it == marker[0] }) {
                closed = true
                i++
                break
            }
            body += lines[i].let { l -> l.drop(minOf(indent, l.length - l.trimStart().length)) }
            i++
        }
        blocks += MdBlock.Code(language, body.joinToString("\n"), closed)
        return i
    }

    private fun readTable(start: Int): Int {
        flushAll()
        val rows = mutableListOf<String>()
        var i = start
        while (i < lines.size && lines[i].trim().startsWith("|")) {
            rows += lines[i].trim()
            i++
        }
        blocks += MdBlock.Code(null, rows.joinToString("\n"), closed = true, table = true)
        return i
    }

    private fun startItem(match: MatchResult) {
        flushParagraph()
        flushItem()
        val indent = match.groupValues[1].fold(0) { sum, c -> sum + if (c == '\t') 4 else 1 }
        val marker = match.groupValues[2]
        val rawLevel = levelFor(indent)
        val ordinal =
            if (marker[0].isDigit()) {
                val number = marker.dropLast(1).toIntOrNull() ?: 1
                (lastOrdinal[rawLevel]?.plus(1) ?: number).also { lastOrdinal[rawLevel] = it }
            } else {
                lastOrdinal.remove(rawLevel)
                null
            }
        lastOrdinal.keys.removeAll { it > rawLevel }
        current = ItemBuilder(minOf(rawLevel, 1), ordinal, mutableListOf(match.groupValues[3]))
        blankSinceItem = false
    }

    private fun levelFor(indent: Int): Int {
        if (indents.isEmpty()) {
            indents += indent
            return 0
        }
        when {
            indent >= indents.last() + 2 -> indents += indent
            indent < indents.last() -> while (indents.size > 1 &&
                indent < indents.last()
                ) {
                    indents.removeAt(indents.lastIndex)
                }
        }
        return indents.size - 1
    }

    private fun flushItem() {
        val item = current ?: return
        items += MdListItem(item.level, item.ordinal, parseInlines(joinLines(item.lines)))
        current = null
    }

    private fun flushList() {
        flushItem()
        if (items.isNotEmpty()) blocks += MdBlock.ListBlock(items.toList())
        items.clear()
        indents.clear()
        lastOrdinal.clear()
        blankSinceItem = false
    }

    private fun flushParagraph() {
        if (paragraph.isEmpty()) return
        blocks += MdBlock.Paragraph(parseInlines(joinLines(paragraph)))
        paragraph.clear()
    }

    private fun flushAll() {
        flushParagraph()
        flushList()
    }
}

/** Soft breaks become a space; a trailing backslash or two trailing spaces become a hard break (`\n`). */
private fun joinLines(lines: List<String>): String {
    val sb = StringBuilder()
    lines.forEachIndexed { index, raw ->
        val line = raw.trimStart()
        when {
            index == lines.lastIndex -> sb.append(line.trimEnd())
            line.endsWith("\\") && !line.endsWith("\\\\") -> sb.append(line.dropLast(1)).append('\n')
            line.endsWith("  ") -> sb.append(line.trimEnd()).append('\n')
            else -> sb.append(line.trimEnd()).append(' ')
        }
    }
    return sb.toString()
}

private const val ESCAPABLE = "\\`*_{}[]()#+-.!|<>~"

private data class Style(
    val bold: Boolean = false,
    val italic: Boolean = false,
)

internal fun parseInlines(text: String): List<MdInline> = InlineParser(text).parse()

private class InlineParser(
    private val src: String,
) {
    fun parse(): List<MdInline> = mergeAdjacent(range(0, src.length, Style()))

    private fun range(
        start: Int,
        end: Int,
        style: Style,
    ): List<MdInline> {
        val out = mutableListOf<MdInline>()
        val buf = StringBuilder()

        fun flush() {
            if (buf.isNotEmpty()) {
                out += MdInline.Text(buf.toString(), style.bold, style.italic)
                buf.setLength(0)
            }
        }

        var i = start
        while (i < end) {
            val c = src[i]
            when {
                c == '\\' && i + 1 < end && src[i + 1] in ESCAPABLE -> {
                    buf.append(src[i + 1])
                    i += 2
                }
                c == '\n' -> {
                    flush()
                    out += MdInline.LineBreak
                    i++
                }
                c == '`' -> {
                    val n = runLength(i, end, '`')
                    val close = findBacktickClose(i + n, end, n)
                    if (close >= 0) {
                        flush()
                        out += MdInline.Text(codeContent(src.substring(i + n, close)), style.bold, style.italic, true)
                        i = close + n
                    } else {
                        buf.append("`".repeat(n))
                        i += n
                    }
                }
                c == '!' && i + 1 < end && src[i + 1] == '[' -> {
                    val link = parseBracket(i + 1, end)
                    if (link != null) {
                        flush()
                        if (link.label.isNotEmpty()) out += MdInline.Text(link.label, style.bold, italic = true)
                        i = link.next
                    } else {
                        buf.append(c)
                        i++
                    }
                }
                c == '[' -> {
                    val link = parseBracket(i, end)
                    if (link != null) {
                        flush()
                        out += linkInline(link, style)
                        i = link.next
                    } else {
                        buf.append(c)
                        i++
                    }
                }
                c == '*' || c == '_' -> {
                    val emphasis = parseEmphasis(i, end, style)
                    if (emphasis != null) {
                        flush()
                        out += emphasis.content
                        i = emphasis.next
                    } else {
                        val n = runLength(i, end, c)
                        buf.append(c.toString().repeat(n))
                        i += n
                    }
                }
                else -> {
                    buf.append(c)
                    i++
                }
            }
        }
        flush()
        return out
    }

    private fun runLength(
        from: Int,
        end: Int,
        char: Char,
    ): Int {
        var j = from
        while (j < end && src[j] == char) j++
        return j - from
    }

    private fun findBacktickClose(
        from: Int,
        end: Int,
        n: Int,
    ): Int {
        var j = from
        while (j < end) {
            if (src[j] == '`') {
                val run = runLength(j, end, '`')
                if (run == n) return j
                j += run
            } else {
                j++
            }
        }
        return -1
    }

    private fun codeContent(raw: String): String {
        val text = raw.replace('\n', ' ')
        return if (text.length > 2 && text.startsWith(" ") && text.endsWith(" ") && text.isNotBlank()) {
            text.substring(1, text.length - 1)
        } else {
            text
        }
    }

    private class Emphasis(
        val content: List<MdInline>,
        val next: Int,
    )

    private fun parseEmphasis(
        i: Int,
        end: Int,
        style: Style,
    ): Emphasis? {
        val char = src[i]
        val run = runLength(i, end, char)
        if (run > 3) return null
        val after = i + run
        if (after >= end || src[after].isWhitespace()) return null
        if (char == '_' && i > 0 && src[i - 1].isLetterOrDigit()) return null
        val closeStart = findCloser(char, run, after, end) ?: return null
        val inner =
            style.copy(
                bold = style.bold || run >= 2,
                italic = style.italic || run != 2,
            )
        return Emphasis(range(after, closeStart, inner), closeStart + run)
    }

    /** Position where the closing marker of [marker]*[size] starts, or null if the span is never closed. */
    private fun findCloser(
        marker: Char,
        size: Int,
        from: Int,
        end: Int,
    ): Int? {
        var j = from
        while (j < end) {
            val c = src[j]
            when {
                c == '\\' -> j += 2
                c == '`' -> {
                    val n = runLength(j, end, '`')
                    val close = findBacktickClose(j + n, end, n)
                    j = if (close >= 0) close + n else j + n
                }
                c == marker -> {
                    val run = runLength(j, end, marker)
                    val canClose =
                        j > from &&
                            !src[j - 1].isWhitespace() &&
                            !(marker == '_' && j + run < end && src[j + run].isLetterOrDigit())
                    val fits =
                        when (size) {
                            1 -> run == 1 || run == 3
                            2 -> run == 2 || run == 3
                            else -> run == 3
                        }
                    if (canClose && fits) return j + run - size
                    j += run
                }
                else -> j++
            }
        }
        return null
    }

    private class Bracket(
        val label: String,
        val url: String,
        val next: Int,
    )

    /** `[label](url)` starting at the `[` in [open]; null if it is not a well formed link. */
    private fun parseBracket(
        open: Int,
        end: Int,
    ): Bracket? {
        var depth = 0
        var j = open
        var closeLabel = -1
        while (j < end) {
            when (src[j]) {
                '\\' -> j++
                '[' -> depth++
                ']' -> {
                    depth--
                    if (depth == 0) {
                        closeLabel = j
                        break
                    }
                }
            }
            j++
        }
        if (closeLabel < 0 || closeLabel + 1 >= end || src[closeLabel + 1] != '(') return null
        var k = closeLabel + 2
        var parens = 1
        while (k < end) {
            when (src[k]) {
                '(' -> parens++
                ')' -> {
                    parens--
                    if (parens == 0) break
                }
            }
            k++
        }
        if (k >= end) return null
        val url = src.substring(closeLabel + 2, k).trim()
        if (url.any { it.isWhitespace() }) return null
        val label = plainText(range(open + 1, closeLabel, Style()))
        return Bracket(label, url, k + 1)
    }

    private fun plainText(inlines: List<MdInline>): String =
        inlines.joinToString("") {
            when (it) {
                is MdInline.Text -> it.text
                is MdInline.Link -> it.label
                MdInline.LineBreak -> " "
            }
        }

    /** Valid https link, or the label followed by the plain URL (chat.md section 5). */
    private fun linkInline(
        link: Bracket,
        style: Style,
    ): List<MdInline> =
        if (isSafeHttpsUrl(link.url) && link.label.isNotEmpty()) {
            listOf(MdInline.Link(link.label, link.url, style.bold, style.italic))
        } else {
            val text = if (link.label.isEmpty()) link.url else "${link.label} (${link.url})"
            listOf(MdInline.Text(text, style.bold, style.italic))
        }

    private fun mergeAdjacent(inlines: List<MdInline>): List<MdInline> {
        val out = mutableListOf<MdInline>()
        for (item in inlines) {
            val last = out.lastOrNull()
            if (item is MdInline.Text &&
                last is MdInline.Text &&
                item.bold == last.bold &&
                item.italic == last.italic &&
                item.code == last.code
            ) {
                out[out.lastIndex] = last.copy(text = last.text + item.text)
            } else {
                out += item
            }
        }
        return out
    }
}

/** True for absolute `https` URLs with a host and no whitespace/control characters. */
fun isSafeHttpsUrl(url: String): Boolean {
    if (url.any { it.isWhitespace() || it.isISOControl() }) return false
    if (!url.startsWith("https://", ignoreCase = true)) return false
    return try {
        !URI(url).host.isNullOrEmpty()
    } catch (e: java.net.URISyntaxException) {
        false
    }
}
