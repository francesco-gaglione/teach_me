package com.teachme.core.model

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

private class FakePrefs(
    on: Boolean = true,
    asked: Boolean = false,
) : ReviewReminderPrefs {
    private val state = MutableStateFlow(on)
    private var askedFlag = asked
    override val enabled: Flow<Boolean> = state

    override suspend fun isEnabled() = state.value

    override suspend fun setEnabled(enabled: Boolean) {
        state.value = enabled
    }

    override suspend fun permissionAsked() = askedFlag

    override suspend fun markPermissionAsked() {
        askedFlag = true
    }
}

private class FakePermission(
    var granted: Boolean,
) : NotificationPermission {
    override fun isGranted() = granted
}

private class FakeScheduler : ReviewReminderScheduler {
    val calls = mutableListOf<String>()

    override fun schedule() {
        calls += "schedule"
    }

    override fun cancel() {
        calls += "cancel"
    }
}

class ReviewReminderGateTest {
    private val scheduler = FakeScheduler()
    private val permission = FakePermission(granted = true)
    private val prefs = FakePrefs()
    private val gate = ReviewReminderGate(prefs, permission, scheduler)

    private val signedIn = AuthState.SignedIn(Session("http://teach.test", "t", AuthUser("u", "a", "A", true)))

    @Test
    fun switchOnSchedulesAndSwitchOffCancels() =
        runTest {
            gate.setEnabled(false)
            gate.setEnabled(true)
            assertEquals(listOf("cancel", "schedule"), scheduler.calls)
            assertTrue(prefs.isEnabled())
        }

    @Test
    fun noPermissionMeansNoWork() =
        runTest {
            permission.granted = false
            gate.setEnabled(true)
            gate.onSession(signedIn)
            assertEquals(listOf("cancel", "cancel"), scheduler.calls)
        }

    @Test
    fun theWorkerRunsOnlyWithSwitchOnAndPermission() =
        runTest {
            assertTrue(gate.mayRun())
            assertTrue(scheduler.calls.isEmpty())

            permission.granted = false
            assertFalse(gate.mayRun())
            permission.granted = true
            prefs.setEnabled(false)
            assertFalse(gate.mayRun())
            assertEquals(listOf("cancel", "cancel"), scheduler.calls)
        }

    @Test
    fun signInSchedulesOnlyWithSwitchOnAndPermission() =
        runTest {
            gate.onSession(signedIn)
            prefs.setEnabled(false)
            gate.onSession(signedIn)
            assertEquals(listOf("schedule", "cancel"), scheduler.calls)
        }

    @Test
    fun signOutAndExpiredSessionCancel() =
        runTest {
            gate.onSession(AuthState.SignedOut(SignedOutReason.LoggedOut, "", null))
            gate.onSession(AuthState.SignedOut(SignedOutReason.Expired, "", null))
            gate.onSession(AuthState.Loading)
            assertEquals(listOf("cancel", "cancel"), scheduler.calls)
        }
}

class NotificationPermissionAskerTest {
    @Test
    fun switchAsksOnlyWhenPermissionIsMissing() {
        assertTrue(NotificationPermissionAsker(FakePrefs(), FakePermission(false)).shouldAskFromSwitch())
        assertFalse(NotificationPermissionAsker(FakePrefs(), FakePermission(true)).shouldAskFromSwitch())
    }

    @Test
    fun cardAsksOnceAndOnlyWithSwitchOnAndPermissionMissing() =
        runTest {
            val prefs = FakePrefs()
            val asker = NotificationPermissionAsker(prefs, FakePermission(false))
            assertTrue(asker.shouldAskFromCard())
            asker.markAsked()
            assertFalse(asker.shouldAskFromCard())

            assertFalse(NotificationPermissionAsker(FakePrefs(on = false), FakePermission(false)).shouldAskFromCard())
            assertFalse(NotificationPermissionAsker(FakePrefs(), FakePermission(true)).shouldAskFromCard())
        }

    @Test
    fun askingFromTheSwitchUsesUpTheCardAsk() =
        runTest {
            val asker = NotificationPermissionAsker(FakePrefs(), FakePermission(false))
            asker.markAsked()
            assertFalse(asker.shouldAskFromCard())
        }
}
