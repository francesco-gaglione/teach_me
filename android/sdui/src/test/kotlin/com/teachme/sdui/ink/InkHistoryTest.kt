package com.teachme.sdui.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Eraser and Undo/Redo (handwriting.md section 6). */
class InkHistoryTest {
    private fun InkEdit.history() = (this as InkEdit.Applied).history

    private fun InkHistory.ids() = set.strokes.map { it.id }

    private fun write(
        h: InkHistory,
        vararg ids: String,
    ) = ids.fold(h) { acc, id -> acc.write(stroke(id)).history() }

    @Test
    fun `undo and redo of a written stroke`() {
        val h = write(InkHistory(), "a", "b")
        val undone = h.undo().history()
        assertEquals(listOf("a"), undone.ids())
        assertTrue(undone.canRedo)
        assertEquals(listOf("a", "b"), undone.redo().history().ids())
    }

    @Test
    fun `empty stacks are no-ops`() {
        assertEquals(InkEdit.NoOp, InkHistory().undo())
        assertEquals(InkEdit.NoOp, InkHistory().redo())
        assertEquals(InkEdit.NoOp, write(InkHistory(), "a").erase(listOf("zz")))
        assertEquals(InkEdit.NoOp, write(InkHistory(), "a").erase(emptyList()))
    }

    @Test
    fun `undoing an erase restores the original drawing order`() {
        val h = write(InkHistory(), "a", "b", "c", "d", "e").erase(listOf("b", "d")).history()
        assertEquals(listOf("a", "c", "e"), h.ids())
        val back = h.undo().history()
        assertEquals(listOf("a", "b", "c", "d", "e"), back.ids())
        assertEquals(listOf("a", "c", "e"), back.redo().history().ids())
    }

    @Test
    fun `an erase pass touching strokes of different blocks is one operation`() {
        val strokes = listOf(stroke("a", block = "p1"), stroke("b", block = "p2"), stroke("c", block = "p3"))
        var h = InkHistory()
        for (s in strokes) h = h.write(s).history()
        h = h.erase(listOf("a", "c")).history()
        assertEquals(listOf("b"), h.ids())
        assertEquals(4, h.undoStack.size)
        assertEquals(listOf("a", "b", "c"), h.undo().history().ids())
    }

    @Test
    fun `a new operation empties redo`() {
        val h = write(InkHistory(), "a", "b").undo().history()
        assertTrue(h.canRedo)
        assertFalse(h.write(stroke("c")).history().canRedo)
        assertFalse(h.erase(listOf("a")).history().canRedo)
    }

    @Test
    fun `the 51st operation drops the oldest`() {
        var h = InkHistory()
        for (i in 1..51) h = h.write(stroke("s$i")).history()
        assertEquals(50, h.undoStack.size)
        repeat(50) { h = h.undo().history() }
        assertFalse(h.canUndo)
        assertEquals(listOf("s1"), h.ids())
    }

    @Test
    fun `exactly 50 operations are all undoable`() {
        var h = InkHistory()
        for (i in 1..50) h = h.write(stroke("s$i")).history()
        repeat(50) { h = h.undo().history() }
        assertTrue(h.ids().isEmpty())
    }

    @Test
    fun `a write over the limit is rejected and changes nothing`() {
        val full = InkHistory(InkSet.of(List(2000) { stroke("s$it") }))
        val edit = full.write(stroke("x"))
        assertEquals(InkEdit.Rejected(InkLimit.TOO_MANY_STROKES), edit)
    }

    @Test
    fun `undo of an erase that would pass the limit is rejected and keeps the stacks`() {
        val erased = write(InkHistory(), "a").erase(listOf("a")).history()
        val full = InkHistory(InkSet.of(List(2000) { stroke("s$it") }), erased.undoStack, erased.redoStack)
        assertEquals(InkEdit.Rejected(InkLimit.TOO_MANY_STROKES), full.undo())
        assertEquals(2, full.undoStack.size)
    }

    @Test
    fun `redo of a write over the limit is rejected`() {
        val undone = write(InkHistory(), "a").undo().history()
        val full = InkHistory(InkSet.of(List(2000) { stroke("s$it") }), undone.undoStack, undone.redoStack)
        assertEquals(InkEdit.Rejected(InkLimit.TOO_MANY_STROKES), full.redo())
        assertTrue(full.canRedo)
    }

    // --- eraser ---

    private val geo = BlockGeometry(left = 0f, top = 0f, width = 1000f)

    private fun line(
        id: String,
        width: InkWidth,
        vararg pts: Int,
        block: String = "p1",
    ) = Stroke(id, block, InkColor.INK, width, pts.toList())

