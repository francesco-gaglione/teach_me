package com.teachme.feature.lesson

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.Intent
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.TeachHandwriting
import com.teachme.core.designsystem.TeachHighlight
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachBottomSheet
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachScreen
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachToastHost
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.gradeBlocker
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyHint
import com.teachme.core.ui.LlmKeyNotice
import com.teachme.core.ui.LocalLlmKeyStatus
import com.teachme.core.ui.LocalOpenLlmKeySettings
import com.teachme.sdui.highlight.LocalHighlightHost
import com.teachme.sdui.highlight.inkNotesA11y
import com.teachme.sdui.ink.InkEdit
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.LessonProgress
import com.teachme.sdui.logic.SummaryBuilder
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.Heading
import com.teachme.sdui.model.Paragraph
import com.teachme.sdui.render.BlockGap
import com.teachme.sdui.render.DefaultBlockRegistry
import com.teachme.sdui.render.ExerciseStates
import com.teachme.sdui.render.GlossarySheet
import com.teachme.sdui.render.LessonLinkTarget
import com.teachme.sdui.render.LocalSdui
import com.teachme.sdui.render.SduiEnvironment
import com.teachme.sdui.render.SduiHost
import com.teachme.sdui.render.SourceSheet
import com.teachme.sdui.render.dp
import com.teachme.sdui.render.rememberHardwareKeyboard
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.launch

private val CONTEXT_PANE_TOP = 24.dp
private val COLUMN_BOTTOM_PADDING = 96.dp
private val REVEAL_SLIDE = 24.dp
private const val SCROLL_DEBOUNCE_MS = 500L
private const val REVEAL_CLEAR_MS = 450L
private const val SLOW_LOADING_MS = 8_000L

/**
 * The lesson screen (docs/design/screens/lesson.md). [onAskAgent] opens the Tutor chat with the composer
 * prefilled; null means "not wired yet" and the screen says so instead of doing nothing.
 */
@Composable
fun LessonRoute(
    onClose: () -> Unit,
    onAskAgent: ((LessonAskRequest) -> Unit)?,
    modifier: Modifier = Modifier,
    onOpenLesson: (String) -> Unit = {},
    viewModel: LessonViewModel = hiltViewModel(),
    audioViewModel: LessonAudioViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val audioState by audioViewModel.state.collectAsStateWithLifecycle()
    val audioControls = remember(audioState, audioViewModel) { LessonAudioControls(audioState, audioViewModel::toggle) }
    // Reading stops when the app goes to the background and when the screen leaves (audio.md section 6).
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) {
        audioViewModel.stop()
        audioViewModel.onStopped()
    }
    LifecycleEventEffect(Lifecycle.Event.ON_START) { audioViewModel.onStarted() }
    DisposableEffect(audioViewModel) { onDispose { audioViewModel.stop() } }
    val toast = remember { TeachToastState() }
    val palette = remember { HighlightPaletteState() }
    palette.Run()
    val toastScope = rememberCoroutineScope()
    val close = {
        viewModel.flushState()
        onClose()
    }
    BackHandler(onBack = close)
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { viewModel.flushState() }
    // The highlights leave now, without waiting for the debounce, when the app is paused and when the screen is
    // disposed (closing and leaving inside the app never pause the activity). A refused set is not sent again by
    // these: only a new edit, opening the lesson and "Riprova ora" do. highlight.md section 11.
    LifecycleEventEffect(Lifecycle.Event.ON_PAUSE) {
        viewModel.syncHighlightsNow()
        viewModel.syncInkNow()
    }
    // Back in the foreground: send what is queued, grade pending answers once, follow the closing job.
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { viewModel.onResume() }
    val tutorUnavailable = stringResource(R.string.lesson_tutor_unavailable)
    val ask: (String) -> Unit =
        remember(onAskAgent, tutorUnavailable, viewModel) {
            { prompt ->
                val request = viewModel.askRequest(prompt)
                if (onAskAgent != null && request != null) {
                    onAskAgent(request)
                } else {
                    toastScope.launch { toast.show(tutorUnavailable) }
                }
            }
        }
    // While the palette is on screen the toasts make room for it (highlight.md section 6).
    val toastLift = if (palette.visible) TeachHighlight.paletteToastLift else 0.dp
    TeachScreen(
        modifier = modifier,
        toast = { TeachToastHost(toast, Modifier.padding(bottom = toastLift)) },
    ) {
        when (val s = state) {
            LessonUiState.Loading -> LoadingContent(close)
            LessonUiState.LoadFailed ->
                MessageContent(
                    close,
                    StatusVariant.Danger,
                    R.string.lesson_load_failed_title,
                    R.string.lesson_load_failed_body,
                    R.string.lesson_retry,
                    viewModel::retryLoad,
                    null,
                )
            LessonUiState.Gone ->
                MessageContent(
                    close,
                    StatusVariant.Info,
                    R.string.lesson_gone_title,
                    null,
                    R.string.lesson_back_to_topic,
                    close,
                    null,
                )
            LessonUiState.Unusable ->
                MessageContent(
                    close,
                    StatusVariant.Info,
                    R.string.lesson_unusable_title,
                    null,
                    R.string.lesson_back_to_topic,
                    close,
                    { ask("") },
                )
            is LessonUiState.Ready ->
                CompositionLocalProvider(LocalLessonAudio provides audioControls) {
                    LessonContent(s, viewModel, audioViewModel, toast, palette, close, ask, onOpenLesson)
                }
        }
    }
}

