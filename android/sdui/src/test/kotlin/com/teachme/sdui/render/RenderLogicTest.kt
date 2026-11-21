package com.teachme.sdui.render

import com.teachme.sdui.model.CATALOG_V1_TYPES
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.CalloutVariant
import com.teachme.sdui.model.Heading
import com.teachme.sdui.model.LessonLink
import com.teachme.sdui.model.Paragraph
import org.junit.Assert.assertEquals
import org.junit.Test

class RenderLogicTest {
    private val h1 = Heading("h1", level = 1, text = "T")
    private val h2 = Heading("h2", level = 2, text = "S")
    private val h3 = Heading("h3", level = 3, text = "s")
    private val p = Paragraph("p", text = "x")
    private val p2 = Paragraph("p2", text = "y")
    private val c = Callout("c", variant = CalloutVariant.Tip, text = "z")
    private val l1 = LessonLink("l1", lessonId = "a", label = "L")
    private val l2 = LessonLink("l2", lessonId = "b", label = "M")

    @Test
    fun `vertical rhythm`() {
        assertEquals(BlockGap.None, gapBetween(null, p))
        assertEquals(BlockGap.Heading, gapBetween(h2, p))
        assertEquals(BlockGap.Heading, gapBetween(h1, c))
        assertEquals(BlockGap.Paragraph, gapBetween(p, p2))
        assertEquals(BlockGap.Block, gapBetween(p, c))
        assertEquals(BlockGap.Section, gapBetween(p, h2))
        assertEquals(BlockGap.H3, gapBetween(p, h3))
        assertEquals(BlockGap.Heading, gapBetween(h2, h3))
        assertEquals(BlockGap.Small, gapBetween(l1, l2))
        // A second level 1 heading is drawn like a section title.
        assertEquals(BlockGap.Section, gapBetween(p, Heading("h9", level = 1, text = "again"), titleBlockId = "h1"))
    }

    @Test
    fun `the registry covers the whole catalog`() {
        assertEquals(CATALOG_V1_TYPES, DefaultBlockRegistry.types)
    }
}
