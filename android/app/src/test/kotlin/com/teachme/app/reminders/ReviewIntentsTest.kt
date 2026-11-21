package com.teachme.app.reminders

import android.Manifest
import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Intent
import androidx.test.core.app.ApplicationProvider
import com.teachme.app.MainActivity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class ReviewIntentsTest {
    private val context: Application = ApplicationProvider.getApplicationContext()

    @Test
    fun `the notification tap opens the main screen asking for the review, on top of an open app`() {
        val pending = AndroidReviewNotifier(context).contentIntent()
        val intent = shadowOf(pending).savedIntent

        assertTrue(ReviewIntents.isOpenReview(intent))
        assertEquals(MainActivity::class.java.name, intent.component?.className)
        // An open app gets onNewIntent instead of a second copy.
        assertTrue(intent.flags and Intent.FLAG_ACTIVITY_SINGLE_TOP != 0)
        assertTrue(shadowOf(pending).isActivity)
    }

    @Test
    fun `a normal launch is not a review request`() {
        assertFalse(ReviewIntents.isOpenReview(Intent(Intent.ACTION_MAIN)))
        assertFalse(ReviewIntents.isOpenReview(null))
    }

    @Test
    fun `a pending request survives a re-creation and is served once`() {
        // Fresh start: the intent decides.
        assertTrue(ReviewIntents.initialRequest(saved = null, intentIsOpenReview = true))
        assertFalse(ReviewIntents.initialRequest(saved = null, intentIsOpenReview = false))
        // Rotation while the request is still waiting for the login: kept.
        assertTrue(ReviewIntents.initialRequest(saved = true, intentIsOpenReview = true))
        // Rotation after it was served: the same intent must not open the review again.
        assertFalse(ReviewIntents.initialRequest(saved = false, intentIsOpenReview = true))
    }

    private val manager get() = context.getSystemService(NotificationManager::class.java)

    private fun allowNotifications() {
        shadowOf(context).grantPermissions(Manifest.permission.POST_NOTIFICATIONS)
    }

    @Test
    fun `a silent update does not bring back a notification the user took away`() {
        allowNotifications()
        val notifier = AndroidReviewNotifier(context)
        assertTrue(notifier.show(3, 2, alert = true))
        assertEquals(1, manager.activeNotifications.size)

        manager.cancel(AndroidReviewNotifier.NOTIFICATION_ID)
        // Handled (the caller records the count) but nothing is posted.
        assertTrue(notifier.show(2, 2, alert = false))
        assertEquals(0, manager.activeNotifications.size)

        // A real growth alerts and posts again.
        assertTrue(notifier.show(5, 3, alert = true))
        assertEquals(1, manager.activeNotifications.size)
    }

    @Test
    fun `a silent update of a visible notification updates it in place`() {
        allowNotifications()
        val notifier = AndroidReviewNotifier(context)
        notifier.show(3, 2, alert = true)
        assertTrue(notifier.show(2, 1, alert = false))
        assertEquals(1, manager.activeNotifications.size)
    }

    @Test
    fun `notifications turned off in the system count as blocked`() {
        allowNotifications()
        assertTrue(hasNotificationPermission(context))
        shadowOf(manager).setNotificationsEnabled(false)
        assertFalse(hasNotificationPermission(context))
        assertFalse(AndroidReviewNotifier(context).show(3, 2, alert = true))
    }

    @Test
    fun `a channel turned off counts as blocked`() {
        allowNotifications()
        val notifier = AndroidReviewNotifier(context)
        notifier.show(3, 2, alert = true)
        manager.deleteNotificationChannel(AndroidReviewNotifier.CHANNEL_ID)
        manager.createNotificationChannel(
            NotificationChannel(AndroidReviewNotifier.CHANNEL_ID, "Ripassi", NotificationManager.IMPORTANCE_NONE),
        )
        assertFalse(hasNotificationPermission(context))
    }
}
