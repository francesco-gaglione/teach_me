package com.teachme.feature.login

import androidx.compose.ui.text.input.TextFieldValue
import androidx.lifecycle.viewModelScope
import app.cash.turbine.test
import com.teachme.core.model.AddressError
import com.teachme.core.model.AuthState
import com.teachme.core.model.AuthUser
import com.teachme.core.model.LoginOutcome
import com.teachme.core.model.PendingCounts
import com.teachme.core.model.RememberedUser
import com.teachme.core.model.SignedOutReason
import com.teachme.core.model.UnreachableReason
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
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
class LoginViewModelTest {
    private val user = AuthUser("u1", "valentina", "Valentina", hasLlmKey = false)
    private val remembered = RememberedUser("u1", "valentina", "Valentina")
    private val created = mutableListOf<LoginViewModel>()
    private var now = 0L

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    private fun vm(
        state: AuthState,
        auth: FakeAuth = FakeAuth(),
        sessions: FakeSessions = FakeSessions(state),
        pending: FakePending = FakePending(),
    ): LoginViewModel = LoginViewModel(sessions, auth, pending, { now }, { "Samsung SM-X710" }).also { created += it }

    private fun firstAccess() = AuthState.SignedOut(SignedOutReason.FirstRun, "", null)

    private fun returning(reason: SignedOutReason) =
        AuthState.SignedOut(reason, "https://teach.tailnet.ts.net", remembered)

    /** Runs a body and always cancels the ViewModel scope, so no timer is left running (the "slow" hint, the clock). */
    private fun test(body: suspend kotlinx.coroutines.test.TestScope.() -> Unit) =
        runTest {
            try {
                body()
            } finally {
                created.forEach { it.viewModelScope.cancel() }
            }
        }

    private fun LoginViewModel.type(
        address: String? = null,
        username: String? = null,
        password: String? = null,
    ) {
        address?.let(::onAddressChange)
        username?.let(::onUsernameChange)
        password?.let { onPasswordChange(TextFieldValue(it)) }
    }

    @Test
    fun `the first access starts empty, with the address as a field and nothing focused`() =
        test {
            val vm = vm(firstAccess())
            runCurrent()

            val s = vm.state.value
            assertTrue(s.loaded)
            assertEquals(LoginVariant.FirstAccess, s.variant)
            assertTrue(s.addressEditable)
            assertNull(s.focus)
            assertEquals("", s.username)
        }

    @Test
    fun `a returning user finds the address as a row, the name filled in and the focus on the password`() =
        test {
            val vm = vm(returning(SignedOutReason.LoggedOut))
            runCurrent()

            val s = vm.state.value
            assertEquals(LoginVariant.Return, s.variant)
            assertFalse(s.addressEditable)
            assertEquals("valentina", s.username)
            assertEquals(LoginField.Password, s.focus?.target)
        }

    @Test
    fun `the update variant puts the focus on the user name`() =
        test {
            val vm = vm(AuthState.SignedOut(SignedOutReason.Upgrade, "http://teach.test", null))
            runCurrent()

            assertEquals(LoginVariant.Update, vm.state.value.variant)
            assertEquals(
                LoginField.Username,
                vm.state.value.focus
                    ?.target,
            )
            assertFalse(vm.state.value.addressEditable)
        }

    @Test
    fun `a session that ended says how much is waiting on the device`() =
        test {
            val vm =
                vm(returning(SignedOutReason.Expired), pending = FakePending(PendingCounts(reviews = 3, answers = 2)))
            runCurrent()

            assertEquals(LoginVariant.SessionEnded, vm.state.value.variant)
            assertEquals(PendingCounts(3, 2), vm.state.value.pending)
        }

