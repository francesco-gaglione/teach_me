package com.teachme.feature.chat

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.traversalIndex
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.TeachPill
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.ChatMessage
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.ChatRole
import com.teachme.core.model.MessagePart
import com.teachme.core.ui.LoadErrorState
import com.teachme.feature.mission.ConfirmState
import com.teachme.feature.mission.MissionChatContext
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import java.time.Instant

private val AT_BOTTOM_THRESHOLD = 80.dp
private val CONTEXT_PANE_MIN_WINDOW = 1100.dp
private const val COMPOSER_MAX_HEIGHT_FRACTION = 0.4f
private val KEY_STEP = 48.dp
private const val PILL_FADE_MS = 100

/** Horizontal geometry of the chat (chat.md 2): reading column, optional context pane, centred as a group. */
internal data class ChatGeometry(
    val columnStart: Dp,
    val columnWidth: Dp,
    val showContextPane: Boolean,
) {
    val paneStart: Dp get() = columnStart + columnWidth + 32.dp
}

internal fun chatGeometry(
    available: Dp,
    margin: Dp,
    readingMax: Dp,
    paneWidth: Dp,
    paneAllowed: Boolean,
): ChatGeometry {
    val column = minOf(readingMax, available - margin * 2)
    val pane = paneAllowed && available >= CONTEXT_PANE_MIN_WINDOW
    val group = if (pane) column + 32.dp + paneWidth else column
    return ChatGeometry(((available - group) / 2).coerceAtLeast(margin), column, pane)
}

/**
 * Stateless chat body: message list, composer and context pane. [header] is the Topic top bar and chip:
 * fixed above, or the first item of the list when [headerScrolls] (short windows, chat.md 2 and topic.md).
 */
@Composable
internal fun ChatContent(
    state: ChatUiState,
    topicTitle: String,
    draft: TextFieldValue,
    onDraftChange: (TextFieldValue) -> Unit,
    scrollToBottomSignal: Int,
    header: @Composable () -> Unit,
    headerScrolls: Boolean,
    copy: (String) -> Unit,
    onSend: () -> Unit,
    onRetryMessage: (String) -> Unit,
    onReconnectNow: () -> Unit,
    onLoadOlder: () -> Unit,
    onRetryLoad: () -> Unit,
    onDismissSendError: () -> Unit,
    onOpenSettings: () -> Unit,
    composerFocus: FocusRequester,
    mission: MissionChatContext,
    modifier: Modifier = Modifier,
    onSuggestionPicked: () -> Unit = {},
    composerHint: String? = null,
    onComposerFocusChange: (Boolean) -> Unit = {},
    lessonContext: ChatLessonContext? = null,
    onRemoveLessonContext: () -> Unit = {},
    visitLesson: ChatLessonContext? = null,
    onBackToLesson: (() -> Unit)? = null,
    lessonRequestCard: LessonRequestCardSlot? = null,
) {
    val widthClass = currentWindowWidthClass()
    var jumpToProposal by remember { mutableIntStateOf(0) }
    BoxWithConstraints(modifier.fillMaxSize()) {
        val geometry =
            chatGeometry(
                available = maxWidth,
                margin = screenMargin(widthClass),
                readingMax = TeachTheme.layout.readingMaxWidth,
                paneWidth = TeachTheme.layout.contextPaneWidth,
                paneAllowed = widthClass == WindowWidthClass.Expanded,
            )
        val listState = rememberLazyListState()
        val ready = state.load == ChatLoad.Ready
        Column(Modifier.fillMaxSize()) {
            if (!headerScrolls || !ready) header()
            Box(Modifier.weight(1f).fillMaxWidth()) {
                when (val load = state.load) {
                    is ChatLoad.Failed -> LoadErrorState(load.error, onRetryLoad, onOpenSettings)
                    ChatLoad.Loading ->
                        Column(
                            Modifier.padding(start = geometry.columnStart).width(geometry.columnWidth),
                        ) { ChatSkeleton() }
                    ChatLoad.Ready ->
                        MessageList(
                            state,
                            topicTitle,
                            geometry,
                            listState,
                            scrollToBottomSignal,
                            header.takeIf { headerScrolls },
                            copy,
                            onRetryMessage,
                            onReconnectNow,
                            onLoadOlder,
                            onSuggestion = { s ->
                                onDraftChange(TextFieldValue(s, TextRange(s.length)))
                                onSuggestionPicked()
                            },
                            showSuggestions = draft.text.isEmpty(),
                            mission = mission,
                            jumpToProposalSignal = jumpToProposal,
                            lessonRequestCard = lessonRequestCard,
                        )
                }
                if (geometry.showContextPane) {
                    ContextPane(
                        topicTitle,
                        mission,
                        onGoToProposal = { jumpToProposal++ },
                        lesson = (visitLesson ?: lessonContext)?.takeIf { onBackToLesson != null },
                        onBackToLesson = onBackToLesson,
                        modifier =
                            Modifier
                                .align(Alignment.TopStart)
                                .padding(start = geometry.paneStart, top = TeachTheme.spacing.md)
                                .semantics { traversalIndex = 10f },
                    )
                }
            }
            if (ready) {
                val maxField = this@BoxWithConstraints.maxHeight * COMPOSER_MAX_HEIGHT_FRACTION
                Column(
                    Modifier
                        .padding(
                            start = geometry.columnStart,
                            top = TeachTheme.spacing.xs,
                            bottom = TeachTheme.spacing.sm,
                        ).width(geometry.columnWidth),
                ) {
                    ChatComposer(
                        draft,
                        onDraftChange,
                        state,
                        maxField,
                        onSend,
                        onDismissSendError,
                        onOpenSettings,
                        focusRequester = composerFocus,
                        hint = composerHint ?: stringResource(R.string.chat_composer_hint),
                        suspendedReason =
                            if (mission.confirm is ConfirmState.Confirming) {
                                stringResource(R.string.chat_reason_confirming)
                            } else {
                                null
                            },
                        onFocusChange = onComposerFocusChange,
                        contextChip =
                            lessonContext?.let { context -> { LessonContextChip(context, onRemoveLessonContext) } },
                    )
                }
            }
        }
        state.announcement?.let { Announcer(it, topicTitle) }
    }
}

