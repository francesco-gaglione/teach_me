package com.teachme.core.model

import kotlinx.coroutines.flow.Flow

/** Starts and stops the hourly check. [cancel] also takes the notification away. Implemented in `:app`. */
interface ReviewReminderScheduler {
    fun schedule()

    fun cancel()
}

/** Whether the app may post notifications (always true before Android 13). Implemented in `:app`. */
interface NotificationPermission {
    fun isGranted(): Boolean
}

/** Device-only preferences of the review notifications (ADR 0012 points 3 and 6). Implemented in `:core:data`. */
interface ReviewReminderPrefs {
    /** The "Notifiche dei ripassi" switch: on by default, not tied to a user. */
    val enabled: Flow<Boolean>

    suspend fun isEnabled(): Boolean

    suspend fun setEnabled(enabled: Boolean)

    /** True once the app has asked for the notification permission, from the switch or from the Home card. */
    suspend fun permissionAsked(): Boolean

    suspend fun markPermissionAsked()
}

/**
 * The one place that decides whether the hourly check runs: only with a valid session, the switch on and the permission
 * granted (ADR 0012 points 6 and 7).
 */
class ReviewReminderGate(
    private val prefs: ReviewReminderPrefs,
    private val permission: NotificationPermission,
    private val scheduler: ReviewReminderScheduler,
) {
    /** Re-reads switch and permission (they can change outside the app) and schedules or cancels accordingly. */
    suspend fun sync() {
        if (prefs.isEnabled() && permission.isGranted()) scheduler.schedule() else scheduler.cancel()
    }

    /**
     * For the worker, before any network: true when the check may run. With the switch off or the permission gone it
     * cancels work and notification and returns false.
     */
    suspend fun mayRun(): Boolean {
        if (prefs.isEnabled() && permission.isGranted()) return true
        scheduler.cancel()
        return false
    }

    /** Applies the session: a signed-in user gets [sync], anything else (sign-out, `401`) cancels work and notification. */
    suspend fun onSession(state: AuthState) {
        when (state) {
            is AuthState.SignedIn -> sync()
            is AuthState.SignedOut -> scheduler.cancel()
            AuthState.Loading -> Unit
        }
    }

    suspend fun setEnabled(enabled: Boolean) {
        prefs.setEnabled(enabled)
        sync()
    }
}

/** When the app may ask for `POST_NOTIFICATIONS`: from the switch, and once from the Home card; never at start. */
class NotificationPermissionAsker(
    private val prefs: ReviewReminderPrefs,
    private val permission: NotificationPermission,
) {
    /** Turning the switch on without the permission always asks. */
    fun shouldAskFromSwitch(): Boolean = !permission.isGranted()

    /** The first time the Home card shows due cards: only if the switch is on, the permission is missing, and never asked. */
    suspend fun shouldAskFromCard(): Boolean = prefs.isEnabled() && !permission.isGranted() && !prefs.permissionAsked()

    suspend fun markAsked() = prefs.markPermissionAsked()
}
