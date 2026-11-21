package com.teachme.feature.chat

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ActionMenuItem
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachActionMenu
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.model.ChatMessage
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.MessagePart
import com.teachme.core.model.dayMonthFormatter
import com.teachme.core.model.llmKeyVariantOfCode
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyNotice
import com.teachme.feature.chat.markdown.MarkdownText
import com.teachme.feature.mission.MissionChatContext
import com.teachme.feature.mission.MissionProposalCard
import kotlinx.coroutines.delay
import java.util.Locale

private const val PULSE_CYCLE_MS = 1_600
private const val SLOW_HINT_MS = 20_000L
private const val PULSE_LOW = 0.45f
private const val FONT_SCALE_WIDE_BUBBLE = 1.3f
private const val BUBBLE_FRACTION = 0.8f

/**
 * Draws the card of a `lesson_request` part for a job id; the Boolean says whether this card is the one that announces
 * (the most recent of that job). Provided by the Topic screen, which owns the lessons state.
 */
typealias LessonRequestCardSlot = @Composable (jobId: String, announces: Boolean) -> Unit

/** What the activity line under a streaming message says. */
internal enum class ActivityKind { Writing, Reconnecting, Unreachable }

/** "Three dots + text" activity line (chat.md 6.1, 9.6). The dots are decorative; the text is always there. */
@Composable
internal fun TypingIndicator(
    kind: ActivityKind,
    waitingFirstToken: Boolean,
    onReconnectNow: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var slow by remember { mutableStateOf(false) }
    LaunchedEffect(waitingFirstToken) {
        slow = false
        if (waitingFirstToken) {
            delay(SLOW_HINT_MS)
            slow = true
        }
    }
    val colors = TeachTheme.colors
    Column(modifier, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
        when (kind) {
            ActivityKind.Writing -> {
                val pulse =
                    if (TeachTheme.reducedMotion) {
                        1f
                    } else {
                        rememberInfiniteTransition(label = "typing")
                            .animateFloat(
                                initialValue = PULSE_LOW,
                                targetValue = 1f,
                                animationSpec = infiniteRepeatable(tween(PULSE_CYCLE_MS / 2), RepeatMode.Reverse),
                                label = "typing-alpha",
                            ).value
                    }
                Row(
                    Modifier.heightIn(min = TeachTheme.spacing.lg),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                ) {
                    Row(
                        Modifier.alpha(pulse).clearAndSetSemantics { },
                        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
                    ) {
                        repeat(3) { Box(Modifier.size(6.dp).background(colors.inkMuted, CircleShape)) }
                    }
                    TeachText(
                        stringResource(if (slow) R.string.chat_typing_slow else R.string.chat_typing),
                        style = TeachTheme.typography.ui.bodyS,
                        color = colors.inkSoft,
                    )
                }
            }
            ActivityKind.Reconnecting ->
                Row(
                    Modifier.heightIn(min = TeachTheme.spacing.lg),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                ) {
                    TeachIcon(TeachIcons.WifiOff, null, tint = colors.inkSoft, size = TeachTheme.layout.iconSizeSmall)
                    TeachText(
                        stringResource(R.string.chat_connection_lost),
                        style = TeachTheme.typography.ui.bodyS,
                        color = colors.inkSoft,
                    )
                }
            ActivityKind.Unreachable -> {
                TeachText(
                    stringResource(R.string.chat_unreachable),
                    style = TeachTheme.typography.ui.bodyS,
                    color = colors.inkSoft,
                )
                TeachButton(stringResource(R.string.chat_reconnect_now), onReconnectNow, variant = ButtonVariant.Text)
            }
        }
    }
}

@Composable
internal fun DaySeparator(
    kind: DayKind,
    date: java.time.LocalDate,
    modifier: Modifier = Modifier,
) {
    val locale = androidx.compose.ui.text.intl.Locale.current.platformLocale
    val label =
        when (kind) {
            DayKind.Today -> stringResource(R.string.chat_day_today)
            DayKind.Yesterday -> stringResource(R.string.chat_day_yesterday)
            DayKind.ThisYear -> date.format(dayMonthFormatter(locale))
            DayKind.OtherYear -> date.format(dayMonthFormatter(locale, withYear = true))
        }
    TeachText(
        label,
        modifier.fillMaxWidth().padding(vertical = TeachTheme.layout.blockGap).semantics { heading() },
        style = TeachTheme.typography.ui.bodyS,
        color = TeachTheme.colors.inkMuted,
        textAlign = TextAlign.Center,
    )
}

