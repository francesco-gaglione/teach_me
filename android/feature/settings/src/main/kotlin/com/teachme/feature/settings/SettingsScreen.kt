package com.teachme.feature.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.ChipVariant
import com.teachme.core.designsystem.components.DialogErrorPanel
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachCard
import com.teachme.core.designsystem.components.TeachConfirmDialog
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachListRow
import com.teachme.core.designsystem.components.TeachRowDivider
import com.teachme.core.designsystem.components.TeachScreen
import com.teachme.core.designsystem.components.TeachStatusChip
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachTextField
import com.teachme.core.designsystem.components.TeachToastHost
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.components.TeachTopBar
import com.teachme.core.model.ApiError
import com.teachme.core.model.AuthUser
import com.teachme.core.model.ConnectionResult
import com.teachme.core.model.FlashInbox
import com.teachme.core.model.FlashKind
import com.teachme.core.model.HealthResponse
import com.teachme.core.model.LlmKeyState
import com.teachme.core.model.LlmProvider
import com.teachme.core.model.PendingCounts
import com.teachme.core.model.UnreachableReason
import com.teachme.core.ui.SecretField
import com.teachme.core.ui.openAppNotificationSettings
import com.teachme.core.ui.pendingPhrase
import com.teachme.core.ui.rememberNotificationPermissionRequest

/**
 * Settings entry point (settings.md): account and sign-out, LLM key, password, connection, then the service rows.
 * @param extraContent shown at the end (debug builds put the sample lessons here).
 */
@Composable
fun SettingsRoute(
    inbox: FlashInbox,
    onBack: () -> Unit,
    onOpenPassword: () -> Unit,
    onOpenSuspended: () -> Unit = {},
    onOpenMissingComponents: () -> Unit = {},
    onOpenLicenses: () -> Unit = {},
    viewModel: SettingsViewModel = hiltViewModel(),
    listeningViewModel: ListeningViewModel = hiltViewModel(),
    extraContent: @Composable ColumnScope.() -> Unit = {},
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val listening by listeningViewModel.state.collectAsStateWithLifecycle()
    val voiceFailedText = stringResource(R.string.settings_listen_save_failed)
    val toast = remember { TeachToastState() }
    val keyField = remember { FocusRequester() }
    val savedText = stringResource(R.string.settings_key_saved)
    val savedRetryText = stringResource(R.string.settings_key_saved_retry)
    val removedText = stringResource(R.string.settings_key_removed)
    val passwordText = stringResource(R.string.password_changed_toast)

    val context = LocalContext.current
    val askNotificationPermission = rememberNotificationPermissionRequest { viewModel.onNotificationPermissionResult() }

    LifecycleEventEffect(Lifecycle.Event.ON_START) {
        viewModel.onStarted()
        listeningViewModel.onStarted()
    }
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) {
        viewModel.onStopped()
        listeningViewModel.onStopped()
    }
    // Leaving the composition (forward navigation) stops a sample too, not only the ON_STOP event.
    DisposableEffect(listeningViewModel) { onDispose { listeningViewModel.onStopped() } }
    LaunchedEffect(listeningViewModel) {
        listeningViewModel.effects.collect { toast.show(voiceFailedText) }
    }
    LaunchedEffect(viewModel) {
        viewModel.effects.collect { effect ->
            when (effect) {
                is SettingsEffect.KeySaved -> toast.show(if (effect.fromNotice) savedRetryText else savedText)
                SettingsEffect.KeyRemoved -> toast.show(removedText)
                SettingsEffect.FocusKey -> runCatching { keyField.requestFocus() }
                SettingsEffect.RequestNotificationPermission -> askNotificationPermission()
            }
        }
    }
    // The change-password screen leaves a message for this one when it closes.
    LaunchedEffect(Unit) {
        if (inbox.take(FlashKind.PasswordChanged) != null) toast.show(passwordText)
    }

    SettingsScreen(
        state = state,
        keyField = viewModel.keyField,
        keyFocus = keyField,
        toast = toast,
        extraContent = extraContent,
        listening = listening,
        listeningActions =
            ListeningActions(
                onSpeed = listeningViewModel::onSpeedSelected,
                onVoice = listeningViewModel::onVoiceSelected,
                onSample = listeningViewModel::onSampleClicked,
            ),
        actions =
            SettingsActions(
                onBack = onBack,
                onLogout = viewModel::onLogoutClicked,
                onLogoutDismiss = viewModel::onLogoutDismissed,
                onLogoutConfirm = viewModel::onLogoutConfirmed,
                onKeyChange = viewModel::onKeyChange,
                onKeyToggle = viewModel::toggleKeyVisible,
                onKeySave = viewModel::saveKey,
                onKeyReplace = viewModel::startReplacingKey,
                onKeyCancel = viewModel::cancelKeyWriting,
                onKeyRemoveAsk = viewModel::askRemoveKey,
                onKeyRemoveDismiss = viewModel::dismissRemoveKey,
                onKeyRemoveConfirm = viewModel::confirmRemoveKey,
                onScrolledToKey = viewModel::onScrolledToKey,
                onOpenPassword = onOpenPassword,
                onOpenSuspended = onOpenSuspended,
                onTestConnection = viewModel::testConnection,
                onOpenMissingComponents = onOpenMissingComponents,
                onOpenLicenses = onOpenLicenses,
                onReviewRemindersChange = viewModel::onReviewRemindersChanged,
                onOpenNotificationSettings = { context.openAppNotificationSettings() },
            ),
    )
}

