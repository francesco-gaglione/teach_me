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
    @Test
    fun `pathological inputs finish quickly`() {
        val inputs =
            listOf(
                "[[".repeat(200_000) + "]]",
                "[a](".repeat(100_000) + ")",
                "`".repeat(100_000),
                "*a ".repeat(100_000),
                "[^".repeat(100_000) + "]",
            )
        val start = System.nanoTime()
        for (input in inputs) InlineMarkdown.parse(input)
        assertTrue("took ${(System.nanoTime() - start) / 1_000_000} ms", System.nanoTime() - start < 5_000_000_000L)
    }
}
