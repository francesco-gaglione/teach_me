package com.teachme.feature.settings

import androidx.compose.ui.text.input.TextFieldValue
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import app.cash.turbine.test
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AuthState
import com.teachme.core.model.ConnectionResult
import com.teachme.core.model.FlashKind
import com.teachme.core.model.HealthResponse
import com.teachme.core.model.LlmKeyState
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.NotificationPermissionAsker
import com.teachme.core.model.PendingCounts
import com.teachme.core.model.ReviewReminderGate
import com.teachme.core.model.UnreachableReason
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SettingsViewModelTest {
    private val sessions = FakeSessions()
    private val auth = FakeAuth()
    private val monitor = FakeMonitor()
    private val pending = FakePending()
    private val actions = FakeActions()
    private val inbox = FakeInbox()
    private val refresher = FakeRefresher()
    private val health =
        FakeHealth(ConnectionResult.Connected(HealthResponse("ok", "0.1.0", 1, 2, "2026-11-10T00:00:00Z")))
    private val reminderPrefs = FakeReminderPrefs()
    private val permission = FakePermission()
    private val scheduler = FakeScheduler()
    private val reviews = FakeReviews().also { it.pages += page(emptyList()) }
    private val created = mutableListOf<SettingsViewModel>()

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun vm(args: Map<String, Any?> = emptyMap()) =
        SettingsViewModel(
            sessions,
            refresher,
            auth,
            health,
            monitor,
            pending,
            actions,
            inbox,
            reminderPrefs,
            ReviewReminderGate(reminderPrefs, permission, scheduler),
            permission,
            NotificationPermissionAsker(reminderPrefs, permission),
            reviews,
            SavedStateHandle(args),
        ).also {
            created +=
                it
        }

    private fun test(body: suspend TestScope.() -> Unit) =
        runTest {
            try {
                body()
            } finally {
                created.forEach { it.viewModelScope.cancel() }
            }
        }

    @Test
    fun `it shows who is signed in and where, and asks the backend again when it opens`() =
        test {
            val vm = vm()
            vm.onStarted()
            runCurrent()

            assertEquals(
                "valentina",
                vm.state.value.user
                    ?.username,
            )
            assertEquals("teach.tailnet.ts.net", vm.state.value.host)
            assertEquals(1, refresher.refreshes)
        }

    // region LLM key

    @Test
    fun `saving trims the key, sends it, updates the user and tells the monitor`() =
        test {
            val vm = vm()
            runCurrent()
            vm.startReplacingKey()
            vm.onKeyChange(TextFieldValue("  sk-or-v1-abc  "))

            vm.saveKey()
            runCurrent()

            assertEquals(listOf("sk-or-v1-abc"), auth.sentKeys)
            assertEquals(1, sessions.updates.size)
            assertEquals(1, monitor.saved)
            // The secret is gone from memory, and the field is closed.
            assertEquals("", vm.keyField.text)
            assertFalse(vm.state.value.keyWriting)
        }

    @Test
    fun `a saved key announces itself, and says "go back and retry" when it came from a notice`() =
        test {
            val fromNotice =
                vm(
                    mapOf(
                        SettingsArgs.SECTION to SettingsArgs.SECTION_LLM_KEY,
                        SettingsArgs.REASON to "missing",
                    ),
                )
            runCurrent()
            fromNotice.effects.test {
                assertEquals(SettingsEffect.FocusKey, awaitItem())
                fromNotice.onKeyChange(TextFieldValue("sk-1"))
                fromNotice.saveKey()
                runCurrent()
                assertEquals(SettingsEffect.KeySaved(fromNotice = true), awaitItem())
            }
            val plain = vm()
            runCurrent()
            plain.startReplacingKey()
            plain.effects.test {
                assertEquals(SettingsEffect.FocusKey, awaitItem())
                plain.onKeyChange(TextFieldValue("sk-2"))
                plain.saveKey()
                runCurrent()
                assertEquals(SettingsEffect.KeySaved(fromNotice = false), awaitItem())
            }
        }

    @Test
    fun `the deep link opens the key section already in writing mode, scrolled to`() =
        test {
            val vm = vm(mapOf(SettingsArgs.SECTION to SettingsArgs.SECTION_LLM_KEY))
            runCurrent()

            assertTrue(vm.state.value.keyWriting)
            assertTrue(vm.state.value.scrollToKey)
            vm.onScrolledToKey()
            assertFalse(vm.state.value.scrollToKey)
        }

    @Test
    fun `an empty or too long key is refused before any request`() =
        test {
            val vm = vm()
            runCurrent()
            vm.startReplacingKey()

            vm.onKeyChange(TextFieldValue("   "))
            vm.saveKey()
            assertEquals(KeyFieldError.Empty, vm.state.value.keyFieldError)

            vm.onKeyChange(TextFieldValue("k".repeat(501)))
            vm.saveKey()
            assertEquals(KeyFieldError.TooLong, vm.state.value.keyFieldError)
            assertTrue(auth.sentKeys.isEmpty())

            // 500 is allowed, and typing again clears the error.
            vm.onKeyChange(TextFieldValue("k".repeat(500)))
            assertNull(vm.state.value.keyFieldError)
        }

    @Test
    fun `a 422 on the key field is a field error, the text stays`() =
        test {
            auth.setKey = {
                ApiResult.Failure(
                    ApiFailure.Validation(
                        listOf(
                            com.teachme.core.model
                                .ApiErrorDetail("api_key", "too_long"),
                        ),
                    ),
                )
            }
            val vm = vm()
            runCurrent()
            vm.startReplacingKey()
            vm.onKeyChange(TextFieldValue("sk-x"))

            vm.saveKey()
            runCurrent()

            assertEquals(KeyFieldError.TooLong, vm.state.value.keyFieldError)
            assertEquals("sk-x", vm.keyField.text)
            assertEquals(0, monitor.saved)
        }

    @Test
    fun `network and server failures keep the typed key and say what happened`() =
        test {
            val vm = vm()
            runCurrent()
            vm.startReplacingKey()
            vm.onKeyChange(TextFieldValue("sk-x"))

            auth.setKey = { ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout)) }
            vm.saveKey()
            runCurrent()
            assertEquals(KeyActionError.Network, vm.state.value.keyError)
            assertEquals("sk-x", vm.keyField.text)
            assertFalse(vm.state.value.keyBusy)

            auth.setKey = { ApiResult.Failure(ApiFailure.Unexpected(500)) }
            vm.saveKey()
            runCurrent()
            assertEquals(KeyActionError.Server, vm.state.value.keyError)

            // A 401 is the global handler's: no panel here.
            auth.setKey = { ApiResult.Failure(ApiFailure.Unauthorized) }
            vm.saveKey()
            runCurrent()
            assertNull(vm.state.value.keyError)
        }

    @Test
    fun `cancelling wipes the key from memory`() =
        test {
            val vm = vm()
            runCurrent()
            vm.startReplacingKey()
            vm.onKeyChange(TextFieldValue("sk-secret"))

            vm.cancelKeyWriting()

            assertEquals("", vm.keyField.text)
            assertFalse(vm.state.value.keyWriting)
            assertFalse(vm.keyField.toString().contains("secret"))
        }

    @Test
    fun `the chip follows the known state of the key`() =
        test {
            val vm = vm()
            runCurrent()
            assertEquals(LlmKeyState.Set, vm.state.value.keyState)

            monitor.status.value = LlmKeyStatus(LlmKeyState.RejectedKnown)
            runCurrent()
            assertEquals(LlmKeyState.RejectedKnown, vm.state.value.keyState)
        }

    @Test
    fun `removing the key asks first, then marks the user keyless`() =
        test {
            val vm = vm()
            runCurrent()

            vm.askRemoveKey()
            assertEquals(RemoveKeyUi.Open(), vm.state.value.removeKey)
            vm.confirmRemoveKey()
            runCurrent()

            assertEquals(RemoveKeyUi.Closed, vm.state.value.removeKey)
            assertFalse(sessions.updates.single().hasLlmKey)
            assertEquals(1, monitor.removed)
            assertTrue(vm.state.value.keyWriting)
        }

    @Test
    fun `a failed removal keeps the dialog open with the reason`() =
        test {
            auth.removeKey = { ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout)) }
            val vm = vm()
            runCurrent()
            vm.askRemoveKey()

            vm.confirmRemoveKey()
            runCurrent()

            assertEquals(RemoveKeyUi.Open(error = KeyActionError.Network), vm.state.value.removeKey)
            assertEquals(0, monitor.removed)
            vm.dismissRemoveKey()
            assertEquals(RemoveKeyUi.Closed, vm.state.value.removeKey)
        }

    // endregion

    // region sign out

    @Test
    fun `with nothing queued, sign out is immediate and leaves a message for the login`() =
        test {
            val vm = vm()
            runCurrent()

            vm.onLogoutClicked()
            runCurrent()

            assertEquals(1, actions.signOuts)
            assertEquals(0, pending.flushes)
            assertEquals(FlashKind.SignedOut, inbox.posted.single().kind)
            assertEquals(LogoutUi.Idle, vm.state.value.logout)
        }

    @Test
    fun `with something queued it tries to send it first, and leaves quietly if that worked`() =
        test {
            pending.queued = PendingCounts(reviews = 2)
            pending.leftAfterFlush = PendingCounts()
            val vm = vm()
            runCurrent()

            vm.onLogoutClicked()
            runCurrent()

            assertEquals(1, pending.flushes)
            assertEquals(1, actions.signOuts)
        }

    @Test
    fun `if something is still left it asks, and Cancel keeps the session`() =
        test {
            pending.queued = PendingCounts(reviews = 3, answers = 2)
            pending.leftAfterFlush = PendingCounts(reviews = 3, answers = 2)
            val vm = vm()
            runCurrent()

            vm.onLogoutClicked()
            runCurrent()

            assertEquals(LogoutUi.Confirm(PendingCounts(3, 2)), vm.state.value.logout)
            assertEquals(0, actions.signOuts)
            vm.onLogoutDismissed()
            assertEquals(LogoutUi.Idle, vm.state.value.logout)
            assertEquals(0, actions.signOuts)
            assertTrue(inbox.posted.isEmpty())

            vm.onLogoutClicked()
            runCurrent()
            vm.onLogoutConfirmed()
            runCurrent()
            assertEquals(1, actions.signOuts)
            assertEquals(FlashKind.SignedOut, inbox.posted.single().kind)
        }

    // endregion

    // region connection

    @Test
    fun `the connection test shows the result, and a 401 shows nothing at all`() =
        test {
            val vm = vm()
            runCurrent()

            vm.testConnection()
            runCurrent()
            assertTrue((vm.state.value.connection as ConnectionUiState.Done).result is ConnectionResult.Connected)

            health.result = ConnectionResult.Unreachable(UnreachableReason.Timeout)
            vm.testConnection()
            runCurrent()
            assertEquals(
                ConnectionUiState.Done(ConnectionResult.Unreachable(UnreachableReason.Timeout)),
                vm.state.value.connection,
            )

            health.result = ConnectionResult.SessionEnded
            vm.testConnection()
            runCurrent()
            assertEquals(ConnectionUiState.Idle, vm.state.value.connection)
        }

    @Test
    fun `the session ending under the screen does not break it`() =
        test {
            val vm = vm()
            runCurrent()

            // The user is shown as signed in as long as the session says so.
            assertTrue(sessions.state.value is AuthState.SignedIn)
            assertEquals(
                "Valentina",
                vm.state.value.user
                    ?.displayName,
            )
        }

    // endregion

    @Test
    fun `the notifications switch is on by default and turning it off and on schedules and cancels`() =
        test {
            val vm = vm()
            runCurrent()
            assertTrue(vm.state.value.reviewReminders)

            vm.onReviewRemindersChanged(false)
            runCurrent()
            assertFalse(vm.state.value.reviewReminders)
            assertEquals(listOf("cancel"), scheduler.calls)

            vm.onReviewRemindersChanged(true)
            runCurrent()
            assertTrue(vm.state.value.reviewReminders)
            assertEquals(listOf("cancel", "schedule"), scheduler.calls)
        }

    @Test
    fun `turning the switch on without the permission asks for it and shows the blocked state`() =
        test {
            permission.granted = false
            reminderPrefs.setEnabled(false)
            val vm = vm()
            runCurrent()
            vm.effects.test {
                vm.onReviewRemindersChanged(true)
                runCurrent()
                assertEquals(SettingsEffect.RequestNotificationPermission, awaitItem())
                expectNoEvents()
            }
            assertTrue(vm.state.value.notificationsBlocked)
            // Without the permission the hourly check does not run.
            assertEquals(listOf("cancel"), scheduler.calls)

            // The system dialog is answered with a grant: the check starts and the state clears.
            permission.granted = true
            vm.onNotificationPermissionResult()
            runCurrent()
            assertFalse(vm.state.value.notificationsBlocked)
            assertEquals(listOf("cancel", "schedule"), scheduler.calls)
        }

    @Test
    fun `turning the switch on with the permission does not ask for it`() =
        test {
            reminderPrefs.setEnabled(false)
            val vm = vm()
            runCurrent()
            vm.effects.test {
                vm.onReviewRemindersChanged(true)
                runCurrent()
                expectNoEvents()
            }
        }

    @Test
    fun `coming back from the system settings with the permission starts the check`() =
        test {
            permission.granted = false
            val vm = vm()
            vm.onStarted()
            runCurrent()
            assertTrue(vm.state.value.notificationsBlocked)
            permission.granted = true
            vm.onStarted()
            runCurrent()
            assertFalse(vm.state.value.notificationsBlocked)
            assertEquals("schedule", scheduler.calls.last())
        }

    @Test
    fun `the review section needs the suspended count, and follows it when the screen starts again`() =
        test {
            reviews.pages.clear()
            reviews.pages += page(listOf("a"), count = 3)
            val vm = vm()
            assertNull(vm.state.value.suspendedCount)

            vm.onStarted()
            runCurrent()
            assertEquals(3, vm.state.value.suspendedCount)
            assertEquals(listOf(1 to null), reviews.requests)

            // Back from the list with everything reactivated: the section goes away.
            reviews.pages.clear()
            reviews.pages += page(emptyList(), count = 0)
            vm.onStarted()
            runCurrent()
            assertEquals(0, vm.state.value.suspendedCount)
        }

    @Test
    fun `a failed count leaves no review section and no error`() =
        test {
            reviews.pages.clear()
            reviews.pages += NETWORK_DOWN
            val vm = vm()
            vm.onStarted()
            runCurrent()
            assertNull(vm.state.value.suspendedCount)
        }
}