@Composable
private fun LoadingContent(onClose: () -> Unit) {
    var slow by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(SLOW_LOADING_MS)
        slow = true
    }
    Column(Modifier.fillMaxSize()) {
        LessonTopBar("", null, 0f, showIndexButton = false, onClose = onClose, onIndex = {})
        Column(
            Modifier.padding(screenMargin()),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            TeachStatusPanel(
                StatusVariant.Progress,
                stringResource(if (slow) R.string.lesson_loading_slow else R.string.lesson_loading),
            )
        }
    }
}

@Composable
private fun MessageContent(
    onClose: () -> Unit,
    variant: StatusVariant,
    title: Int,
    body: Int?,
    action: Int,
    onAction: () -> Unit,
    onAsk: (() -> Unit)?,
) {
    Column(Modifier.fillMaxSize()) {
        LessonTopBar("", null, 0f, showIndexButton = false, onClose = onClose, onIndex = {})
        Column(
            Modifier.padding(screenMargin()).fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
        ) {
            TeachStatusPanel(
                variant = variant,
                title = stringResource(title),
                body = body?.let { stringResource(it) },
                icon = if (variant == StatusVariant.Danger) TeachIcons.CloudOff else null,
            )
            TeachButton(stringResource(action), onAction, Modifier.fillMaxWidth())
            if (onAsk != null) {
                TeachButton(
                    stringResource(R.string.lesson_ask_tutor),
                    onAsk,
                    Modifier.fillMaxWidth(),
                    variant = ButtonVariant.Secondary,
                )
            }
        }
    }
}

/** Sheet currently on screen, kept as a string so it survives rotation. */
private object SheetCode {
    const val NONE = ""
    const val INDEX = "index"

    fun term(key: String) = "term:$key"

    fun source(key: String) = "source:$key"
}

