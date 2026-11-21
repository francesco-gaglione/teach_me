package com.teachme.sdui.logic

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class OrderShuffleTest {
    @Test
    fun `fnv1a matches the reference vectors`() {
        assertEquals(0x811C9DC5.toInt(), OrderShuffle.fnv1a(""))
        assertEquals(0xE40C292C.toInt(), OrderShuffle.fnv1a("a"))
        assertEquals(0xBF9CF968.toInt(), OrderShuffle.fnv1a("foobar"))
    }

    @Test
    fun `same id gives the same order every time`() {
        val ids = listOf("s1", "s2", "s3", "s4", "s5")
        assertEquals(OrderShuffle.shuffle("order_1", ids), OrderShuffle.shuffle("order_1", ids))
    }

    @Test
    fun `result is a permutation, never the solution, at most half in place`() {
        for (n in 2..8) {
            val ids = (1..n).map { "step$it" }
            for (seed in 0 until 400) {
                val p = OrderShuffle.shuffle("block_$seed", ids)
                assertEquals("n=$n seed=$seed", ids.sorted(), p.sorted())
                assertNotEquals("n=$n seed=$seed", ids, p)
                val inPlace = ids.indices.count { p[it] == ids[it] }
                assertTrue("n=$n seed=$seed inPlace=$inPlace", inPlace * 2 <= n || n == 2)
            }
        }
    }

    @Test
    fun `different ids usually give different orders`() {
        val ids = (1..6).map { "s$it" }
        val distinct = (0 until 50).map { OrderShuffle.shuffle("b$it", ids) }.toSet()
        assertTrue(distinct.size > 10)
    }

    @Test
    fun `pinned vector for a known block id`() {
        // Expected values computed with an independent Python implementation of order_steps.md.
        val ids = listOf("a", "b", "c", "d", "e")
        assertEquals(listOf("d", "e", "c", "a", "b"), OrderShuffle.shuffle("order_compile", ids))
        assertEquals(listOf("s2", "s1", "s3"), OrderShuffle.shuffle("o1", listOf("s1", "s2", "s3")))
    }
}
