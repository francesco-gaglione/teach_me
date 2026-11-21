package com.teachme.core.data

import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import com.teachme.core.model.UserScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ReviewAlertReferenceTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val scopes = mutableListOf<CoroutineScope>()

    @After
    fun tearDown() = scopes.forEach { it.cancel() }

    /** A DataStore over the same file, as a new process would open it (the previous one is closed first). */
    private fun reference(): DataStoreReviewAlertReference {
        scopes.forEach { it.cancel() }
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO).also { scopes += it }
        return DataStoreReviewAlertReference(
            PreferenceDataStoreFactory.create(scope = scope) { folder.root.resolve("alerts.preferences_pb") },
        )
    }

    private val a = UserScope("a", "http://teach.test")
    private val b = UserScope("b", "http://teach.test")

    @Test
    fun keptPerUserAndAcrossInstances() =
        runTest {
            reference().apply {
                assertEquals(0, get(a))
                set(a, 4)
                set(b, 2)
            }
            kotlinx.coroutines.delay(100)
            assertEquals(4, reference().get(a))
            assertEquals(2, reference().get(b))
        }

    @Test
    fun zeroResetsIt() =
        runTest {
            val ref = reference()
            ref.set(a, 4)
            ref.set(a, 0)
            assertEquals(0, ref.get(a))
        }
}

class ReviewReminderPrefsTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val scopes = mutableListOf<CoroutineScope>()

    @After
    fun tearDown() = scopes.forEach { it.cancel() }

    private fun prefs(): DataStoreReviewReminderPrefs {
        scopes.forEach { it.cancel() }
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO).also { scopes += it }
        return DataStoreReviewReminderPrefs(
            PreferenceDataStoreFactory.create(scope = scope) { folder.root.resolve("reminders.preferences_pb") },
        )
    }

    @Test
    fun onByDefaultAndNotAskedYet() =
        runTest {
            val p = prefs()
            assertEquals(true, p.isEnabled())
            assertEquals(false, p.permissionAsked())
        }

    @Test
    fun switchAndAskedSurviveANewInstance() =
        runTest {
            prefs().apply {
                setEnabled(false)
                markPermissionAsked()
            }
            kotlinx.coroutines.delay(100)
            val again = prefs()
            assertEquals(false, again.isEnabled())
            assertEquals(true, again.permissionAsked())
        }
}