@OptIn(ExperimentalFoundationApi::class, kotlinx.coroutines.FlowPreview::class)
@Composable
private fun LessonContent(
    ready: LessonUiState.Ready,
    vm: LessonViewModel,
    audioVm: LessonAudioViewModel,
    toast: TeachToastState,
    palette: HighlightPaletteState,
    onClose: () -> Unit,
    onAsk: (String) -> Unit,
    onOpenLesson: (String) -> Unit,
) {
    val context = LocalContext.current
    val clipboard = LocalClipboard.current
    val view = LocalView.current
    val scope = rememberCoroutineScope()
    val layout = TeachTheme.layout
    val density = LocalDensity.current
    val latest = rememberUpdatedState(ready)
    var sheet by rememberSaveable { mutableStateOf(SheetCode.NONE) }
    val copiedToast = stringResource(R.string.lesson_copied_toast)
    val announceTemplate = stringResource(R.string.lesson_section_announce)
    val hardwareKeyboard = rememberHardwareKeyboard()
    val reducedMotion = TeachTheme.reducedMotion

    val sectionOfBlock =
        remember(ready.sections) {
            ready.sections.sections
                .flatMap { s -> s.items.map { it.id to s.index } }
                .toMap()
        }
    val states =
        remember(vm) {
            object : ExerciseStates {
                override fun get(blockId: String): ExerciseState = latest.value.exercise(blockId)

                override fun update(
                    blockId: String,
                    transform: (ExerciseState) -> ExerciseState,
                ) = vm.updateExercise(blockId, transform)

                override fun isLocked(blockId: String): Boolean {
                    val section = sectionOfBlock[blockId] ?: return false
                    return LessonProgress.isLocked(section, latest.value.saved.revealedCount)
                }
            }
        }
    val highlightLimit = stringResource(R.string.lesson_highlight_limit)
    val highlightRemoved = stringResource(R.string.lesson_highlight_removed)
    val highlightUndo = stringResource(R.string.lesson_highlight_undo)
    val notesCleared = stringResource(R.string.lesson_ink_cleared)
    val inkLimit = stringResource(R.string.lesson_ink_limit)
    val inkUndone = stringResource(R.string.lesson_ink_undone)
    val inkRedone = stringResource(R.string.lesson_ink_redone)
    val toastJob = remember { androidx.compose.runtime.mutableStateOf<kotlinx.coroutines.Job?>(null) }
    val showToast =
        remember(toast, scope) {
            { message: String, action: String?, onAction: (() -> Unit)? ->
                // A new toast replaces the old one: its timer must not hide the new message early.
                toastJob.value?.cancel()
                toastJob.value = scope.launch { toast.show(message, action, onAction) }
                Unit
            }
        }
    val eraserOnSpoken = stringResource(R.string.lesson_eraser_on_spoken)
    val eraserOffSpoken = stringResource(R.string.lesson_eraser_off_spoken)
    val announcer = remember { PoliteAnnouncer() }
    announcer.Run()
    // Undo and Redo of the notes (palette buttons and keyboard): a limit says so, a change is announced to TalkBack.
    val inkStep =
        rememberUpdatedState { edit: InkEdit, spoken: String ->
            when (edit) {
                is InkEdit.Applied -> announcer.say(spoken)
                is InkEdit.Rejected -> showToast(inkLimit, null, null)
                InkEdit.NoOp -> Unit
            }
        }
    val undoInk = { inkStep.value(vm.undoInk(), inkUndone) }
    val redoInk = { inkStep.value(vm.redoInk(), inkRedone) }
    val highlightHost =
        remember(vm) {
            LessonHighlightHost(
                state = { latest.value },
                vm = vm,
                showToast = showToast,
                limitMessage = { highlightLimit },
                removedMessage = { highlightRemoved },
                undoLabel = { highlightUndo },
                notesClearedMessage = { notesCleared },
                penSeen = { palette.penSeen },
            )
        }
    val openKey by rememberUpdatedState(LocalOpenLlmKeySettings.current)
    // The answer panels explain a missing or refused key and offer it (llm-key.md section 5.1).
    val keyProblem = LocalLlmKeyStatus.current.gradeBlocker
    val host =
        remember(vm, context) {
            object : SduiHost {
                override fun openTerm(key: String) {
                    vm.termOpened(key)
                    sheet = SheetCode.term(key)
                }

                override fun openSource(key: String) {
                    sheet = SheetCode.source(key)
                }

                override fun openUrl(url: String) {
                    openInBrowser(context, url)
                }

                override fun askAgent(promptHint: String) = onAsk(promptHint)

                override fun openLesson(lessonId: String) = onOpenLesson(lessonId)

                // A lesson of the backend links to a lesson of its own Topic; a sample lesson links nowhere.
                override fun lessonLinkTarget(lessonId: String): LessonLinkTarget? =
                    if (latest.value.lesson != null) LessonLinkTarget(null) else null

                override fun reevaluate(blockId: String) = vm.reevaluate(blockId)

                override fun openKeySettings(variant: LlmKeyVariant) = openKey(variant)

                override fun copyText(text: String) {
                    scope.launch {
                        clipboard.setClipEntry(ClipEntry(ClipData.newPlainText(null, text)))
                        // Android 13+ confirms the copy by itself.
                        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) toast.show(copiedToast)
                    }
                }
            }
        }
    val environment =
        remember(ready.document, host, states, hardwareKeyboard, ready.readOnly, keyProblem, ready.contentLanguage) {
            SduiEnvironment(
                ready.document,
                host,
                states,
                hardwareKeyboard,
                readOnly = ready.readOnly,
                keyProblem = keyProblem,
                contentLanguage = ready.contentLanguage,
            )
        }

    val items =
        remember(ready.document, ready.sections, ready.saved.revealedCount, ready.readOnly, ready.qualityWarnings) {
            buildLessonItems(
                ready.document,
                ready.sections,
                ready.saved.revealedCount,
                showContinue = !ready.readOnly,
                showQualityNote = ready.qualityWarnings.isNotEmpty() && ready.isLastSection,
            )
        }
    val listState = rememberLazyListState()
    val latestItems = rememberUpdatedState(items)
    val columnFocus = remember { FocusRequester() }
    val paneFocus = remember { FocusRequester() }
    var paneHasFocus by remember { mutableStateOf(false) }
    val scrollGap = with(density) { CONTEXT_PANE_TOP.roundToPx() }
    var pendingScrollKey by remember { mutableStateOf<String?>(null) }

    fun scrollToKey(key: String) {
        val index = indexOfKey(items, key)
        if (index >= 0) {
            scope.launch {
                if (reducedMotion) {
                    listState.scrollToItem(
                        index,
                        -scrollGap,
                    )
                } else {
                    listState.animateScrollToItem(index, -scrollGap)
                }
            }
        }
    }

    // "Vai al blocco" of the quality note: scroll the block to 24 dp from the top, then give it the focus.
    val qualityNote =
        remember(ready.qualityWarnings, items) {
            buildQualityNote(ready.qualityWarnings) { id -> indexOfKey(items, blockKey(id)) >= 0 }
        }
    val qualityFocus = remember { FocusRequester() }
    var qualityFocusKey by remember { mutableStateOf<String?>(null) }

    fun goToBlock(id: String) {
        val key = blockKey(id)
        val index = indexOfKey(items, key)
        if (index < 0) return
        scope.launch {
            if (reducedMotion) {
                listState.scrollToItem(
                    index,
                    -scrollGap,
                )
            } else {
                listState.animateScrollToItem(index, -scrollGap)
            }
            qualityFocusKey = key
            withFrameNanos { }
            runCatching { qualityFocus.requestFocus() }
        }
    }

    val kindNames = blockKindNames()

    fun blockLabel(id: String): String {
        val block = (items.firstOrNull { it.key == blockKey(id) } as? LessonItem.BlockItem)?.block
        return (block as? Heading)?.text ?: (kindNames[block?.type] ?: kindNames.getValue(null))
    }

    // Leaving the screen (close, background, dispose) stores the exact reading position first.
    fun rememberPosition() {
        val index = listState.firstVisibleItemIndex
        vm.saveScroll(latestItems.value.getOrNull(index)?.key, listState.firstVisibleItemScrollOffset)
    }
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) {
        rememberPosition()
        vm.flushState()
    }
    DisposableEffect(Unit) {
        onDispose {
            rememberPosition()
            vm.flushState()
            vm.syncHighlightsNow()
            vm.syncInkNow()
        }
    }
    // A hardware keyboard needs focus inside the lesson from the start, or Ctrl+Enter/F6 go nowhere.
    LaunchedEffect(Unit) { runCatching { columnFocus.requestFocus() } }
    // Restore the reading position once, when the lesson first appears.
    LaunchedEffect(Unit) {
        val saved = latest.value.saved
        val index = saved.scrollBlockId?.let { indexOfKey(items, it) } ?: -1
        if (index >= 0) listState.scrollToItem(index, saved.scrollOffsetPx)
    }
    LaunchedEffect(listState) {
        snapshotFlow { listState.firstVisibleItemIndex to listState.firstVisibleItemScrollOffset }
            .debounce(SCROLL_DEBOUNCE_MS)
            .collect { (index, offset) -> vm.saveScroll(latestItems.value.getOrNull(index)?.key, offset) }
    }

    // A new section: scroll its title under the bar, announce it, then forget the animation flag.
    LaunchedEffect(ready.justRevealed) {
        val revealed = ready.justRevealed ?: return@LaunchedEffect
        val section = ready.sections.sections.getOrNull(revealed)
        val first = firstItemOfSection(items, revealed)
        if (first >= 0) {
            if (reducedMotion) {
                listState.scrollToItem(
                    first,
                    -scrollGap,
                )
            } else {
                listState.animateScrollToItem(first, -scrollGap)
            }
        }
        if (section?.number != null) {
            @Suppress("DEPRECATION")
            view.announceForAccessibility(
                String.format(
                    announceTemplate,
                    section.number,
                    ready.sections.numberedCount,
                    section.title.orEmpty(),
                ),
            )
        }
        delay(REVEAL_CLEAR_MS)
        vm.animationShown()
    }
    LaunchedEffect(pendingScrollKey, ready.showSummary) {
        val key = pendingScrollKey
        if (key != null && !ready.showSummary) {
            scrollToKey(key)
            pendingScrollKey = null
        }
    }

    val audio = LocalLessonAudio.current.state
    val stoppedAtExercise = stringResource(R.string.lesson_audio_stopped_at_exercise)
    val playbackFailed = stringResource(R.string.lesson_audio_playback_failed)
    val slots = { readingSlots(latestItems.value) { latest.value.exercise(it).done } }

    // Continuous reading: bring a block into view only when it is not fully visible (lesson-audio.md section 3).
    fun revealIfHidden(key: String) {
        val info = listState.layoutInfo
        val row = info.visibleItemsInfo.firstOrNull { it.key == key }
        if (row == null ||
            !isFullyVisible(row.offset, row.size, info.viewportStartOffset, info.viewportEndOffset)
        ) {
            scrollToKey(key)
        }
    }
    val reveal by rememberUpdatedState(::revealIfHidden)
    // A manual scroll takes the page back until this reading ends; the bar `accent` still shows where the voice is.
    var autoScrollSuspended by remember { mutableStateOf(false) }
    LaunchedEffect(audio.continuous) { if (!audio.continuous) autoScrollSuspended = false }
    LaunchedEffect(listState) {
        listState.interactionSource.interactions.collect {
            if (it is DragInteraction.Start && audioVm.state.value.continuous) autoScrollSuspended = true
        }
    }
    LaunchedEffect(audioVm) {
        audioVm.events.collect { event ->
            when (event) {
                is LessonAudioEvent.Reading -> if (!autoScrollSuspended) reveal(blockKey(event.blockId))
                // The reading ends here: the page goes to the exercise even if the user scrolled by hand before.
                is LessonAudioEvent.StoppedAtExercise -> {
                    reveal(blockKey(event.blockId))
                    scope.launch { toast.show(stoppedAtExercise) }
                }
                LessonAudioEvent.PlaybackFailed -> scope.launch { toast.show(playbackFailed) }
                LessonAudioEvent.Finished ->
                    latestItems.value.lastOrNull { it is LessonItem.Continue }?.let { reveal(it.key) }
            }
        }
    }
    val listenAction =
        if (audio.available && !audio.continuous) {
            LessonListenAction(audio.canListenToLesson) {
                val startId = firstSlotAtOrAfter(latestItems.value, slots(), listState.firstVisibleItemIndex)
                if (startId != null) audioVm.listenToLesson(startId, slots)
            }
        } else {
            null
        }

    val current = ready.sections.sections[ready.currentIndex]
    val missing = LessonProgress.missing(current) { ready.exercise(it) }
    val onContinue = { vm.continueLesson() }
    val onLockedTap = {
        missing.firstOrNull()?.let { scrollToKey(blockKey(it.id)) }
        Unit
    }
    val subtitle =
        when {
            ready.readOnly -> stringResource(R.string.lesson_subtitle_completed)
            !ready.sections.showsProgress -> null
            current.number == null -> stringResource(R.string.lesson_subtitle_intro)
            else -> stringResource(R.string.lesson_subtitle_section, current.number!!, ready.sections.numberedCount)
        }
    val progress = LessonProgress.value(ready.sections, current, { ready.exercise(it) }, ready.saved.completed)
    val title =
        ready.document.title.ifBlank {
            (ready.document.blocks.firstOrNull() as? com.teachme.sdui.model.Heading)?.text.orEmpty()
        }

    var topicTerm by remember { mutableStateOf<GlossaryTerm?>(null) }
    val queueIndicator: (@Composable () -> Unit)? =
        if (ready.lesson != null) {
            // A completed lesson has no queued answers, but its highlights can still be waiting to be sent.
            {
                QueueIndicator(
                    queued = if (ready.readOnly) 0 else ready.queued,
                    // Also sends the highlights now.
                    onRetryNow = vm::retryQueueNow,
                    highlightsFailing = ready.highlightSyncFailing,
                    notesFailing = ready.inkSyncFailing,
                    notesTooLarge = ready.inkTooLarge,
                )
            }
        } else {
            null
        }
    androidx.compose.runtime.CompositionLocalProvider(
        LocalSdui provides environment,
        LocalHighlightHost provides highlightHost,
    ) {
        BoxWithConstraints(
            Modifier
                .fillMaxSize()
                .penWatcher(palette::onPenEvent)
                .onPreviewKeyEvent { e ->
                    when {
                        e.type == KeyEventType.KeyDown && e.key == Key.F6 -> {
                            if (paneHasFocus) columnFocus.requestFocus() else paneFocus.requestFocus()
                            true
                        }
                        e.type == KeyEventType.KeyDown && e.isAltPressed && e.key == Key.DirectionLeft -> {
                            onClose()
                            true
                        }
                        // Outside an exercise Ctrl+Enter is "Continua"; inside, the exercise card takes it.
                        // Preview phase, so a focused button does not turn it into a plain click.
                        e.type == KeyEventType.KeyDown &&
                            e.isCtrlPressed &&
                            (e.key == Key.Enter || e.key == Key.NumPadEnter) &&
                            !environment.focusInExercise -> {
                            if (missing.isEmpty()) onContinue() else onLockedTap()
                            true
                        }
                        else -> false
                    }
                }
                // Not in the preview pass: a text field that is focused takes Ctrl+Z for its own text first, and only
                // what nobody handled reaches the notes (handwriting.md section 6.2).
                .onKeyEvent { e ->
                    when (inkShortcut(e)) {
                        InkShortcut.UNDO -> {
                            undoInk()
                            true
                        }
                        InkShortcut.REDO -> {
                            redoInk()
                            true
                        }
                        null -> false
                    }
                },
        ) {
            val wide = maxWidth >= layout.contextPaneBreakpoint
            val margin = screenMargin()
            val columnWidth: Dp =
                if (wide) {
                    layout.readingMaxWidth
                } else {
                    minOf(
                        layout.readingMaxWidth,
                        maxWidth - margin * 2,
                    )
                }
            val paneGroupWidth = layout.readingMaxWidth + layout.contextPaneGap + layout.contextPaneWidth
            val groupWidth = if (wide) paneGroupWidth else columnWidth
            val startPad = ((maxWidth - groupWidth) / 2).coerceAtLeast(margin)
            val endPad = (maxWidth - startPad - columnWidth).coerceAtLeast(0.dp)

            if (ready.showSummary) {
                Column(Modifier.fillMaxSize()) {
                    LessonTopBar(title, null, 1f, showIndexButton = false, onClose = onClose, onIndex = {})
                    val summary =
                        remember(ready.sections, ready.saved) {
                            SummaryBuilder.build(
                                ready.sections,
                                { ready.exercise(it) },
                            ) { ready.document.glossaryEntry(it)?.term }
                        }
                    val backToTopic = {
                        vm.requestLessonsTab()
                        onClose()
                    }
                    val askDone = stringResource(R.string.lesson_summary_ask_done, title)
                    val askEmpty = stringResource(R.string.lesson_summary_ask_empty, title)
                    LessonSummaryView(
                        objective = ready.document.objective,
                        pdfTitle =
                            ready.document.sources
                                .firstOrNull { it.isUserPdf }
                                ?.title,
                        pdfMore = (ready.document.sources.count { it.isUserPdf } - 1).coerceAtLeast(0),
                        summary = summary,
                        completion = ready.completion,
                        closedElsewhere = ready.closedElsewhere,
                        onBackToTopic = backToTopic,
                        onAskTutor = { empty -> onAsk(if (empty) askEmpty else askDone) },
                        onReview = {
                            pendingScrollKey = blockKey(it)
                            vm.dismissSummary()
                        },
                        onReread = vm::dismissSummary,
                        onOpenTerm = { topicTerm = it },
                        onSeeRegister = {
                            vm.requestProgressTab()
                            onClose()
                        },
                        onRetrySend = vm::retryCompletion,
                        onRetryJob = vm::retryCompletionJob,
                    )
                }
            } else {
                Column(Modifier.fillMaxSize()) {
                    LessonTopBar(
                        title = title,
                        subtitle = subtitle,
                        progress = progress,
                        showIndexButton = !wide,
                        onClose = onClose,
                        onIndex = { sheet = SheetCode.INDEX },
                        queueIndicator = queueIndicator,
                        listen = listenAction,
                    )
                    AnimatedVisibility(
                        visible = audio.continuous,
                        enter = fadeIn(tween(if (reducedMotion) 0 else TeachTheme.motion.baseMs)),
                        exit = fadeOut(tween(if (reducedMotion) 0 else TeachTheme.motion.fastMs)),
                    ) {
                        AudioStrip(
                            speed = audio.speed,
                            onStop = audioVm::stop,
                            onCycleSpeed = audioVm::cycleSpeed,
                            contentPadding = PaddingValues(start = startPad, end = endPad),
                        )
                    }
                    val ink =
                        rememberInkSurface(
                            ready = ready,
                            vm = vm,
                            listState = listState,
                            items = items,
                            audio = audio,
                            left = with(density) { startPad.toPx() },
                            width = with(density) { columnWidth.toPx() },
                            onLimit = { showToast(inkLimit, null, null) },
                            areaRight =
                                if (wide) {
                                    with(density) { (startPad + layout.readingMaxWidth + layout.contextPaneGap).toPx() }
                                } else {
                                    Float.MAX_VALUE
                                },
                        )
                    Box(Modifier.weight(1f).fillMaxWidth().inkInput(ink)) {
                        LazyColumn(
                            state = listState,
                            modifier = Modifier.fillMaxSize().focusRequester(columnFocus).focusGroup(),
                            contentPadding =
                                PaddingValues(
                                    start = startPad,
                                    end = endPad,
                                    top = CONTEXT_PANE_TOP,
                                    bottom = COLUMN_BOTTOM_PADDING,
                                ),
                        ) {
                            items(items.size, key = { items[it].key }, contentType = { items[it]::class }) { index ->
                                val item = items[index]
                                // A control row below the previous block takes 16 dp of the gap (lesson-audio.md section 1).
                                when (item) {
                                    LessonItem.NewerCatalogBanner ->
                                        TeachStatusPanel(
                                            StatusVariant.Info,
                                            stringResource(R.string.lesson_newer_catalog_title),
                                            body = stringResource(R.string.lesson_newer_catalog_body),
                                            modifier = Modifier.padding(bottom = TeachTheme.layout.blockGap),
                                        )
                                    is LessonItem.Objective -> ObjectiveLine(item)
                                    is LessonItem.BlockItem ->
                                        Box(
                                            Modifier
                                                .padding(top = blockTopPadding(items, index, audio))
                                                .sectionReveal(ready.justRevealed == item.sectionIndex)
                                                .then(
                                                    if (qualityFocusKey == item.key) {
                                                        Modifier.focusRequester(qualityFocus).focusable()
                                                    } else {
                                                        Modifier
                                                    },
                                                )
                                                // The text blocks say their notes together with their highlights.
                                                .then(
                                                    if (item.block is Paragraph || item.block is Callout) {
                                                        Modifier
                                                    } else {
                                                        Modifier.inkNotesA11y(item.block.id)
                                                    },
                                                ),
                                        ) { AudioListenable(item.block) { DefaultBlockRegistry.Render(item.block) } }
                                    LessonItem.QualityNote ->
                                        QualityNoteCard(qualityNote, ::blockLabel, ::goToBlock)
                                    is LessonItem.Continue ->
                                        Column(
                                            Modifier.padding(top = BlockGap.Block.dp()),
                                            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
                                        ) {
                                            // The refusal of "Completa lezione" (blocking), or the proactive suggestion.
                                            val notice = ready.keyNotice
                                            if (notice != null) {
                                                LlmKeyNotice(
                                                    notice.variant,
                                                    LlmKeyContext.CompleteLesson,
                                                    notice.id,
                                                    onRetry = onContinue,
                                                    retrying = ready.completing,
                                                    again = notice.again,
                                                )
                                            } else if (ready.isLastSection && missing.isEmpty()) {
                                                LlmKeyHint(LlmKeyContext.CompleteLessonHint)
                                            }
                                            ContinueBar(
                                                isLast = ready.isLastSection,
                                                missing = missing.size,
                                                busy = ready.completing,
                                                onContinue = onContinue,
                                                onLockedTap = onLockedTap,
                                            )
                                        }
                                }
                            }
                        }
                        if (wide) {
                            Column(
                                Modifier
                                    .align(Alignment.TopStart)
                                    .padding(
                                        start = startPad + layout.readingMaxWidth + layout.contextPaneGap,
                                        top = CONTEXT_PANE_TOP,
                                        bottom = CONTEXT_PANE_TOP,
                                    ).width(layout.contextPaneWidth)
                                    .fillMaxHeight()
                                    .focusRequester(paneFocus)
                                    .onFocusChanged { paneHasFocus = it.hasFocus }
                                    .focusGroup()
                                    .verticalScroll(rememberScrollState()),
                            ) {
                                LessonContextContent(
                                    ready = ready,
                                    onOpenSection = { index -> firstKey(items, index)?.let(::scrollToKey) },
                                    onOpenTerm = host::openTerm,
                                    onOpenSource = host::openSource,
                                    onDisclosure = vm::setDisclosure,
                                )
                            }
                        }
                        // Over everything in the column, panel included (it clips to the writing area); no touches.
                        InkLayers(ink)
                    }
                }
            }

            if (!ready.showSummary) {
                LessonPalette(
                    visible = palette.visible,
                    compact = maxWidth < TeachHandwriting.paletteCollapsedBelowWindowWidth,
                    tool = ready.penTool,
                    onTool = vm::setPenTool,
                    highlightColor = ready.highlightColor,
                    onHighlightColor = vm::setHighlightColor,
                    inkColor = ready.inkColor,
                    onInkColor = vm::setInkColor,
                    inkWidth = ready.inkWidth,
                    onInkWidth = vm::setInkWidth,
                    canUndo = ready.canUndoInk,
                    canRedo = ready.canRedoInk,
                    onUndo = undoInk,
                    onRedo = redoInk,
                    eraserOn = ready.eraserMode,
                    onEraser = { on ->
                        vm.setEraserMode(on)
                        announcer.say(if (on) eraserOnSpoken else eraserOffSpoken)
                    },
                    onTouched = palette::onPaletteTouched,
                    modifier = Modifier.align(Alignment.BottomEnd).padding(TeachHighlight.paletteMargin),
                )
            }

            topicTerm?.let { term ->
                val current = ready.lesson
                val origin =
                    term.lessonId?.let { id ->
                        if (current != null &&
                            id == current.id
                        ) {
                            LessonOrigin(id, current.number, current.title)
                        } else {
                            LessonOrigin(id, null, null)
                        }
                    }
                TopicGlossarySheet(
                    term = term,
                    origin = origin,
                    numberOf = { id -> current?.takeIf { it.id == id }?.number },
                    onOpenLesson =
                        origin?.takeIf { it.lessonId != current?.id }?.let {
                            { id ->
                                topicTerm = null
                                onOpenLesson(id)
                            }
                        },
                    onDismiss = { topicTerm = null },
                )
            }
            when {
                sheet == SheetCode.INDEX ->
                    TeachBottomSheet(stringResource(R.string.lesson_index_button), onDismissRequest = {
                        sheet =
                            SheetCode.NONE
                    }) {
                        LessonContextContent(
                            ready = ready,
                            onOpenSection = { index ->
                                sheet = SheetCode.NONE
                                firstKey(items, index)?.let(::scrollToKey)
                            },
                            onOpenTerm = host::openTerm,
                            onOpenSource = host::openSource,
                            onDisclosure = vm::setDisclosure,
                        )
                        TeachButton(stringResource(R.string.lesson_close_sheet), {
                            sheet = SheetCode.NONE
                        }, Modifier.fillMaxWidth(), variant = ButtonVariant.Secondary)
                    }
                sheet.startsWith("term:") ->
                    ready.document.glossaryEntry(sheet.removePrefix("term:"))?.let {
                        GlossarySheet(it) {
                            sheet =
                                SheetCode.NONE
                        }
                    }
                sheet.startsWith("source:") -> {
                    val key = sheet.removePrefix("source:")
                    val source = ready.document.source(key)
                    val number = ready.document.sourceNumber(key)
                    if (source != null && number != null) {
                        SourceSheet(number, source, onOpen = host::openUrl, onDismiss = { sheet = SheetCode.NONE })
                    }
                }
            }
        }
    }
}