@Composable
private fun Announcer(
    announcement: Announcement,
    topicTitle: String,
) {
    val text =
        when (announcement.kind) {
            AnnouncementKind.Writing -> stringResource(R.string.chat_announce_writing)
            AnnouncementKind.Answered -> stringResource(R.string.chat_announce_answered)
            AnnouncementKind.Failed -> stringResource(R.string.chat_announce_failed)
            AnnouncementKind.MissionProposed -> stringResource(R.string.chat_announce_mission, topicTitle)
            AnnouncementKind.LessonRequested -> stringResource(R.string.chat_announce_lesson_request)
        }
    // Invisible, persistent live region; the id makes two equal announcements distinct.
    key(announcement.id) {
        Box(
            Modifier
                .size(1.dp)
                .semantics {
                    liveRegion = LiveRegionMode.Polite
                    contentDescription = text
                },
        )
    }
}

@Composable
private fun MessageList(
    state: ChatUiState,
    topicTitle: String,
    geometry: ChatGeometry,
    listState: LazyListState,
    scrollToBottomSignal: Int,
    scrollingHeader: (@Composable () -> Unit)?,
    copy: (String) -> Unit,
    onRetryMessage: (String) -> Unit,
    onReconnectNow: () -> Unit,
    onLoadOlder: () -> Unit,
    onSuggestion: (String) -> Unit,
    showSuggestions: Boolean,
    mission: MissionChatContext,
    jumpToProposalSignal: Int,
    lessonRequestCard: LessonRequestCardSlot?,
) {
    val rows = remember(state.messages) { buildChatRows(state.messages) }
    val announcers = remember(state.messages) { state.messages.lessonRequestAnnouncers() }
    val today = remember { today() }
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val thresholdPx = with(density) { AT_BOTTOM_THRESHOLD.toPx() }
    val stepPx = with(density) { KEY_STEP.toPx() }
    val atBottom by remember(listState) { derivedStateOf { listState.isAtBottom(thresholdPx) } }
    var pill by remember { mutableStateOf(false) }
    val headerCount = if (scrollingHeader != null) 1 else 0

    // Read in composition on purpose: it still reflects the layout from before this content change.
    val wasAtBottom = atBottom
    val last = state.messages.lastOrNull()
    // Only the last message matters: older messages prepended at the top must not raise the pill.
    val contentKey = lastMessageKey(state.messages)
    // Saveable: after a rotation the restored scroll position is kept instead of jumping to the bottom.
    var initialDone by rememberSaveable { mutableStateOf(false) }
    var handledSignal by remember { mutableIntStateOf(scrollToBottomSignal) }

    // 1. Opening: jump to the bottom, no animation.
    LaunchedEffect(Unit) {
        if (initialDone) return@LaunchedEffect
        snapshotFlow { listState.layoutInfo.totalItemsCount }.first { it > 0 }
        listState.scrollToBottom()
        initialDone = true
    }

    // 2. The user sent a message: always go to the bottom.
    LaunchedEffect(scrollToBottomSignal) {
        if (scrollToBottomSignal != handledSignal) {
            handledSignal = scrollToBottomSignal
            withFrameNanos { }
            listState.scrollToBottom()
            pill = false
        }
    }

    // 3. New content: follow the bottom only if already there and the streaming message is anchored.
    LaunchedEffect(contentKey) {
        if (!initialDone || last == null) return@LaunchedEffect
        val anchored = last.status != ChatMessageStatus.Streaming || listState.lastItemTopVisible()
        if (wasAtBottom && anchored) {
            listState.scrollToBottom()
            if (last.status == ChatMessageStatus.Streaming && !listState.lastItemTopVisible()) {
                // The message outgrew the screen: stay at its beginning and let the user read (segmentation).
                listState.scrollToItem(listState.layoutInfo.totalItemsCount - 1, 0)
                pill = true
            }
        } else if (!wasAtBottom) {
            pill = true
        }
    }

    // 3b. "Vai alla proposta" (context pane): bring the card of the open draft into view.
    var handledJump by remember { mutableIntStateOf(jumpToProposalSignal) }
    LaunchedEffect(jumpToProposalSignal) {
        if (jumpToProposalSignal == handledJump) return@LaunchedEffect
        handledJump = jumpToProposalSignal
        val draftVersion = mission.mission?.draft?.version ?: return@LaunchedEffect
        val index =
            rows.indexOfLast {
                it.message.missionProposal
                    ?.mission
                    ?.version == draftVersion
            }
        if (index >= 0) {
            val before = headerCount + if (state.olderLoading || state.olderFailed) 1 else 0
            listState.animateScrollToItem(before + index)
        }
    }

    // 4. The pill goes away by itself when the user reaches the bottom.
    LaunchedEffect(listState) {
        snapshotFlow { listState.isAtBottom(thresholdPx) }.collect { if (it) pill = false }
    }

    // 5. Window or composer size changed: someone at the bottom stays at the bottom.
    LaunchedEffect(listState) {
        var previousHeight = listState.layoutInfo.viewportSize.height
        var previousAtBottom = true
        snapshotFlow {
            listState.layoutInfo.viewportSize.height to listState.isAtBottom(thresholdPx)
        }.collect { (height, bottom) ->
            if (height != previousHeight) {
                previousHeight = height
                if (previousAtBottom) listState.scrollToBottom()
            } else {
                previousAtBottom = bottom
            }
        }
    }

    // 6. Older messages: load when the first visible item is among the first few.
    LaunchedEffect(listState, state.olderCursor, initialDone) {
        if (!initialDone) return@LaunchedEffect
        snapshotFlow { listState.firstVisibleItemIndex }.collect {
            if (it <= PREFETCH_OLDER_INDEX + headerCount &&
                state.olderCursor != null &&
                !state.olderFailed
            ) {
                onLoadOlder()
            }
        }
    }

    val listDescription = stringResource(R.string.chat_messages_list)
    Box(Modifier.fillMaxSize()) {
        LazyColumn(
            state = listState,
            modifier =
                Modifier
                    .fillMaxSize()
                    .semantics {
                        contentDescription = listDescription
                        isTraversalGroup = true
                    }.onKeyEvent { event ->
                        if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
                        val page = (listState.layoutInfo.viewportSize.height - stepPx).coerceAtLeast(stepPx)
                        when {
                            event.key == Key.DirectionDown -> scope.launch { listState.scrollBy(stepPx) }
                            event.key == Key.DirectionUp -> scope.launch { listState.scrollBy(-stepPx) }
                            event.key == Key.PageDown -> scope.launch { listState.scrollBy(page) }
                            event.key == Key.PageUp -> scope.launch { listState.scrollBy(-page) }
                            event.key == Key.MoveEnd && event.isCtrlPressed ->
                                scope.launch {
                                    listState.scrollToBottom()
                                }
                            else -> return@onKeyEvent false
                        }
                        true
                    }.focusable(),
            contentPadding = PaddingValues(vertical = TeachTheme.spacing.md),
        ) {
            if (scrollingHeader != null) item(key = "header") { scrollingHeader() }
            if (state.olderLoading || state.olderFailed) {
                item(key = "older") {
                    OlderMessagesRow(
                        state.olderFailed,
                        onLoadOlder,
                        Modifier.padding(start = geometry.columnStart).width(geometry.columnWidth),
                    )
                }
            }
            if (rows.isEmpty()) {
                item(key = "empty") {
                    ChatEmptyState(
                        topicTitle,
                        showSuggestions,
                        onSuggestion,
                        Modifier.padding(start = geometry.columnStart).width(geometry.columnWidth),
                    )
                }
            }
            items(rows, key = { it.message.id }) { row ->
                Column(Modifier.padding(start = geometry.columnStart).width(geometry.columnWidth)) {
                    ChatRowView(
                        row,
                        state,
                        today,
                        copy,
                        onRetryMessage,
                        onReconnectNow,
                        mission,
                        lessonRequestCard,
                        announcers,
                    )
                }
            }
        }
        AnimatedVisibility(
            visible = pill,
            modifier =
                Modifier
                    .align(
                        Alignment.BottomStart,
                    ).padding(start = geometry.columnStart, bottom = TeachTheme.spacing.sm)
                    .width(geometry.columnWidth),
            enter = fadeIn(tween(PILL_FADE_MS)),
            exit = fadeOut(tween(PILL_FADE_MS)),
        ) {
            Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                TeachPill(
                    stringResource(R.string.chat_pill_new),
                    TeachIcons.ArrowDown,
                    onClick = {
                        pill = false
                        scope.launch { listState.scrollToBottom() }
                    },
                )
            }
        }
    }
}