data class SettingsActions(
    val onBack: () -> Unit = {},
    val onLogout: () -> Unit = {},
    val onLogoutDismiss: () -> Unit = {},
    val onLogoutConfirm: () -> Unit = {},
    val onKeyChange: (TextFieldValue) -> Unit = {},
    val onKeyToggle: () -> Unit = {},
    val onKeySave: () -> Unit = {},
    val onKeyReplace: () -> Unit = {},
    val onKeyCancel: () -> Unit = {},
    val onKeyRemoveAsk: () -> Unit = {},
    val onKeyRemoveDismiss: () -> Unit = {},
    val onKeyRemoveConfirm: () -> Unit = {},
    val onScrolledToKey: () -> Unit = {},
    val onOpenPassword: () -> Unit = {},
    val onOpenSuspended: () -> Unit = {},
    val onTestConnection: () -> Unit = {},
    val onOpenMissingComponents: () -> Unit = {},
    val onOpenLicenses: () -> Unit = {},
    val onReviewRemindersChange: (Boolean) -> Unit = {},
    val onOpenNotificationSettings: () -> Unit = {},
)

@Composable
fun SettingsScreen(
    state: SettingsUiState,
    keyField: SecretField,
    toast: TeachToastState,
    actions: SettingsActions,
    keyFocus: FocusRequester = remember { FocusRequester() },
    extraContent: @Composable ColumnScope.() -> Unit = {},
    listening: ListeningUiState = ListeningUiState(),
    listeningActions: ListeningActions = ListeningActions(),
) {
    TeachScreen(
        topBar = {
            TeachTopBar(
                title = stringResource(R.string.settings_title),
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
        toast = { TeachToastHost(toast) },
    ) {
        Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            SettingsForm(state, keyField, keyFocus, actions, extraContent, listening, listeningActions)
        }
    }
    LogoutDialog(state, actions)
    RemoveKeyDialog(state.removeKey, actions)
}

@Composable
private fun SettingsForm(
    state: SettingsUiState,
    keyField: SecretField,
    keyFocus: FocusRequester,
    actions: SettingsActions,
    extraContent: @Composable ColumnScope.() -> Unit,
    listening: ListeningUiState,
    listeningActions: ListeningActions,
) {
    val layout = TeachTheme.layout
    val scroll = rememberScrollState()
    var keySectionTop by remember { mutableIntStateOf(-1) }
    // Deep link from a key notice: the key section goes to the top, once (settings.md).
    LaunchedEffect(state.scrollToKey, keySectionTop) {
        if (state.scrollToKey && keySectionTop >= 0) {
            scroll.scrollTo(keySectionTop)
            actions.onScrolledToKey()
        }
    }
    Column(
        modifier =
            Modifier
                .widthIn(max = layout.readingMaxWidth)
                .fillMaxWidth()
                .verticalScroll(scroll)
                .imePadding()
                .padding(layout.screenMarginCompact),
        verticalArrangement = Arrangement.spacedBy(layout.blockGap),
    ) {
        Section(stringResource(R.string.settings_section_account)) { AccountCard(state, actions) }
        Section(
            stringResource(R.string.settings_section_key),
            Modifier.onGloballyPositioned { keySectionTop = it.positionInParent().y.toInt() },
        ) { KeyCard(state, keyField, keyFocus, actions) }
        Section(stringResource(R.string.settings_section_password)) {
            TeachCard(Modifier.fillMaxWidth()) {
                TeachListRow(
                    title = stringResource(R.string.settings_password_row),
                    onClick = actions.onOpenPassword,
                    contentDescription = stringResource(R.string.settings_password_row),
                    trailing = {
                        TeachIcon(TeachIcons.ChevronRight, contentDescription = null, tint = TeachTheme.colors.inkSoft)
                    },
                )
            }
        }
        // Only with something to reactivate: absent while loading, on error and at zero (suspended.md section 1).
        val suspended = state.suspendedCount
        if (suspended != null && suspended > 0) {
            Section(
                stringResource(R.string.settings_section_review),
            ) { SuspendedLinkRow(suspended, actions.onOpenSuspended) }
        }
        Section(stringResource(R.string.settings_section_notifications)) { NotificationsCard(state, actions) }
        // Hidden altogether when read-aloud is off on the server or the voice list could not be read.
        if (listening.visible) {
            Section(stringResource(R.string.settings_section_listening)) { ListeningCard(listening, listeningActions) }
        }
        Section(stringResource(R.string.settings_section_connection)) { ConnectionCard(state, actions) }
        LinkRows(actions)
        extraContent()
    }
}

@Composable
private fun SuspendedLinkRow(
    count: Int,
    onClick: () -> Unit,
) {
    TeachCard(Modifier.fillMaxWidth()) {
        TeachListRow(
            title = stringResource(R.string.settings_suspended_row),
            onClick = onClick,
            contentDescription = stringResource(R.string.settings_suspended_row_a11y, count),
            leadingIcon = TeachIcons.Pause,
            trailing = {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    TeachText(
                        count.toString(),
                        style = TeachTheme.typography.ui.body,
                        color = TeachTheme.colors.inkSoft,
                    )
                    TeachIcon(TeachIcons.ChevronRight, contentDescription = null, tint = TeachTheme.colors.inkSoft)
                }
            },
        )
    }
}

