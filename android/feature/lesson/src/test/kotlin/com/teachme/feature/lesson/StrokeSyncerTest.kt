package com.teachme.feature.lesson

import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ManualUserScopes
import com.teachme.core.model.PendingCounts
import com.teachme.core.model.StoredStrokes
import com.teachme.core.model.StrokeRecord
import com.teachme.core.model.UserScope
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkWidth
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class StrokeSyncerTest {
    private val backend = FakeLessons()
    private val store = MemoryStrokeStore()
    private val scopes = ManualUserScopes()
    private val syncer = StrokeSyncer(backend, store, scopes)

    private fun rec(
        n: Int,
        color: String = "ink",
    ) = StrokeRecord("id-$n", "p1", color, "medium", listOf(n, n))

    private fun invalid(
        index: Int,
        field: String = "points",
    ) = ApiFailure.Validation(listOf(ApiErrorDetail("strokes[$index].$field", "invalid")))

    @Test
    fun `a saved set is sent whole in drawing order and stops being dirty, but stays stored for offline opening`() =
        runTest {
            syncer.save("L1", listOf(rec(2), rec(1)))
            assertEquals(StrokeFlushStatus.Done, syncer.flush("L1").status)
            assertEquals(listOf(listOf(rec(2), rec(1))), backend.strokeRequests)
            assertEquals(StoredStrokes("L1", listOf(rec(2), rec(1)), dirty = false), store.get("L1"))
            syncer.flush("L1")
            assertEquals(1, backend.strokeRequests.size)
        }

    @Test
    fun `erasing everything is a dirty empty set and is sent as an empty array`() =
        runTest {
            syncer.save("L1", emptyList())
            assertTrue(syncer.hasPending("L1"))
            syncer.flush("L1")
            assertEquals(listOf(emptyList<StrokeRecord>()), backend.strokeRequests)
        }

    @Test
    fun `a failure keeps the set dirty and the next flush sends the same set again`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            backend.strokeFailures += NETWORK_DOWN
            assertEquals(StrokeFlushStatus.Offline, syncer.flush("L1").status)
            assertTrue(syncer.hasPending("L1"))
            backend.strokeFailures += ApiFailure.Unexpected(503)
            assertEquals(StrokeFlushStatus.Offline, syncer.flush("L1").status)
            assertEquals(StrokeFlushStatus.Done, syncer.flush("L1").status)
            assertEquals(3, backend.strokeRequests.size)
            assertEquals(listOf(rec(0)), backend.storedStrokes)
            assertFalse(syncer.hasPending("L1"))
        }

    @Test
    fun `an edit made while the request is out is not lost, and goes in the next request`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            var edited = false
            backend.onPutStrokes = {
                if (!edited) {
                    edited = true
                    syncer.save("L1", listOf(rec(0), rec(1)))
                }
            }
            assertEquals(StrokeFlushStatus.Done, syncer.flush("L1").status)
            assertEquals(listOf(listOf(rec(0)), listOf(rec(0), rec(1))), backend.strokeRequests)
            assertFalse(syncer.hasPending("L1"))
        }

    @Test
    fun `a pending set survives a restart and wins over the one of the server`() =
        runTest {
            syncer.save("L1", listOf(rec(0), rec(1)))
            val reborn = StrokeSyncer(backend, store, scopes)
            assertEquals(listOf(rec(0), rec(1)), reborn.resolve("L1", listOf(rec(9)), reborn.stamp()))
            reborn.flushAll()
            assertEquals(listOf(rec(0), rec(1)), backend.storedStrokes)
            assertEquals(listOf(rec(9)), reborn.resolve("L1", listOf(rec(9)), reborn.stamp()))
            assertEquals(StoredStrokes("L1", listOf(rec(9)), dirty = false), store.get("L1"))
            assertEquals(listOf(rec(9)), reborn.local("L1"))
        }

    @Test
    fun `a 422 drops only the stroke it points at, found in what was sent, and sends the rest`() =
        runTest {
            syncer.save("L1", listOf(rec(0), rec(1), rec(2)))
            backend.strokeFailures += invalid(1)
            val result = syncer.flush("L1")
            assertEquals(StrokeFlushStatus.Done, result.status)
            assertEquals(listOf(rec(1)), result.discarded)
            assertEquals(listOf(rec(0), rec(2)), backend.storedStrokes)
            assertEquals(listOf(rec(0), rec(2)), store.get("L1")?.items)
        }

    @Test
    fun `after three drops a set that is still refused is parked, not retried in a loop`() =
        runTest {
            syncer.save("L1", (0..5).map { rec(it) })
            repeat(4) { backend.strokeFailures += invalid(0, "block_id") }
            val result = syncer.flush("L1")
            assertEquals(StrokeFlushStatus.Parked, result.status)
            assertEquals(3, result.discarded.size)
            assertEquals(4, backend.strokeRequests.size)
            assertTrue(store.get("L1")!!.parked)
            assertTrue(store.get("L1")!!.dirty)
            assertEquals(StrokeFlushStatus.Parked, syncer.flush("L1").status)
            syncer.flushAll()
            assertEquals(4, backend.strokeRequests.size)
            // An edit takes it out of the parking.
            syncer.save("L1", listOf(rec(0)))
            assertFalse(store.get("L1")!!.parked)
            assertEquals(StrokeFlushStatus.Done, syncer.flush("L1").status)
        }

    @Test
    fun `a 422 without an index parks the set at once, and only an explicit retry tries it again`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            backend.strokeFailures += ApiFailure.Validation(listOf(ApiErrorDetail("strokes", "too_many_points")))
            val first = syncer.flush("L1")
            assertEquals(StrokeFlushStatus.Parked, first.status)
            assertFalse(first.tooLarge)
            assertTrue(first.discarded.isEmpty())
            assertEquals(listOf(rec(0)), store.get("L1")?.items)
            backend.strokeFailures += ApiFailure.Validation(emptyList())
            assertEquals(StrokeFlushStatus.Parked, syncer.flush("L1", retryParked = true).status)
            assertEquals(2, backend.strokeRequests.size)
            assertEquals(StrokeFlushStatus.Parked, syncer.flush("L1").status)
            assertEquals(2, backend.strokeRequests.size)
        }

    @Test
    fun `a 413 parks the set, which stays stored and is not retried by itself`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            backend.strokeFailures += ApiFailure.Unexpected(413)
            val result = syncer.flush("L1")
            assertEquals(StrokeFlushStatus.Parked, result.status)
            assertTrue("the queue is told it is the size", result.tooLarge)
            assertTrue(result.discarded.isEmpty())
            assertTrue(store.get("L1")!!.parked)
            assertEquals(StrokeFlushStatus.Parked, syncer.flush("L1").status)
            assertFalse("a parked set that is not sent again says nothing new", syncer.flush("L1").tooLarge)
            assertEquals(1, backend.strokeRequests.size)
            // Another unexpected status is a server problem: retried later.
            syncer.save("L1", listOf(rec(0), rec(1)))
            backend.strokeFailures += ApiFailure.Unexpected(500)
            assertEquals(StrokeFlushStatus.Offline, syncer.flush("L1").status)
            assertFalse(store.get("L1")!!.parked)
        }

    @Test
    fun `404 forgets the lesson and 401 keeps the set and stops the sweep`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            backend.strokeFailures += ApiFailure.NotFound
            assertEquals(StrokeFlushStatus.Gone, syncer.flush("L1").status)
            assertNull(store.get("L1"))
            syncer.save("L2", listOf(rec(0)))
            syncer.save("L3", listOf(rec(0)))
            backend.strokeFailures += ApiFailure.Unauthorized
            syncer.flushAll()
            assertEquals(2, backend.strokeRequests.size)
            assertTrue(syncer.hasPending("L2"))
            assertTrue(syncer.hasPending("L3"))
        }

    @Test
    fun `a flush belongs to the user whose set it read`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            backend.onPutStrokes = { scopes.switchTo(UserScope("user-b", "http://teach.test")) }
            assertEquals(StrokeFlushStatus.Offline, syncer.flush("L1").status)
            scopes.switchTo(UserScope("user-a", "http://teach.test"))
            assertTrue(syncer.hasPending("L1"))
            assertNotNull(store.get("L1"))
        }

    @Test
    fun `the pen choices default to highlighter, ink and medium, are remembered, and unknown values fall back`() =
        runTest {
            assertEquals(PenTool.HIGHLIGHTER, syncer.tool())
            assertEquals(InkColor.INK, syncer.inkColor())
            assertEquals(InkWidth.MEDIUM, syncer.width())
            syncer.setTool(PenTool.PEN)
            syncer.setInkColor(InkColor.RED)
            syncer.setWidth(InkWidth.THICK)
            assertEquals("pen", store.tool)
            assertEquals("red", store.color)
            assertEquals("thick", store.width)
            assertEquals(PenTool.PEN, syncer.tool())
            assertEquals(InkColor.RED, syncer.inkColor())
            assertEquals(InkWidth.THICK, syncer.width())
            store.tool = "brush"
            store.color = "teal"
            store.width = "huge"
            assertEquals(PenTool.HIGHLIGHTER, syncer.tool())
            assertEquals(InkColor.INK, syncer.inkColor())
            assertEquals(InkWidth.MEDIUM, syncer.width())
        }

    @Test
    fun `a set read from the server while a flush was out never replaces the sent one`() =
        runTest {
            backend.storedStrokes = listOf(rec(0))
            syncer.save("L1", listOf(rec(0), rec(1)))
            lateinit var stampOfTheFetch: FetchStamp
            backend.onPutStrokes = { stampOfTheFetch = syncer.stamp() }
            assertEquals(StrokeFlushStatus.Done, syncer.flush("L1").status)
            val shown = syncer.resolve("L1", listOf(rec(0)), stampOfTheFetch)
            assertEquals(listOf(rec(0), rec(1)), shown)
            assertEquals(listOf(rec(0), rec(1)), store.get("L1")?.items)
        }

    @Test
    fun `a fetch that started before a flush and answers after it does not replace the local set either`() =
        runTest {
            syncer.save("L1", listOf(rec(0), rec(1)))
            val stamp = syncer.stamp()
            syncer.flush("L1")
            assertEquals(listOf(rec(0), rec(1)), syncer.resolve("L1", listOf(rec(0)), stamp))
            assertEquals(listOf(rec(9)), syncer.resolve("L1", listOf(rec(9)), syncer.stamp()))
        }

    @Test
    fun `pendingLessons counts the lessons with unsent notes, and the pending counts add them to the total`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            syncer.save("L2", emptyList())
            syncer.resolve("L3", listOf(rec(1)), syncer.stamp())
            assertEquals(2, syncer.pendingLessons())
            val counts = PendingCounts(reviews = 1, strokes = syncer.pendingLessons())
            assertEquals(3, counts.total)
            assertFalse(counts.isEmpty)
        }

    @Test
    fun `strokes map to records and back, an unknown colour is ink and an unknown width is medium`() {
        val stroke = rec(3, color = "magenta").copy(width = "huge").toStroke()
        assertEquals(InkColor.INK, stroke.color)
        assertEquals(InkWidth.MEDIUM, stroke.width)
        assertEquals(rec(3), stroke.toRecord())
    }

    @Test
    fun `a disk that refuses the write is a failed send, not a crash, and the set is sent when the disk is back`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            store.diskFailing = true
            assertEquals(StrokeFlushStatus.Offline, syncer.flush("L1").status)
            syncer.flushAll()
            assertTrue(syncer.hasPending("L1"))
            assertEquals(listOf(rec(0)), store.get("L1")?.items)
            store.diskFailing = false
            assertEquals(StrokeFlushStatus.Done, syncer.flush("L1").status)
            assertFalse(syncer.hasPending("L1"))
        }

    @Test
    fun `a server set read around a PUT without a local copy is shown but not written`() =
        runTest {
            val shown = syncer.resolve("L1", listOf(rec(5)), FetchStamp(0, true))
            assertEquals(listOf(rec(5)), shown)
            assertNull(store.get("L1"))
            // Not around a PUT: written as the clean copy.
            assertEquals(listOf(rec(5)), syncer.resolve("L1", listOf(rec(5)), syncer.stamp()))
            assertEquals(StoredStrokes("L1", listOf(rec(5)), dirty = false), store.get("L1"))
        }

    @Test
    fun `a disk that refuses the offline copy does not stop the lesson from opening`() =
        runTest {
            store.diskFailing = true
            assertEquals(listOf(rec(5)), syncer.resolve("L1", listOf(rec(5)), syncer.stamp()))
        }
}
