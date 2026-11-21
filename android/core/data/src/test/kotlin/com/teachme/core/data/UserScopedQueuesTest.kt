package com.teachme.core.data

import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import com.teachme.core.model.ManualUserScopes
import com.teachme.core.model.QueuedAttempt
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.UserScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** What a user left in a queue is theirs: read, counted and sent under their own session only (ADR 0008). */
internal class UserScopedQueuesTest : DataStoreTest() {
    private val a = UserScope("user-a", "http://teach.test")
    private val b = UserScope("user-b", "http://teach.test")
    private val aElsewhere = UserScope("user-a", "http://other.test")

    private fun attempt(
        lesson: String = "L1",
        block: String = "q1",
    ) = QueuedAttempt(lesson, block, "quiz_single", 1, buildJsonObject { put("option", "b") }, "correct", 1L)

    private fun review(card: String = "c1") = QueuedReview(card, "good", 1L, 0)

    /** The list as the single-user versions wrote it: one JSON array under an unscoped key. */
    private fun legacyReviews(vararg cards: String) =
        kotlinx.serialization.json.Json.encodeToString(
            kotlinx.serialization.builtins.ListSerializer(QueuedReview.serializer()),
            cards.map { review(it) },
        )

    @Test
    fun `the attempts of A are not seen, counted or removed under B`() =
        runTest {
            val scopes = ManualUserScopes(a)
            val queue = DataStoreAttemptQueue(store("queue"), scopes)
            queue.enqueue(attempt(block = "q1"))
            queue.enqueue(attempt(block = "q2"))

            scopes.switchTo(b)
            assertTrue(queue.pending("L1").isEmpty())
            assertTrue(queue.lessonsWithPending().isEmpty())
            assertEquals(0, queue.pendingTotal())
            assertEquals(0, queue.count("L1").first())
            queue.enqueue(attempt(block = "q9"))
            queue.clear("L1")

            scopes.switchTo(a)
            assertEquals(listOf("q1", "q2"), queue.pending("L1").map { it.blockId })
            assertEquals(2, queue.pendingTotal())
        }

    @Test
    fun `the grades of A stay with A, also across a restart`() =
        runTest {
            val scopes = ManualUserScopes(a)
            DataStoreReviewQueue(store("reviews"), scopes).enqueue(review("card-of-a"))
            killProcess()

            val reopened = DataStoreReviewQueue(store("reviews"), scopes)
            assertEquals(listOf("card-of-a"), reopened.pending().map { it.cardId })
            scopes.switchTo(b)
            assertTrue(reopened.pending().isEmpty())
            assertEquals(false, reopened.remove(review("card-of-a")))
            scopes.switchTo(a)
            assertEquals(1, reopened.pending().size)
        }

    @Test
    fun `the same user on another backend is another list`() =
        runTest {
            val scopes = ManualUserScopes(a)
            val queue = DataStoreReviewQueue(store("reviews"), scopes)
            queue.enqueue(review())

            scopes.switchTo(aElsewhere)

            assertTrue(queue.pending().isEmpty())
        }

    @Test
    fun `the count follows the user who is signed in`() =
        runTest {
            val scopes = ManualUserScopes(a)
            val queue = DataStoreReviewQueue(store("reviews"), scopes)
            queue.enqueue(review("c1"))
            queue.enqueue(review("c2"))

            val seen = mutableListOf<Int>()
            val job =
                kotlinx.coroutines
                    .CoroutineScope(
                        kotlinx.coroutines.Dispatchers.Unconfined,
                    ).launchCollect(queue.count(), seen)
            scopes.switchTo(b)
            queue.enqueue(review("only-b"))
            // The collector is on another thread's schedule: wait for the last value.
            kotlinx.coroutines.withTimeout(5_000) { while (seen.lastOrNull() != 1) kotlinx.coroutines.delay(10) }
            job.cancel()

            assertEquals(2, seen.first())
            assertEquals(1, seen.last())
        }