    @Test
    fun `touched is within 12 dp plus half the width`() {
        val s = line("a", InkWidth.THICK, 0, 0, 10_000, 0) // horizontal at y = 0, half width 2 dp

        fun hit(y: Float) = InkEraser.touched(listOf(s), { geo }, listOf(InkPoint(500f, y)), 1f)
        assertEquals(listOf("a"), hit(14f))
        assertEquals(emptyList<String>(), hit(14.5f))
    }

    @Test
    fun `the radius is in dp so it scales with the density`() {
        val s = line("a", InkWidth.THIN, 0, 0, 10_000, 0)
        assertEquals(emptyList<String>(), InkEraser.touched(listOf(s), { geo }, listOf(InkPoint(500f, 20f)), 1f))
        assertEquals(listOf("a"), InkEraser.touched(listOf(s), { geo }, listOf(InkPoint(500f, 20f)), 2f))
    }

    @Test
    fun `one pass touches strokes of different blocks and skips unmeasured ones`() {
        val a = line("a", InkWidth.THIN, 0, 0, 10_000, 0, block = "p1")
        val b = line("b", InkWidth.THIN, 0, 0, 10_000, 0, block = "p2")
        val c = line("c", InkWidth.THIN, 0, 0, 10_000, 0, block = "gone")
        val far = line("d", InkWidth.THIN, 0, 5_000, 10_000, 5_000, block = "p1")
        val geoOf = { id: String ->
            when (id) {
                "p1" -> geo
                "p2" -> BlockGeometry(0f, 5f, 1000f)
                else -> null
            }
        }
        val path = listOf(InkPoint(200f, 2f), InkPoint(300f, 2f))
        assertEquals(listOf("a", "b"), InkEraser.touched(listOf(a, b, c, far), geoOf, path, 1f))
    }

    @Test
    fun `a dot stroke is touched by distance to its point`() {
        val dot = line("a", InkWidth.MEDIUM, 5_000, 5_000)
        assertEquals(listOf("a"), InkEraser.touched(listOf(dot), { geo }, listOf(InkPoint(510f, 500f)), 1f))
        assertEquals(emptyList<String>(), InkEraser.touched(listOf(dot), { geo }, listOf(InkPoint(520f, 500f)), 1f))
    }

    @Test
    fun `a long two point line is touched in its middle and not beyond the reach`() {
        // 300 dp wide block at density 1: the line runs from x = 0 to x = 300, only its two ends are points.
        val g = BlockGeometry(0f, 0f, 300f)
        val s = line("a", InkWidth.THICK, 0, 0, 10_000, 0)
        assertEquals(listOf("a"), InkEraser.touched(listOf(s), { g }, listOf(InkPoint(150f, 0f)), 1f))
        val reach = 12f + 2f
        assertEquals(listOf("a"), InkEraser.touched(listOf(s), { g }, listOf(InkPoint(150f, reach)), 1f))
        assertEquals(emptyList<String>(), InkEraser.touched(listOf(s), { g }, listOf(InkPoint(150f, reach + 1f)), 1f))
    }

    @Test
    fun `an empty path touches nothing`() {
        assertEquals(
            emptyList<String>(),
            InkEraser.touched(listOf(line("a", InkWidth.THIN, 0, 0)), { geo }, emptyList(), 1f),
        )
    }

    @Test
    fun `the eraser measures only the strokes near the samples`() {
        // 2000 strokes of 100 points in a column of rows, 1 px apart in x at density 1: only the row at y = 0 is near.
        val strokes =
            List(2_000) { n ->
                val y = n * 1_000
                Stroke("s$n", "p1", InkColor.INK, InkWidth.THIN, (0 until 100).flatMap { listOf(it * 50, y) })
            }
        val bounds = StrokeBoundsCache()
        val ids = InkEraser.touched(strokes, { geo }, listOf(InkPoint(250f, 0f)), 1f, bounds)
        assertEquals(listOf("s0"), ids)
        assertTrue("only the near stroke is measured", bounds.segmentTests <= 99L)
        // Boxes are kept: a second pass works out none.
        val computed = bounds.computed
        InkEraser.touched(strokes, { geo }, listOf(InkPoint(250f, 0f)), 1f, bounds)
        assertEquals(computed, bounds.computed)
    }

    @Test
    fun `a refreshed stroke with equal points keeps its box`() {
        val a = line("a", InkWidth.THIN, 0, 0, 10_000, 0)
        val bounds = StrokeBoundsCache()
        InkEraser.touched(listOf(a), { geo }, listOf(InkPoint(500f, 0f)), 1f, bounds)
        val copy = a.copy(points = a.points.toList())
        InkEraser.touched(listOf(copy), { geo }, listOf(InkPoint(500f, 0f)), 1f, bounds)
        assertEquals(1, bounds.computed)
    }
}