@Composable
private fun Section(
    title: String,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachText(
            title,
            Modifier.semantics { heading() },
            style = TeachTheme.typography.ui.label,
            color = TeachTheme.colors.inkSoft,
        )
        content()
    }
}

// region Account

@Composable
private fun AccountCard(
    state: SettingsUiState,
    actions: SettingsActions,
) {
    val user = state.user
    TeachCard(Modifier.fillMaxWidth()) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
            if (user != null) {
                val usernameText = stringResource(R.string.settings_account_username, user.username)
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                    TeachText(user.displayName, style = TeachTheme.typography.ui.title3)
                    TeachText(
                        "@${user.username}",
                        Modifier.semantics { contentDescription = usernameText },
                        style = TeachTheme.typography.ui.body,
                        color = TeachTheme.colors.inkSoft,
                    )
                }
            }
            val sending = state.logout == LogoutUi.Sending
            TeachButton(
                stringResource(if (sending) R.string.settings_logout_sending else R.string.settings_logout),
                actions.onLogout,
                Modifier.fillMaxWidth(),
                variant = ButtonVariant.Secondary,
                loading = sending,
                leadingIcon = TeachIcons.LogOut,
            )
        }
    }
}

@Composable
private fun LogoutDialog(
    state: SettingsUiState,
    actions: SettingsActions,
) {
    val confirm = state.logout as? LogoutUi.Confirm ?: return
    val phrase = pendingPhrase(confirm.left)
    val username = state.user?.username.orEmpty()
    TeachConfirmDialog(
        title = stringResource(R.string.settings_logout_dialog_title),
        body =
            stringResource(
                if (confirm.left.total ==
                    1
                ) {
                    R.string.settings_logout_dialog_body_one
                } else {
                    R.string.settings_logout_dialog_body_many
                },
                phrase.replaceFirstChar { it.uppercase() },
                username,
            ),
        confirmLabel = stringResource(R.string.settings_logout_confirm),
        dismissLabel = stringResource(R.string.settings_logout_cancel),
        onConfirm = actions.onLogoutConfirm,
        onDismiss = actions.onLogoutDismiss,
    )
}