/** Right-aligned bubble with the literal text the user wrote (chat.md 4.2). Long press opens "Copia". */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun UserMessage(
    text: String,
    copy: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var menu by rememberSaveable { mutableStateOf(false) }
    val fraction = if (LocalDensity.current.fontScale > FONT_SCALE_WIDE_BUBBLE) 1f else BUBBLE_FRACTION
    val description = stringResource(R.string.chat_user_description, text)
    val copyLabel = stringResource(R.string.chat_copy_action)
    Box(modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
        Box(Modifier.fillMaxWidth(fraction).wrapContentWidth(Alignment.End)) {
            TeachText(
                text,
                Modifier
                    .background(TeachTheme.colors.accentSoft, TeachTheme.shapes.md)
                    .combinedClickable(onClick = {}, onLongClick = { menu = true })
                    .padding(horizontal = TeachTheme.spacing.md, vertical = TeachTheme.spacing.sm)
                    .clearAndSetSemantics {
                        contentDescription = description
                        onLongClick(copyLabel) {
                            copy(text)
                            true
                        }
                        customActions =
                            listOf(
                                CustomAccessibilityAction(copyLabel) {
                                    copy(text)
                                    true
                                },
                            )
                    },
                style = TeachTheme.typography.ui.bodyL,
                color = TeachTheme.colors.ink,
            )
            TeachActionMenu(
                expanded = menu,
                onDismiss = { menu = false },
                title = copyLabel,
                items =
                    listOf(
                        ActionMenuItem(copyLabel, TeachIcons.Copy, {
                            menu = false
                            copy(text)
                        }),
                    ),
            )
        }
    }
}

/** Title and body of a failed message by `error.code` (chat.md 9.2). */
@Composable
private fun failureTexts(
    code: String?,
    hasPartialText: Boolean,
): Pair<String, String> {
    val title =
        stringResource(
            if (code == "rate_limited") R.string.chat_failed_title_rate else R.string.chat_failed_title_interrupted,
        )
    val body =
        stringResource(
            when (code) {
                "upstream_failed" -> R.string.chat_failed_body_upstream
                "rate_limited" -> R.string.chat_failed_body_rate
                "internal" -> R.string.chat_failed_body_internal
                else -> R.string.chat_failed_body_other
            },
        )
    return title to if (hasPartialText) "$body ${stringResource(R.string.chat_failed_partial)}" else body
}

/** Danger panel under the partial text of a failed message; "Riprova" only on the last message (chat.md 9.2). */
@Composable
internal fun ChatErrorPanel(
    code: String?,
    hasPartialText: Boolean,
    canRetry: Boolean,
    retrying: Boolean,
    onRetry: () -> Unit,
    identity: Any,
    modifier: Modifier = Modifier,
    creditAgain: Int = 0,
) {
    val keyVariant = code.llmKeyVariantOfCode()
    if (keyVariant != null) {
        // The key is the problem, not the answer (llm-key.md section 6). An older message cannot be retried: the panel
        // then only points at Settings.
        LlmKeyNotice(
            keyVariant,
            if (hasPartialText) LlmKeyContext.ChatReplyPartial else LlmKeyContext.ChatReply,
            identity,
            modifier,
            onRetry = if (canRetry) onRetry else null,
            retrying = retrying,
            again = creditAgain,
        )
        return
    }
    if (!canRetry) {
        TeachText(
            stringResource(R.string.chat_failed_old),
            modifier,
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkMuted,
        )
        return
    }
    val (title, body) = failureTexts(code, hasPartialText)
    TeachStatusPanel(StatusVariant.Danger, title, modifier, body = body) {
        TeachButton(
            text = stringResource(if (retrying) R.string.chat_retry_loading else R.string.chat_retry),
            onClick = onRetry,
            variant = ButtonVariant.Secondary,
            loading = retrying,
            leadingIcon = TeachIcons.RotateCcw,
        )
    }
}

/**
 * A message of the Tutor: label, parts in order (unknown parts skipped), activity line while streaming,
 * error panel when failed, copy footer when complete (chat.md 4.1).
 */
