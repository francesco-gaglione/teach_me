package com.teachme.sdui.render

import androidx.compose.ui.text.style.Hyphens
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ContentLanguageTest {
    @Test
    fun `an absent lang means the language of the topic`() {
        assertEquals("en", effectiveLang(null, "en"))
        assertEquals("it", effectiveLang("  ", "it"))
        assertEquals("fr", effectiveLang("fr", "en"))
    }

    @Test
    fun `hyphenation is automatic only in the language of the topic`() {
        assertEquals(Hyphens.Auto, hyphensFor("en", "en"))
        assertEquals(Hyphens.Auto, hyphensFor("en-GB", "en"))
        assertEquals(Hyphens.None, hyphensFor("en", "it"))
        assertEquals(Hyphens.None, hyphensFor("it", "en"))
    }

    @Test
    fun `a malformed lang never crashes`() {
        listOf("xx_??", "", "  ", "!!!", "-", ",,,", "it-", "a".repeat(200), "it_IT" + 0.toChar()).forEach { lang ->
            assertNotNull(lang, localesFor(lang))
            assertNotNull(lang, localesFor(effectiveLang(lang, "en")))
            hyphensFor(lang, "it")
            isTopicLanguage(lang, "it")
        }
    }

    @Test
    fun `same primary subtag counts as the topic language`() {
        assertTrue(isTopicLanguage("IT-it", "it"))
        assertFalse(isTopicLanguage("fr", "it"))
    }
}