    @Test
    fun `nobody signed in means an empty list and writes that go nowhere`() =
        runTest {
            val scopes = ManualUserScopes(null)
            val queue = DataStoreReviewQueue(store("reviews"), scopes)

            queue.enqueue(review())

            assertTrue(queue.pending().isEmpty())
            scopes.switchTo(a)
            assertTrue(queue.pending().isEmpty())
        }

    @Test
    fun `the data of the single-user install goes to the first user only`() =
        runTest {
            val ds = store("reviews")
            ds.edit { it[stringPreferencesKey(REVIEWS_KEY)] = legacyReviews("old") }
            val scopes = ManualUserScopes(a)
            val queue = DataStoreReviewQueue(ds, scopes)

            claimLegacyList(ds, REVIEWS_KEY, a)
            // The second user who signs in finds nothing left to claim.
            claimLegacyList(ds, REVIEWS_KEY, b)

            assertEquals(listOf("old"), queue.pending().map { it.cardId })
            scopes.switchTo(b)
            assertTrue(queue.pending().isEmpty())
            assertNull(ds.data.first()[stringPreferencesKey(REVIEWS_KEY)])
        }

    @Test
    fun `claiming merges with what the user already has`() =
        runTest {
            val ds = store("reviews")
            val scopes = ManualUserScopes(a)
            val queue = DataStoreReviewQueue(ds, scopes)
            queue.enqueue(review("new"))
            ds.edit { it[stringPreferencesKey(REVIEWS_KEY)] = legacyReviews("old") }

            claimLegacyList(ds, REVIEWS_KEY, a)

            assertEquals(setOf("old", "new"), queue.pending().map { it.cardId }.toSet())
        }

    @Test
    fun `claiming with nothing to claim changes nothing`() =
        runTest {
            val ds = store("queue")
            val scopes = ManualUserScopes(a)
            val queue = DataStoreAttemptQueue(ds, scopes)
            queue.enqueue(attempt())

            claimLegacyList(ds, ATTEMPTS_KEY, a)

            assertEquals(1, queue.pendingTotal())
        }
}

private fun kotlinx.coroutines.CoroutineScope.launchCollect(
    flow: kotlinx.coroutines.flow.Flow<Int>,
    into: MutableList<Int>,
) = flow.onEach { into += it }.launchIn(this)

internal class UserScopedStoresTest : DataStoreTest() {
    private val a = UserScope("user-a", "http://teach.test")
    private val b = UserScope("user-b", "http://teach.test")

    @Test
    fun `lesson progress and downloaded lessons are per user`() =
        kotlinx.coroutines.runBlocking<Unit> {
            val ds = store("lesson_state")
            ds.edit { it[stringPreferencesKey("doc:L1")] = "legacy" }
            claimLegacyKeys(ds, a) { !it.contains(SCOPE_SEPARATOR) }
            // The second user finds nothing to claim.
            claimLegacyKeys(ds, b) { !it.contains(SCOPE_SEPARATOR) }

            val prefs =
                ds.data
                    .first()
                    .asMap()
                    .mapKeys { it.key.name }
            assertEquals(mapOf("${a.key}$SCOPE_SEPARATOR" + "doc:L1" to "legacy"), prefs)
        }

    @Test
    fun `claiming keeps string sets as sets and leaves other users keys alone`() =
        kotlinx.coroutines.runBlocking<Unit> {
            val ds = store("banner")
            val setKey =
                androidx.datastore.preferences.core
                    .stringSetPreferencesKey("dismissed_document_ids")
            val mine =
                androidx.datastore.preferences.core
                    .stringSetPreferencesKey("dismissed_document_ids|other@x")
            ds.edit {
                it[setKey] = setOf("d1", "d2")
                it[mine] = setOf("zz")
            }

            claimLegacyKeys(ds, a) { it == "dismissed_document_ids" }

            val prefs = ds.data.first()
            assertNull(prefs[setKey])
            assertEquals(setOf("zz"), prefs[mine])
            assertEquals(
                setOf("d1", "d2"),
                prefs[
                    androidx.datastore.preferences.core
                        .stringSetPreferencesKey("${a.key}|dismissed_document_ids"),
                ],
            )
        }
}