@Composable
private fun ChatRowView(
    row: ChatRow,
    state: ChatUiState,
    today: java.time.LocalDate,
    copy: (String) -> Unit,
    onRetryMessage: (String) -> Unit,
    onReconnectNow: () -> Unit,
    mission: MissionChatContext,
    lessonRequestCard: LessonRequestCardSlot?,
    announcers: Set<String>,
) {
    val spacing =
        when (row.gap) {
            RowGap.None -> 0.dp
            RowGap.Tight -> TeachTheme.layout.blockGapTight
            RowGap.Block -> TeachTheme.layout.blockGap
        }
    row.day?.let { DaySeparator(dayKind(it, today), it) }
    Box(Modifier.padding(top = if (row.day != null) 0.dp else spacing)) {
        val message = row.message
        when (message.role) {
            ChatRole.User ->
                UserMessage((message.parts.firstOrNull() as? MessagePart.Text)?.text.orEmpty(), copy)
            ChatRole.Assistant ->
                TutorMessage(
                    message = message,
                    showLabel = row.showLabel,
                    activity =
                        when (state.connection) {
                            ChatConnection.Online -> ActivityKind.Writing
                            ChatConnection.Reconnecting -> ActivityKind.Reconnecting
                            ChatConnection.Unreachable -> ActivityKind.Unreachable
                        },
                    canRetry = row.isLast && state.retryableMessageId == message.id,
                    retrying = state.retrying,
                    creditAgain = state.creditRepeats,
                    copy = copy,
                    onRetry = { onRetryMessage(message.id) },
                    onReconnectNow = onReconnectNow,
                    mission = mission,
                    lessonRequestCard = lessonRequestCard,
                    lessonRequestAnnounces = message.id in announcers,
                )
        }
    }
}