// endregion

// region LLM key

@Composable
private fun KeyCard(
    state: SettingsUiState,
    keyField: SecretField,
    keyFocus: FocusRequester,
    actions: SettingsActions,
) {
    val missing = state.keyState == LlmKeyState.Missing
    val rejected = state.keyState == LlmKeyState.RejectedKnown
    val writing = missing || state.keyWriting
    TeachCard(Modifier.fillMaxWidth()) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
            KeyChip(state.keyState)
            TeachText(
                stringResource(R.string.settings_key_intro),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkSoft,
            )
            if (writing) {
                KeyWriter(state, keyField, keyFocus, actions, cancellable = !missing)
            } else {
                TeachText(
                    stringResource(
                        if (rejected) R.string.settings_key_rejected_note else R.string.settings_key_set_note,
                    ),
                    style = if (rejected) TeachTheme.typography.ui.body else TeachTheme.typography.ui.bodyS,
                    color = if (rejected) TeachTheme.colors.ink else TeachTheme.colors.inkSoft,
                )
                KeyButtons(rejected, actions)
            }
        }
    }
}

@Composable
private fun KeyChip(keyState: LlmKeyState) {
    val (text, icon, variant, description) =
        when (keyState) {
            LlmKeyState.Missing ->
                ChipLook(
                    stringResource(R.string.settings_key_chip_missing),
                    TeachIcons.Info,
                    ChipVariant.Info,
                    stringResource(R.string.settings_key_chip_desc_missing),
                )
            LlmKeyState.Set ->
                ChipLook(
                    stringResource(R.string.settings_key_chip_set),
                    TeachIcons.Check,
                    ChipVariant.Neutral,
                    stringResource(R.string.settings_key_chip_desc_set),
                )
            LlmKeyState.RejectedKnown ->
                ChipLook(
                    stringResource(R.string.settings_key_chip_rejected),
                    TeachIcons.AlertTriangle,
                    ChipVariant.Warning,
                    stringResource(R.string.settings_key_chip_desc_rejected),
                )
        }
    Box(Modifier.semantics(mergeDescendants = true) { contentDescription = description }) {
        TeachStatusChip(text, icon, variant = variant)
    }
}

private data class ChipLook(
    val text: String,
    val icon: Int,
    val variant: ChipVariant,
    val description: String,
)

