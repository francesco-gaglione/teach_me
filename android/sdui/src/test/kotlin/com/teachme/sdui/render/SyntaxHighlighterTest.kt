package com.teachme.sdui.render

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SyntaxHighlighterTest {
    private fun kinds(
        code: String,
        lang: String,
    ) = SyntaxHighlighter.tokenize(code, lang).map { code.substring(it.start, it.end) to it.kind }

    @Test
    fun `rust keywords strings comments and macros`() {
        val code = "let a = String::from(\"ciao\"); // note\nprintln!(\"{}\", a);"
        val tokens = kinds(code, "rust")
        assertTrue("let" to TokenKind.Keyword in tokens)
        assertTrue("String" to TokenKind.Type in tokens)
        assertTrue("from" to TokenKind.Function in tokens)
        assertTrue("\"ciao\"" to TokenKind.Str in tokens)
        assertTrue("// note" to TokenKind.Comment in tokens)
        assertTrue("println" to TokenKind.Function in tokens)
    }

    @Test
    fun `rust lifetime is not a string`() {
        val tokens = kinds("fn f<'a>(x: &'a str) -> char { 'x' }", "rust")
        assertTrue("'a" to TokenKind.Type in tokens)
        assertTrue("'x'" to TokenKind.Str in tokens)
    }

    @Test
    fun `numbers and aliases`() {
        assertTrue("42" to TokenKind.Number in kinds("x = 42", "py"))
        assertTrue("# c" to TokenKind.Comment in kinds("# c", "python"))
    }

    @Test
    fun `unknown languages and broken input never throw`() {
        assertEquals(emptyList<Token>(), SyntaxHighlighter.tokenize("whatever", "text"))
        assertEquals(emptyList<Token>(), SyntaxHighlighter.tokenize("whatever", "output"))
        for (lang in listOf("rust", "bash", "json", "sql", "yaml")) {
            SyntaxHighlighter.tokenize("\"unterminated /* \\", lang)
            SyntaxHighlighter.tokenize("'", lang)
            SyntaxHighlighter.tokenize("", lang)
        }
    }

    @Test
    fun `tokens never overlap and stay in range`() {
        val code = "git add -f \"a b\" # tail\nif [ -f x ]; then echo 1; fi"
        var last = 0
        for (t in SyntaxHighlighter.tokenize(code, "bash")) {
            assertTrue(t.start >= last && t.end <= code.length && t.end > t.start)
            last = t.end
        }
    }
}
