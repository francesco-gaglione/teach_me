package com.teachme.sdui.inline

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class InlineCodeDisplayTest {
    private val hair = ' '
    private val nbsp = ' '
    private val zwsp = '​'

    @Test
    fun `a short span is padded and cannot break`() {
        val text = "let b = a;"
        assertEquals("$hair" + "let${nbsp}b$nbsp=${nbsp}a;" + "$hair", inlineCodeDisplay(text))
        assertFalse(inlineCodeDisplay(text).drop(1).dropLast(1).contains(' '))
    }

    @Test
    fun `24 characters is still unbreakable and 25 is breakable`() {
        val at24 = "a(b, c) = d.e(f, g, h);x"
        assertEquals(24, at24.length)
        val shown24 = inlineCodeDisplay(at24)
        assertFalse(shown24.contains(zwsp))
        assertFalse(shown24.contains(' '))
        val at25 = at24 + "y"
        val shown25 = inlineCodeDisplay(at25)
        assertTrue(shown25.contains(zwsp))
        assertTrue(shown25.contains(' '))
    }

    @Test
    fun `a long span breaks only after open paren, double colon and dot`() {
        val shown = inlineCodeDisplay("std::collections::HashMap::new(capacity)")
        assertEquals("$hair" + "std::${zwsp}collections::${zwsp}HashMap::${zwsp}new(${zwsp}capacity)" + "$hair", shown)
        assertEquals(
            "$hair" + "alpha.beta.gamma.delta.epsilon.zeta".replace(".", ".$zwsp") + "$hair",
            inlineCodeDisplay("alpha.beta.gamma.delta.epsilon.zeta"),
        )
    }

    @Test
    fun `a single colon is not a break point and nothing is added at the end`() {
        val shown = inlineCodeDisplay("let value_with_a_long_name: Vec<u32> = foo(")
        assertFalse(shown.contains(zwsp + " "))
        assertFalse(shown.endsWith("$zwsp$hair"))
    }
}
