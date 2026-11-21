package com.teachme.feature.settings

import androidx.compose.ui.text.input.TextFieldValue
import androidx.lifecycle.viewModelScope
import app.cash.turbine.test
import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.FlashKind
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
class ChangePasswordViewModelTest {
    private val auth = FakeAuth()
    private val inbox = FakeInbox()
    private val created = mutableListOf<ChangePasswordViewModel>()

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun vm() = ChangePasswordViewModel(auth, inbox).also { created += it }

    private fun test(body: suspend TestScope.() -> Unit) =
        runTest {
            try {
                body()
            } finally {
                created.forEach { it.viewModelScope.cancel() }
            }
        }

    private fun ChangePasswordViewModel.fill(
        old: String = "old-password",
        new: String = "a brand new passphrase",
        again: String = new,
    ) {
        onCurrentChange(TextFieldValue(old))
        onNewChange(TextFieldValue(new))
        onRepeatChange(TextFieldValue(again))
    }

    @Test
    fun `local checks come first and the first mistake takes the focus`() =
        test {
            val vm = vm()

            vm.submit()
            assertEquals(PasswordFieldError.CurrentEmpty, vm.state.value.currentError)
            assertEquals(PasswordFieldError.NewEmpty, vm.state.value.newError)
            assertEquals(PasswordField.Current, vm.state.value.focus)

            vm.fill(new = "short")
            vm.submit()
            assertEquals(PasswordFieldError.TooShort(5), vm.state.value.newError)
            assertEquals(PasswordField.New, vm.state.value.focus)

            vm.fill(new = "x".repeat(129))
            vm.submit()
            assertEquals(PasswordFieldError.TooLong, vm.state.value.newError)

            vm.fill(new = "a brand new passphrase", again = "a brand new passphrasE")
            vm.submit()
            assertEquals(PasswordFieldError.Mismatch, vm.state.value.repeatError)
            assertEquals(PasswordField.Repeat, vm.state.value.focus)
            assertTrue(auth.sentPasswords.isEmpty())
        }

    @Test
    fun `lengths count characters, not bytes or code units`() {
        // Ten emoji are ten characters (twenty UTF-16 units): long enough. Nine are not.
        assertNull(validateNewPassword("😀".repeat(10)))
        assertEquals(PasswordFieldError.TooShort(9), validateNewPassword("😀".repeat(9)))
        assertNull(validateNewPassword("x".repeat(128)))
        assertEquals(PasswordFieldError.TooLong, validateNewPassword("x".repeat(129)))
        assertNull(validateNewPassword("          "))
    }

    @Test
    fun `a good change sends both, empties the three fields and goes back`() =
        test {
            val vm = vm()
            vm.fill()

            vm.effects.test {
                vm.submit()
                runCurrent()
                assertEquals(ChangePasswordEffect.Done, awaitItem())
            }

            assertEquals("old-password" to "a brand new passphrase", auth.sentPasswords.single())
            assertEquals("", vm.current.text + vm.new.text + vm.repeat.text)
            assertEquals(FlashKind.PasswordChanged, inbox.posted.single().kind)
        }

    @Test
    fun `the wrong current password is a field error with the text selected, the others stay`() =
        test {
            auth.password = { _, _ ->
                ApiResult.Failure(ApiFailure.Validation(listOf(ApiErrorDetail("current_password", "wrong"))))
            }
            val vm = vm()
            vm.fill()

            vm.submit()
            runCurrent()

            assertEquals(PasswordFieldError.CurrentWrong, vm.state.value.currentError)
            assertEquals(PasswordField.Current, vm.state.value.focus)
            assertEquals("old-password", vm.current.text)
            assertEquals("a brand new passphrase", vm.new.text)
            assertEquals("old-password".length, vm.current.value.selection.end - vm.current.value.selection.start)
            assertFalse(vm.state.value.busy)
        }

    @Test
    fun `the server can refuse the new password too`() =
        test {
            auth.password = { _, _ ->
                ApiResult.Failure(ApiFailure.Validation(listOf(ApiErrorDetail("new_password", "too_short"))))
            }
            val vm = vm()
            vm.fill()

            vm.submit()
            runCurrent()

            assertEquals(PasswordFieldError.TooShort(null), vm.state.value.newError)
            assertEquals(PasswordField.New, vm.state.value.focus)
        }

    @Test
    fun `network and server errors keep the typed values and say what happened`() =
        test {
            val vm = vm()
            vm.fill()

            auth.password = { _, _ -> ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout)) }
            vm.submit()
            runCurrent()
            assertEquals(PasswordActionError.Network, vm.state.value.actionError)

            auth.password = { _, _ -> ApiResult.Failure(ApiFailure.Unexpected(500)) }
            vm.submit()
            runCurrent()
            assertEquals(PasswordActionError.Server, vm.state.value.actionError)

            auth.password = { _, _ -> ApiResult.Failure(ApiFailure.Unauthorized) }
            vm.submit()
            runCurrent()
            assertNull(vm.state.value.actionError)
            assertEquals("old-password", vm.current.text)
        }

    @Test
    fun `each field has its own show toggle, and they hide again when the app stops`() =
        test {
            val vm = vm()

            vm.toggleVisible(PasswordField.New)
            assertEquals(setOf(PasswordField.New), vm.state.value.visible)
            vm.toggleVisible(PasswordField.Current)
            vm.toggleVisible(PasswordField.New)
            assertEquals(setOf(PasswordField.Current), vm.state.value.visible)
            vm.onStopped()
            assertTrue(
                vm.state.value.visible
                    .isEmpty(),
            )
        }

    @Test
    fun `the secrets do not show in any printable state`() =
        test {
            val vm = vm()
            vm.fill(old = "hunter2-hunter2", new = "correct horse battery")

            assertFalse(
                vm.state.value
                    .toString()
                    .contains("hunter2"),
            )
            assertFalse(vm.current.toString().contains("hunter2"))
            assertFalse(vm.new.toString().contains("horse"))
        }
}