    @Test
    fun `checks run before the network and the first mistake takes the focus`() =
        test {
            val auth = FakeAuth()
            val vm = vm(firstAccess(), auth)
            runCurrent()

            vm.submit()
            runCurrent()
            assertEquals(AddressError.Empty, vm.state.value.addressError)
            assertEquals(
                LoginField.Address,
                vm.state.value.focus
                    ?.target,
            )

            vm.type(address = "teach.local", username = "")
            vm.submit()
            assertEquals(AddressError.InvalidScheme, vm.state.value.addressError)

            vm.type(address = "https://teach.local")
            vm.submit()
            assertTrue(vm.state.value.usernameError)
            assertEquals(
                LoginField.Username,
                vm.state.value.focus
                    ?.target,
            )

            vm.type(username = "valentina")
            vm.submit()
            assertTrue(vm.state.value.passwordError)
            assertEquals(
                LoginField.Password,
                vm.state.value.focus
                    ?.target,
            )
            assertTrue(auth.logins.isEmpty())
        }

    @Test
    fun `a good login sends the trimmed name and the password as typed, signs in, and forgets the password`() =
        test {
            val auth = FakeAuth().apply { outcome = { LoginOutcome.Success("https://teach.local", "tm_t", user) } }
            val sessions = FakeSessions(firstAccess())
            val vm = vm(firstAccess(), auth, sessions)
            runCurrent()
            vm.type("https://teach.local/", "  Valentina  ", " a password with spaces ")

            vm.submit()
            runCurrent()

            val call = auth.logins.single()
            assertEquals("https://teach.local", call.baseUrl)
            assertEquals("Valentina", call.username)
            // Spaces are part of the password: never trimmed.
            assertEquals(" a password with spaces ", call.password)
            assertEquals("Samsung SM-X710", call.deviceName)
            assertEquals(Triple("https://teach.local", "tm_t", user), sessions.signIns.single())
            assertEquals("", vm.password.text)
        }

    @Test
    fun `the form stays busy until the session is written, so a second tap cannot log in twice`() =
        test {
            val gate = CompletableDeferred<Unit>()
            val auth = FakeAuth().apply { outcome = { LoginOutcome.Success("https://teach.local", "tm_t", user) } }
            val sessions = FakeSessions(firstAccess()).apply { signInGate = { gate.await() } }
            val vm = vm(firstAccess(), auth, sessions)
            runCurrent()
            vm.type("https://teach.local", "valentina", "pw")

            vm.submit()
            runCurrent()
            // The login answered, the write is still going on.
            assertTrue(vm.state.value.submitting)
            vm.submit()
            runCurrent()
            assertEquals(1, auth.logins.size)

            vm.effects.test {
                gate.complete(Unit)
                runCurrent()
                // The effect (autofill commit) comes only once the session is saved.
                assertEquals(LoginEffect.Succeeded, awaitItem())
                assertEquals(1, sessions.signIns.size)
            }
            assertFalse(vm.state.value.submitting)
        }

    @Test
    fun `a failure writing the session shows the unexpected panel instead of crashing, and no success`() =
        test {
            val auth = FakeAuth().apply { outcome = { LoginOutcome.Success("https://teach.local", "tm_t", user) } }
            val sessions = FakeSessions(firstAccess()).apply { signInFails = true }
            val vm = vm(firstAccess(), auth, sessions)
            runCurrent()
            vm.type("https://teach.local", "valentina", "pw")

            vm.effects.test {
                vm.submit()
                runCurrent()
                expectNoEvents()
                cancelAndIgnoreRemainingEvents()
            }

            assertEquals(LoginPanel.Unexpected(null, null), vm.state.value.panel)
            assertFalse(vm.state.value.submitting)
            // Nothing was saved, so the typed password is still there to try again.
            assertEquals("pw", vm.password.text)
        }

    @Test
    fun `while the request runs the button is busy and a second tap does nothing`() =
        test {
            val gate = CompletableDeferred<LoginOutcome>()
            val auth = FakeAuth().apply { outcome = { gate.await() } }
            val vm = vm(returning(SignedOutReason.LoggedOut), auth)
            runCurrent()
            vm.type(password = "pw")

            vm.submit()
            runCurrent()
            vm.submit()
            runCurrent()

            assertTrue(vm.state.value.submitting)
            assertEquals(1, auth.logins.size)
            advanceTimeBy(SLOW_HINT_MS + 1)
            assertTrue(vm.state.value.slow)
            gate.complete(LoginOutcome.InvalidCredentials)
            runCurrent()
            assertFalse(vm.state.value.submitting)
            assertFalse(vm.state.value.slow)
        }

