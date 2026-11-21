package com.teachme.feature.lesson

import com.teachme.sdui.logic.SectionBuilder
import com.teachme.sdui.model.LessonParser
import com.teachme.sdui.model.ParseResult
import org.junit.Assert.assertEquals
import org.junit.Test

class LessonKeysTest {
    private fun keysOf(text: String): List<String> {
        val document = (LessonParser.parse(text) as ParseResult.Usable).document
        val sections = SectionBuilder.build(document)
        return buildLessonItems(document, sections, sections.sections.size).map { it.key }
    }

    @Test
    fun `blocks without an id never share a key`() {
        // Non-adjacent id-less neutral rows used to collide ("Key was already used").
        val text =
            """{"blocks":[5,{"id":"p","type":"paragraph","text":"a"},null,{"id":"q","type":"paragraph","text":"b"},7]}"""
        val keys = keysOf(text)
        assertEquals(keys.size, keys.toSet().size)
    }

    @Test
    fun `block ids equal to reserved keys do not collide`() {
        val text =
            """{"catalog_version":2,"objective":"Saper fare","blocks":[
                {"id":"banner","type":"heading","level":1,"text":"T"},
                {"id":"objective","type":"paragraph","text":"a"},
                {"id":"continue_0","type":"paragraph","text":"b"}]}"""
        val keys = keysOf(text)
        assertEquals(keys.size, keys.toSet().size)
    }
}
