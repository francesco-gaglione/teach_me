package com.teachme.core.data

import com.teachme.core.model.ManualUserScopes
import com.teachme.core.model.StoredStrokes
import com.teachme.core.model.StrokeRecord
import com.teachme.core.model.UserScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The notes waiting to be sent and the pen choices are kept per user and survive a restart (ADR 0008). */
internal class StrokeStoreTest : DataStoreTest() {
    private val a = UserScope("user-a", "http://teach.test")
    private val b = UserScope("user-b", "http://teach.test")

    private fun record(n: Int = 0) = StrokeRecord("id-$n", "p1", "ink", "medium", listOf(n, n, n + 1, n + 1))

    @Test
    fun `a pending set, in order, and the pen choices survive a restart`() =
        runTest {
            val first = DataStoreStrokeStore(store("st"), ManualUserScopes(a), folder.root)
            first.update("L1") { StoredStrokes("L1", listOf(record(2), record(1)), dirty = true, parked = true) }
            first.update("L2") { StoredStrokes("L2", emptyList(), dirty = true) }
            first.setPenTool("pen")
            first.setInkColor("red")
            first.setPenWidth("thick")
            killProcess()
            val second = DataStoreStrokeStore(store("st"), ManualUserScopes(a), folder.root)
            assertEquals(listOf(record(2), record(1)), second.get("L1")?.items)
            assertTrue(second.get("L1")!!.dirty)
            assertTrue(second.get("L1")!!.parked)
            assertEquals(emptyList<StrokeRecord>(), second.get("L2")?.items)
            assertEquals(listOf("L1", "L2"), second.dirtyLessons())
            assertEquals("pen", second.penTool())
            assertEquals("red", second.inkColor())
            assertEquals("thick", second.penWidth())
        }

    @Test
    fun `the pen choices are null until chosen`() =
        runTest {
            val store = DataStoreStrokeStore(store("st"), ManualUserScopes(a), folder.root)
            assertNull(store.penTool())
            assertNull(store.inkColor())
            assertNull(store.penWidth())
        }

    @Test
    fun `what A left is not seen or changed under B, and each has their own pen choices`() =
        runTest {
            val scopes = ManualUserScopes(a)
            val store = DataStoreStrokeStore(store("st"), scopes, folder.root)
            store.update("L1") { StoredStrokes("L1", listOf(record()), dirty = true) }
            store.setPenTool("pen")
            scopes.switchTo(b)
            assertNull(store.get("L1"))
            assertTrue(store.dirtyLessons().isEmpty())
            assertNull(store.penTool())
            store.update("L1") { StoredStrokes("L1", listOf(record(3)), dirty = true) }
            store.setPenTool("highlighter")
            scopes.switchTo(a)
            assertEquals(listOf(record()), store.get("L1")?.items)
            assertEquals("pen", store.penTool())
        }

    @Test
    fun `update is one read-modify-write, a null result removes the entry, without a user nothing is kept`() =
        runTest {
            val scopes = ManualUserScopes(a)
            val store = DataStoreStrokeStore(store("st"), scopes, folder.root)
            store.update("L1") { StoredStrokes("L1", listOf(record()), dirty = true) }
            store.update("L1") { it?.copy(dirty = false) }
            assertEquals(false, store.get("L1")?.dirty)
            store.update("L1") { null }
            assertNull(store.get("L1"))
            scopes.switchTo(null)
            store.update("L1") { StoredStrokes("L1", listOf(record()), dirty = true) }
            store.setInkColor("blue")
            assertNull(store.get("L1"))
            assertNull(store.inkColor())
        }

    @Test
    fun `the copies already sent are limited to the most recent ones, the unsent ones are never dropped`() =
        runTest {
            val store = DataStoreStrokeStore(store("st"), ManualUserScopes(a), folder.root)
            store.update("DIRTY") { StoredStrokes("DIRTY", listOf(record()), dirty = true) }
            repeat(MAX_CLEAN_STROKE_SETS + 5) { n ->
                store.update("L$n") { StoredStrokes("L$n", listOf(record(n)), dirty = false) }
            }
            assertNull(store.get("L0"))
            assertNull(store.get("L4"))
            assertEquals(listOf(record(5)), store.get("L5")?.items)
            assertEquals(listOf("DIRTY"), store.dirtyLessons())
        }

    @Test
    fun `an empty copy of the backend's set is not kept`() =
        runTest {
            val store = DataStoreStrokeStore(store("st"), ManualUserScopes(a), folder.root)
            store.update("L1") { StoredStrokes("L1", emptyList(), dirty = false) }
            assertNull(store.get("L1"))
            store.update("L2") { StoredStrokes("L2", emptyList(), dirty = true) }
            assertEquals(listOf("L2"), store.dirtyLessons())
        }

