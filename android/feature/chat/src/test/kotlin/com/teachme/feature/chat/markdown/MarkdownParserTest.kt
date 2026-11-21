package com.teachme.feature.chat.markdown

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MarkdownParserTest {
    private fun parse(text: String) = MarkdownParser.parse(text)

    private fun inlines(text: String): List<MdInline> = (parse(text).single() as MdBlock.Paragraph).inlines

    private fun t(
        text: String,
        bold: Boolean = false,
        italic: Boolean = false,
        code: Boolean = false,
    ) = MdInline.Text(text, bold, italic, code)

    @Test
    fun `empty input has no blocks`() {
        assertEquals(emptyList<MdBlock>(), parse(""))
        assertEquals(emptyList<MdBlock>(), parse("\n\n  \n"))
    }

    @Test
    fun `paragraphs are separated by blank lines and soft breaks are spaces`() {
        val blocks = parse("Prima riga\nseconda riga\n\nAltro paragrafo")
        assertEquals(2, blocks.size)
        assertEquals(listOf(t("Prima riga seconda riga")), (blocks[0] as MdBlock.Paragraph).inlines)
    }

    @Test
    fun `hard breaks with two spaces or a backslash`() {
        assertEquals(listOf(t("a"), MdInline.LineBreak, t("b")), inlines("a  \nb"))
        assertEquals(listOf(t("a"), MdInline.LineBreak, t("b")), inlines("a\\\nb"))
    }

    @Test
    fun `bold italic and both`() {
        assertEquals(listOf(t("un "), t("termine", bold = true), t(" chiave")), inlines("un **termine** chiave"))
        assertEquals(listOf(t("un "), t("termine", bold = true), t(" chiave")), inlines("un __termine__ chiave"))
        assertEquals(listOf(t("a "), t("b", italic = true), t(" c")), inlines("a *b* c"))
        assertEquals(listOf(t("a "), t("b", italic = true), t(" c")), inlines("a _b_ c"))
        assertEquals(listOf(t("x", bold = true, italic = true)), inlines("***x***"))
    }

    @Test
    fun `nested emphasis`() {
        assertEquals(
            listOf(t("a ", bold = true), t("b", bold = true, italic = true), t(" c", bold = true)),
            inlines("**a *b* c**"),
        )
        assertEquals(
            listOf(t("a ", italic = true), t("b", bold = true, italic = true), t(" c", italic = true)),
            inlines("*a **b** c*"),
        )
    }

    @Test
    fun `unclosed markers stay literal`() {
        assertEquals(listOf(t("un **termine")), inlines("un **termine"))
        assertEquals(listOf(t("a *b")), inlines("a *b"))
        assertEquals(listOf(t("a `b")), inlines("a `b"))
        assertEquals(listOf(t("[testo](https://x")), inlines("[testo](https://x"))
    }

    @Test
    fun `markers followed by space are not emphasis`() {
        assertEquals(listOf(t("2 * 3 * 4")), inlines("2 * 3 * 4"))
    }

    @Test
    fun `intraword underscores are literal`() {
        assertEquals(listOf(t("snake_case_name")), inlines("snake_case_name"))
    }

    @Test
    fun `inline code`() {
        assertEquals(listOf(t("usa "), t("let x = 1", code = true), t(" qui")), inlines("usa `let x = 1` qui"))
        assertEquals(listOf(t("**no**", code = true)), inlines("`**no**`"))
        assertEquals(listOf(t("a`b", code = true)), inlines("``a`b``"))
    }

    @Test
    fun `escapes give literal characters`() {
        assertEquals(listOf(t("*non corsivo*")), inlines("\\*non corsivo\\*"))
    }

    @Test
    fun `https link`() {
        assertEquals(
            listOf(t("vedi "), MdInline.Link("la guida", "https://doc.rust-lang.org/book/"), t(".")),
            inlines("vedi [la guida](https://doc.rust-lang.org/book/)."),
        )
    }

    @Test
    fun `non https links show label and url as plain text`() {
        assertEquals(listOf(t("clic (http://x.it)")), inlines("[clic](http://x.it)"))
        assertEquals(listOf(t("ok (javascript:alert(1))")), inlines("[ok](javascript:alert(1))"))
        assertEquals(listOf(t("f (file:///etc/passwd)")), inlines("[f](file:///etc/passwd)"))
        assertEquals(listOf(t("x (intent:#Intent;end)")), inlines("[x](intent:#Intent;end)"))
        assertEquals(listOf(t("x (https://)")), inlines("[x](https://)"))
    }

    @Test
    fun `bare urls are not linkified`() {
        assertEquals(listOf(t("vai su https://example.com ora")), inlines("vai su https://example.com ora"))
    }

    @Test
    fun `images show only the alt text in italic`() {
        assertEquals(listOf(t("un gatto", italic = true)), inlines("![un gatto](https://x.it/g.png)"))
        assertEquals(emptyList<MdInline>(), inlines("![](https://x.it/g.png)"))
    }

    @Test
    fun `html stays literal`() {
        assertEquals(listOf(t("<b>x</b> e <br>")), inlines("<b>x</b> e <br>"))
    }

    @Test
    fun `headings become bold paragraphs`() {
        val block = parse("## Titolo").single()
        assertEquals(MdBlock.Heading(listOf(t("Titolo"))), block)
        assertTrue(parse("#hashtag").single() is MdBlock.Paragraph)
        assertTrue(parse("##### cinque").single() is MdBlock.Paragraph)
    }

    @Test
    fun `block quotes stay literal`() {
        assertEquals(listOf(t("> citazione")), inlines("> citazione"))
    }

    @Test
    fun `horizontal rule`() {
        assertEquals(listOf<MdBlock>(MdBlock.Rule), parse("---"))
        assertEquals(listOf<MdBlock>(MdBlock.Rule), parse("* * *"))
        assertEquals(3, parse("a\n\n---\n\nb").size)
    }

    @Test
    fun `fenced code with language`() {
        val block = parse("Prima\n```rust\nlet s = 1;\n  let t = s;\n```\nDopo")[1]
        assertEquals(MdBlock.Code("rust", "let s = 1;\n  let t = s;", closed = true), block)
        assertEquals(3, parse("Prima\n```rust\nx\n```\nDopo").size)
    }

    @Test
    fun `a backtick run with a backtick in the rest of the line is not a fence`() {
        val blocks = parse("```cargo run``` testo\n\nSecondo paragrafo")
        assertEquals(2, blocks.size)
        assertTrue(blocks[0] is MdBlock.Paragraph)
        assertTrue(blocks[1] is MdBlock.Paragraph)
        assertTrue(parse("```a``` testo").single() is MdBlock.Paragraph)
        // The language is the first word of the info string; tilde fences may contain backticks.
        assertEquals("rust", (parse("``` rust extra\nx\n```").single() as MdBlock.Code).language)
        assertEquals("a`b", (parse("~~~a`b\nx\n~~~").single() as MdBlock.Code).language)
    }

    @Test
    fun `very long rule-like lines do not hang`() {
        val blocks = parse("-".repeat(200_000))
        assertTrue(blocks.single() is MdBlock.Paragraph)
    }

    @Test
    fun `unclosed fence is already a code block`() {
        val block = parse("```rust\nlet x").single() as MdBlock.Code
        assertFalse(block.closed)
        assertEquals("let x", block.code)
        assertEquals("rust", block.language)
    }

    @Test
    fun `fence without language and inside it markdown is not interpreted`() {
        val block = parse("```\n**non grassetto**\n- no\n```").single() as MdBlock.Code
        assertEquals(null, block.language)
        assertEquals("**non grassetto**\n- no", block.code)
    }

    @Test
    fun `tables render as code`() {
        val block = parse("| a | b |\n|---|---|\n| 1 | 2 |").single() as MdBlock.Code
        assertTrue(block.table)
        assertEquals("| a | b |\n|---|---|\n| 1 | 2 |", block.code)
    }

    @Test
    fun `bullet list`() {
        val list = parse("- uno\n- due\n* tre\n+ quattro").single() as MdBlock.ListBlock
        assertEquals(4, list.items.size)
        assertTrue(list.items.all { it.ordinal == null && it.level == 0 })
        assertEquals(listOf(t("due")), list.items[1].inlines)
    }

    @Test
    fun `numbered list keeps the starting number`() {
        val list = parse("3. tre\n4. quattro\n7. cinque").single() as MdBlock.ListBlock
        assertEquals(listOf(3, 4, 5), list.items.map { it.ordinal })
    }

    @Test
    fun `nested lists up to two levels and the third is flattened`() {
        val list = parse("- a\n  - b\n    - c\n- d").single() as MdBlock.ListBlock
        assertEquals(listOf(0, 1, 1, 0), list.items.map { it.level })
    }

    @Test
    fun `numbered inside bullets restarts the numbering`() {
        val list = parse("- a\n  1. x\n  2. y\n- b\n  1. z").single() as MdBlock.ListBlock
        assertEquals(listOf(null, 1, 2, null, 1), list.items.map { it.ordinal })
    }

    @Test
    fun `list item continuation lines belong to the item`() {
        val list = parse("- una voce\n  che continua\n- due").single() as MdBlock.ListBlock
        assertEquals(listOf(t("una voce che continua")), list.items[0].inlines)
    }

    @Test
    fun `blank line between items keeps one list and blank then text ends it`() {
        assertEquals(1, parse("- a\n\n- b").size)
        val blocks = parse("- a\n\ntesto")
        assertTrue(blocks[0] is MdBlock.ListBlock)
        assertTrue(blocks[1] is MdBlock.Paragraph)
    }

    @Test
    fun `a list started while streaming appears at once`() {
        val list = parse("Testo\n- ").last() as MdBlock.ListBlock
        assertEquals(1, list.items.size)
        assertEquals(emptyList<MdInline>(), list.items[0].inlines)
    }

    @Test
    fun `list marker lookalikes are not lists`() {
        assertTrue(parse("**forte** inizio").single() is MdBlock.Paragraph)
        assertTrue(parse("-5 gradi").single() is MdBlock.Paragraph)
        assertTrue(parse("2024. un anno").single() is MdBlock.ListBlock)
    }

    @Test
    fun `list and paragraph blocks interrupt each other`() {
        val blocks = parse("Intro\n- a\n- b\nFine")
        assertEquals(2, blocks.size)
        // "Fine" without a blank line is a lazy continuation of the last item (CommonMark does the same).
        assertEquals(listOf(t("b Fine")), (blocks[1] as MdBlock.ListBlock).items[1].inlines)
    }

    @Test
    fun `crlf is normalized`() {
        assertEquals(2, parse("a\r\n\r\nb").size)
    }

    @Test
    fun `never throws on every prefix of a rich document`() {
        val doc =
            "Ciao **mondo** e *corsivo* con `codice` [link](https://x.it) ![alt](https://i.it/a.png)\n\n" +
                "1. uno\n   - sotto\n2. due\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|---|---|\n\n# Titolo\n---\n> q\n\\*x\\*"
        for (end in 0..doc.length) {
            parse(doc.substring(0, end))
        }
        assertTrue(parse(doc).size > 5)
    }

    @Test
    fun `random garbage does not throw`() {
        val alphabet = "*_`[]()!\\#>-|~\n 1.a:h/t"
        val random = java.util.Random(42)
        repeat(3000) {
            val text = String(CharArray(random.nextInt(40)) { alphabet[random.nextInt(alphabet.length)] })
            parse(text)
        }
    }
}