/** Identity and size of the last message: changes when a message is appended or the last one grows or finishes. */
internal data class LastMessageKey(
    val id: String?,
    val size: Int?,
)

internal fun lastMessageKey(messages: List<ChatMessage>): LastMessageKey {
    val last = messages.lastOrNull() ?: return LastMessageKey(null, null)
    return LastMessageKey(
        last.id,
        last.status.ordinal * 1_000_000 + last.parts.sumOf { (it as? MessagePart.Text)?.text?.length ?: 1 },
    )
}

private val previewNow = Instant.parse("2026-11-08T14:36:00Z")

private fun previewMsg(
    id: String,
    role: ChatRole,
    status: ChatMessageStatus,
    text: String?,
    offsetSec: Long,
) = ChatMessage(
    id,
    "t",
    role,
    status,
    if (text == null) emptyList() else listOf(MessagePart.Text(text)),
    null,
    previewNow.plusSeconds(offsetSec),
    previewNow.plusSeconds(offsetSec),
)

@Composable
private fun ChatPreview(state: ChatUiState) {
    Box(Modifier.background(TeachTheme.colors.bg)) {
        ChatContent(
            state = state,
            topicTitle = "Rust",
            draft = TextFieldValue(""),
            onDraftChange = {},
            scrollToBottomSignal = 0,
            header = {},
            headerScrolls = false,
            copy = {},
            onSend = {},
            onRetryMessage = {},
            onReconnectNow = {},
            onLoadOlder = {},
            onRetryLoad = {},
            onDismissSendError = {},
            onOpenSettings = {},
            composerFocus = remember { FocusRequester() },
            mission = previewMissionContext(),
        )
    }
}