private fun firstKey(
    items: List<LessonItem>,
    sectionIndex: Int,
): String? = items.getOrNull(firstItemOfSection(items, sectionIndex))?.key

private fun openInBrowser(
    context: android.content.Context,
    url: String,
) {
    if (!url.startsWith("https://", ignoreCase = true)) return
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri()).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    } catch (_: ActivityNotFoundException) {
        // No browser installed: nothing sensible to do.
    }
}

/** The objective under the title: what the learner will be able to do, and how long it takes. */
@Composable
private fun ObjectiveLine(item: LessonItem.Objective) {
    val colors = TeachTheme.colors
    Column(
        Modifier.padding(top = TeachTheme.layout.blockGapTight).fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            TeachIcon(
                TeachIcons.Target,
                contentDescription = null,
                tint = colors.inkSoft,
                size = TeachTheme.layout.iconSizeSmall,
            )
            TeachText(
                stringResource(R.string.lesson_objective_label),
                style = TeachTheme.typography.ui.label,
                color = colors.inkSoft,
            )
        }
        InlineText(item.text, TeachTheme.typography.reading.body, interactive = true)
        if (item.minutes > 0) {
            TeachText(
                pluralStringResource(R.plurals.lesson_minutes, item.minutes, item.minutes),
                style = TeachTheme.typography.ui.body,
                color = colors.inkSoft,
            )
        }
        item.pdfTitle?.let { PdfLine(it, item.pdfMore) }
    }
}

