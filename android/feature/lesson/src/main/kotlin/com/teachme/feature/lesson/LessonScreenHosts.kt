package com.teachme.feature.lesson

import android.content.ClipData
import android.os.Build
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.key
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.model.LlmKeyVariant
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.LessonProgress
import com.teachme.sdui.render.ExerciseStates
import com.teachme.sdui.render.LessonLinkTarget
import com.teachme.sdui.render.SduiHost
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.launch

private const val SCROLL_DEBOUNCE_MS = 500L
private const val REVEAL_CLEAR_MS = 450L

/** The state of the exercises as the blocks see it: read from the latest state, written through the view model. */
@Composable
internal fun rememberExerciseStates(
    vm: LessonViewModel,
    latest: State<LessonUiState.Ready>,
    sectionOfBlock: Map<String, Int>,
): ExerciseStates =
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

/** What the blocks ask of the screen: open a term, a source or a link, ask the Tutor, copy, "Valuta ora". */
@Composable
internal fun rememberSduiHost(
    vm: LessonViewModel,
    latest: State<LessonUiState.Ready>,
    toast: TeachToastState,
    onSheet: (String) -> Unit,
    onAsk: (String) -> Unit,
    onOpenLesson: (String) -> Unit,
    openKeySettings: (LlmKeyVariant) -> Unit,
): SduiHost {
    val context = LocalContext.current
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    val copiedToast = stringResource(R.string.lesson_copied_toast)
    return remember(vm, context) {
        object : SduiHost {
            override fun openTerm(key: String) {
                vm.termOpened(key)
                onSheet(SheetCode.term(key))
            }

            override fun openSource(key: String) {
                onSheet(SheetCode.source(key))
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

            override fun openKeySettings(variant: LlmKeyVariant) = openKeySettings(variant)

            override fun copyText(text: String) {
                scope.launch {
                    clipboard.setClipEntry(ClipEntry(ClipData.newPlainText(null, text)))
                    // Android 13+ confirms the copy by itself.
                    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) toast.show(copiedToast)
                }
            }
        }
    }
}

/** Reading position (saved on leaving, restored on opening), keyboard focus, and the scroll of a freshly revealed section. */
@OptIn(kotlinx.coroutines.FlowPreview::class)
@Composable
internal fun LessonScrollEffects(
    ready: LessonUiState.Ready,
    vm: LessonViewModel,
    listState: LazyListState,
    items: List<LessonItem>,
    latestItems: State<List<LessonItem>>,
    latest: State<LessonUiState.Ready>,
    columnFocus: FocusRequester,
    scrollGap: Int,
    pendingScrollKey: String?,
    onPendingScrollDone: () -> Unit,
    scrollToKey: (String) -> Unit,
) {
    val view = LocalView.current
    val reducedMotion = TeachTheme.reducedMotion
    val announceTemplate = stringResource(R.string.lesson_section_announce)

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
            onPendingScrollDone()
        }
    }
}

/** Continuous reading: follows the voice with the list, and builds the "Ascolta" action of the top bar. */
@Composable
internal fun rememberLessonListening(
    audio: LessonAudioUiState,
    audioVm: LessonAudioViewModel,
    listState: LazyListState,
    latestItems: State<List<LessonItem>>,
    latest: State<LessonUiState.Ready>,
    toast: TeachToastState,
    scope: CoroutineScope,
    scrollToKey: (String) -> Unit,
): LessonListenAction? {
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
    return listenAction
}