/** Replace and Remove side by side, one under the other once the text is large (settings.md, "Accessibilità"). */
@Composable
private fun KeyButtons(
    rejected: Boolean,
    actions: SettingsActions,
) {
    val stacked = LocalDensity.current.fontScale > STACK_FONT_SCALE
    val replace: @Composable (Modifier) -> Unit = {
        TeachButton(
            stringResource(R.string.settings_key_replace),
            actions.onKeyReplace,
            it,
            variant = if (rejected) ButtonVariant.Primary else ButtonVariant.Secondary,
        )
    }
    val remove: @Composable (Modifier) -> Unit = {
        TeachButton(
            stringResource(R.string.settings_key_remove),
            actions.onKeyRemoveAsk,
            it,
            variant = ButtonVariant.Destructive,
        )
    }
    if (stacked) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            replace(Modifier.fillMaxWidth())
            remove(Modifier.fillMaxWidth())
        }
    } else {
        Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            replace(Modifier)
            remove(Modifier)
        }
    }
}

private const val STACK_FONT_SCALE = 1.3f

@Composable
private fun KeyWriter(
    state: SettingsUiState,
    keyField: SecretField,
    keyFocus: FocusRequester,
    actions: SettingsActions,
    cancellable: Boolean,
) {
    val error =
        state.keyFieldError?.let {
            stringResource(
                if (it ==
                    KeyFieldError.Empty
                ) {
                    R.string.settings_key_error_empty
                } else {
                    R.string.settings_key_error_long
                },
            )
        }
    TeachTextField(
        value = keyField.value,
        onValueChange = { if (!state.keyBusy) actions.onKeyChange(it) },
        label = stringResource(R.string.settings_key_field_label),
        supportingText = stringResource(R.string.settings_key_field_hint),
        errorText = error,
        visualTransformation = if (state.keyVisible) VisualTransformation.None else PasswordVisualTransformation(),
        keyboardOptions =
            KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                keyboardType = KeyboardType.Password,
                imeAction = ImeAction.Done,
            ),
        keyboardActions = KeyboardActions(onDone = { actions.onKeySave() }),
        // A secret to paste, not a credential to remember: no autofill (no content type is set), no suggestions.
        trailing = {
            TeachIconButton(
                icon = if (state.keyVisible) TeachIcons.EyeOff else TeachIcons.Eye,
                contentDescription =
                    stringResource(if (state.keyVisible) R.string.settings_key_hide else R.string.settings_key_show),
                onClick = actions.onKeyToggle,
                toggled = state.keyVisible,
            )
        },
        fieldModifier = Modifier.focusRequester(keyFocus),
        modifier = Modifier.fillMaxWidth(),
    )
    state.keyError?.let { KeyActionPanel(it) }
    TeachButton(
        stringResource(if (state.keyBusy) R.string.settings_key_saving else R.string.settings_key_save),
        actions.onKeySave,
        Modifier.fillMaxWidth(),
        loading = state.keyBusy,
    )
    if (cancellable) {
        TeachButton(
            stringResource(R.string.settings_key_cancel),
            actions.onKeyCancel,
            variant = ButtonVariant.Text,
            enabled = !state.keyBusy,
        )
    }
}

@Composable
private fun KeyActionPanel(error: KeyActionError) {
    if (error == KeyActionError.Network) {
        TeachStatusPanel(
            StatusVariant.Warning,
            stringResource(R.string.settings_key_action_network_title),
            body = stringResource(R.string.settings_key_save_network_body),
            icon = TeachIcons.CloudOff,
        )
    } else {
        TeachStatusPanel(
            StatusVariant.Danger,
            stringResource(R.string.settings_key_action_server_title),
            body = stringResource(R.string.settings_key_save_server_body),
        )
    }
}

