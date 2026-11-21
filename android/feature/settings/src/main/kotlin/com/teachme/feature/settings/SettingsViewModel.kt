package com.teachme.feature.settings

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AuthState
import com.teachme.core.model.AuthUser
import com.teachme.core.model.ConnectionResult
import com.teachme.core.model.Flash
import com.teachme.core.model.FlashInbox
import com.teachme.core.model.FlashKind
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyState
import com.teachme.core.model.NotificationPermission
import com.teachme.core.model.NotificationPermissionAsker
import com.teachme.core.model.PendingCounts
import com.teachme.core.model.PendingSync
import com.teachme.core.model.ReviewReminderGate
import com.teachme.core.model.ReviewReminderPrefs
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.SessionActions
import com.teachme.core.model.SessionRefresher
import com.teachme.core.model.SessionRepository
import com.teachme.core.model.hostOf
import com.teachme.core.network.AuthApi
import com.teachme.core.network.HealthApi
import com.teachme.core.ui.SecretField
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

/** State of the "Prova connessione" status area. */
sealed interface ConnectionUiState {
    data object Idle : ConnectionUiState

    data class Testing(
        val host: String,
    ) : ConnectionUiState

    data class Done(
        val result: ConnectionResult,
    ) : ConnectionUiState
}

/** Why saving or removing the key failed in a way that is not about the typed text (settings.md, "Errori dell'azione"). */
enum class KeyActionError { Network, Server }

enum class KeyFieldError { Empty, TooLong }

/** The "Esci" flow (settings.md): sending what is queued, then maybe asking. */
sealed interface LogoutUi {
    data object Idle : LogoutUi

    /** Trying to send the queues, at most 3 s. */
    data object Sending : LogoutUi

    /** Something is still queued: the dialog asks. */
    data class Confirm(
        val left: PendingCounts,
    ) : LogoutUi
}

sealed interface RemoveKeyUi {
    data object Closed : RemoveKeyUi

    data class Open(
        val busy: Boolean = false,
        val error: KeyActionError? = null,
    ) : RemoveKeyUi
}

data class SettingsUiState(
    val user: AuthUser? = null,
    val host: String = "",
    val keyState: LlmKeyState = LlmKeyState.Set,
    val keyWriting: Boolean = false,
    val keyVisible: Boolean = false,
    val keyBusy: Boolean = false,
    val keyFieldError: KeyFieldError? = null,
    val keyError: KeyActionError? = null,
    /** Came from a key notice: the toast after saving says the user can go back and retry. */
    val fromKeyNotice: Boolean = false,
    /** Asks the screen to scroll the key section to the top once (the deep link); [scrollSeq] makes it repeatable. */
    val scrollToKey: Boolean = false,
    val removeKey: RemoveKeyUi = RemoveKeyUi.Closed,
    val logout: LogoutUi = LogoutUi.Idle,
    val connection: ConnectionUiState = ConnectionUiState.Idle,
    /** The "Notifiche dei ripassi" switch (device only, on by default). */
    val reviewReminders: Boolean = true,
    /** The switch is on but the system does not allow notifications: the card says so and links to system settings. */
    val notificationsBlocked: Boolean = false,
    /** `suspended_count`; null while it loads or when it failed: the "Ripasso" section is then absent (suspended.md). */
    val suspendedCount: Int? = null,
) {
    val isTesting: Boolean get() = connection is ConnectionUiState.Testing
}

/** One-shot events for the screen. */
sealed interface SettingsEffect {
    data class KeySaved(
        val fromNotice: Boolean,
    ) : SettingsEffect

    data object KeyRemoved : SettingsEffect

    /** The screen puts the key field in focus (deep link, "Sostituisci"). */
    data object FocusKey : SettingsEffect

    /** The switch was turned on without the permission: the screen shows the system dialog. */
    data object RequestNotificationPermission : SettingsEffect
}

internal const val KEY_MAX_LENGTH = 500
internal const val LOGOUT_FLUSH_MS = 3_000L

/** Argument names of the deep link `settings?section=llm-key&reason=missing|rejected`. */
object SettingsArgs {
    const val SECTION = "section"
    const val REASON = "reason"
    const val SECTION_LLM_KEY = "llm-key"
}

