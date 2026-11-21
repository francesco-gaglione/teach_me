package com.teachme.sdui.ink

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

internal fun stroke(
    id: String,
    points: Int = 1,
    block: String = "p1",
) = Stroke(id, block, InkColor.INK, InkWidth.MEDIUM, List(points * 2) { it })

/** Contract model and the limits of the ordered set (lessons.md section 11). */
class InkModelTest {
    @Test
    fun `unknown colour and width read as ink and medium`() {
        val s =
            Json.decodeFromString<Stroke>(
                """{"id":"a","block_id":"p1","color":"mauve","width":"huge","points":[1,2]}""",
            )
        assertEquals(InkColor.INK, s.color)
        assertEquals(InkWidth.MEDIUM, s.width)
        assertEquals("p1", s.blockId)
        assertEquals(1, s.pointCount)
    }

    @Test
    fun `known values round trip with the contract names`() {
        val s = Stroke("a", "p1", InkColor.GREEN, InkWidth.THICK, listOf(1, 2, 3, 4))
        val text = Json.encodeToString(Stroke.serializer(), s)
        assertTrue(text.contains("\"block_id\":\"p1\"") && text.contains("\"green\"") && text.contains("\"thick\""))
        assertEquals(s, Json.decodeFromString<Stroke>(text))
    }

    @Test
    fun `the 2000th stroke is accepted and the 2001st refused`() {
        var set = InkSet.EMPTY
        repeat(2000) { set = (set.add(stroke("s$it")) as InkSetResult.Ok).set }
        assertEquals(2000, set.size)
        assertEquals(InkSetResult.Rejected(InkLimit.TOO_MANY_STROKES), set.add(stroke("x")))
    }

    @Test
    fun `1000 points in a stroke are fine and 1001 are refused`() {
        assertTrue(InkSet.EMPTY.add(stroke("a", 1000)) is InkSetResult.Ok)
        assertEquals(InkSetResult.Rejected(InkLimit.TOO_MANY_POINTS_IN_STROKE), InkSet.EMPTY.add(stroke("a", 1001)))
    }

    @Test
    fun `the total of 200000 points is a limit`() {
        var set = InkSet.EMPTY
        repeat(200) { set = (set.add(stroke("s$it", 1000)) as InkSetResult.Ok).set }
        assertEquals(200_000, set.totalPoints)
        assertEquals(InkSetResult.Rejected(InkLimit.TOO_MANY_POINTS), set.add(stroke("x", 1)))
    }

    @Test
    fun `a refused addition leaves the set as it was`() {
        val set = (InkSet.EMPTY.add(stroke("a")) as InkSetResult.Ok).set
        InkSet.EMPTY.add(stroke("big", 1001))
        assertEquals(listOf("a"), set.strokes.map { it.id })
    }

    @Test
    fun `remove recounts the points`() {
        val set = InkSet.of(listOf(stroke("a", 3), stroke("b", 5)))
        assertEquals(8, set.totalPoints)
        assertEquals(5, set.remove(setOf("a")).totalPoints)
    }
}
