package com.teachme.feature.lesson

import com.teachme.sdui.model.Block
import com.teachme.sdui.model.ChoiceOption
import com.teachme.sdui.model.CodeBlock
import com.teachme.sdui.model.Flashcard
import com.teachme.sdui.model.Heading
import com.teachme.sdui.model.Paragraph
import com.teachme.sdui.model.QuizSingle
import com.teachme.sdui.render.BlockGap
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ReadingPlanTest {
    private fun row(
        block: Block,
        ordinal: Int,
    ) = LessonItem.BlockItem(block, 0, BlockGap.Block, ordinal)

    private val quiz = QuizSingle("q1", question = "?", options = listOf(ChoiceOption("a", "A")), correct = "a")
    private val optional = Flashcard("f1", front = "f", back = "b", required = false)
    private val items: List<LessonItem> =
        listOf(
            Heading("h1", level = 1, text = "T"),
            Paragraph("p1", text = "one"),
            CodeBlock("c1", code = "x"),
            Paragraph("p2", text = "two"),
            optional,
            quiz,
            Paragraph("p3", text = "three"),
        ).mapIndexed { i, b -> row(b, i) } + LessonItem.Continue(0)

    @Test
    fun `slots keep the document order, gate a required open exercise and drop the other exercises`() {
        val slots = readingSlots(items) { false }
        assertEquals(
            listOf(
                ReadingSlot.Read("h1"),
                ReadingSlot.Read("p1"),
                ReadingSlot.Read("c1"),
                ReadingSlot.Read("p2"),
                ReadingSlot.Gate("q1"),
                ReadingSlot.Read("p3"),
            ),
            slots,
        )
    }

    @Test
    fun `an answered required exercise is no gate any more`() {
        val slots = readingSlots(items) { it == "q1" }
        assertTrue(slots.none { it is ReadingSlot.Gate })
        assertEquals(listOf("h1", "p1", "c1", "p2", "p3"), slots.map { it.blockId })
    }

    @Test
    fun `starts from the first slot at or after the first visible row`() {
        val slots = readingSlots(items) { false }
        assertEquals("h1", firstSlotAtOrAfter(items, slots, 0))
        assertEquals("p2", firstSlotAtOrAfter(items, slots, 3))
        // The optional exercise is no slot: the next block is the start.
        assertEquals("q1", firstSlotAtOrAfter(items, slots, 4))
        // Only the "Continua" bar is left below.
        assertNull(firstSlotAtOrAfter(items, slots, 7))
    }

    @Test
    fun `a row is fully visible only when it is entirely inside the window`() {
        assertTrue(isFullyVisible(10, 100, 0, 110))
        assertFalse(isFullyVisible(-1, 100, 0, 500))
        assertFalse(isFullyVisible(450, 100, 0, 500))
    }
}