@Composable
internal fun TutorMessage(
    message: ChatMessage,
    showLabel: Boolean,
    activity: ActivityKind,
    canRetry: Boolean,
    retrying: Boolean,
    copy: (String) -> Unit,
    onRetry: () -> Unit,
    onReconnectNow: () -> Unit,
    modifier: Modifier = Modifier,
    mission: MissionChatContext? = null,
    creditAgain: Int = 0,
    lessonRequestCard: LessonRequestCardSlot? = null,
    lessonRequestAnnounces: Boolean = false,
) {
    val streaming = message.status == ChatMessageStatus.Streaming
    val streamingDescription = stringResource(R.string.chat_state_streaming)
    Column(
        modifier.fillMaxWidth().semantics { if (streaming) stateDescription = streamingDescription },
        verticalArrangement = Arrangement.spacedBy(TeachTheme.layout.blockGapTight),
    ) {
        if (showLabel) {
            TeachText(
                stringResource(R.string.chat_tutor_label),
                Modifier.semantics { heading() },
                style = TeachTheme.typography.ui.labelS,
                color = TeachTheme.colors.inkSoft,
            )
        }
        val texts = message.parts.filterIsInstance<MessagePart.Text>().filter { it.text.isNotBlank() }
        if (texts.isNotEmpty()) {
            SelectionContainer {
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.layout.blockGapTight)) {
                    texts.forEach { MarkdownText(it.text, copy) }
                }
            }
        } else if (!streaming && message.status == ChatMessageStatus.Complete && !message.hasVisibleParts) {
            TeachText(
                stringResource(R.string.chat_unknown_content),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkMuted,
            )
        }
        // Native parts arrive whole, after the text, and sit outside the selectable text (mission.md design 2).
        val proposal = message.missionProposal
        if (proposal != null && mission != null) MissionProposalCard(proposal, mission)
        val lessonRequest = message.lessonRequest
        if (lessonRequest != null && lessonRequestCard != null) {
            lessonRequestCard(lessonRequest.jobId, lessonRequestAnnounces)
        }
        if (streaming) {
            TypingIndicator(activity, waitingFirstToken = texts.isEmpty(), onReconnectNow = onReconnectNow)
        }
        if (message.status == ChatMessageStatus.Failed) {
            ChatErrorPanel(
                message.error?.code,
                texts.isNotEmpty(),
                canRetry,
                retrying,
                onRetry,
                message.id,
                creditAgain = creditAgain,
            )
        }
        if (message.status == ChatMessageStatus.Complete && message.plainText.isNotBlank()) {
            // 48 dp target, icon aligned with the text margin (chat.md 4.1).
            CopyButton(
                { message.plainText },
                stringResource(R.string.chat_copy_message),
                copy,
                Modifier.offset(x = (-12).dp),
            )
        }
    }
}

private fun previewMessage(
    id: String,
    status: ChatMessageStatus,
    text: String?,
    code: String? = null,
) = ChatMessage(
    id,
    "t",
    com.teachme.core.model.ChatRole.Assistant,
    status,
    if (text == null) emptyList() else listOf(MessagePart.Text(text)),
    code?.let {
        com.teachme.core.model
            .MessageError(it, "")
    },
    java.time.Instant.EPOCH,
    java.time.Instant.EPOCH,
)

@Composable
private fun MessagesPreviewContent() {
    Column(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.layout.blockGap),
    ) {
        UserMessage("Voglio imparare Rust per lavoro", {})
        TutorMessage(
            previewMessage("1", ChatMessageStatus.Complete, "Bene. Che tipo di **progetti** hai in mente?"),
            true,
            ActivityKind.Writing,
            false,
            false,
            {},
            {},
            {},
        )
        TutorMessage(
            previewMessage("2", ChatMessageStatus.Streaming, "Le ownership di Rust funzionano"),
            true,
            ActivityKind.Writing,
            false,
            false,
            {},
            {},
            {},
        )
        TutorMessage(
            previewMessage("3", ChatMessageStatus.Streaming, null),
            true,
            ActivityKind.Reconnecting,
            false,
            false,
            {},
            {},
            {},
        )
        TutorMessage(
            previewMessage("4", ChatMessageStatus.Failed, "Le ownership di Rust funz", "upstream_failed"),
            true,
            ActivityKind.Writing,
            true,
            false,
            {},
            {},
            {},
        )
    }
}

@Preview(name = "Messages light", showBackground = true, widthDp = 360, heightDp = 900)
@Composable
private fun MessagesLightPreview() = TeachTheme(darkTheme = false) { MessagesPreviewContent() }

@Preview(name = "Messages dark", showBackground = true, widthDp = 360, heightDp = 900)
@Composable
private fun MessagesDarkPreview() = TeachTheme(darkTheme = true) { MessagesPreviewContent() }

@Preview(name = "Messages font 2.0", showBackground = true, widthDp = 360, heightDp = 1400, fontScale = 2f)
@Composable
private fun MessagesLargePreview() = TeachTheme(darkTheme = false) { MessagesPreviewContent() }
