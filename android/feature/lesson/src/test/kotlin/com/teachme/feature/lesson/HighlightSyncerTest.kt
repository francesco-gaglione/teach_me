package com.teachme.feature.lesson

import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.HighlightRecord
import com.teachme.core.model.ManualUserScopes
import com.teachme.core.model.StoredHighlights
import com.teachme.core.model.UserScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HighlightSyncerTest {
    private val backend = FakeLessons()
    private val store = MemoryHighlightStore()
    private val scopes = ManualUserScopes()
    private val syncer = HighlightSyncer(backend, store, scopes)

    private fun rec(
        start: Int,
        block: String = "p1",
        color: String = "yellow",
    ) = HighlightRecord(block, start, start + 3, color)

    private fun invalid(index: Int) =
        ApiFailure.Validation(listOf(ApiErrorDetail("highlights[$index].start", "overlap")))

    @Test
    fun `a saved set is sent whole and stops being dirty, but stays stored for offline opening`() =
        runTest {
            syncer.save("L1", listOf(rec(0), rec(10)))
            val result = syncer.flush("L1")
            assertEquals(HighlightFlushStatus.Done, result.status)
            assertEquals(listOf(listOf(rec(0), rec(10))), backend.highlightRequests)
            assertEquals(StoredHighlights("L1", listOf(rec(0), rec(10)), dirty = false), store.get("L1"))
            // Nothing to send: no request.
            syncer.flush("L1")
            assertEquals(1, backend.highlightRequests.size)
        }

    @Test
    fun `erasing everything is a dirty empty set and is sent as an empty array`() =
        runTest {
            syncer.save("L1", emptyList())
            assertTrue(syncer.hasPending("L1"))
            syncer.flush("L1")
            assertEquals(listOf(emptyList<HighlightRecord>()), backend.highlightRequests)
        }

    @Test
    fun `a failure keeps the set dirty and the next flush sends the same set again`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            backend.highlightFailures += NETWORK_DOWN
            assertEquals(HighlightFlushStatus.Offline, syncer.flush("L1").status)
            assertTrue(syncer.hasPending("L1"))
            backend.highlightFailures += ApiFailure.Unexpected(503)
            assertEquals(HighlightFlushStatus.Offline, syncer.flush("L1").status)
            assertEquals(HighlightFlushStatus.Done, syncer.flush("L1").status)
            assertEquals(3, backend.highlightRequests.size)
            assertEquals(listOf(rec(0)), backend.storedHighlights)
            assertFalse(syncer.hasPending("L1"))
        }

    @Test
    fun `an edit made while the request is out is not lost, and goes in the next request`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            var edited = false
            backend.onPutHighlights = {
                if (!edited) {
                    edited = true
                    syncer.save("L1", listOf(rec(0), rec(10)))
                }
            }
            assertEquals(HighlightFlushStatus.Done, syncer.flush("L1").status)
            assertEquals(listOf(listOf(rec(0)), listOf(rec(0), rec(10))), backend.highlightRequests)
            assertFalse(syncer.hasPending("L1"))
            assertEquals(listOf(rec(0), rec(10)), store.get("L1")?.items)
        }

    @Test
    fun `a pending set survives a restart and wins over the one of the server`() =
        runTest {
            syncer.save("L1", listOf(rec(0), rec(10)))
            // The process dies: a new syncer over the same disk.
            val reborn = HighlightSyncer(backend, store, scopes)
            assertEquals(listOf(rec(0), rec(10)), reborn.resolve("L1", listOf(rec(50, "p9")), reborn.stamp()))
            assertTrue(reborn.hasPending("L1"))
            reborn.flushAll()
            assertEquals(listOf(rec(0), rec(10)), backend.storedHighlights)
            // Once sent, the next opening shows the backend's set, which becomes the stored one.
            assertEquals(listOf(rec(50, "p9")), reborn.resolve("L1", listOf(rec(50, "p9")), reborn.stamp()))
            assertEquals(StoredHighlights("L1", listOf(rec(50, "p9")), dirty = false), store.get("L1"))
            assertEquals(listOf(rec(50, "p9")), reborn.local("L1"))
        }

    @Test
    fun `a 422 drops only the entry it points at, found in what was sent, and sends the rest`() =
        runTest {
            syncer.save("L1", listOf(rec(0), rec(10), rec(20)))
            backend.highlightFailures += invalid(1)
            val result = syncer.flush("L1")
            assertEquals(HighlightFlushStatus.Done, result.status)
            assertEquals(listOf(rec(10)), result.discarded)
            assertEquals(listOf(rec(0), rec(20)), backend.storedHighlights)
            assertEquals(listOf(rec(0), rec(20)), store.get("L1")?.items)
        }

    @Test
    fun `after three drops a set that is still refused is parked, not retried in a loop`() =
        runTest {
            syncer.save("L1", (0..5).map { rec(it * 10) })
            repeat(4) { backend.highlightFailures += invalid(0) }
            val result = syncer.flush("L1")
            assertEquals(HighlightFlushStatus.Parked, result.status)
            assertEquals(3, result.discarded.size)
            assertEquals(4, backend.highlightRequests.size)
            assertTrue(store.get("L1")!!.parked)
            assertTrue(store.get("L1")!!.dirty)
            // Not sent again by the periodic flushes...
            assertEquals(HighlightFlushStatus.Parked, syncer.flush("L1").status)
            syncer.flushAll()
            assertEquals(4, backend.highlightRequests.size)
            // ...but an edit takes it out of the parking, and opening the lesson tries once more.
            syncer.save("L1", listOf(rec(0)))
            assertFalse(store.get("L1")!!.parked)
            assertEquals(HighlightFlushStatus.Done, syncer.flush("L1").status)
        }

    @Test
    fun `a 422 that points at no entry parks the set at once, and opening the lesson tries it once`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            backend.highlightFailures += ApiFailure.Validation(listOf(ApiErrorDetail("highlights", "too_many")))
            val first = syncer.flush("L1")
            assertEquals(HighlightFlushStatus.Parked, first.status)
            assertTrue(first.discarded.isEmpty())
            assertEquals(listOf(rec(0)), store.get("L1")?.items)
            backend.highlightFailures += ApiFailure.Validation(emptyList())
            assertEquals(HighlightFlushStatus.Parked, syncer.flush("L1", retryParked = true).status)
            assertEquals(2, backend.highlightRequests.size)
            assertEquals(HighlightFlushStatus.Parked, syncer.flush("L1").status)
            assertEquals(2, backend.highlightRequests.size)
        }

    @Test
    fun `404 forgets the lesson and 401 keeps the set and stops the sweep`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            backend.highlightFailures += ApiFailure.NotFound
            assertEquals(HighlightFlushStatus.Gone, syncer.flush("L1").status)
            assertNull(store.get("L1"))
            syncer.save("L2", listOf(rec(0)))
            syncer.save("L3", listOf(rec(0)))
            backend.highlightFailures += ApiFailure.Unauthorized
            syncer.flushAll()
            // L1 got its 404, L2 its 401, and L3 was not tried.
            assertEquals(2, backend.highlightRequests.size)
            assertTrue(syncer.hasPending("L2"))
            assertTrue(syncer.hasPending("L3"))
        }

    @Test
    fun `a flush belongs to the user whose set it read`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            backend.onPutHighlights = { scopes.switchTo(UserScope("user-b", "http://teach.test")) }
            assertEquals(HighlightFlushStatus.Offline, syncer.flush("L1").status)
            scopes.switchTo(UserScope("user-a", "http://teach.test"))
            assertTrue(syncer.hasPending("L1"))
            assertNotNull(store.get("L1"))
        }

    @Test
    fun `the colour is remembered, and an unknown one is yellow`() =
        runTest {
            assertEquals(com.teachme.sdui.highlight.HighlightColor.YELLOW, syncer.color())
            syncer.setColor(com.teachme.sdui.highlight.HighlightColor.PINK)
            assertEquals(com.teachme.sdui.highlight.HighlightColor.PINK, syncer.color())
            store.color = "turquoise"
            assertEquals(com.teachme.sdui.highlight.HighlightColor.YELLOW, syncer.color())
        }

    @Test
    fun `a set read from the server while a flush was out never replaces the sent one`() =
        runTest {
            backend.storedHighlights = listOf(rec(0))
            syncer.save("L1", listOf(rec(0), rec(10)))
            lateinit var stampOfTheFetch: FetchStamp
            backend.onPutHighlights = {
                // The lesson is opened (the GET starts, and answers with the old set) while the PUT is out.
                stampOfTheFetch = syncer.stamp()
            }
            assertEquals(HighlightFlushStatus.Done, syncer.flush("L1").status)
            assertFalse(syncer.hasPending("L1"))
            // The old answer arrives after the PUT ended and cleared dirty.
            val shown = syncer.resolve("L1", listOf(rec(0)), stampOfTheFetch)
            assertEquals(listOf(rec(0), rec(10)), shown)
            assertEquals(listOf(rec(0), rec(10)), store.get("L1")?.items)
            // The edit after it extends the right set.
            syncer.save("L1", shown + rec(20))
            assertEquals(listOf(rec(0), rec(10), rec(20)), store.get("L1")?.items)
        }

    @Test
    fun `a fetch that started before a flush and answers after it does not replace the local set either`() =
        runTest {
            syncer.save("L1", listOf(rec(0), rec(10)))
            val stamp = syncer.stamp()
            syncer.flush("L1")
            assertEquals(listOf(rec(0), rec(10)), syncer.resolve("L1", listOf(rec(0)), stamp))
            // A fetch made after the flush is trusted again.
            assertEquals(listOf(rec(50, "p9")), syncer.resolve("L1", listOf(rec(50, "p9")), syncer.stamp()))
        }

    @Test
    fun `pendingLessons counts the lessons with unsent changes`() =
        runTest {
            syncer.save("L1", listOf(rec(0)))
            syncer.save("L2", emptyList())
            syncer.resolve("L3", listOf(rec(1)), syncer.stamp())
            assertEquals(2, syncer.pendingLessons())
        }
}