private val previewConversation =
    listOf(
        previewMsg("1", ChatRole.Assistant, ChatMessageStatus.Complete, "Ciao! Perché vuoi imparare **Rust**?", 0),
        previewMsg("2", ChatRole.User, ChatMessageStatus.Complete, "Per lavoro", 10),
        previewMsg("3", ChatRole.Assistant, ChatMessageStatus.Streaming, "Bene. Che tipo di progetti", 20),
    )

@Preview(name = "Chat expanded light", showBackground = true, widthDp = 1280, heightDp = 700)
@Composable
private fun ChatExpandedPreview() =
    TeachTheme(darkTheme = false) {
        ChatPreview(ChatUiState(load = ChatLoad.Ready, messages = previewConversation))
    }

@Preview(name = "Chat expanded dark empty", showBackground = true, widthDp = 1280, heightDp = 700)
@Composable
private fun ChatEmptyDarkPreview() = TeachTheme(darkTheme = true) { ChatPreview(ChatUiState(load = ChatLoad.Ready)) }

@Preview(name = "Chat compact reconnecting", showBackground = true, widthDp = 360, heightDp = 640)
@Composable
private fun ChatCompactPreview() =
    TeachTheme(darkTheme = false) {
        ChatPreview(
            ChatUiState(
                load = ChatLoad.Ready,
                messages = previewConversation,
                connection = ChatConnection.Reconnecting,
            ),
        )
    }

@Preview(name = "Chat compact loading", showBackground = true, widthDp = 360, heightDp = 400)
@Composable
private fun ChatLoadingPreview() = TeachTheme(darkTheme = false) { ChatPreview(ChatUiState()) }
