package com.teachme.sdui.inline

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class InlineMarkdownTest {
    private fun parse(s: String) = InlineMarkdown.parse(s)

    @Test
    fun `plain text`() {
        assertEquals(listOf(Inline.Text("Ciao mondo")), parse("Ciao mondo"))
        assertEquals(emptyList<Inline>(), parse(""))
    }

    @Test
    fun `bold italic and code`() {
        assertEquals(
            listOf(
                Inline.Text("a "),
                Inline.Text("forte", bold = true),
                Inline.Text(" b "),
                Inline.Text("lieve", italic = true),
                Inline.Text(" c "),
                Inline.Code("x = 1"),
            ),
            parse("a **forte** b *lieve* c `x = 1`"),
        )
    }

    @Test
    fun `bold and italic nest`() {
        assertEquals(
            listOf(
                Inline.Text("a ", bold = true),
                Inline.Text("b", bold = true, italic = true),
                Inline.Text(" c", bold = true),
            ),
            parse("**a *b* c**"),
        )
    }

    @Test
    fun `code is literal`() {
        assertEquals(listOf(Inline.Code("**x** [[k]] [^s]")), parse("`**x** [[k]] [^s]`"))
        assertEquals(listOf(Inline.Code("<T> # [[")), parse("`<T> # [[`"))
    }

    @Test
    fun `terms with and without visible text`() {
        assertEquals(
            listOf(
                Inline.Text("Quando "),
                Inline.Term("ownership", "lo sposti"),
                Inline.Text(" e "),
                Inline.Term("borrow", null),
            ),
            parse("Quando [[ownership|lo sposti]] e [[borrow]]"),
        )
    }

    @Test
    fun `citations`() {
        assertEquals(
            listOf(Inline.Text("vero"), Inline.Cite("rust_book_4_1"), Inline.Text(".")),
            parse("vero[^rust_book_4_1]."),
        )
    }

    @Test
    fun `only https links are links`() {
        assertEquals(listOf(Inline.Link("guida", "https://example.com/a")), parse("[guida](https://example.com/a)"))
        assertEquals(listOf(Inline.Text("guida (http://example.com)")), parse("[guida](http://example.com)"))
        assertEquals(listOf(Inline.Text("x (ftp://h)")), parse("[x](ftp://h)"))
    }

    @Test
    fun `unclosed or malformed markers stay literal and never throw`() {
        val inputs =
            listOf(
                "**non chiuso",
                "*solo",
                "`aperto",
                "[[k",
                "[[Bad Key]]",
                "[[k|]]",
                "[^",
                "[^Bad]",
                "[testo](",
                "[testo]",
                "a ** b",
                "****",
                "[[]]",
                "`",
                "**a*",
                "*a**",
                "[[a|b[c]]",
                "\\",
                "[^a][^b]",
                "[[a]][[b]]",
                "*`*`*",
            )
        for (input in inputs) {
            val out = parse(input)
            assertEquals(input, true, out.isNotEmpty() || input.isEmpty())
        }
        assertEquals(listOf(Inline.Text("**non chiuso")), parse("**non chiuso"))
        assertEquals(listOf(Inline.Text("[[Bad Key]]")), parse("[[Bad Key]]"))
        assertEquals(listOf(Inline.Text("`aperto")), parse("`aperto"))
        assertEquals(listOf(Inline.Text("a ** b")), parse("a ** b"))
    }

    @Test
    fun `markers inside a term label are not interpreted`() {
        assertEquals(listOf(Inline.Term("k", "a b")), parse("[[k|a b]]"))
    }

    @Test
    fun `plain text strips markup and resolves terms`() {
        val plain =
            InlineMarkdown.plainText("Il **valore** `x` e [[own]] o [[own|lo sposti]][^s].") {
                if (it ==
                    "own"
                ) {
                    "ownership"
                } else {
                    null
                }
            }
        assertEquals("Il valore x e ownership o lo sposti.", plain)
    }

    @Test
    fun `emphasis does not swallow code or terms`() {
        assertEquals(
            listOf(Inline.Text("a ", bold = true), Inline.Code("b*c", bold = true), Inline.Text(" d", bold = true)),
            parse("**a `b*c` d**"),
        )
    }
}

class InlineMarkdownLinearityTest {
    private val generators: Map<String, (Int) -> String> =
        mapOf(
            "nested links" to { n -> "[[".repeat(n * 2) + "]]" },
            "link openers" to { n -> "[a](".repeat(n) + ")" },
            "backticks" to { n -> "`".repeat(n) },
            "emphasis openers" to { n -> "*a ".repeat(n) },
            "term openers" to { n -> "[^".repeat(n) + "]" },
        )

    private fun timeMs(input: String): Double {
        var best = Long.MAX_VALUE
        repeat(3) {
            val start = System.nanoTime()
            InlineMarkdown.parse(input)
            best = minOf(best, System.nanoTime() - start)
        }
        return best / 1_000_000.0
    }

    @Test
    fun `pathological inputs scale linearly`() {
        val n = 25_000
        // Warm-up so JIT compilation does not skew the small-size measurement.
        for (gen in generators.values) InlineMarkdown.parse(gen(n))
        for ((name, gen) in generators) {
            val small = maxOf(timeMs(gen(n)), 1.0)
            val large = timeMs(gen(n * 4))
            val ratio = large / small
            // Linear is ~4x, quadratic ~16x; the threshold sits well between them.
            assertTrue("$name: 4x input took ${ratio}x longer ($small ms -> $large ms)", ratio < 10.0)
            assertTrue("$name: took $large ms", large < 30_000.0)
        }
    }
}
