package com.teachme.feature.chat

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachFilledIconButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.model.CHAT_MAX_CHARS
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyHint
import com.teachme.core.ui.LlmKeyNotice
import java.text.DecimalFormat
import java.text.NumberFormat
import java.util.Locale

private const val COMPOSER_MAX_LINES = 8
private val THIN_SPACE = ' '

/** Formats with a thin-space thousands separator, as in "7 412 / 8000" (chat.md 8). */
private fun Int.grouped(locale: Locale = Locale.getDefault()): String {
    val format = NumberFormat.getIntegerInstance(locale)
    if (format is DecimalFormat) {
        format.decimalFormatSymbols = format.decimalFormatSymbols.apply { groupingSeparator = THIN_SPACE }
    }
    return format.format(this)
}

/** Physical keyboard only: the virtual keyboard keeps Enter = new line (chat.md 8). */
private fun androidx.compose.ui.input.key.KeyEvent.fromPhysicalKeyboard(): Boolean =
    nativeKeyEvent.device?.isVirtual == false

/**
 * Message composer: multiline field, send button, reason line, counter and the send error panel.
 * The field stays editable while the Tutor answers; only sending is disabled (chat.md 8).
 */
@Composable
internal fun ChatComposer(
    value: TextFieldValue,
    onValueChange: (TextFieldValue) -> Unit,
    state: ChatUiState,
    maxFieldHeight: Dp,
    onSend: () -> Unit,
    onDismissError: () -> Unit,
    onOpenSettings: () -> Unit,
    modifier: Modifier = Modifier,
    focusRequester: FocusRequester = remember { FocusRequester() },
    hint: String = stringResource(R.string.chat_composer_hint),
    /** Sending is paused (a mission confirmation is running); the reason is shown above the field. */
    suspendedReason: String? = null,
    onFocusChange: (Boolean) -> Unit = {},
    /** The lesson the question is about ("Sulla lezione 3: ..."), above the field; null when there is none. */
    contextChip: (@Composable () -> Unit)? = null,
) {
    val colors = TeachTheme.colors
    val info = chatInputInfo(value.text)
    val canSend = state.canSend && info.isValid && suspendedReason == null
    val source = remember { MutableInteractionSource() }
    val focused by source.collectIsFocusedAsState()
    val currentOnFocusChange by rememberUpdatedState(onFocusChange)
    LaunchedEffect(focused) { currentOnFocusChange(focused) }
    val borderColor =
        when {
            info.isTooLong -> colors.danger
            focused -> colors.focus
            else -> colors.border
        }
    val borderWidth = if (focused || info.isTooLong) TeachTheme.stroke.selected else TeachTheme.stroke.control
    val label = stringResource(R.string.chat_composer_label)
    val thresholdText =
        stringResource(
            R.string.chat_counter_threshold_announce,
            CHAT_COUNTER_THRESHOLD.toString(),
            CHAT_MAX_CHARS.toString(),
        )
    val tooLongText = stringResource(R.string.chat_counter_over, CHAT_MAX_CHARS.toString(), info.overBy.grouped())

    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        state.sendError?.let { SendErrorPanel(it, onSend, onDismissError) }
        // The key: a refusal of this send (blocking), or the suggestion while it is known missing (llm-key.md 4.2).
        val keyNotice = state.keyNotice
        if (keyNotice != null) {
            LlmKeyNotice(
                keyNotice.notice.variant,
                if (keyNotice.fromRetry) LlmKeyContext.ChatReply else LlmKeyContext.ChatMessage,
                keyNotice.notice.id,
                // Only the credit has an action here: the same request again, with what is in the field.
                onRetry = onSend.takeIf { keyNotice.notice.variant == LlmKeyVariant.Credit },
                retrying = state.sending,
                again = keyNotice.notice.again,
                // Nothing to send again when the user emptied the field: the button is disabled, not silent.
                retryEnabled = info.isValid,
            )
        } else if (state.sendError == null) {
            LlmKeyHint(LlmKeyContext.ChatHint)
        }
        contextChip?.invoke()
        if (state.notice == ComposerNotice.ChatUpdated) {
            ReasonLine(TeachIcons.Info, stringResource(R.string.chat_notice_updated))
        }
        if (suspendedReason != null) ReasonLine(TeachIcons.Clock, suspendedReason)
        AnimatedVisibility(
            visible = state.turnInProgress,
            enter = fadeIn(tween(TeachTheme.motion.fastMs)),
            exit = fadeOut(tween(TeachTheme.motion.fastMs)),
        ) {
            ReasonLine(TeachIcons.Clock, stringResource(R.string.chat_reason_turn))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs), verticalAlignment = Alignment.Bottom) {
            Box(
                Modifier
                    .weight(1f)
                    .heightIn(min = TeachTheme.layout.controlHeight, max = maxFieldHeight)
                    .background(colors.surface, TeachTheme.shapes.sm)
                    .border(borderWidth, borderColor, TeachTheme.shapes.sm)
                    .padding(TeachTheme.spacing.sm),
                contentAlignment = Alignment.CenterStart,
            ) {
                BasicTextField(
                    value = value,
                    onValueChange = onValueChange,
                    readOnly = state.sending,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .focusRequester(focusRequester)
                            .semantics { contentDescription = label }
                            .onPreviewKeyEvent { event ->
                                if (event.type != KeyEventType.KeyDown ||
                                    (event.key != Key.Enter && event.key != Key.NumPadEnter)
                                ) {
                                    return@onPreviewKeyEvent false
                                }
                                if (!event.fromPhysicalKeyboard()) return@onPreviewKeyEvent false
                                if (event.isShiftPressed) {
                                    onValueChange(value.insertNewLine())
                                } else if (canSend) {
                                    onSend()
                                }
                                // Enter never types a new line on a keyboard: Shift+Enter does.
                                true
                            },
                    textStyle =
                        TeachTheme.typography.ui.bodyL
                            .copy(color = colors.ink),
                    cursorBrush = SolidColor(colors.accent),
                    maxLines = COMPOSER_MAX_LINES,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    interactionSource = source,
                    decorationBox = { inner ->
                        Box(contentAlignment = Alignment.CenterStart) {
                            if (value.text.isEmpty()) {
                                TeachText(
                                    hint,
                                    style = TeachTheme.typography.ui.bodyL,
                                    color = colors.inkMuted,
                                )
                            }
                            inner()
                        }
                    },
                )
            }
            TeachFilledIconButton(
                icon = TeachIcons.Send,
                contentDescription = stringResource(R.string.chat_send),
                onClick = onSend,
                enabled = canSend,
                loading = state.sending,
                disabledDescription =
                    suspendedReason
                        ?: stringResource(
                            if (state.turnInProgress) {
                                R.string.chat_send_disabled_busy
                            } else {
                                R.string.chat_send_disabled_empty
                            },
                        ),
            )
        }
        if (info.isTooLong) {
            Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                TeachIcon(TeachIcons.AlertCircle, null, tint = colors.danger, size = TeachTheme.layout.iconSizeSmall)
                TeachText(tooLongText, style = TeachTheme.typography.ui.bodyS, color = colors.danger)
            }
        }
        if (info.showCounter) {
            TeachText(
                stringResource(R.string.chat_counter, info.length.grouped(), CHAT_MAX_CHARS.grouped()),
                Modifier.fillMaxWidth().clearAndSetSemantics {
                    contentDescription = if (info.isTooLong) tooLongText else thresholdText
                    liveRegion = LiveRegionMode.Polite
                },
                style = TeachTheme.typography.ui.bodyS,
                color = if (info.isTooLong) colors.danger else colors.inkSoft,
                textAlign = TextAlign.End,
            )
        }
    }
}

