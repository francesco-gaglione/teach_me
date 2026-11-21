package com.teachme.sdui.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The stroke in progress: closing, splitting, simplification (handwriting.md sections 4.1, 5.3, 8). */
class StrokeBuilderTest {
    private val geo = BlockGeometry(left = 0f, top = 0f, width = 1000f)
    private val area = WritingArea(0f, 0f, 1000f, 1000f)
    private var ids = 0

    private fun start(
        x: Float = 10f,
        y: Float = 10f,
        density: Float = 1f,
        geometry: BlockGeometry = geo,
    ) = StrokeBuilder.start("p1", geometry, InkColor.RED, InkWidth.THIN, area, density, x, y) { "id${ids++}" }

    @Test
    fun `a tap is a one point stroke with the style and block`() {
        val b = start(100f, 200f)!!
        val out = b.finish()
        assertEquals(1, out.size)
        assertEquals(listOf(1_000, 2_000), out[0].points)
        assertEquals("p1", out[0].blockId)
        assertEquals(InkColor.RED, out[0].color)
        assertEquals(InkWidth.THIN, out[0].width)
    }

    @Test
    fun `a start outside the area writes nothing`() {
        assertNull(start(-1f, 10f))
        assertNull(start(10f, 1001f))
    }

    @Test
    fun `a point outside the area closes the stroke and a return does not resume it`() {
        val b = start(10f, 10f)!!
        assertTrue(b.add(500f, 10f))
        assertFalse(b.add(1500f, 10f))
        assertFalse(b.isOpen)
        assertFalse(b.add(600f, 10f))
        val out = b.finish()
        assertEquals(1, out.size)
        assertEquals(listOf(100, 100, 5_000, 100), out[0].points)
    }

    @Test
    fun `a point out of the contract range closes the stroke`() {
        val wide = WritingArea(-1_000_000f, -1_000_000f, 1_000_000f, 1_000_000f)
        val b = StrokeBuilder.start("p1", geo, InkColor.INK, InkWidth.MEDIUM, wide, 1f, 0f, 0f) { "i" }!!
        assertTrue(b.add(50_000f * 1000f / 10_000f, 0f))
        assertFalse(b.add(50_000f * 1000f / 10_000f + 1f, 0f))
        assertFalse(b.isOpen)
    }

    @Test
    fun `exactly 800 points stay in one stroke`() {
        val b = start(0f, 0f)!!
        for (i in 1 until 800) assertTrue(b.add(i.toFloat(), (i % 2) * 20f))
        assertTrue(b.drain().isEmpty())
        assertEquals(800, b.livePoints.size)
        assertEquals(1, b.finish().size)
    }

    @Test
    fun `the 801st point splits into a new stroke from the last point`() {
        val b = start(0f, 0f, density = 0.0001f)!!
        for (i in 1..800) b.add(i.toFloat(), (i % 2) * 20f)
        val first = b.drain()
        assertEquals(1, first.size)
        assertEquals(800, first[0].pointCount)
        val rest = b.finish()
        assertEquals(1, rest.size)
        assertEquals(2, rest[0].pointCount)
        assertEquals(first[0].points.takeLast(2), rest[0].points.take(2))
        assertEquals(first[0].blockId, rest[0].blockId)
        assertEquals(first[0].color, rest[0].color)
        assertEquals(first[0].width, rest[0].width)
        assertNotNull(rest[0].id)
        assertTrue(first[0].id != rest[0].id)
    }

    @Test
    fun `a straight line collapses to its ends and a corner is kept`() {
        val straight = start(0f, 0f)!!
        for (i in 1..50) straight.add(i * 10f, i * 5f)
        assertEquals(2, straight.finish()[0].pointCount)

        val corner = start(0f, 0f)!!
        for (i in 1..10) corner.add(i * 10f, 0f)
        for (i in 1..10) corner.add(100f, i * 10f)
        assertEquals(listOf(0, 0, 1_000, 0, 1_000, 1_000), corner.finish()[0].points)
    }

    @Test
    fun `a deviation under 0,4 dp is dropped and one over it kept, in pixels of the density`() {
        fun run(
            offset: Float,
            density: Float,
        ): Int {
            val b = start(0f, 0f, density)!!
            b.add(50f, offset)
            b.add(100f, 0f)
            return b.finish()[0].pointCount
        }
        assertEquals(2, run(0.7f, 2f))
        assertEquals(3, run(0.9f, 2f))
        assertEquals(3, run(0.7f, 1f))
    }

    @Test
    fun `points that quantize to the same integers collapse`() {
        val b = start(0f, 0f)!!
        b.add(0.01f, 0.01f)
        assertEquals(listOf(0, 0), b.finish()[0].points)
    }
}