    @Test
    fun `wrong credentials keep everything typed, select the password, and say more from the third try`() =
        test {
            val auth = FakeAuth()
            val vm = vm(returning(SignedOutReason.LoggedOut), auth)
            runCurrent()
            vm.type(password = "wrong-password")

            vm.submit()
            runCurrent()

            val first = vm.state.value
            assertEquals(LoginPanel.InvalidCredentials(repeated = false), first.panel)
            assertEquals("valentina", first.username)
            assertEquals("wrong-password", vm.password.text)
            assertEquals(14, vm.password.value.selection.end - vm.password.value.selection.start)
            assertEquals(LoginField.Password, first.focus?.target)
            assertTrue(first.focus!!.selectAll)

            repeat(2) {
                vm.submit()
                runCurrent()
            }
            assertEquals(LoginPanel.InvalidCredentials(repeated = true), vm.state.value.panel)
        }

    @Test
    fun `typing closes the panel`() =
        test {
            val vm = vm(returning(SignedOutReason.LoggedOut))
            runCurrent()
            vm.type(password = "x")
            vm.submit()
            runCurrent()
            assertTrue(vm.state.value.panel != null)

            vm.type(password = "xy")

            assertNull(vm.state.value.panel)
        }

    @Test
    fun `too many attempts block the button for that name until the time is up`() =
        test {
            val auth = FakeAuth().apply { outcome = { LoginOutcome.RateLimited(840) } }
            val vm = vm(returning(SignedOutReason.LoggedOut), auth)
            runCurrent()
            vm.type(password = "pw")
            now = 1_000

            vm.submit()
            runCurrent()

            val panel = vm.state.value.panel as LoginPanel.RateLimited
            assertEquals("valentina", panel.username)
            assertEquals(1_000L + 840_000L, panel.untilMs)
            assertTrue(panel.waitKnown)
            assertTrue(vm.state.value.blocked)

            // Tapping again, or typing in the password, changes nothing while the block lasts.
            vm.submit()
            runCurrent()
            assertEquals(1, auth.logins.size)
            vm.type(password = "pw2")
            assertTrue(vm.state.value.blocked)

            vm.onClockTick(now + 100_000)
            assertTrue(vm.state.value.blocked)
            val lifted = vm.state.value.unblockedSeq
            vm.onClockTick(panel.untilMs)
            assertFalse(vm.state.value.blocked)
            assertNull(vm.state.value.panel)
            assertEquals(lifted + 1, vm.state.value.unblockedSeq)
            // The password was not emptied.
            assertEquals("pw2", vm.password.text)
        }

    @Test
    fun `another user name is a new try`() =
        test {
            val auth = FakeAuth().apply { outcome = { LoginOutcome.RateLimited(60) } }
            val vm = vm(returning(SignedOutReason.LoggedOut), auth)
            runCurrent()
            vm.type(password = "pw")
            vm.submit()
            runCurrent()
            assertTrue(vm.state.value.blocked)

            vm.onUsernameChange("anna")

            assertFalse(vm.state.value.blocked)
            assertNull(vm.state.value.panel)
        }

    @Test
    fun `without retry-after the block is a minute`() =
        test {
            val auth = FakeAuth().apply { outcome = { LoginOutcome.RateLimited(null) } }
            val vm = vm(returning(SignedOutReason.LoggedOut), auth)
            runCurrent()
            vm.type(password = "pw")

            vm.submit()
            runCurrent()

            val panel = vm.state.value.panel as LoginPanel.RateLimited
            assertFalse(panel.waitKnown)
            assertEquals(DEFAULT_BLOCK_MS, panel.untilMs - now)
        }

