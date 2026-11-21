package com.teachme.app.reminders

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.teachme.app.MainActivity
import com.teachme.app.R
import com.teachme.core.model.NotificationPermission
import com.teachme.core.model.ReviewNotifier
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The single "Ripassi" notification (ADR 0012 points 4 and 8): fixed id, updated in place. Updates that must not make
 * noise use `setOnlyAlertOnce`, so only a notification that is new (or posted with [alert]) can sound or vibrate.
 */
@Singleton
class AndroidReviewNotifier
    @Inject
    constructor(
        @ApplicationContext private val context: Context,
    ) : ReviewNotifier {
        override fun show(
            dueCount: Int,
            estimatedMinutes: Int,
            alert: Boolean,
        ): Boolean {
            // Blocked (permission, app switch or channel): nothing to show, and the caller must not use up the growth.
            if (!canPost()) return false
            ensureChannel()
            val manager = NotificationManagerCompat.from(context)
            // `setOnlyAlertOnce` only works while the notification is on screen. A silent update of one the user dismissed
            // or tapped would come back with sound every hour: leave it away (the caller still records the count).
            if (!alert && !isActive()) return true
            val resources = context.resources
            val notification =
                NotificationCompat
                    .Builder(context, CHANNEL_ID)
                    .setSmallIcon(R.drawable.ic_notification_review)
                    .setContentTitle(
                        resources.getQuantityString(R.plurals.review_notification_title, dueCount, dueCount),
                    ).apply {
                        if (estimatedMinutes > 0) {
                            setContentText(
                                resources.getQuantityString(
                                    R.plurals.review_notification_minutes,
                                    estimatedMinutes,
                                    estimatedMinutes,
                                ),
                            )
                        }
                    }.setCategory(NotificationCompat.CATEGORY_REMINDER)
                    .setPriority(NotificationCompat.PRIORITY_DEFAULT)
                    .setAutoCancel(true)
                    // A silent update of a notification already on screen: no sound, no vibration, no heads-up.
                    .setOnlyAlertOnce(!alert)
                    .setSilent(!alert)
                    .setContentIntent(contentIntent())
                    .build()
            @Suppress("MissingPermission") // checked by canPost()
            manager.notify(NOTIFICATION_ID, notification)
            return true
        }

        private fun isActive(): Boolean =
            context
                .getSystemService(NotificationManager::class.java)
                .activeNotifications
                .any { it.id == NOTIFICATION_ID }

        override fun cancel() {
            NotificationManagerCompat.from(context).cancel(NOTIFICATION_ID)
        }

        private fun canPost(): Boolean = hasNotificationPermission(context)

        private fun ensureChannel() {
            val channel =
                NotificationChannel(
                    CHANNEL_ID,
                    context.getString(R.string.review_channel_name),
                    NotificationManager.IMPORTANCE_DEFAULT,
                ).apply { description = context.getString(R.string.review_channel_description) }
            context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }

        /** The tap opens the review session (ADR 0012 point 5), also when the app is already open. */
        internal fun contentIntent(): PendingIntent =
            PendingIntent.getActivity(
                context,
                0,
                ReviewIntents.openReview(context),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )

        companion object {
            const val CHANNEL_ID = "reviews"
            const val NOTIFICATION_ID = 7001
        }
    }

/**
 * True when "Ripassi" can really be posted: the runtime permission (Android 13+), the app-level switch of the system
 * settings and a channel that is not turned off.
 */
internal fun hasNotificationPermission(context: Context): Boolean {
    val runtime =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
    if (!runtime || !NotificationManagerCompat.from(context).areNotificationsEnabled()) return false
    // The channel does not exist until the first post: then there is nothing the user could have turned off.
    val channel = NotificationManagerCompat.from(context).getNotificationChannel(AndroidReviewNotifier.CHANNEL_ID)
    return channel == null || channel.importance != NotificationManagerCompat.IMPORTANCE_NONE
}

@Singleton
class AndroidNotificationPermission
    @Inject
    constructor(
        @ApplicationContext private val context: Context,
    ) : NotificationPermission {
        override fun isGranted(): Boolean = hasNotificationPermission(context)
    }

/** The intent behind the notification tap, and how [MainActivity] recognizes it. */
object ReviewIntents {
    const val ACTION_OPEN_REVIEW = "com.teachme.app.action.OPEN_REVIEW"

    /**
     * SINGLE_TOP + CLEAR_TOP: an open app gets `onNewIntent` instead of a second copy, a closed one starts and reads it.
     */
    fun openReview(context: Context): Intent =
        Intent(context, MainActivity::class.java)
            .setAction(ACTION_OPEN_REVIEW)
            .addFlags(
                Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP,
            )

    const val STATE_OPEN_REVIEW = "open_review_pending"

    /** The pending request at activity creation: the saved one after a re-creation, the intent's on a fresh start. */
    fun initialRequest(
        saved: Boolean?,
        intentIsOpenReview: Boolean,
    ): Boolean = saved ?: intentIsOpenReview

    fun isOpenReview(intent: Intent?): Boolean = intent?.action == ACTION_OPEN_REVIEW
}
