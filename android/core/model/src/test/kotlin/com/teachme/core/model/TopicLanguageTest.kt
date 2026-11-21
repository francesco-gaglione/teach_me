package com.teachme.core.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class TopicLanguageTest {
    @Test
    fun `wire values map, a missing one is Italian and an unknown one is Unknown`() {
        assertEquals(TopicLanguage.En, TopicLanguage.fromWire("en"))
        assertEquals(TopicLanguage.It, TopicLanguage.fromWire("it"))
        assertEquals(TopicLanguage.It, TopicLanguage.fromWire(null))
        assertEquals(TopicLanguage.Unknown, TopicLanguage.fromWire("fr"))
        assertEquals(TopicLanguage.Unknown, TopicLanguage.fromWire(""))
    }

    @Test
    fun `Unknown is never offered when creating a Topic`() {
        assertFalse(TopicLanguage.Unknown in TopicLanguage.selectable)
        assertEquals(listOf(TopicLanguage.It, TopicLanguage.En), TopicLanguage.selectable)
    }

    @Test
    fun `default is English only for an English device`() {
        assertEquals(TopicLanguage.En, TopicLanguage.defaultFor("en"))
        assertEquals(TopicLanguage.En, TopicLanguage.defaultFor("EN"))
        assertEquals(TopicLanguage.It, TopicLanguage.defaultFor("it"))
        assertEquals(TopicLanguage.It, TopicLanguage.defaultFor("de"))
        assertEquals(TopicLanguage.It, TopicLanguage.defaultFor(null))
    }
}
