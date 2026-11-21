package com.teachme.feature.login

import android.app.Activity
import android.content.ContextWrapper
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalAutofillManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.contentType
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextRange
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
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ButtonSize
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachScreen
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachTextField
import com.teachme.core.designsystem.components.TeachToastHost
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.components.rememberTextFieldValue
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.model.AddressError
import com.teachme.core.model.FlashInbox
import com.teachme.core.model.FlashKind
import com.teachme.core.model.PendingCounts
import com.teachme.core.model.UnreachableReason
import com.teachme.core.model.hostOf
import com.teachme.core.ui.SecretField
import com.teachme.core.ui.pendingPhrase
import kotlinx.coroutines.delay
import kotlin.math.ceil

/**
 * The login screen (login.md). Opens as the first destination without a session, or over the stack when a session ends.
 * Navigation after a successful login is driven by the session state in the host, not by this screen.
 */
@Composable
fun LoginRoute(
    inbox: FlashInbox,
    viewModel: LoginViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val toast = remember { TeachToastState() }
    val autofill = LocalAutofillManager.current
    val signedOutText = stringResource(R.string.login_signed_out_toast)

    LaunchedEffect(viewModel) {
        viewModel.effects.collect { autofill?.commit() }
    }
    LaunchedEffect(state.loaded) {
        if (state.loaded && inbox.take(FlashKind.SignedOut) != null) toast.show(signedOutText)
    }
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { viewModel.onStopped() }
    // Behind this screen there is nothing that works without a session: Back leaves the app instead of revealing it.
    val activity = LocalContext.current.findActivity()
    BackHandler { activity?.moveTaskToBack(true) }

    LoginScreen(
        state = state,
        password = viewModel.password,
        toast = toast,
        actions =
            LoginActions(
                onAddressChange = viewModel::onAddressChange,
                onAddressFocusLost = viewModel::onAddressFocusLost,
                onUsernameChange = viewModel::onUsernameChange,
                onPasswordChange = viewModel::onPasswordChange,
                onTogglePassword = viewModel::togglePasswordVisible,
                onChangeAddress = viewModel::onChangeAddress,
                onSubmit = viewModel::submit,
                onClockTick = viewModel::onClockTick,
                elapsedNow = viewModel::elapsedNow,
            ),
    )
}

data class LoginActions(
    val onAddressChange: (String) -> Unit = {},
    val onAddressFocusLost: () -> Unit = {},
    val onUsernameChange: (String) -> Unit = {},
    val onPasswordChange: (TextFieldValue) -> Unit = {},
    val onTogglePassword: () -> Unit = {},
    val onChangeAddress: () -> Unit = {},
    val onSubmit: () -> Unit = {},
    val onClockTick: (Long) -> Unit = {},
    val elapsedNow: () -> Long = { 0L },
)

@Composable
fun LoginScreen(
    state: LoginUiState,
    password: SecretField,
    toast: TeachToastState,
    actions: LoginActions,
) {
    TeachScreen(toast = { TeachToastHost(toast) }) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            if (state.loaded) LoginForm(state, password, actions)
        }
    }
}