@Composable
private fun RemoveKeyDialog(
    ui: RemoveKeyUi,
    actions: SettingsActions,
) {
    val open = ui as? RemoveKeyUi.Open ?: return
    val error =
        open.error?.let {
            if (it == KeyActionError.Network) {
                DialogErrorPanel(
                    stringResource(R.string.settings_key_remove_network_title),
                    stringResource(R.string.settings_key_remove_network_body),
                )
            } else {
                DialogErrorPanel(
                    stringResource(R.string.settings_key_remove_server_title),
                    stringResource(R.string.settings_key_remove_server_body),
                )
            }
        }
    TeachConfirmDialog(
        title = stringResource(R.string.settings_key_remove_title),
        body = stringResource(R.string.settings_key_remove_body),
        confirmLabel = stringResource(R.string.settings_key_remove_confirm),
        dismissLabel = stringResource(R.string.settings_key_remove_cancel),
        onConfirm = actions.onKeyRemoveConfirm,
        onDismiss = actions.onKeyRemoveDismiss,
        destructive = true,
        loading = open.busy,
        loadingLabel = stringResource(R.string.settings_key_remove_busy),
        error = error,
    )
}

// endregion

// region Notifications

/** The device switch of the review notifications; with the system permission missing it says so and links to settings. */
@Composable
private fun NotificationsCard(
    state: SettingsUiState,
    actions: SettingsActions,
) {
    val on = state.reviewReminders
    val title = stringResource(R.string.settings_notifications_row)
    val stateText = stringResource(if (on) R.string.settings_notifications_on else R.string.settings_notifications_off)
    TeachCard(Modifier.fillMaxWidth()) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
            TeachListRow(
                title = title,
                subtitle = stringResource(R.string.settings_notifications_note),
                onClick = { actions.onReviewRemindersChange(!on) },
                contentDescription = title,
                modifier =
                    Modifier.semantics {
                        role = Role.Switch
                        toggleableState = ToggleableState(on)
                        stateDescription = stateText
                    },
                trailing = {
                    // The state is a word and an icon, not color alone.
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        if (on) TeachIcon(TeachIcons.Check, contentDescription = null, tint = TeachTheme.colors.accent)
                        TeachText(
                            stateText,
                            style = TeachTheme.typography.ui.label,
                            color = if (on) TeachTheme.colors.accent else TeachTheme.colors.inkSoft,
                        )
                    }
                },
            )
            if (state.notificationsBlocked) {
                TeachText(
                    stringResource(R.string.settings_notifications_blocked),
                    style = TeachTheme.typography.ui.bodyS,
                    color = TeachTheme.colors.inkSoft,
                )
                TeachButton(
                    stringResource(R.string.settings_notifications_open_settings),
                    actions.onOpenNotificationSettings,
                    Modifier.fillMaxWidth(),
                    variant = ButtonVariant.Secondary,
                )
            }
        }
    }
}

// endregion

// region Connection

@Composable
private fun ConnectionCard(
    state: SettingsUiState,
    actions: SettingsActions,
) {
    TeachCard(Modifier.fillMaxWidth()) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                TeachText(
                    stringResource(R.string.settings_connection_backend),
                    style = TeachTheme.typography.ui.label,
                    color = TeachTheme.colors.inkSoft,
                )
                TeachText(state.host, style = TeachTheme.typography.ui.bodyL)
            }
            TeachText(
                stringResource(R.string.settings_connection_note),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
            TeachButton(
                text =
                    stringResource(
                        if (state.isTesting) R.string.settings_test_button_running else R.string.settings_test_button,
                    ),
                onClick = actions.onTestConnection,
                modifier = Modifier.fillMaxWidth(),
                variant = ButtonVariant.Secondary,
                loading = state.isTesting,
            )
            ConnectionStatusArea(state.connection)
        }
    }
}

