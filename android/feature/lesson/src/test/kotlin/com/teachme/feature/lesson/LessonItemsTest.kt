package com.teachme.feature.lesson

import com.teachme.sdui.logic.SectionBuilder
import com.teachme.sdui.model.LessonParser
import com.teachme.sdui.model.ParseResult
import com.teachme.sdui.render.BlockGap
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class LessonItemsTest {
    private val document =
        run {
            val dir =
                generateSequence(File(System.getProperty("user.dir").orEmpty()).absoluteFile) { it.parentFile }
                    .map { File(it, "docs/contracts/fixtures") }
                    .first { it.isDirectory }
            (LessonParser.parse(File(dir, "lesson-rust-ownership.json").readText()) as ParseResult.Usable).document
        }
    private val sections = SectionBuilder.build(document)

    @Test
    fun `only revealed sections are listed and a single continue bar closes them`() {
        val one = buildLessonItems(document, sections, 1)
        val two = buildLessonItems(document, sections, 2)
        assertEquals(1, one.count { it is LessonItem.Continue })
        assertEquals(1, two.count { it is LessonItem.Continue })
        assertTrue(two.size > one.size)
        assertEquals(LessonItem.Continue(1), two.last())
        assertTrue(one.filterIsInstance<LessonItem.BlockItem>().all { it.sectionIndex == 0 })
    }

    @Test
    fun `objective follows the title and keys are unique`() {
        val items = buildLessonItems(document, sections, sections.sections.size)
        assertTrue(items[1] is LessonItem.Objective)
        assertEquals(items.size, items.map { it.key }.toSet().size)
        assertEquals(BlockGap.Block, (items[2] as LessonItem.BlockItem).gap)
    }

    @Test
    fun `a revealed section title gets the section gap`() {
        val items = buildLessonItems(document, sections, 2)
        val title = items.filterIsInstance<LessonItem.BlockItem>().first { it.sectionIndex == 1 }
        assertEquals(BlockGap.Section, title.gap)
        assertEquals(items.indexOf(title), firstItemOfSection(items, 1))
    }

    private fun withPdfs(count: Int): com.teachme.sdui.model.LessonDocument =
        document.copy(
            sources =
                document.sources.mapIndexed { index, source ->
                    if (index <
                        count
                    ) {
                        source.copy(url = null, title = "Rust in Action (pp. ${index + 1}-9)")
                    } else {
                        source
                    }
                },
        )

    @Test
    fun `the lesson header names the first PDF and counts the others`() {
        val none = buildLessonItems(document, sections, 1).filterIsInstance<LessonItem.Objective>().single()
        assertEquals(null, none.pdfTitle)
        assertEquals(0, none.pdfMore)

        val pdfs = withPdfs(2)
        val items = buildLessonItems(pdfs, SectionBuilder.build(pdfs), 1)
        val objective = items.filterIsInstance<LessonItem.Objective>().single()
        assertEquals("Rust in Action (pp. 1-9)", objective.pdfTitle)
        assertEquals(1, objective.pdfMore)
    }
}
