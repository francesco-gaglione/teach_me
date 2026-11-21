package com.teachme.sdui.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Screen point <-> contract coordinates and the choice of the block (handwriting.md section 5.2). */
class InkGeometryTest {
    private val geo = BlockGeometry(left = 100f, top = 500f, width = 600f)

    @Test
    fun `a point converts to ten-thousandths of the block width`() {
        assertEquals(ContractPoint(0, 0), geo.toContract(100f, 500f))
        assertEquals(ContractPoint(10_000, 5_000), geo.toContract(700f, 800f))
        assertEquals(ContractPoint(-1_667, -833), geo.toContract(0f, 450f))
    }

    @Test
    fun `contract coordinates scale with the current width`() {
        val wide = BlockGeometry(left = 0f, top = 0f, width = 1200f)
        val p = wide.toScreen(5_000, 2_500)
        assertEquals(600f, p.x, 0.01f)
        assertEquals(300f, p.y, 0.01f)
    }

    @Test
    fun `the allowed ranges include their edges and refuse one more`() {
        val g = BlockGeometry(0f, 0f, 10_000f)
        assertEquals(ContractPoint(-40_000, -40_000), g.toContract(-40_000f, -40_000f))
        assertEquals(ContractPoint(50_000, 200_000), g.toContract(50_000f, 200_000f))
        assertNull(g.toContract(-40_001f, 0f))
        assertNull(g.toContract(50_001f, 0f))
        assertNull(g.toContract(0f, -40_001f))
        assertNull(g.toContract(0f, 200_001f))
    }

    @Test
    fun `a block without width gives no coordinates`() {
        assertNull(BlockGeometry(0f, 0f, 0f).toContract(1f, 1f))
    }

    private val blocks =
        listOf(
            BlockExtent("a", top = 0f, height = 100f),
            BlockExtent("b", top = 124f, height = 50f),
            BlockExtent("c", top = 198f, height = 80f),
        )

    @Test
    fun `a point inside a block, margins included, picks that block`() {
        assertEquals("a", pickBlock(blocks, 50f))
        assertEquals("b", pickBlock(blocks, 150f))
        assertEquals("c", pickBlock(blocks, 250f))
    }

    @Test
    fun `a point in a gap goes to the nearest block`() {
        assertEquals("a", pickBlock(blocks, 105f))
        assertEquals("b", pickBlock(blocks, 119f))
    }

    @Test
    fun `halfway in the gap goes to the block above`() {
        assertEquals("a", pickBlock(blocks, 112f))
        assertEquals("b", pickBlock(blocks, 112.01f))
        assertEquals("b", pickBlock(blocks, 186f))
    }

    @Test
    fun `above the first and below the last block pick those`() {
        assertEquals("a", pickBlock(blocks, -500f))
        assertEquals("c", pickBlock(blocks, 5_000f))
    }

    @Test
    fun `no visible block picks nothing`() {
        assertNull(pickBlock(emptyList(), 10f))
    }
}