    @Test
    fun `an unreachable backend turns the address row into a field with the focus`() =
        test {
            val auth = FakeAuth().apply { outcome = { LoginOutcome.Unreachable(UnreachableReason.Timeout) } }
            val vm = vm(returning(SignedOutReason.LoggedOut), auth)
            runCurrent()
            vm.type(password = "pw")
            assertFalse(vm.state.value.addressEditable)

            vm.submit()
            runCurrent()

            val s = vm.state.value
            assertEquals(LoginPanel.Unreachable(UnreachableReason.Timeout), s.panel)
            assertTrue(s.addressEditable)
            assertEquals(LoginField.Address, s.focus?.target)
        }

    @Test
    fun `an address that is not a backend does the same, other trouble leaves the row alone`() =
        test {
            val auth = FakeAuth().apply { outcome = { LoginOutcome.NotRecognized } }
            val vm = vm(returning(SignedOutReason.LoggedOut), auth)
            runCurrent()
            vm.type(password = "pw")
            vm.submit()
            runCurrent()
            assertEquals(LoginPanel.NotRecognized, vm.state.value.panel)
            assertTrue(vm.state.value.addressEditable)

            val calm =
                vm(returning(SignedOutReason.LoggedOut), FakeAuth().apply { outcome = { LoginOutcome.NotReady } })
            runCurrent()
            calm.type(password = "pw")
            calm.submit()
            runCurrent()
            assertEquals(LoginPanel.NotReady, calm.state.value.panel)
            assertFalse(calm.state.value.addressEditable)

            val odd =
                vm(
                    returning(SignedOutReason.LoggedOut),
                    FakeAuth().apply {
                        outcome =
                            { LoginOutcome.Unexpected(500, "r1") }
                    },
                )
            runCurrent()
            odd.type(password = "pw")
            odd.submit()
            runCurrent()
            assertEquals(LoginPanel.Unexpected(500, "r1"), odd.state.value.panel)
        }

    @Test
    fun `change makes the address a field that stays, and the password is hidden again when the app stops`() =
        test {
            val vm = vm(returning(SignedOutReason.LoggedOut))
            runCurrent()

            vm.onChangeAddress()
            assertTrue(vm.state.value.addressEditable)
            assertEquals(
                LoginField.Address,
                vm.state.value.focus
                    ?.target,
            )
            assertTrue(
                vm.state.value.focus!!
                    .selectAll,
            )

            vm.togglePasswordVisible()
            assertTrue(vm.state.value.passwordVisible)
            vm.onStopped()
            assertFalse(vm.state.value.passwordVisible)
        }

    @Test
    fun `the password is not part of any state that could be logged or saved`() =
        test {
            val vm = vm(returning(SignedOutReason.LoggedOut))
            vm.type(password = "very-secret")

            assertFalse(vm.password.toString().contains("very-secret"))
            assertFalse(
                vm.state.value
                    .toString()
                    .contains("very-secret"),
            )
            vm.password.clear()
            assertEquals("", vm.password.text)
        }

    @Test
    fun `wait times round up the way the screen shows them`() {
        assertEquals(Wait(WaitUnit.Seconds, 1), waitOf(1))
        assertEquals(Wait(WaitUnit.Seconds, 59), waitOf(59))
        assertEquals(Wait(WaitUnit.Minutes, 1), waitOf(60))
        assertEquals(Wait(WaitUnit.Minutes, 2), waitOf(61))
        assertEquals(Wait(WaitUnit.Minutes, 14), waitOf(840))
        assertEquals(Wait(WaitUnit.Minutes, 60), waitOf(3_599))
        assertEquals(Wait(WaitUnit.Hours, 1), waitOf(3_600))
        assertEquals(Wait(WaitUnit.Hours, 2), waitOf(3_601))
        assertEquals(1_000L, tickMillis(59))
        assertEquals(10_000L, tickMillis(60))
    }
}