    @Test
    fun `each lesson has its own file under the folder of the user, and a name cannot leave the folder`() =
        runTest {
            val store = DataStoreStrokeStore(store("st"), ManualUserScopes(a), folder.root)
            store.update("L1") { StoredStrokes("L1", listOf(record()), dirty = true) }
            store.update("../evil") { StoredStrokes("../evil", listOf(record()), dirty = true) }
            val files =
                folder.root
                    .walkTopDown()
                    .filter { it.isFile && it.extension == "json" }
                    .toList()
            assertEquals(2, files.size)
            assertTrue(files.all { it.parentFile.parentFile.name == "strokes" })
            assertEquals(listOf("../evil"), store.get("../evil")?.let { listOf(it.lessonId) })
            // No temporary file is left behind.
            assertTrue(folder.root.walkTopDown().none { it.name.endsWith(".tmp") })
        }

    @Test
    fun `a damaged file is a lesson without local notes, not a crash, and the next write replaces it`() =
        runTest {
            val store = DataStoreStrokeStore(store("st"), ManualUserScopes(a), folder.root)
            store.update("L1") { StoredStrokes("L1", listOf(record()), dirty = true) }
            store.update("L2") { StoredStrokes("L2", listOf(record(1)), dirty = true) }
            folder.root
                .walkTopDown()
                .first { it.isFile && it.extension == "json" }
                .writeText("{ not json")
            val readable = listOf("L1", "L2").filter { store.get(it) != null }
            assertEquals(1, readable.size)
            assertEquals(readable, store.dirtyLessons())
            val broken = listOf("L1", "L2").first { it !in readable }
            store.update(broken) { old ->
                assertNull(old)
                StoredStrokes(broken, listOf(record(7)), dirty = true)
            }
            assertEquals(listOf(record(7)), store.get(broken)?.items)
        }

    @Test
    fun `concurrent updates of one lesson are serialized and none is lost`() =
        runTest {
            val store = DataStoreStrokeStore(store("st"), ManualUserScopes(a), folder.root)
            store.update("L1") { StoredStrokes("L1", emptyList(), dirty = true) }
            kotlinx.coroutines.coroutineScope {
                repeat(20) { n ->
                    launch(kotlinx.coroutines.Dispatchers.IO) {
                        store.update("L1") { it!!.copy(items = it.items + record(n)) }
                    }
                }
            }
            assertEquals(20, store.get("L1")?.items?.size)
        }

    private fun corruptPoints(root: java.io.File) {
        root
            .walkTopDown()
            .filter { it.isFile && it.extension == "json" }
            .forEach { f ->
                val header = f.readLines().first()
                val stamp = f.lastModified()
                f.writeText(header + "\n{ points are gone")
                f.setLastModified(stamp)
            }
    }

    @Test
    fun `the count of unsent lessons reads the header only, never the points`() =
        runTest {
            val store = DataStoreStrokeStore(store("st"), ManualUserScopes(a), folder.root)
            store.update("L1") { StoredStrokes("L1", listOf(record()), dirty = true) }
            store.update("L2") { StoredStrokes("L2", listOf(record(1)), dirty = false) }
            corruptPoints(folder.root)
            assertEquals(listOf("L1"), store.dirtyLessons())
            // Reading the set finds it unreadable: it is no notes at all, and it stops being counted.
            assertNull(store.get("L1"))
            assertTrue(store.dirtyLessons().isEmpty())
        }

    @Test
    fun `pruning reads the header only, a clean file with unreadable points is pruned like any other`() =
        runTest {
            val store = DataStoreStrokeStore(store("st"), ManualUserScopes(a), folder.root)
            store.update("DIRTY") { StoredStrokes("DIRTY", listOf(record()), dirty = true) }
            repeat(MAX_CLEAN_STROKE_SETS) { n ->
                store.update("L$n") { StoredStrokes("L$n", listOf(record(n)), dirty = false) }
            }
            corruptPoints(folder.root)
            store.update("NEW") { StoredStrokes("NEW", listOf(record(9)), dirty = false) }
            val names =
                folder.root
                    .walkTopDown()
                    .filter { it.isFile && it.extension == "json" }
                    .map { it.name }
                    .toList()
            assertEquals(MAX_CLEAN_STROKE_SETS + 1, names.size)
            assertTrue(names.none { it.startsWith("L0.") })
            assertTrue(names.any { it.startsWith("DIRTY.") })
            assertEquals(listOf("DIRTY"), store.dirtyLessons())
        }
}