/** "Dal tuo PDF: ..." under the title: makes it plain that the lesson follows the learner's document (lesson.md 3.3). */
@Composable
internal fun PdfLine(
    title: String,
    more: Int,
    modifier: Modifier = Modifier,
) {
    val colors = TeachTheme.colors
    Row(
        modifier,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
    ) {
        TeachIcon(
            TeachIcons.FileText,
            contentDescription = null,
            tint = colors.inkSoft,
            size = TeachTheme.layout.iconSizeSmall,
            modifier = Modifier.padding(top = TeachTheme.spacing.xxs),
        )
        TeachText(
            if (more > 0) {
                stringResource(R.string.lesson_pdf_line_more, title, more)
            } else {
                stringResource(R.string.lesson_pdf_line, title)
            },
            style = TeachTheme.typography.ui.bodyS,
            color = colors.inkSoft,
        )
    }
}

/** Fade + slide (at most 24 dp) of a freshly revealed section; instant with "reduce animations". */
@Composable
private fun Modifier.sectionReveal(enabled: Boolean): Modifier {
    if (!enabled) return this
    val progress = remember { Animatable(0f) }
    val spec = TeachTheme.motion.sectionReveal
    val reduced = TeachTheme.reducedMotion
    LaunchedEffect(Unit) { progress.animateTo(1f, tween(if (reduced) 0 else spec.durationMs, easing = spec.easing)) }
    val slide = with(LocalDensity.current) { REVEAL_SLIDE.toPx() }
    return graphicsLayer {
        alpha = progress.value
        translationY = (1f - progress.value) * slide
    }
}

