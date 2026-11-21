package com.teachme.core.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import com.teachme.core.model.QueuedAttempt
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class AttemptQueueTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val scopes = mutableListOf<CoroutineScope>()
    private val userScopes =
        com.teachme.core.model
            .ManualUserScopes()

    @Before
    fun setUp() = Unit

    @After
    fun tearDown() = scopes.forEach { it.cancel() }

    /** A DataStore over the same file, as a new process would open it. */
    private fun store(): DataStore<Preferences> {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO).also { scopes += it }
        return PreferenceDataStoreFactory.create(scope = scope) { folder.root.resolve("queue.preferences_pb") }
    }

    private fun attempt(
        lesson: String = "L1",
        block: String = "q1",
        number: Int = 1,
        text: String = "b",
    ) = QueuedAttempt(lesson, block, "quiz_single", number, buildJsonObject { put("option", text) }, "correct", 1L)

    @Test
    fun `queued attempts survive a restart, in order`() =
        runTest {
            val first = DataStoreAttemptQueue(store(), userScopes)
            first.enqueue(attempt(block = "q1"))
            first.enqueue(attempt(block = "q2"))
            first.enqueue(attempt(lesson = "L2", block = "q9"))

            // The process dies; a new one opens the same file: what the app finds after being killed.
            scopes.forEach { it.cancel() }
            scopes.clear()
            val second = DataStoreAttemptQueue(store(), userScopes)
            assertEquals(listOf("q1", "q2"), second.pending("L1").map { it.blockId })
            assertEquals(listOf("L1", "L2"), second.lessonsWithPending())
            assertEquals(2, second.count("L1").first())
        }

    @Test
    fun `the same pair replaces the queued attempt and a different attempt number is another one`() =
        runTest {
            val queue = DataStoreAttemptQueue(store(), userScopes)
            queue.enqueue(attempt(text = "a"))
            queue.enqueue(attempt(text = "c"))
            queue.enqueue(attempt(number = 2, text = "d"))
            val pending = queue.pending("L1")
            assertEquals(2, pending.size)
            assertEquals("c", (pending[0].answer["option"] as kotlinx.serialization.json.JsonPrimitive).content)
            assertEquals(2, pending[1].attemptNumber)
        }

    @Test
    fun `removing sent attempts leaves the others and clearing empties a lesson`() =
        runTest {
            val queue = DataStoreAttemptQueue(store(), userScopes)
            val a = attempt(block = "q1")
            val b = attempt(block = "q2")
            queue.enqueue(a)
            queue.enqueue(b)
            queue.enqueue(attempt(lesson = "L2"))
            queue.remove(listOf(a))
            assertEquals(listOf("q2"), queue.pending("L1").map { it.blockId })
            queue.clear("L1")
            assertTrue(queue.pending("L1").isEmpty())
            assertEquals(listOf("L2"), queue.lessonsWithPending())
            assertEquals(0, queue.count("L1").first())
        }

    @Test
    fun `damaged stored data is dropped, not a crash`() =
        runTest {
            val ds = store()
            ds.edit { it[stringPreferencesKey("queue")] = "{{{ not json" }
            val queue = DataStoreAttemptQueue(ds, userScopes)
            assertTrue(queue.pending("L1").isEmpty())
            queue.enqueue(attempt())
            assertEquals(1, queue.pending("L1").size)
        }
}