@HiltViewModel
class SettingsViewModel
    @Inject
    constructor(
        private val sessions: SessionRepository,
        private val refresher: SessionRefresher,
        private val auth: AuthApi,
        private val healthApi: HealthApi,
        private val llmKey: LlmKeyMonitor,
        private val pending: PendingSync,
        private val actions: SessionActions,
        private val inbox: FlashInbox,
        private val reminderPrefs: ReviewReminderPrefs,
        private val reminderGate: ReviewReminderGate,
        private val notificationPermission: NotificationPermission,
        private val permissionAsker: NotificationPermissionAsker,
        private val reviews: ReviewRepository,
        savedState: SavedStateHandle,
    ) : ViewModel() {
        /** The key being typed: memory only, never saved state (settings.md). */
        val keyField = SecretField()

        private val direct = savedState.get<String>(SettingsArgs.SECTION) == SettingsArgs.SECTION_LLM_KEY

        private val _state =
            MutableStateFlow(SettingsUiState(fromKeyNotice = direct, scrollToKey = direct, keyWriting = direct))
        val state: StateFlow<SettingsUiState> = _state.asStateFlow()

        private val effectChannel = Channel<SettingsEffect>(Channel.BUFFERED)
        val effects: Flow<SettingsEffect> = effectChannel.receiveAsFlow()

        private var testJob: Job? = null

        init {
            viewModelScope.launch {
                sessions.state.collect { auth ->
                    val session = (auth as? AuthState.SignedIn)?.session ?: return@collect
                    _state.update { it.copy(user = session.user, host = hostOf(session.baseUrl)) }
                }
            }
            viewModelScope.launch {
                llmKey.status.collect { status -> _state.update { it.copy(keyState = status.state) } }
            }
            viewModelScope.launch {
                reminderPrefs.enabled.collect { on ->
                    _state.update {
                        it.copy(reviewReminders = on, notificationsBlocked = on && !notificationPermission.isGranted())
                    }
                }
            }
            if (direct) effectChannel.trySend(SettingsEffect.FocusKey)
        }

        /** Opening Settings asks `GET /me` again, silently: a key set from the server shows up (llm-key.md). */
        fun onStarted() {
            viewModelScope.launch { refresher.refresh() }
            // The user may have changed the system permission while the app was away.
            viewModelScope.launch { syncReminders() }
            loadSuspendedCount()
        }

        /**
         * `GET /reviews/suspended?limit=1` for the count. Coming back from the list asks again, so the row follows
         * the reactivations; an answer that fails removes the row (it is a shortcut, not information owed).
         */
        private fun loadSuspendedCount() {
            viewModelScope.launch {
                val count = (reviews.suspended(1, null) as? ApiResult.Success)?.value?.suspendedCount
                _state.update { it.copy(suspendedCount = count) }
            }
        }

        // region Review notifications

        /** The switch of "Notifiche dei ripassi": on schedules the hourly check, off cancels it and the notification. */
        fun onReviewRemindersChanged(on: Boolean) {
            viewModelScope.launch {
                reminderGate.setEnabled(on)
                if (on && permissionAsker.shouldAskFromSwitch()) {
                    permissionAsker.markAsked()
                    effectChannel.send(SettingsEffect.RequestNotificationPermission)
                }
                refreshBlocked()
            }
        }

        /** The system dialog closed: a grant starts the hourly check. */
        fun onNotificationPermissionResult() {
            viewModelScope.launch { syncReminders() }
        }

        private suspend fun syncReminders() {
            reminderGate.sync()
            refreshBlocked()
        }

        private suspend fun refreshBlocked() {
            val blocked = reminderPrefs.isEnabled() && !notificationPermission.isGranted()
            _state.update { it.copy(notificationsBlocked = blocked) }
        }

        // endregion

        fun onScrolledToKey() = _state.update { it.copy(scrollToKey = false) }

        // region LLM key

        fun startReplacingKey() {
            _state.update { it.copy(keyWriting = true, keyFieldError = null, keyError = null) }
            effectChannel.trySend(SettingsEffect.FocusKey)
        }

        fun cancelKeyWriting() {
            keyField.clear()
            _state.update { it.copy(keyWriting = false, keyFieldError = null, keyError = null, keyVisible = false) }
        }

        fun onKeyChange(value: androidx.compose.ui.text.input.TextFieldValue) {
            val changed = value.text != keyField.text
            keyField.onChange(value)
            if (changed) _state.update { it.copy(keyFieldError = null, keyError = null) }
        }

        fun toggleKeyVisible() = _state.update { it.copy(keyVisible = !it.keyVisible) }

        /** Back in the background the secret is masked again. */
        fun onStopped() = _state.update { it.copy(keyVisible = false) }

        fun saveKey() {
            val s = _state.value
            if (s.keyBusy) return
            val key = keyField.text.trim()
            val fieldError =
                when {
                    key.isEmpty() -> KeyFieldError.Empty
                    key.length > KEY_MAX_LENGTH -> KeyFieldError.TooLong
                    else -> null
                }
            if (fieldError != null) {
                _state.update { it.copy(keyFieldError = fieldError) }
                return
            }
            _state.update { it.copy(keyBusy = true, keyError = null, keyFieldError = null) }
            viewModelScope.launch {
                when (val result = auth.setLlmKey(key)) {
                    is ApiResult.Success -> {
                        sessions.updateUser(result.value)
                        llmKey.noteSaved(result.serverDate)
                        keyField.clear()
                        val fromNotice = _state.value.fromKeyNotice
                        _state.update { it.copy(keyBusy = false, keyWriting = false, keyVisible = false) }
                        effectChannel.send(SettingsEffect.KeySaved(fromNotice))
                    }
                    is ApiResult.Failure -> {
                        val failure = result.failure
                        _state.update {
                            when {
                                failure is ApiFailure.Validation && failure.details.any { d -> d.field == "api_key" } ->
                                    it.copy(
                                        keyBusy = false,
                                        // `api_key` is the only field the backend can complain about here.
                                        keyFieldError = KeyFieldError.TooLong,
                                    )
                                failure is ApiFailure.Network ->
                                    it.copy(
                                        keyBusy = false,
                                        keyError = KeyActionError.Network,
                                    )
                                // A 401 is the global handler's: no panel here.
                                failure == ApiFailure.Unauthorized -> it.copy(keyBusy = false)
                                else -> it.copy(keyBusy = false, keyError = KeyActionError.Server)
                            }
                        }
                    }
                }
            }
        }

        fun askRemoveKey() = _state.update { it.copy(removeKey = RemoveKeyUi.Open()) }

        fun dismissRemoveKey() {
            if ((_state.value.removeKey as? RemoveKeyUi.Open)?.busy == true) return
            _state.update { it.copy(removeKey = RemoveKeyUi.Closed) }
        }

        fun confirmRemoveKey() {
            val open = _state.value.removeKey as? RemoveKeyUi.Open ?: return
            if (open.busy) return
            _state.update { it.copy(removeKey = RemoveKeyUi.Open(busy = true)) }
            viewModelScope.launch {
                when (val result = auth.removeLlmKey()) {
                    is ApiResult.Success -> {
                        (sessions.state.value as? AuthState.SignedIn)?.session?.user?.let {
                            sessions.updateUser(it.copy(hasLlmKey = false))
                        }
                        llmKey.noteRemoved()
                        _state.update { it.copy(removeKey = RemoveKeyUi.Closed, keyWriting = true) }
                        effectChannel.send(SettingsEffect.KeyRemoved)
                    }
                    is ApiResult.Failure ->
                        _state.update {
                            it.copy(
                                removeKey =
                                    if (result.failure == ApiFailure.Unauthorized) {
                                        RemoveKeyUi.Closed
                                    } else {
                                        RemoveKeyUi.Open(
                                            error =
                                                if (result.failure is ApiFailure.Network) {
                                                    KeyActionError.Network
                                                } else {
                                                    KeyActionError.Server
                                                },
                                        )
                                    },
                            )
                        }
                }
            }
        }

        // endregion

        // region Sign out

        /** "Esci": send what is queued for at most 3 s; ask only if something is still left (settings.md). */
        fun onLogoutClicked() {
            if (_state.value.logout != LogoutUi.Idle) return
            viewModelScope.launch {
                val queued = pending.counts()
                if (queued.isEmpty) {
                    signOut()
                    return@launch
                }
                _state.update { it.copy(logout = LogoutUi.Sending) }
                val left = pending.flush(LOGOUT_FLUSH_MS)
                if (left.isEmpty) {
                    signOut()
                } else {
                    _state.update { it.copy(logout = LogoutUi.Confirm(left)) }
                }
            }
        }

        fun onLogoutDismissed() = _state.update { it.copy(logout = LogoutUi.Idle) }

        fun onLogoutConfirmed() {
            viewModelScope.launch { signOut() }
        }

        private suspend fun signOut() {
            inbox.post(Flash(FlashKind.SignedOut))
            actions.signOut()
            keyField.clear()
            _state.update { it.copy(logout = LogoutUi.Idle) }
        }

        // endregion

        // region Connection

        fun testConnection() {
            if (_state.value.isTesting) return
            val host = _state.value.host
            testJob =
                viewModelScope.launch {
                    _state.update { it.copy(connection = ConnectionUiState.Testing(host)) }
                    val result = healthApi.check()
                    _state.update {
                        // A 401 ends the session: the login opens, this screen has nothing to say.
                        it.copy(
                            connection =
                                if (result ==
                                    ConnectionResult.SessionEnded
                                ) {
                                    ConnectionUiState.Idle
                                } else {
                                    ConnectionUiState.Done(result)
                                },
                        )
                    }
                }
        }

        // endregion

        override fun onCleared() {
            testJob?.cancel()
            keyField.clear()
        }
    }