private fun TextFieldValue.insertNewLine(): TextFieldValue {
    val start = selection.min
    val end = selection.max
    val updated = text.substring(0, start) + "\n" + text.substring(end)
    return copy(text = updated, selection = TextRange(start + 1))
}

@Composable
private fun ReasonLine(
    icon: Int,
    text: String,
) {
    Row(
        Modifier.padding(vertical = TeachTheme.spacing.xxs),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TeachIcon(icon, null, tint = TeachTheme.colors.inkSoft, size = TeachTheme.layout.iconSizeSmall)
        TeachText(text, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
    }
}

/** "Messaggio non inviato" above the composer (chat.md 9.3): the text stays in the field. */
@Composable
private fun SendErrorPanel(
    error: SendError,
    onRetry: () -> Unit,
    onDismiss: () -> Unit,
) {
    val body =
        stringResource(
            when (error) {
                SendError.Network -> R.string.chat_send_error_network
                else -> R.string.chat_send_error_server
            },
        )
    Box {
        TeachStatusPanel(
            StatusVariant.Danger,
            stringResource(R.string.chat_send_error_title),
            Modifier.fillMaxWidth(),
            body = body,
        ) {
            TeachButton(stringResource(R.string.chat_send_error_retry), onRetry, variant = ButtonVariant.Secondary)
        }
        TeachIconButton(
            TeachIcons.X,
            stringResource(R.string.chat_dismiss),
            onDismiss,
            Modifier.align(Alignment.TopEnd),
        )
    }
}

@Composable
private fun ComposerPreviewContent(
    text: String,
    state: ChatUiState,
) {
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        ChatComposer(TextFieldValue(text), {}, state, 240.dp, {}, {}, {})
    }
}

@Preview(name = "Composer idle", showBackground = true, widthDp = 360)
@Composable
private fun ComposerIdlePreview() = TeachTheme(darkTheme = false) { ComposerPreviewContent("", ChatUiState()) }

@Preview(name = "Composer sendError dark", showBackground = true, widthDp = 360)
@Composable
private fun ComposerErrorPreview() =
    TeachTheme(darkTheme = true) { ComposerPreviewContent("Ciao", ChatUiState(sendError = SendError.Network)) }

@Preview(name = "Composer too long", showBackground = true, widthDp = 360)
@Composable
private fun ComposerTooLongPreview() =
    TeachTheme(darkTheme = false) { ComposerPreviewContent("a".repeat(8312), ChatUiState()) }

@Preview(name = "Composer font 2.0", showBackground = true, widthDp = 360, fontScale = 2f)
@Composable
private fun ComposerLargePreview() = TeachTheme(darkTheme = false) { ComposerPreviewContent("", ChatUiState()) }
