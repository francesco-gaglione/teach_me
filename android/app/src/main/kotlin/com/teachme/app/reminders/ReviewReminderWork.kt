package com.teachme.app.reminders

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import com.teachme.core.model.NotificationPermission
import com.teachme.core.model.NotificationPermissionAsker
import com.teachme.core.model.ReviewAlertReference
import com.teachme.core.model.ReviewNotifier
import com.teachme.core.model.ReviewReminderCheck
import com.teachme.core.model.ReviewReminderGate
import com.teachme.core.model.ReviewReminderPrefs
import com.teachme.core.model.ReviewReminderScheduler
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.SessionRepository
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import java.util.concurrent.TimeUnit
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Hourly check of the due cards (ADR 0012 point 2). Whatever happens the result is `success`: a network or server error
 * must show nothing, and a `retry` would only re-run the check with a backoff on top of an hourly schedule that already
 * tries again. A `401` ends the session through the global handler; the check itself clears the notification.
 */
@HiltWorker
class ReviewReminderWorker
    @AssistedInject
    constructor(
        @Assisted context: Context,
        @Assisted params: WorkerParameters,
        private val check: ReviewReminderCheck,
        private val gate: ReviewReminderGate,
    ) : CoroutineWorker(context, params) {
        override suspend fun doWork(): Result {
            // Switch off or permission gone since the last sync: stop, without network and without touching the reference.
            if (!gate.mayRun()) return Result.success()
            check.run()
            return Result.success()
        }
    }

@Singleton
class WorkManagerReviewReminderScheduler
    @Inject
    constructor(
        @ApplicationContext private val context: Context,
        private val notifier: ReviewNotifier,
    ) : ReviewReminderScheduler {
        override fun schedule() {
            val request =
                PeriodicWorkRequestBuilder<ReviewReminderWorker>(1, TimeUnit.HOURS)
                    .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                    .build()
            // KEEP: scheduling again at every start must not push back the next run.
            WorkManager
                .getInstance(context)
                .enqueueUniquePeriodicWork(WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }

        override fun cancel() {
            WorkManager.getInstance(context).cancelUniqueWork(WORK_NAME)
            notifier.cancel()
        }

        companion object {
            const val WORK_NAME = "review-reminder"
        }
    }

@Module
@InstallIn(SingletonComponent::class)
abstract class ReminderBindingsModule {
    @Binds
    abstract fun bindNotifier(impl: AndroidReviewNotifier): ReviewNotifier

    @Binds
    abstract fun bindScheduler(impl: WorkManagerReviewReminderScheduler): ReviewReminderScheduler

    @Binds
    abstract fun bindPermission(impl: AndroidNotificationPermission): NotificationPermission
}

@Module
@InstallIn(SingletonComponent::class)
object ReminderProvidesModule {
    @Provides
    fun provideCheck(
        sessions: SessionRepository,
        reviews: ReviewRepository,
        reference: ReviewAlertReference,
        notifier: ReviewNotifier,
    ) = ReviewReminderCheck(sessions, reviews, reference, notifier)
}

@Module
@InstallIn(SingletonComponent::class)
object ReminderGateModule {
    @Provides
    @Singleton
    fun provideGate(
        prefs: ReviewReminderPrefs,
        permission: NotificationPermission,
        scheduler: ReviewReminderScheduler,
    ) = ReviewReminderGate(prefs, permission, scheduler)

    @Provides
    @Singleton
    fun provideAsker(
        prefs: ReviewReminderPrefs,
        permission: NotificationPermission,
    ) = NotificationPermissionAsker(prefs, permission)
}
