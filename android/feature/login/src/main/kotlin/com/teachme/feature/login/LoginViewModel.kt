package com.teachme.feature.login

import android.os.Build
import android.os.SystemClock
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.AddressError
import com.teachme.core.model.AuthState
import com.teachme.core.model.LoginOutcome
import com.teachme.core.model.PendingCounts
import com.teachme.core.model.PendingSync
import com.teachme.core.model.SessionRepository
import com.teachme.core.model.normalizedBaseUrl
import com.teachme.core.model.validateAddress
import com.teachme.core.network.AuthApi
import com.teachme.core.ui.SecretField
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

/** A clock that does not jump with the wall time: the "Troppi tentativi" wait is measured on it. */
fun interface ElapsedClock {
    fun elapsedMs(): Long
}

/** `Samsung SM-X710`: what identifies this session in the server's list (login.md). */
fun interface DeviceNameProvider {
    fun deviceName(): String?
}

@Module
@InstallIn(SingletonComponent::class)
object LoginProvidesModule {
    @Provides
    fun provideElapsedClock(): ElapsedClock = ElapsedClock { SystemClock.elapsedRealtime() }

    @Provides
    fun provideDeviceName(): DeviceNameProvider =
        DeviceNameProvider {
            val maker = Build.MANUFACTURER.orEmpty().replaceFirstChar { it.uppercase() }
            "$maker ${Build.MODEL.orEmpty()}".trim().take(MAX_DEVICE_NAME).ifBlank { null }
        }
}

private const val MAX_DEVICE_NAME = 60

/** Where the keyboard focus goes next; [seq] makes the same target requestable twice. */
data class FocusRequest(
    val target: LoginField,
    val seq: Int,
    val selectAll: Boolean = false,
)

enum class LoginField { Address, Username, Password }

data class LoginUiState(
    val loaded: Boolean = false,
    val variant: LoginVariant = LoginVariant.FirstAccess,
    /** The saved address, shown as a read-only row until the user asks to change it. */
    val savedAddress: String = "",
    val addressEditable: Boolean = true,
    val address: String = "",
    val username: String = "",
    /** Bumped when the ViewModel itself replaces the typed texts (loading the remembered values). */
    val fieldsVersion: Int = 0,
    val addressError: AddressError? = null,
    val usernameError: Boolean = false,
    val passwordError: Boolean = false,
    val passwordVisible: Boolean = false,
    val submitting: Boolean = false,
    val slow: Boolean = false,
    val panel: LoginPanel? = null,
    val pending: PendingCounts = PendingCounts(),
    val focus: FocusRequest? = null,
    /** Raised when a rate-limit block runs out, for the one "you can try again" announcement. */
    val unblockedSeq: Int = 0,
) {
    /** "Accedi" is off only while a rate-limit block on this user name lasts. */
    val blocked: Boolean get() = (panel as? LoginPanel.RateLimited)?.username == username.trim().lowercase()
    val remembered: Boolean get() = variant == LoginVariant.Return || variant == LoginVariant.SessionEnded
}

sealed interface LoginEffect {
    /** Credentials accepted: the screen asks the system to offer saving them (autofill). */
    data object Succeeded : LoginEffect
}

