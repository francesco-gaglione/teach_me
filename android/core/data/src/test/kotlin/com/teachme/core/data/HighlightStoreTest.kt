package com.teachme.core.data

import com.teachme.core.model.HighlightRecord
import com.teachme.core.model.ManualUserScopes
import com.teachme.core.model.StoredHighlights
import com.teachme.core.model.UserScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The highlights waiting to be sent and the colour of the pen are kept per user and survive a restart (ADR 0008). */
internal class HighlightStoreTest : DataStoreTest() {
    private val a = UserScope("user-a", "http://teach.test")
    private val b = UserScope("user-b", "http://teach.test")

    private fun record(
        start: Int = 0,
        color: String = "yellow",
    ) = HighlightRecord("p1", start, start + 5, color)

    @Test
    fun `a pending set and the colour survive a restart`() =
        runTest {
            val first = DataStoreHighlightStore(store("hl"), ManualUserScopes(a))
            first.update("L1") { StoredHighlights("L1", listOf(record(), record(10, "pink")), dirty = true) }
            first.update("L2") { StoredHighlights("L2", emptyList(), dirty = true) }
            first.setColor("green")
            killProcess()
            val second = DataStoreHighlightStore(store("hl"), ManualUserScopes(a))
            assertEquals(listOf(record(), record(10, "pink")), second.get("L1")?.items)
            assertTrue(second.get("L1")!!.dirty)
            // "Erased everything, not sent yet" is not the same as nothing pending.
            assertEquals(emptyList<HighlightRecord>(), second.get("L2")?.items)
            assertTrue(second.get("L2")!!.dirty)
            assertEquals(listOf("L1", "L2"), second.dirtyLessons())
            assertEquals("green", second.color())
        }

    @Test
    fun `what A left is not seen or changed under B, and each has their own colour`() =
        runTest {
            val scopes = ManualUserScopes(a)
            val store = DataStoreHighlightStore(store("hl"), scopes)
            store.update("L1") { StoredHighlights("L1", listOf(record()), dirty = true) }
            store.setColor("blue")
            scopes.switchTo(b)
            assertNull(store.get("L1"))
            assertTrue(store.dirtyLessons().isEmpty())
            assertNull(store.color())
            store.update("L1") { StoredHighlights("L1", listOf(record(3)), dirty = true) }
            store.setColor("pink")
            scopes.switchTo(a)
            assertEquals(listOf(record()), store.get("L1")?.items)
            assertEquals("blue", store.color())
        }

    @Test
    fun `update is one read-modify-write, a null result removes the entry, without a user nothing is kept`() =
        runTest {
            val scopes = ManualUserScopes(a)
            val store = DataStoreHighlightStore(store("hl"), scopes)
            store.update("L1") { StoredHighlights("L1", listOf(record()), dirty = true) }
            store.update("L1") { it?.copy(dirty = false) }
            assertEquals(false, store.get("L1")?.dirty)
            store.update("L1") { null }
            assertNull(store.get("L1"))
            scopes.switchTo(null)
            store.update("L1") { StoredHighlights("L1", listOf(record()), dirty = true) }
            store.setColor("pink")
            assertNull(store.get("L1"))
            assertNull(store.color())
        }

    @Test
    fun `the copies already sent are limited to the most recent ones, the unsent ones are never dropped`() =
        runTest {
            val store = DataStoreHighlightStore(store("hl"), ManualUserScopes(a))
            store.update("DIRTY") { StoredHighlights("DIRTY", listOf(record()), dirty = true) }
            repeat(MAX_CLEAN_HIGHLIGHT_SETS + 5) { n ->
                store.update("L$n") { StoredHighlights("L$n", listOf(record(n)), dirty = false) }
            }
            // The oldest copies are gone, the most recent stay.
            assertNull(store.get("L0"))
            assertNull(store.get("L4"))
            assertEquals(listOf(record(5)), store.get("L5")?.items)
            assertEquals(
                listOf(record(MAX_CLEAN_HIGHLIGHT_SETS + 4)),
                store.get("L${MAX_CLEAN_HIGHLIGHT_SETS + 4}")?.items,
            )
            assertEquals(listOf("DIRTY"), store.dirtyLessons())
            assertEquals(listOf(record()), store.get("DIRTY")?.items)
        }

    @Test
    fun `an empty copy of the backend's set is not kept, and a set that did not change is not rewritten`() =
        runTest {
            val store = DataStoreHighlightStore(store("hl"), ManualUserScopes(a))
            store.update("L1") { StoredHighlights("L1", emptyList(), dirty = false) }
            assertNull(store.get("L1"))
            // An empty set that waits to be sent is not nothing.
            store.update("L2") { StoredHighlights("L2", emptyList(), dirty = true) }
            assertEquals(listOf("L2"), store.dirtyLessons())
            store.update("L3") { StoredHighlights("L3", listOf(record()), dirty = false) }
            store.update("L4") { StoredHighlights("L4", listOf(record(1)), dirty = false) }
            // Same content again: it stays where it was in the order, so it is not "refreshed" ahead of L4.
            store.update("L3") { StoredHighlights("L3", listOf(record()), dirty = false) }
            assertEquals(listOf(record()), store.get("L3")?.items)
        }
}