@Composable
private fun LoginForm(
    state: LoginUiState,
    password: SecretField,
    actions: LoginActions,
) {
    val layout = TeachTheme.layout
    val spacing = TeachTheme.spacing
    val wide = currentWindowWidthClass() != WindowWidthClass.Compact
    val addressFocus = remember { FocusRequester() }
    val usernameFocus = remember { FocusRequester() }
    val passwordFocus = remember { FocusRequester() }
    // The two non-secret fields keep their text locally and synchronously (the IME always sees the latest value).
    val address = rememberTextFieldValue(state.address, state.fieldsVersion)
    val username = rememberTextFieldValue(state.username, state.fieldsVersion)

    LaunchedEffect(state.focus?.seq) {
        val request = state.focus ?: return@LaunchedEffect
        val (requester, field) =
            when (request.target) {
                LoginField.Address -> addressFocus to address
                LoginField.Username -> usernameFocus to username
                LoginField.Password -> passwordFocus to null
            }
        // The field may have just been created (the address row turns into a field): wait one frame.
        androidx.compose.runtime.withFrameNanos { }
        runCatching { requester.requestFocus() }
        if (request.selectAll) {
            if (field != null) field.value = field.value.copy(selection = TextRange(0, field.value.text.length))
            if (request.target == LoginField.Password) password.selectAll()
        }
    }

    Column(
        modifier =
            Modifier
                .widthIn(max = layout.dialogWidthWide)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .imePadding()
                .padding(
                    start = if (wide) layout.screenMarginMedium else layout.screenMarginCompact,
                    end = if (wide) layout.screenMarginMedium else layout.screenMarginCompact,
                    top = if (wide) spacing.xxxl else spacing.md,
                    bottom = spacing.md,
                ).onPreviewKeyEvent { event ->
                    // Ctrl+Enter submits from any field (login.md, "Invio").
                    val enter = event.key == Key.Enter || event.key == Key.NumPadEnter
                    if (enter && event.isCtrlPressed && event.type == KeyEventType.KeyDown) {
                        actions.onSubmit()
                        true
                    } else {
                        false
                    }
                },
        verticalArrangement = Arrangement.spacedBy(layout.blockGap),
    ) {
        ReasonPanel(state)
        Column(verticalArrangement = Arrangement.spacedBy(spacing.xs)) {
            TeachText(
                stringResource(R.string.login_title),
                Modifier.semantics { heading() },
                style = TeachTheme.typography.ui.title1,
            )
            if (state.variant.showsIntro) {
                TeachText(
                    stringResource(R.string.login_intro),
                    style = TeachTheme.typography.ui.bodyL,
                    color = TeachTheme.colors.inkSoft,
                )
            }
        }

        Column(verticalArrangement = Arrangement.spacedBy(spacing.md)) {
            if (state.addressEditable) {
                AddressField(state, address.value, { address.value = it }, actions, addressFocus)
            } else {
                AddressRow(state.savedAddress, actions.onChangeAddress)
            }
            TeachTextField(
                value = username.value,
                onValueChange = {
                    val changed = it.text != username.value.text
                    username.value = it
                    if (changed) actions.onUsernameChange(it.text)
                },
                label = stringResource(R.string.login_username_label),
                errorText = if (state.usernameError) stringResource(R.string.login_username_error) else null,
                readOnly = state.submitting,
                keyboardOptions =
                    KeyboardOptions(
                        capitalization = KeyboardCapitalization.None,
                        autoCorrectEnabled = false,
                        keyboardType = KeyboardType.Text,
                        imeAction = ImeAction.Next,
                    ),
                fieldModifier =
                    Modifier
                        .focusRequester(usernameFocus)
                        .semantics { contentType = ContentType.Username },
                modifier = Modifier.fillMaxWidth(),
            )
            PasswordField(state, password, actions, passwordFocus)
        }

        Column(verticalArrangement = Arrangement.spacedBy(spacing.sm)) {
            StatusArea(state, actions)
            SubmitButton(state, actions)
            if (state.slow) {
                TeachText(
                    stringResource(R.string.login_slow),
                    style = TeachTheme.typography.ui.bodyS,
                    color = TeachTheme.colors.inkSoft,
                )
            }
        }
        TeachText(
            stringResource(R.string.login_no_account),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

@Composable
private fun ReasonPanel(state: LoginUiState) {
    when (state.variant) {
        LoginVariant.SessionEnded -> {
            val base = stringResource(R.string.login_reason_body_expired)
            val counts = state.pending
            val body =
                if (counts.isEmpty) {
                    base
                } else {
                    stringResource(
                        R.string.login_reason_body_expired_pending,
                        base,
                        pendingPhrase(counts).replaceFirstChar { it.uppercase() },
                        stringResource(
                            if (counts.total ==
                                1
                            ) {
                                R.string.login_reason_pending_verb_one
                            } else {
                                R.string.login_reason_pending_verb_many
                            },
                        ),
                        state.username.ifBlank { "" },
                    )
                }
            TeachStatusPanel(StatusVariant.Info, stringResource(R.string.login_reason_title_expired), body = body)
        }
        LoginVariant.Update ->
            TeachStatusPanel(
                StatusVariant.Info,
                stringResource(R.string.login_reason_title_update),
                body = stringResource(R.string.login_reason_body_update),
            )
        else -> Unit
    }
}

@Composable
private fun AddressField(
    state: LoginUiState,
    value: TextFieldValue,
    onValueChange: (TextFieldValue) -> Unit,
    actions: LoginActions,
    focus: FocusRequester,
) {
    val error =
        state.addressError?.let {
            stringResource(
                when (it) {
                    AddressError.Empty -> R.string.login_address_error_empty
                    AddressError.InvalidScheme -> R.string.login_address_error_scheme
                    AddressError.InvalidHost -> R.string.login_address_error_host
                },
            )
        }
    TeachTextField(
        value = value,
        onValueChange = {
            val changed = it.text != value.text
            onValueChange(it)
            if (changed) actions.onAddressChange(it.text)
        },
        label = stringResource(R.string.login_address_label),
        placeholder = stringResource(R.string.login_address_placeholder),
        supportingText = stringResource(R.string.login_address_hint),
        errorText = error,
        readOnly = state.submitting,
        keyboardOptions =
            KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                keyboardType = KeyboardType.Uri,
                imeAction = ImeAction.Next,
            ),
        onFocusLost = actions.onAddressFocusLost,
        fieldModifier = Modifier.focusRequester(focus),
        modifier = Modifier.fillMaxWidth(),
    )
}

/** The saved address as one read-only row with "Cambia" (login.md, "Rientro"). */
@Composable
private fun AddressRow(
    address: String,
    onChange: () -> Unit,
) {
    val description = stringResource(R.string.login_address_row_desc, hostOf(address))
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .heightIn(min = TeachTheme.layout.controlHeight)
                .semantics(mergeDescendants = true) { contentDescription = description },
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TeachText(
            stringResource(R.string.login_address_row_label),
            style = TeachTheme.typography.ui.label,
            color = TeachTheme.colors.inkSoft,
        )
        TeachText(hostOf(address), Modifier.weight(1f), style = TeachTheme.typography.ui.bodyL)
        TeachButton(stringResource(R.string.login_address_change), onChange, variant = ButtonVariant.Text)
    }
}

@Composable
private fun PasswordField(
    state: LoginUiState,
    password: SecretField,
    actions: LoginActions,
    focus: FocusRequester,
) {
    TeachTextField(
        value = password.value,
        onValueChange = { if (!state.submitting) actions.onPasswordChange(it) },
        label = stringResource(R.string.login_password_label),
        errorText = if (state.passwordError) stringResource(R.string.login_password_error) else null,
        visualTransformation = if (state.passwordVisible) VisualTransformation.None else PasswordVisualTransformation(),
        keyboardOptions =
            KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                keyboardType = KeyboardType.Password,
                imeAction = ImeAction.Done,
            ),
        keyboardActions = KeyboardActions(onDone = { actions.onSubmit() }),
        trailing = {
            TeachIconButton(
                icon = if (state.passwordVisible) TeachIcons.EyeOff else TeachIcons.Eye,
                contentDescription =
                    stringResource(
                        if (state.passwordVisible) R.string.login_password_hide else R.string.login_password_show,
                    ),
                onClick = actions.onTogglePassword,
                toggled = state.passwordVisible,
            )
        },
        // Autofill is wanted here, and only for credentials (base-components, "Mascherato").
        fieldModifier = Modifier.focusRequester(focus).semantics { contentType = ContentType.Password },
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun SubmitButton(
    state: LoginUiState,
    actions: LoginActions,
) {
    val blocked = state.blocked
    val limit = state.panel as? LoginPanel.RateLimited
    val remaining = limit?.let { rememberRemaining(it, actions) }
    val availableIn = remaining?.let { waitText(it) }
    val availableText = availableIn?.let { stringResource(R.string.login_button_available_in, it) }
    TeachButton(
        text = stringResource(if (state.submitting) R.string.login_button_busy else R.string.login_button),
        onClick = actions.onSubmit,
        modifier =
            Modifier
                .fillMaxWidth()
                .semantics { if (availableText != null) stateDescription = availableText },
        size = ButtonSize.Large,
        enabled = !blocked,
        loading = state.submitting,
    )
}

/** Seconds left of a rate-limit block, ticking every second (or every 10 s over a minute); lifts the block at zero. */
@Composable
private fun rememberRemaining(
    block: LoginPanel.RateLimited,
    actions: LoginActions,
): Long {
    var remaining by remember(block) { mutableLongStateOf(secondsLeft(block, actions.elapsedNow())) }
    LaunchedEffect(block) {
        while (true) {
            val now = actions.elapsedNow()
            val left = secondsLeft(block, now)
            if (left <= 0) {
                actions.onClockTick(now)
                break
            }
            remaining = left
            delay(tickMillis(left))
        }
    }
    return remaining
}

private fun secondsLeft(
    block: LoginPanel.RateLimited,
    now: Long,
): Long = ceil((block.untilMs - now) / MS_PER_SECOND).toLong()

private const val MS_PER_SECOND = 1_000.0

@Composable
private fun waitText(seconds: Long): String {
    val wait = waitOf(seconds)
    return when (wait.unit) {
        WaitUnit.Seconds -> pluralStringResource(R.plurals.login_wait_seconds, wait.amount, wait.amount)
        WaitUnit.Minutes -> pluralStringResource(R.plurals.login_wait_minutes, wait.amount, wait.amount)
        WaitUnit.Hours -> pluralStringResource(R.plurals.login_wait_hours, wait.amount, wait.amount)
    }
}

/** The outcome of the last try, above "Accedi" (login.md, "Stati ed errori"). */
@Composable
private fun StatusArea(
    state: LoginUiState,
    actions: LoginActions,
) {
    when (val panel = state.panel) {
        null -> Unit
        is LoginPanel.InvalidCredentials ->
            TeachStatusPanel(
                StatusVariant.Warning,
                stringResource(R.string.login_panel_invalid_title),
                body =
                    stringResource(
                        if (panel.repeated) {
                            R.string.login_panel_invalid_body_repeated
                        } else {
                            R.string.login_panel_invalid_body
                        },
                    ),
                icon = TeachIcons.KeyRound,
            )
        is LoginPanel.RateLimited -> RateLimitedPanel(panel, actions, state.unblockedSeq)
        is LoginPanel.Unreachable ->
            TeachStatusPanel(
                StatusVariant.Warning,
                stringResource(R.string.login_panel_unreachable_title),
                body = unreachableBody(panel.reason),
                icon = TeachIcons.CloudOff,
            )
        LoginPanel.NotReady ->
            TeachStatusPanel(
                StatusVariant.Warning,
                stringResource(R.string.login_panel_notready_title),
                body = stringResource(R.string.login_panel_notready_body),
                icon = TeachIcons.Clock,
            )
        LoginPanel.NotRecognized ->
            TeachStatusPanel(
                StatusVariant.Danger,
                stringResource(R.string.login_panel_unknown_title),
                body = stringResource(R.string.login_panel_unknown_body),
            )
        is LoginPanel.Unexpected -> UnexpectedPanel(panel)
    }
}

@Composable
private fun unreachableBody(reason: UnreachableReason): String {
    val why =
        when (reason) {
            UnreachableReason.Timeout -> R.string.login_reason_timeout
            UnreachableReason.UnknownHost -> R.string.login_reason_unknown_host
            UnreachableReason.ConnectionRefused -> R.string.login_reason_refused
            UnreachableReason.Tls -> R.string.login_reason_tls
            UnreachableReason.Other -> return stringResource(R.string.login_panel_unreachable_body)
        }
    return stringResource(R.string.login_panel_unreachable_body_reason, stringResource(why))
}

@Composable
private fun RateLimitedPanel(
    panel: LoginPanel.RateLimited,
    actions: LoginActions,
    unblockedSeq: Int,
) {
    val remaining = rememberRemaining(panel, actions)
    val wait = waitText(remaining)
    val view = LocalView.current
    val announceBlocked =
        if (panel.waitKnown) {
            stringResource(R.string.login_announce_limited, wait)
        } else {
            stringResource(R.string.login_announce_limited_unknown)
        }
    // One announcement when the block starts: no new one at every tick (login.md, TalkBack).
    LaunchedEffect(panel) { view.announceForAccessibility(announceBlocked) }
    TeachStatusPanel(
        StatusVariant.Warning,
        stringResource(R.string.login_panel_limited_title),
        body =
            if (panel.waitKnown) {
                stringResource(R.string.login_panel_limited_body, wait)
            } else {
                stringResource(R.string.login_panel_limited_body_unknown)
            },
        icon = TeachIcons.Clock,
        announce = false,
    )
}

@Composable
private fun UnexpectedPanel(panel: LoginPanel.Unexpected) {
    var expanded by remember(panel) { mutableStateOf(false) }
    TeachStatusPanel(
        StatusVariant.Danger,
        stringResource(R.string.login_panel_unexpected_title),
        body =
            panel.httpStatus?.let { stringResource(R.string.login_panel_unexpected_body_code, it) }
                ?: stringResource(R.string.login_panel_unexpected_body),
    ) {
        if (panel.requestId != null) {
            TeachButton(
                stringResource(R.string.login_panel_details),
                { expanded = !expanded },
                variant = ButtonVariant.Text,
            )
            if (expanded) {
                TeachText(
                    stringResource(R.string.login_panel_request_id, panel.requestId),
                    style = TeachTheme.typography.ui.bodyS,
                    color = TeachTheme.colors.inkSoft,
                )
            }
        }
    }
}

// region Previews

private val previewBase =
    LoginUiState(
        loaded = true,
        variant = LoginVariant.FirstAccess,
        address = "https://teach.tailnet.ts.net",
        username = "valentina",
    )

@Composable
private fun PreviewHost(
    state: LoginUiState,
    dark: Boolean = false,
    secret: String = "password-di-prova",
) {
    TeachTheme(darkTheme = dark) {
        Box(Modifier.background(TeachTheme.colors.bg)) {
            val field = remember { SecretField().also { it.set(secret) } }
            LoginScreen(state, field, remember { TeachToastState() }, LoginActions())
        }
    }
}

@Preview(name = "First access", showBackground = true)
@Composable
private fun PreviewFirstAccess() = PreviewHost(previewBase.copy(address = "", username = ""), secret = "")

@Preview(name = "Update", showBackground = true)
@Composable
private fun PreviewUpdate() =
    PreviewHost(
        previewBase.copy(
            variant = LoginVariant.Update,
            addressEditable = false,
            savedAddress = "https://teach.tailnet.ts.net",
        ),
    )

@Preview(name = "Session ended with queue", showBackground = true)
@Composable
private fun PreviewSessionEnded() =
    PreviewHost(
        previewBase.copy(
            variant = LoginVariant.SessionEnded,
            addressEditable = false,
            savedAddress = "https://teach.tailnet.ts.net",
            pending = PendingCounts(reviews = 3, answers = 2),
        ),
    )

@Preview(name = "Wrong credentials", showBackground = true)
@Composable
private fun PreviewInvalid() =
    PreviewHost(
        previewBase.copy(
            variant = LoginVariant.Return,
            addressEditable = false,
            savedAddress = "https://teach.tailnet.ts.net",
            panel = LoginPanel.InvalidCredentials(repeated = true),
        ),
    )

@Preview(name = "Wrong credentials dark", showBackground = true)
@Composable
private fun PreviewInvalidDark() =
    PreviewHost(previewBase.copy(panel = LoginPanel.InvalidCredentials(repeated = false)), dark = true)

@Preview(name = "Unreachable", showBackground = true)
@Composable
private fun PreviewUnreachable() =
    PreviewHost(previewBase.copy(panel = LoginPanel.Unreachable(UnreachableReason.Timeout)))

@Preview(name = "Busy", showBackground = true)
@Composable
private fun PreviewBusy() = PreviewHost(previewBase.copy(submitting = true, slow = true))

@Preview(name = "Not recognized", showBackground = true, device = "spec:width=1280dp,height=800dp,dpi=240")
@Composable
private fun PreviewTablet() = PreviewHost(previewBase.copy(panel = LoginPanel.NotRecognized))

@Preview(name = "Font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun PreviewLargeFont() = PreviewHost(previewBase.copy(panel = LoginPanel.Unexpected(500, "01933b4e")))

// endregion

private fun android.content.Context.findActivity(): Activity? {
    var context = this
    while (context is ContextWrapper) {
        if (context is Activity) return context
        context = context.baseContext
    }
    return null
}
