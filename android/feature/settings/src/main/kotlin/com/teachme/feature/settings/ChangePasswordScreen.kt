package com.teachme.feature.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalAutofillManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentType
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.tooling.preview.Preview
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonSize
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachScreen
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachTextField
import com.teachme.core.designsystem.components.TeachTopBar
import com.teachme.core.ui.SecretField

/** The change-password sub-screen (settings.md): a screen, not a dialog, because of the three fields and the keyboard. */
@Composable
fun ChangePasswordRoute(
    onBack: () -> Unit,
    viewModel: ChangePasswordViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val autofill = LocalAutofillManager.current
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { viewModel.onStopped() }
    LaunchedEffect(viewModel) {
        viewModel.effects.collect {
            // The password manager can now offer to update the saved password.
            autofill?.commit()
            onBack()
        }
    }
    ChangePasswordScreen(
        state = state,
        fields = PasswordFields(viewModel.current, viewModel.new, viewModel.repeat),
        actions =
            PasswordActions(
                onBack = onBack,
                onCurrentChange = viewModel::onCurrentChange,
                onNewChange = viewModel::onNewChange,
                onRepeatChange = viewModel::onRepeatChange,
                onCurrentFocusLost = viewModel::onCurrentFocusLost,
                onNewFocusLost = viewModel::onNewFocusLost,
                onRepeatFocusLost = viewModel::onRepeatFocusLost,
                onToggle = viewModel::toggleVisible,
                onSubmit = viewModel::submit,
            ),
    )
}

class PasswordFields(
    val current: SecretField,
    val new: SecretField,
    val repeat: SecretField,
)

data class PasswordActions(
    val onBack: () -> Unit = {},
    val onCurrentChange: (TextFieldValue) -> Unit = {},
    val onNewChange: (TextFieldValue) -> Unit = {},
    val onRepeatChange: (TextFieldValue) -> Unit = {},
    val onCurrentFocusLost: () -> Unit = {},
    val onNewFocusLost: () -> Unit = {},
    val onRepeatFocusLost: () -> Unit = {},
    val onToggle: (PasswordField) -> Unit = {},
    val onSubmit: () -> Unit = {},
)

@Composable
fun ChangePasswordScreen(
    state: ChangePasswordUiState,
    fields: PasswordFields,
    actions: PasswordActions,
) {
    TeachScreen(
        topBar = {
            TeachTopBar(
                title = stringResource(R.string.password_title),
                navigation = {
                    TeachIconButton(
                        icon = TeachIcons.ChevronLeft,
                        contentDescription = stringResource(R.string.settings_back),
                        onClick = actions.onBack,
                        tint = TeachTheme.colors.ink,
                    )
                },
            )
        },
    ) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            Form(state, fields, actions)
        }
    }
}

@Composable
private fun Form(
    state: ChangePasswordUiState,
    fields: PasswordFields,
    actions: PasswordActions,
) {
    val layout = TeachTheme.layout
    val focus = remember { PasswordField.entries.associateWith { FocusRequester() } }
    LaunchedEffect(state.focusSeq) {
        state.focus?.let { runCatching { focus.getValue(it).requestFocus() } }
    }
    Column(
        modifier =
            Modifier
                .widthIn(max = layout.dialogWidthWide)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .imePadding()
                .padding(layout.screenMarginCompact),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        // Always visible and first: read before deciding, not after the tap (settings.md).
        TeachStatusPanel(
            StatusVariant.Info,
            stringResource(R.string.password_notice_title),
            body = stringResource(R.string.password_notice_body),
        )
        PasswordInput(
            fields.current,
            PasswordField.Current,
            state,
            R.string.password_current_label,
            ContentType.Password,
            ImeAction.Next,
            actions,
            focus.getValue(PasswordField.Current),
            actions.onCurrentChange,
            actions.onCurrentFocusLost,
            error = state.currentError,
            onNext = { focus.getValue(PasswordField.New).requestFocus() },
        )
        PasswordInput(
            fields.new,
            PasswordField.New,
            state,
            R.string.password_new_label,
            ContentType.NewPassword,
            ImeAction.Next,
            actions,
            focus.getValue(PasswordField.New),
            actions.onNewChange,
            actions.onNewFocusLost,
            error = state.newError,
            onNext = { focus.getValue(PasswordField.Repeat).requestFocus() },
            hint = stringResource(R.string.password_new_hint),
        )
        PasswordInput(
            fields.repeat,
            PasswordField.Repeat,
            state,
            R.string.password_repeat_label,
            ContentType.NewPassword,
            ImeAction.Done,
            actions,
            focus.getValue(PasswordField.Repeat),
            actions.onRepeatChange,
            actions.onRepeatFocusLost,
            error = state.repeatError,
        )
        state.actionError?.let { ActionPanel(it) }
        TeachButton(
            stringResource(if (state.busy) R.string.password_button_busy else R.string.password_button),
            actions.onSubmit,
            Modifier.fillMaxWidth(),
            size = ButtonSize.Large,
            loading = state.busy,
        )
    }
}