@HiltViewModel
class LoginViewModel
    @Inject
    constructor(
        private val sessions: SessionRepository,
        private val auth: AuthApi,
        private val pendingSync: PendingSync,
        private val clock: ElapsedClock,
        private val device: DeviceNameProvider,
    ) : ViewModel() {
        /** The password: memory only (login.md), never saved state. */
        val password = SecretField()

        private val _state = MutableStateFlow(LoginUiState())
        val state: StateFlow<LoginUiState> = _state.asStateFlow()

        private val effectChannel = Channel<LoginEffect>(Channel.BUFFERED)
        val effects: Flow<LoginEffect> = effectChannel.receiveAsFlow()

        private var failures = 0
        private var focusSeq = 0
        private var submitJob: Job? = null

        init {
            viewModelScope.launch {
                val auth = sessions.awaitReady()
                if (auth is AuthState.SignedOut) load(auth)
            }
        }

        private suspend fun load(out: AuthState.SignedOut) {
            val variant = LoginVariant.of(out.reason)
            val hasAddress = out.baseUrl.isNotBlank() && validateAddress(out.baseUrl) == null
            val pending = if (variant == LoginVariant.SessionEnded) pendingSync.counts() else PendingCounts()
            _state.update {
                it.copy(
                    loaded = true,
                    variant = variant,
                    savedAddress = out.baseUrl,
                    addressEditable = !hasAddress,
                    address = out.baseUrl,
                    username = out.user?.username.orEmpty(),
                    fieldsVersion = it.fieldsVersion + 1,
                    pending = pending,
                    // The password is what a returning user has to type; a first access starts with nothing focused.
                    focus =
                        when (variant) {
                            LoginVariant.Return, LoginVariant.SessionEnded -> request(LoginField.Password)
                            LoginVariant.Update -> request(LoginField.Username)
                            LoginVariant.FirstAccess -> null
                        },
                )
            }
        }

        private fun request(
            field: LoginField,
            selectAll: Boolean = false,
        ) = FocusRequest(field, ++focusSeq, selectAll)

        fun onAddressChange(value: String) = edit { it.copy(address = value, addressError = null) }

        fun onUsernameChange(value: String) {
            edit { it.copy(username = value, usernameError = false) }
            // The block is for one user name: another name is a new try.
            _state.update { s ->
                val panel = s.panel
                if (panel is LoginPanel.RateLimited &&
                    panel.username != value.trim().lowercase()
                ) {
                    s.copy(panel = null)
                } else {
                    s
                }
            }
        }

        fun onPasswordChange(value: androidx.compose.ui.text.input.TextFieldValue) {
            val changed = value.text != password.text
            password.onChange(value)
            if (changed) edit { it.copy(passwordError = false) }
        }

        fun togglePasswordVisible() = _state.update { it.copy(passwordVisible = !it.passwordVisible) }

        /** The app went to the background: the password is hidden again (login.md). */
        fun onStopped() = _state.update { it.copy(passwordVisible = false) }

        fun onChangeAddress() =
            _state.update { it.copy(addressEditable = true, focus = request(LoginField.Address, selectAll = true)) }

        fun onAddressFocusLost() = _state.update { it.copy(addressError = validateAddress(it.address)) }

        /** Any edit closes the panel, except the rate-limit one (it ends with the time, or with another user name). */
        private fun edit(transform: (LoginUiState) -> LoginUiState) =
            _state.update {
                val next = transform(it)
                if (next.panel is LoginPanel.RateLimited) next else next.copy(panel = null)
            }

        fun submit() {
            val s = _state.value
            if (s.submitting || !s.loaded) return
            if (s.blocked) return
            val username = s.username.trim()
            val address = if (s.addressEditable) s.address else s.savedAddress
            val addressError = if (s.addressEditable) validateAddress(address) else null
            val usernameError = username.isEmpty()
            val passwordError = password.text.isEmpty()
            if (addressError != null || usernameError || passwordError) {
                _state.update {
                    it.copy(
                        addressError = addressError,
                        usernameError = usernameError,
                        passwordError = passwordError,
                        focus =
                            request(
                                when {
                                    addressError != null -> LoginField.Address
                                    usernameError -> LoginField.Username
                                    else -> LoginField.Password
                                },
                            ),
                    )
                }
                return
            }
            _state.update { it.copy(submitting = true, slow = false, panel = null) }
            val slowJob = viewModelScope.launch { delay(SLOW_HINT_MS).also { _state.update { it.copy(slow = true) } } }
            submitJob =
                viewModelScope.launch {
                    try {
                        val outcome =
                            auth.login(
                                address.normalizedBaseUrl(),
                                username,
                                password.text,
                                device.deviceName(),
                            )
                        slowJob.cancel()
                        // Busy until the session is written: a second tap meanwhile must not log in twice.
                        handle(outcome, username)
                    } finally {
                        slowJob.cancel()
                        _state.update { it.copy(submitting = false, slow = false) }
                    }
                }
        }

        private suspend fun handle(
            outcome: LoginOutcome,
            username: String,
        ) {
            when (outcome) {
                is LoginOutcome.Success -> {
                    try {
                        sessions.signIn(outcome.baseUrl, outcome.token, outcome.user)
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        // The credentials were right but the session could not be written (disk full...): say so, no crash.
                        _state.update { it.copy(panel = LoginPanel.Unexpected(httpStatus = null, requestId = null)) }
                        return
                    }
                    failures = 0
                    // Only now the session is saved: the system may offer to remember the credentials.
                    effectChannel.send(LoginEffect.Succeeded)
                    password.clear()
                }
                LoginOutcome.InvalidCredentials -> {
                    failures++
                    password.selectAll()
                    _state.update {
                        it.copy(
                            panel = LoginPanel.InvalidCredentials(repeated = failures >= REPEATED_FAILURES),
                            focus = request(LoginField.Password, selectAll = true),
                        )
                    }
                }
                is LoginOutcome.RateLimited -> {
                    val waitMs = outcome.retryAfterSecs?.let { it * 1_000L } ?: DEFAULT_BLOCK_MS
                    _state.update {
                        it.copy(
                            panel =
                                LoginPanel.RateLimited(
                                    username = username.lowercase(),
                                    untilMs = clock.elapsedMs() + waitMs,
                                    waitKnown = outcome.retryAfterSecs != null,
                                ),
                        )
                    }
                }
                is LoginOutcome.Unreachable ->
                    _state.update {
                        it.copy(
                            panel = LoginPanel.Unreachable(outcome.reason),
                            addressEditable = true,
                            focus = request(LoginField.Address),
                        )
                    }
                LoginOutcome.NotReady -> _state.update { it.copy(panel = LoginPanel.NotReady) }
                LoginOutcome.NotRecognized ->
                    _state.update {
                        it.copy(
                            panel = LoginPanel.NotRecognized,
                            addressEditable = true,
                            focus = request(LoginField.Address),
                        )
                    }
                is LoginOutcome.Unexpected ->
                    _state.update { it.copy(panel = LoginPanel.Unexpected(outcome.httpStatus, outcome.requestId)) }
            }
        }

        /** Called by the screen on its clock: lifts the rate-limit block when its time is up. */
        fun onClockTick(nowMs: Long) {
            _state.update {
                val panel = it.panel
                if (panel is LoginPanel.RateLimited && nowMs >= panel.untilMs) {
                    it.copy(panel = null, unblockedSeq = it.unblockedSeq + 1)
                } else {
                    it
                }
            }
        }

        fun elapsedNow(): Long = clock.elapsedMs()

        override fun onCleared() {
            submitJob?.cancel()
            password.clear()
        }
    }