/** Hosts the status panel; the panel itself is the polite live region that TalkBack announces. */
@Composable
private fun ConnectionStatusArea(connection: ConnectionUiState) {
    when (connection) {
        ConnectionUiState.Idle -> Unit
        is ConnectionUiState.Testing ->
            TeachStatusPanel(
                variant = StatusVariant.Progress,
                title = stringResource(R.string.settings_status_testing_title),
                body = stringResource(R.string.settings_status_testing_detail, connection.host),
            )
        is ConnectionUiState.Done ->
            when (val result = connection.result) {
                is ConnectionResult.Connected ->
                    TeachStatusPanel(
                        variant = StatusVariant.Correct,
                        title = stringResource(R.string.settings_status_connected_title),
                        body =
                            stringResource(
                                R.string.settings_status_connected_detail,
                                result.health.version,
                                result.health.catalogVersion,
                            ),
                    )
                // A 401 closes the session and opens the login: no panel here (never reaches this state).
                ConnectionResult.SessionEnded -> Unit
                is ConnectionResult.Unreachable ->
                    TeachStatusPanel(
                        variant = StatusVariant.Warning,
                        title = stringResource(R.string.settings_status_unreachable_title),
                        body = unreachableDetail(result.reason),
                        icon = TeachIcons.CloudOff,
                    )
                is ConnectionResult.Unexpected -> UnexpectedPanel(result)
            }
    }
}

@Composable
private fun unreachableDetail(reason: UnreachableReason): String {
    val reasonText =
        when (reason) {
            UnreachableReason.Timeout -> stringResource(R.string.settings_status_reason_timeout)
            UnreachableReason.UnknownHost -> stringResource(R.string.settings_status_reason_unknown_host)
            UnreachableReason.ConnectionRefused -> stringResource(R.string.settings_status_reason_refused)
            UnreachableReason.Tls -> stringResource(R.string.settings_status_reason_tls)
            UnreachableReason.Other -> return stringResource(R.string.settings_status_unreachable_detail_generic)
        }
    return stringResource(R.string.settings_status_unreachable_detail, reasonText)
}