@Composable
private fun PasswordInput(
    field: SecretField,
    kind: PasswordField,
    state: ChangePasswordUiState,
    labelRes: Int,
    contentType: ContentType,
    ime: ImeAction,
    actions: PasswordActions,
    focus: FocusRequester,
    onChange: (TextFieldValue) -> Unit,
    onFocusLost: () -> Unit,
    error: PasswordFieldError?,
    hint: String? = null,
    onNext: () -> Unit = {},
) {
    val visible = kind in state.visible
    TeachTextField(
        value = field.value,
        onValueChange = { if (!state.busy) onChange(it) },
        label = stringResource(labelRes),
        supportingText = hint,
        errorText = error?.let { passwordErrorText(it) },
        visualTransformation = if (visible) VisualTransformation.None else PasswordVisualTransformation(),
        keyboardOptions =
            KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                keyboardType = KeyboardType.Password,
                imeAction = ime,
            ),
        // "Avanti" goes to the next field, not to the show/hide toggle that sits between them in the focus order.
        keyboardActions = KeyboardActions(onNext = { onNext() }, onDone = { actions.onSubmit() }),
        trailing = {
            TeachIconButton(
                icon = if (visible) TeachIcons.EyeOff else TeachIcons.Eye,
                contentDescription = stringResource(if (visible) R.string.password_hide else R.string.password_show),
                onClick = { actions.onToggle(kind) },
                toggled = visible,
            )
        },
        onFocusLost = onFocusLost,
        // Credentials: autofill on (the manager can propose a generated password for the new ones).
        fieldModifier = Modifier.focusRequester(focus).semantics { this.contentType = contentType },
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun passwordErrorText(error: PasswordFieldError): String =
    when (error) {
        PasswordFieldError.CurrentEmpty -> stringResource(R.string.password_error_current_empty)
        PasswordFieldError.CurrentWrong -> stringResource(R.string.password_error_current_wrong)
        PasswordFieldError.NewEmpty -> stringResource(R.string.password_error_new_empty)
        is PasswordFieldError.TooShort ->
            if (error.length != null) {
                stringResource(R.string.password_error_too_short_now, error.length)
            } else {
                stringResource(R.string.password_error_too_short)
            }
        PasswordFieldError.TooLong -> stringResource(R.string.password_error_too_long)
        PasswordFieldError.Mismatch -> stringResource(R.string.password_error_mismatch)
    }

@Composable
private fun ActionPanel(error: PasswordActionError) {
    if (error == PasswordActionError.Network) {
        TeachStatusPanel(
            StatusVariant.Warning,
            stringResource(R.string.password_action_network_title),
            body = stringResource(R.string.password_action_network_body),
            icon = TeachIcons.CloudOff,
        )
    } else {
        TeachStatusPanel(
            StatusVariant.Danger,
            stringResource(R.string.password_action_server_title),
            body = stringResource(R.string.password_action_server_body),
        )
    }
}

@Composable
private fun PreviewHost(
    state: ChangePasswordUiState,
    dark: Boolean = false,
) {
    TeachTheme(darkTheme = dark) {
        Box(Modifier.background(TeachTheme.colors.bg)) {
            val fields =
                remember {
                    PasswordFields(
                        SecretField().also { it.set("vecchia-password") },
                        SecretField().also { it.set("corta") },
                        SecretField().also { it.set("diversa") },
                    )
                }
            ChangePasswordScreen(state, fields, PasswordActions())
        }
    }
}

@Preview(name = "Empty", showBackground = true)
@Composable
private fun PreviewEmpty() = PreviewHost(ChangePasswordUiState())

@Preview(name = "Field errors", showBackground = true)
@Composable
private fun PreviewErrors() =
    PreviewHost(
        ChangePasswordUiState(
            currentError = PasswordFieldError.CurrentWrong,
            newError = PasswordFieldError.TooShort(5),
            repeatError = PasswordFieldError.Mismatch,
        ),
    )

@Preview(name = "Busy dark", showBackground = true)
@Composable
private fun PreviewBusyDark() = PreviewHost(ChangePasswordUiState(busy = true), dark = true)

@Preview(name = "Network error", showBackground = true)
@Composable
private fun PreviewNetwork() = PreviewHost(ChangePasswordUiState(actionError = PasswordActionError.Network))

@Preview(name = "Font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun PreviewLargeFont() = PreviewHost(ChangePasswordUiState(newError = PasswordFieldError.TooLong))
