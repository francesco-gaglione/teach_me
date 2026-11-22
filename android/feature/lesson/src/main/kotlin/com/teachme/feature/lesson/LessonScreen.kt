package com.teachme.feature.lesson

import android.content.ActivityNotFoundException
import android.content.Intent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import com.teachme.core.designsystem.TeachHandwriting
import com.teachme.core.designsystem.TeachHighlight
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.gradeBlocker
import com.teachme.core.ui.LocalLlmKeyStatus
import com.teachme.core.ui.LocalOpenLlmKeySettings
import com.teachme.sdui.highlight.LocalHighlightHost
import com.teachme.sdui.ink.InkEdit
import com.teachme.sdui.logic.LessonProgress
import com.teachme.sdui.model.Heading
import com.teachme.sdui.render.LocalSdui
import com.teachme.sdui.render.SduiEnvironment
import com.teachme.sdui.render.dp
import com.teachme.sdui.render.rememberHardwareKeyboard
import kotlinx.coroutines.launch

internal val CONTEXT_PANE_TOP = 24.dp
private val COLUMN_BOTTOM_PADDING = 96.dp

@OptIn(ExperimentalFoundationApi::class, kotlinx.coroutines.FlowPreview::class)
@Composable
internal fun LessonContent(
    ready: LessonUiState.Ready,
    vm: LessonViewModel,
    audioVm: LessonAudioViewModel,
    toast: TeachToastState,
    palette: HighlightPaletteState,
    onClose: () -> Unit,
    onAsk: (String) -> Unit,
    onOpenLesson: (String) -> Unit,
) {
    val scope = rememberCoroutineScope()
    val layout = TeachTheme.layout
    val density = LocalDensity.current
    val latest = rememberUpdatedState(ready)
    var sheet by rememberSaveable { mutableStateOf(SheetCode.NONE) }
    val hardwareKeyboard = rememberHardwareKeyboard()
    val reducedMotion = TeachTheme.reducedMotion

    val sectionOfBlock =
        remember(ready.sections) {
            ready.sections.sections
                .flatMap { s -> s.items.map { it.id to s.index } }
                .toMap()
        }
    val states = rememberExerciseStates(vm, latest, sectionOfBlock)
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
        rememberSduiHost(
            vm = vm,
            latest = latest,
            toast = toast,
            onSheet = { sheet = it },
            onAsk = onAsk,
            onOpenLesson = onOpenLesson,
            openKeySettings = { openKey(it) },
        )
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

    LessonScrollEffects(
        ready = ready,
        vm = vm,
        listState = listState,
        items = items,
        latestItems = latestItems,
        latest = latest,
        columnFocus = columnFocus,
        scrollGap = scrollGap,
        pendingScrollKey = pendingScrollKey,
        onPendingScrollDone = { pendingScrollKey = null },
        scrollToKey = ::scrollToKey,
    )

    val audio = LocalLessonAudio.current.state
    val listenAction =
        rememberLessonListening(audio, audioVm, listState, latestItems, latest, toast, scope, ::scrollToKey)

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
                LessonSummaryPane(
                    ready = ready,
                    title = title,
                    vm = vm,
                    onClose = onClose,
                    onAsk = onAsk,
                    onReview = { pendingScrollKey = it },
                    onOpenTerm = { topicTerm = it },
                )
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
                                LessonItemRow(
                                    index = index,
                                    items = items,
                                    ready = ready,
                                    audio = audio,
                                    qualityNote = qualityNote,
                                    qualityFocus = qualityFocus,
                                    qualityFocusKey = qualityFocusKey,
                                    blockLabel = ::blockLabel,
                                    goToBlock = ::goToBlock,
                                    missingCount = missing.size,
                                    onContinue = onContinue,
                                    onLockedTap = onLockedTap,
                                )
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
                LessonPaletteBar(
                    ready = ready,
                    palette = palette,
                    compact = maxWidth < TeachHandwriting.paletteCollapsedBelowWindowWidth,
                    vm = vm,
                    announcer = announcer,
                    onUndo = undoInk,
                    onRedo = redoInk,
                    modifier = Modifier.align(Alignment.BottomEnd).padding(TeachHighlight.paletteMargin),
                )
            }

            LessonSheets(
                ready = ready,
                sheet = sheet,
                onSheet = { sheet = it },
                topicTerm = topicTerm,
                onTopicTerm = { topicTerm = it },
                items = items,
                scrollToKey = ::scrollToKey,
                host = host,
                vm = vm,
                onOpenLesson = onOpenLesson,
            )
        }
    }
}

internal fun firstKey(
    items: List<LessonItem>,
    sectionIndex: Int,
): String? = items.getOrNull(firstItemOfSection(items, sectionIndex))?.key

internal fun openInBrowser(
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
