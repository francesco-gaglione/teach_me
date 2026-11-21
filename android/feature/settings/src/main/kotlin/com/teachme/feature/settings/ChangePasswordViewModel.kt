package com.teachme.feature.settings

import androidx.compose.ui.text.input.TextFieldValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.Flash
import com.teachme.core.model.FlashInbox
import com.teachme.core.model.FlashKind
import com.teachme.core.network.AuthApi
import com.teachme.core.ui.SecretField
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

/** Which of the three fields. */
enum class PasswordField { Current, New, Repeat }

enum class PasswordActionError { Network, Server }

data class ChangePasswordUiState(
    val currentError: PasswordFieldError? = null,
    val newError: PasswordFieldError? = null,
    val repeatError: PasswordFieldError? = null,
    val visible: Set<PasswordField> = emptySet(),
    val busy: Boolean = false,
    val actionError: PasswordActionError? = null,
    /** Where the focus goes after a failed check; [focusSeq] lets the same field be asked twice. */
    val focus: PasswordField? = null,
    val focusSeq: Int = 0,
)

sealed interface ChangePasswordEffect {
    /** Done: the screen completes the autofill context and goes back. */
    data object Done : ChangePasswordEffect
}

@HiltViewModel
class ChangePasswordViewModel
    @Inject
    constructor(
        private val auth: AuthApi,
        private val inbox: FlashInbox,
    ) : ViewModel() {
        /** The three passwords: memory only, never saved state, and cleared when the screen is left. */
        val current = SecretField()
        val new = SecretField()
        val repeat = SecretField()

        private val _state = MutableStateFlow(ChangePasswordUiState())
        val state: StateFlow<ChangePasswordUiState> = _state.asStateFlow()

        private val effectChannel = Channel<ChangePasswordEffect>(Channel.BUFFERED)
        val effects: Flow<ChangePasswordEffect> = effectChannel.receiveAsFlow()

        fun onCurrentChange(value: TextFieldValue) =
            change(current, value) { it.copy(currentError = null, actionError = null) }

        fun onNewChange(value: TextFieldValue) = change(new, value) { it.copy(newError = null, actionError = null) }

        fun onRepeatChange(value: TextFieldValue) =
            change(repeat, value) { it.copy(repeatError = null, actionError = null) }

        private fun change(
            field: SecretField,
            value: TextFieldValue,
            clear: (ChangePasswordUiState) -> ChangePasswordUiState,
        ) {
            val changed = value.text != field.text
            field.onChange(value)
            if (changed) _state.update(clear)
        }

        fun toggleVisible(field: PasswordField) =
            _state.update { it.copy(visible = if (field in it.visible) it.visible - field else it.visible + field) }

        /** Back in the background: every field is masked again. */
        fun onStopped() = _state.update { it.copy(visible = emptySet()) }

        fun onCurrentFocusLost() =
            _state.update {
                it.copy(currentError = if (current.text.isEmpty()) PasswordFieldError.CurrentEmpty else null)
            }

        fun onNewFocusLost() =
            _state.update {
                it.copy(newError = if (new.text.isEmpty()) null else validateNewPassword(new.text))
            }

        fun onRepeatFocusLost() =
            _state.update {
                it.copy(
                    repeatError =
                        if (repeat.text.isNotEmpty() &&
                            repeat.text != new.text
                        ) {
                            PasswordFieldError.Mismatch
                        } else {
                            null
                        },
                )
            }

        fun submit() {
            val s = _state.value
            if (s.busy) return
            val currentError = if (current.text.isEmpty()) PasswordFieldError.CurrentEmpty else null
            val newError = validateNewPassword(new.text)
            val repeatError = if (newError == null && repeat.text != new.text) PasswordFieldError.Mismatch else null
            if (currentError != null || newError != null || repeatError != null) {
                _state.update {
                    it.copy(
                        currentError = currentError,
                        newError = newError,
                        repeatError = repeatError,
                        focus = firstOf(currentError, newError, repeatError),
                        focusSeq = it.focusSeq + 1,
                    )
                }
                return
            }
            _state.update { it.copy(busy = true, actionError = null) }
            viewModelScope.launch {
                when (val result = auth.changePassword(current.text, new.text)) {
                    is ApiResult.Success -> {
                        current.clear()
                        new.clear()
                        repeat.clear()
                        inbox.post(Flash(FlashKind.PasswordChanged))
                        _state.update { it.copy(busy = false) }
                        effectChannel.send(ChangePasswordEffect.Done)
                    }
                    is ApiResult.Failure -> onFailed(result.failure)
                }
            }
        }

        private fun firstOf(
            current: PasswordFieldError?,
            new: PasswordFieldError?,
            repeat: PasswordFieldError?,
        ): PasswordField? =
            when {
                current != null -> PasswordField.Current
                new != null -> PasswordField.New
                repeat != null -> PasswordField.Repeat
                else -> null
            }

        private fun onFailed(failure: ApiFailure) {
            val (currentError, newError) =
                (failure as? ApiFailure.Validation)?.let { serverPasswordErrors(it.details) }
                    ?: (null to null)
            if (currentError != null || newError != null) {
                if (currentError != null) current.selectAll()
                _state.update {
                    it.copy(
                        busy = false,
                        currentError = currentError,
                        newError = newError,
                        focus = firstOf(currentError, newError, null),
                        focusSeq = it.focusSeq + 1,
                    )
                }
                return
            }
            _state.update {
                it.copy(
                    busy = false,
                    actionError =
                        when (failure) {
                            is ApiFailure.Network -> PasswordActionError.Network
                            // A 401 is the global handler's business (it cannot be the wrong current password: that is 422).
                            ApiFailure.Unauthorized -> null
                            else -> PasswordActionError.Server
                        },
                )
            }
        }

        override fun onCleared() {
            current.clear()
            new.clear()
            repeat.clear()
        }
    }