@Composable
private fun UnexpectedPanel(result: ConnectionResult.Unexpected) {
    var expanded by remember(result) { mutableStateOf(false) }
    val apiError = result.error
    TeachStatusPanel(
        variant = StatusVariant.Danger,
        title = stringResource(R.string.settings_status_unexpected_title),
        body =
            result.httpStatus?.let { stringResource(R.string.settings_status_unexpected_detail_code, it) }
                ?: stringResource(R.string.settings_status_unexpected_detail_nocode),
        extra = {
            if (apiError != null && apiError.message.isNotBlank()) {
                TeachText(
                    apiError.message,
                    style = TeachTheme.typography.ui.body,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (apiError?.requestId != null) {
                TeachButton(
                    text = stringResource(R.string.settings_status_details_toggle),
                    onClick = { expanded = !expanded },
                    variant = ButtonVariant.Text,
                )
                if (expanded) {
                    TeachText(
                        stringResource(R.string.settings_status_request_id, apiError.requestId.orEmpty()),
                        style = TeachTheme.typography.ui.bodyS,
                    )
                }
            }
        },
    )
}

// endregion

/** Hairline plus the two sub-screens (settings.md). */
@Composable
private fun LinkRows(actions: SettingsActions) {
    TeachCard(Modifier.fillMaxWidth()) {
        Column {
            LinkRow(stringResource(R.string.settings_row_missing), TeachIcons.Puzzle, actions.onOpenMissingComponents)
            TeachRowDivider()
            LinkRow(stringResource(R.string.settings_row_licenses), TeachIcons.BookOpen, actions.onOpenLicenses)
        }
    }
}

@Composable
private fun LinkRow(
    title: String,
    icon: Int,
    onClick: () -> Unit,
) {
    TeachListRow(
        title = title,
        onClick = onClick,
        contentDescription = title,
        leadingIcon = icon,
        trailing = { TeachIcon(TeachIcons.ChevronRight, contentDescription = null, tint = TeachTheme.colors.inkSoft) },
    )
}

// region Previews

private val previewHealth = HealthResponse("ok", "0.1.0", 1, 1, "2026-11-08T14:36:01.129Z")
private val previewUser =
    AuthUser("u1", "valentina", "Valentina", hasLlmKey = true, llmProvider = LlmProvider.OpenRouter)
private val previewBase = SettingsUiState(user = previewUser, host = "teach.tailnet.ts.net")

@Composable
private fun PreviewHost(
    state: SettingsUiState,
    dark: Boolean = false,
) {
    TeachTheme(darkTheme = dark) {
        Box(Modifier.background(TeachTheme.colors.bg)) {
            SettingsScreen(
                state,
                remember {
                    SecretField().also { it.set("sk-or-v1-abcdef") }
                },
                remember { TeachToastState() },
                SettingsActions(),
            )
        }
    }
}

@Preview(name = "Notifications off", showBackground = true)
@Composable
private fun PreviewNotificationsOff() = PreviewHost(previewBase.copy(reviewReminders = false))

@Preview(name = "Notifications blocked", showBackground = true)
@Composable
private fun PreviewNotificationsBlocked() = PreviewHost(previewBase.copy(notificationsBlocked = true))

@Preview(name = "Notifications blocked dark", showBackground = true)
@Composable
private fun PreviewNotificationsBlockedDark() = PreviewHost(previewBase.copy(notificationsBlocked = true), dark = true)

@Preview(name = "Review section", showBackground = true)
@Composable
private fun PreviewSuspendedRow() = PreviewHost(previewBase.copy(suspendedCount = 3))

@Preview(name = "Review section dark", showBackground = true)
@Composable
private fun PreviewSuspendedRowDark() = PreviewHost(previewBase.copy(suspendedCount = 3), dark = true)

@Preview(name = "Key set", showBackground = true)
@Composable
private fun PreviewKeySet() = PreviewHost(previewBase)

@Preview(name = "Key set dark", showBackground = true)
@Composable
private fun PreviewKeySetDark() = PreviewHost(previewBase, dark = true)

@Preview(name = "Key missing", showBackground = true)
@Composable
private fun PreviewKeyMissing() =
    PreviewHost(previewBase.copy(user = previewUser.copy(hasLlmKey = false), keyState = LlmKeyState.Missing))

@Preview(name = "Key rejected", showBackground = true)
@Composable
private fun PreviewKeyRejected() = PreviewHost(previewBase.copy(keyState = LlmKeyState.RejectedKnown))

@Preview(name = "Key replacing, error", showBackground = true)
@Composable
private fun PreviewKeyReplacing() =
    PreviewHost(
        previewBase.copy(keyWriting = true, keyError = KeyActionError.Network, keyFieldError = KeyFieldError.TooLong),
    )

@Preview(name = "Remove key dialog", showBackground = true)
@Composable
private fun PreviewRemoveDialog() = PreviewHost(previewBase.copy(removeKey = RemoveKeyUi.Open()))

@Preview(name = "Logout dialog", showBackground = true)
@Composable
private fun PreviewLogoutDialog() = PreviewHost(previewBase.copy(logout = LogoutUi.Confirm(PendingCounts(3, 2))))

@Preview(name = "Connected", showBackground = true)
@Composable
private fun PreviewConnected() =
    PreviewHost(previewBase.copy(connection = ConnectionUiState.Done(ConnectionResult.Connected(previewHealth))))

@Preview(name = "Unreachable dark", showBackground = true)
@Composable
private fun PreviewUnreachableDark() =
    PreviewHost(
        previewBase.copy(connection = ConnectionUiState.Done(ConnectionResult.Unreachable(UnreachableReason.Timeout))),
        dark = true,
    )

@Preview(name = "Unexpected", showBackground = true)
@Composable
private fun PreviewUnexpected() =
    PreviewHost(
        previewBase.copy(
            connection =
                ConnectionUiState.Done(
                    ConnectionResult.Unexpected(500, ApiError("internal", "Errore interno.", requestId = "01933b4e")),
                ),
        ),
    )

@Preview(name = "Font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun PreviewLargeFont() = PreviewHost(previewBase)

@Preview(name = "Tablet", showBackground = true, device = "spec:width=1280dp,height=800dp,dpi=240")
@Composable
private fun PreviewTablet() = PreviewHost(previewBase)

// endregion