/** The gap above the block row [index] (a control row below the previous block takes 16 dp of it). */
@Composable
private fun blockTopPadding(
    items: List<LessonItem>,
    index: Int,
    audio: LessonAudioUiState,
): Dp {
    val item = items[index] as? LessonItem.BlockItem ?: return 0.dp
    val before = (items.getOrNull(index - 1) as? LessonItem.BlockItem)?.block
    val trim = if (before != null && hasControlRowBelow(before, audio)) TeachTheme.spacing.md else 0.dp
    return (item.gap.dp() - trim).coerceAtLeast(0.dp)
}

/**
 * The handwriting surface of the screen: settings in, geometry of the blocks out of the list state, edits to the view
 * model. [left] and [width] are the column in the pixels of the box that holds the list.
 */
@Composable
private fun rememberInkSurface(
    ready: LessonUiState.Ready,
    vm: LessonViewModel,
    listState: LazyListState,
    items: List<LessonItem>,
    audio: LessonAudioUiState,
    left: Float,
    width: Float,
    areaRight: Float,
    onLimit: () -> Unit,
): InkSurface {
    val density = LocalDensity.current
    val pads = items.indices.map { with(density) { blockTopPadding(items, it, audio).toPx() } }
    val itemsNow = rememberUpdatedState(items)
    val padsNow = rememberUpdatedState(pads)
    val leftNow = rememberUpdatedState(left)
    val widthNow = rememberUpdatedState(width)
    val rowSizes = remember(listState) { InkRowSizes() }
    val layout =
        remember(listState) {
            derivedStateOf {
                computeInkLayout(
                    listState.layoutInfo,
                    itemsNow.value,
                    padsNow.value,
                    leftNow.value,
                    widthNow.value,
                    rowSizes,
                )
            }
        }
    val surface = remember(vm) { InkSurface(write = vm::writeStroke, erase = vm::eraseStrokes) }
    surface.layout = { layout.value }
    val strokes = ready.ink.set.strokes
    SideEffect {
        surface.penWrites = ready.penTool == PenTool.PEN
        surface.eraserMode = ready.eraserMode
        surface.color = ready.inkColor
        surface.width = ready.inkWidth
        surface.strokes = strokes
        surface.areaRight = areaRight
        surface.onLimit = onLimit
    }
    return surface
}

/** The keyboard shortcuts of the notes (handwriting.md section 6.2). */
internal enum class InkShortcut { UNDO, REDO }

/** Ctrl+Z undoes, Ctrl+Shift+Z and Ctrl+Y redo; anything else, and Z or Y alone, is no shortcut. */
internal fun inkShortcut(e: androidx.compose.ui.input.key.KeyEvent): InkShortcut? {
    if (e.type != KeyEventType.KeyDown || !e.isCtrlPressed || e.isAltPressed) return null
    return when {
        e.key == Key.Z && !e.isShiftPressed -> InkShortcut.UNDO
        e.key == Key.Z || e.key == Key.Y -> InkShortcut.REDO
        else -> null
    }
}
