package com.teachme.feature.chat

import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.stringResource
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.components.rememberTextFieldValue
import com.teachme.feature.mission.ConfirmState
import com.teachme.feature.mission.MissionChatContext
import kotlinx.coroutines.launch

/** The ViewModel of the chat of [topicId]; the Topic screen also reads its state (turn in progress, Topic updates). */
@Composable
fun rememberChatViewModel(topicId: String): ChatViewModel =
    hiltViewModel<ChatViewModel, ChatViewModel.Factory>(key = "chat-$topicId") { it.create(topicId) }

/** Why the composer is asked to take the focus; each reason comes with its own placeholder. */
enum class ComposerFocusRequest {
    /** Card "Modifica": what to change in the mission. */
    EditMission,

    /** Invitation in the Mission tab: what the person already knows about the subject. */
    StartingPoint,
}

/**
 * The chat inside the Topic screen (docs/design/screens/chat.md). [header] (top bar and status chip)
 * scrolls with the list when [headerScrolls], otherwise it stays fixed above it.
 */
@Composable
fun ChatPane(
    topicTitle: String,
    viewModel: ChatViewModel,
    toast: TeachToastState,
    onOpenSettings: () -> Unit,
    header: @Composable () -> Unit,
    headerScrolls: Boolean,
    mission: MissionChatContext,
    modifier: Modifier = Modifier,
    onTopicMaybeGone: () -> Unit = {},
    /** The composer should take the focus and show a hint: "edit the mission" (card "Modifica") or the starting point. */
    focusComposerRequest: ComposerFocusRequest? = null,
    onFocusComposerHandled: () -> Unit = {},
    /** The lesson this chat was opened from: a chip shows above the composer until the message is sent or it is removed. */
    lessonContext: ChatLessonContext? = null,
    onLessonContextConsumed: () -> Unit = {},
    /** The lesson this visit started from: the context pane keeps its "Lezione" block after the chip is gone. */
    visitLesson: ChatLessonContext? = null,
    /** Expanded windows: "Torna alla lezione" in the context pane. */
    onBackToLesson: (() -> Unit)? = null,
    /** A question written by the lesson (`prompt_hint`): it goes into the composer, never sent by itself. */
    prefill: String? = null,
    onPrefillConsumed: () -> Unit = {},
    /** Draws the card of a `lesson_request` part; null where lessons are not available. */
    lessonRequestCard: LessonRequestCardSlot? = null,
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val resources = LocalResources.current
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    val keyboard = LocalSoftwareKeyboardController.current
    val copy = rememberCopyAction(toast)
    val currentOnTopicMaybeGone by androidx.compose.runtime.rememberUpdatedState(onTopicMaybeGone)
    // The draft survives rotation, class changes and navigation (rememberSaveable); it is cleared on send.
    var draft by rememberTextFieldValue()
    val focus = remember { FocusRequester() }
    var scrollSignal by remember { mutableIntStateOf(0) }

    // The stream's lifecycle (onStart/onStop) belongs to the Topic screen: this pane leaves the composition
    // when the Mission tab is shown, and the stream must go on.
    var composerHint by rememberSaveable { mutableStateOf<ComposerFocusRequest?>(null) }
    var editFocusSeen by remember { mutableStateOf(false) }
    val currentOnFocusHandled by androidx.compose.runtime.rememberUpdatedState(onFocusComposerHandled)
    LaunchedEffect(focusComposerRequest, state.load) {
        if (focusComposerRequest == null || state.load != ChatLoad.Ready) return@LaunchedEffect
        composerHint = focusComposerRequest
        editFocusSeen = false
        // The composer is part of the composition only once the history is loaded.
        withFrameNanos { }
        runCatching { focus.requestFocus() }
        keyboard?.show()
        currentOnFocusHandled()
    }

    // ask_agent.md: the composer gets the question with the cursor at the end; an existing draft is kept above it.
    val view = androidx.compose.ui.platform.LocalView.current
    val prefilledAnnouncement = stringResource(R.string.chat_prefilled_announce)
    val currentOnPrefillConsumed by androidx.compose.runtime.rememberUpdatedState(onPrefillConsumed)
    LaunchedEffect(prefill, state.load) {
        if (prefill == null || state.load != ChatLoad.Ready) return@LaunchedEffect
        val existing = draft.text
        val text = if (existing.isBlank()) prefill else existing.trimEnd() + "\n\n" + prefill
        draft =
            androidx.compose.ui.text.input
                .TextFieldValue(
                    text,
                    androidx.compose.ui.text
                        .TextRange(text.length),
                )
        // The composer is part of the composition only once the history is loaded.
        withFrameNanos { }
        runCatching { focus.requestFocus() }
        if (resources.configuration.keyboard == Configuration.KEYBOARD_NOKEYS) keyboard?.show()
        @Suppress("DEPRECATION")
        view.announceForAccessibility(prefilledAnnouncement)
        currentOnPrefillConsumed()
    }

    // A physical keyboard is attached: the composer takes the focus so the user can type at once.
    val hasHardwareKeyboard = resources.configuration.keyboard != Configuration.KEYBOARD_NOKEYS
    LaunchedEffect(state.load, hasHardwareKeyboard) {
        if (hasHardwareKeyboard &&
            state.load == ChatLoad.Ready &&
            state.messages.isEmpty()
        ) {
            runCatching { focus.requestFocus() }
        }
    }

    LaunchedEffect(viewModel) {
        viewModel.effects.collect { effect ->
            when (effect) {
                ChatEffect.Sent -> {
                    composerHint = null
                    onLessonContextConsumed()
                    draft =
                        androidx.compose.ui.text.input
                            .TextFieldValue("")
                    scrollSignal++
                }
                ChatEffect.StreamNotFound -> currentOnTopicMaybeGone()
                ChatEffect.ChatUpdatedToast ->
                    scope.launch {
                        toast.show(
                            resources.getString(R.string.chat_toast_updated),
                        )
                    }
                ChatEffect.RetryFailedToast ->
                    scope.launch { toast.show(resources.getString(R.string.chat_toast_retry_failed)) }
            }
        }
    }

    ChatContent(
        state = state,
        topicTitle = topicTitle,
        draft = draft,
        onDraftChange = { draft = it },
        scrollToBottomSignal = scrollSignal,
        header = header,
        headerScrolls = headerScrolls,
        copy = copy,
        onSend = {
            if (mission.confirm !is ConfirmState.Confirming) {
                viewModel.send(
                    draft.text,
                    lessonContext?.lessonId,
                )
            }
        },
        onRetryMessage = viewModel::retry,
        onReconnectNow = viewModel::reconnectNow,
        onLoadOlder = viewModel::loadOlder,
        onRetryLoad = viewModel::retryLoad,
        onDismissSendError = viewModel::dismissSendError,
        onOpenSettings = onOpenSettings,
        composerFocus = focus,
        lessonContext = lessonContext,
        visitLesson = visitLesson,
        onRemoveLessonContext = onLessonContextConsumed,
        onBackToLesson = onBackToLesson,
        mission = mission,
        lessonRequestCard = lessonRequestCard,
        composerHint =
            when (composerHint) {
                ComposerFocusRequest.EditMission -> stringResource(R.string.chat_composer_hint_mission)
                ComposerFocusRequest.StartingPoint -> stringResource(R.string.chat_composer_hint_starting_point)
                null -> null
            },
        onComposerFocusChange = { focused ->
            // The edit hint goes back to the normal one when the empty field loses the focus.
            if (focused) {
                editFocusSeen = true
            } else if (composerHint != null && editFocusSeen && draft.text.isEmpty()) {
                composerHint = null
            }
        },
        modifier = modifier,
        onSuggestionPicked = {
            focus.requestFocus()
            keyboard?.show()
        },
    )
}
