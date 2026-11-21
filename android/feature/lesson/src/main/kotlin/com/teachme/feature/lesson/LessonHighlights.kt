package com.teachme.feature.lesson

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.pointerInput
import com.teachme.core.designsystem.TeachHighlight
import com.teachme.sdui.highlight.Highlight
import com.teachme.sdui.highlight.HighlightColor
import com.teachme.sdui.highlight.HighlightHost
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The pen seen on this screen (highlight.md sections 2 and 6): the palette appears at the first pen event (hover
 * included), stays while the tip is down and goes [TeachHighlight.paletteIdleHideMs] after the last pen event or touch
 * of the palette. Lives with the screen, never saved.
 */
@Stable
class HighlightPaletteState {
    var penSeen by mutableStateOf(false)
        private set
    var visible by mutableStateOf(false)
        private set
    private var penDown by mutableStateOf(false)

    // Every event only pokes this channel (not state): the timer re-arms without recomposing anything.
    private val activity = Channel<Unit>(Channel.CONFLATED)

    /** A pen event reached the screen; [down] is true while the tip is on the glass. */
    fun onPenEvent(down: Boolean) {
        penSeen = true
        visible = true
        penDown = down
        activity.trySend(Unit)
    }

    /** The learner touched the palette itself: it stays a while longer. */
    fun onPaletteTouched() {
        activity.trySend(Unit)
    }

    @Composable
    internal fun Run() {
        LaunchedEffect(visible, penDown) {
            if (!visible || penDown) return@LaunchedEffect
            // Hides after the idle time without any activity: each one restarts the wait.
            while (withTimeoutOrNull(TeachHighlight.paletteIdleHideMs.toLong()) { activity.receive() } != null) {
                // Activity seen: the wait starts again.
            }
            visible = false
        }
    }
}

/** Reports every pen event (stylus or eraser, hover included) in the main pass, consuming nothing. */
internal fun Modifier.penWatcher(onPen: (down: Boolean) -> Unit): Modifier =
    pointerInput(Unit) {
        awaitPointerEventScope {
            while (true) {
                val event = awaitPointerEvent(PointerEventPass.Initial)
                val pen = event.changes.filter { it.type == PointerType.Stylus || it.type == PointerType.Eraser }
                if (pen.isNotEmpty()) onPen(pen.any { it.pressed })
            }
        }
    }

/**
 * What the text blocks ask of the screen: the set to draw and the edits. Feedback is the screen's: the limit toast,
 * and "Evidenziazione rimossa" with "Annulla" (highlight.md sections 5 and 8).
 */
internal class LessonHighlightHost(
    private val state: () -> LessonUiState.Ready,
    private val vm: LessonViewModel,
    private val showToast: (message: String, action: String?, onAction: (() -> Unit)?) -> Unit,
    private val limitMessage: () -> String,
    private val removedMessage: () -> String,
    private val undoLabel: () -> String,
    private val notesClearedMessage: () -> String,
    private val penSeen: () -> Boolean,
) : HighlightHost {
    override val highlights: List<Highlight> get() = state().highlights
    override val color: HighlightColor get() = state().highlightColor
    override val penWrites: Boolean get() = state().penTool == PenTool.PEN

    override val eraserMode: Boolean get() = state().eraserMode

    override fun noteCount(blockId: String): Int =
        state()
            .ink.set.strokes
            .count { it.blockId == blockId }

    // Without the pen the notes cannot be changed (ADR 0015 point 11): the action exists once the pen was seen.
    override val canClearNotes: Boolean get() = penSeen()

    override fun clearNotes(blockId: String) {
        val operation = vm.clearBlockNotes(blockId) ?: return
        showToast(notesClearedMessage(), undoLabel()) { vm.undoNotesClear(operation) }
    }

    override fun add(
        blockId: String,
        start: Int,
        end: Int,
    ) = limitToast(vm.addHighlight(blockId, start, end))

    override fun erase(
        blockId: String,
        start: Int,
        end: Int,
    ) = limitToast(vm.eraseHighlight(blockId, start, end))

    override fun remove(
        blockId: String,
        position: Int,
    ) {
        val removed = vm.removeHighlightAt(blockId, position)
        if (removed.isEmpty()) return
        showToast(removedMessage(), undoLabel()) { limitToast(vm.restoreHighlights(removed)) }
    }

    private fun limitToast(result: HighlightEdit) {
        if (result == HighlightEdit.LimitReached) showToast(limitMessage(), null, null)
    }
}
