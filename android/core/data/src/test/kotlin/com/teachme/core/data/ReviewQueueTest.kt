package com.teachme.core.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import com.teachme.core.model.QueuedReview
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ReviewQueueTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val scopes = mutableListOf<CoroutineScope>()
    private val userScopes =
        com.teachme.core.model
            .ManualUserScopes()

    @After
    fun tearDown() = scopes.forEach { it.cancel() }

    /** A DataStore over the same file, as a new process would open it. */
    private fun store(): DataStore<Preferences> {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO).also { scopes += it }
        return PreferenceDataStoreFactory.create(scope = scope) { folder.root.resolve("reviews.preferences_pb") }
    }

    private fun review(
        card: String = "c1",
        count: Int = 0,
        rating: String = "good",
        at: Long = 1L,
    ) = QueuedReview(card, rating, at, count)

    @Test
    fun `queued grades survive a restart, in order`() =
        runTest {
            DataStoreReviewQueue(store(), userScopes).apply {
                enqueue(review("c1"))
                enqueue(review("c2"))
                enqueue(review("c3"))
            }
            // The process dies; a new one opens the same file: what the app finds after being killed.
            scopes.forEach { it.cancel() }
            scopes.clear()
            val second = DataStoreReviewQueue(store(), userScopes)
            assertEquals(listOf("c1", "c2", "c3"), second.pending().map { it.cardId })
            assertEquals(3, second.count().first())
        }

    @Test
    fun `the same card version replaces the grade and the next version is another one`() =
        runTest {
            val queue = DataStoreReviewQueue(store(), userScopes)
            queue.enqueue(review(rating = "hard", count = 2))
            queue.enqueue(review(rating = "easy", count = 2))
            queue.enqueue(review(rating = "good", count = 3))
            assertEquals(listOf("easy", "good"), queue.pending().map { it.rating })
        }

    @Test
    fun `remove tells whether the grade was still queued`() =
        runTest {
            val queue = DataStoreReviewQueue(store(), userScopes)
            val a = review("c1")
            queue.enqueue(a)
            assertTrue(queue.remove(a))
            assertFalse(queue.remove(a))
            assertEquals(0, queue.count().first())
        }

    @Test
    fun `damaged stored data is dropped, not a crash`() =
        runTest {
            val ds = store()
            ds.edit { it[stringPreferencesKey("reviews")] = "{{{ not json" }
            val queue = DataStoreReviewQueue(ds, userScopes)
            assertTrue(queue.pending().isEmpty())
            queue.enqueue(review())
            assertEquals(1, queue.pending().size)
        }
}
