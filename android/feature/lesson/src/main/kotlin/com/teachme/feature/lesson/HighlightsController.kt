package com.teachme.feature.lesson

import android.util.Log
import com.teachme.core.model.MAX_HIGHLIGHTS
import com.teachme.sdui.highlight.Highlight
import com.teachme.sdui.highlight.HighlightColor
import com.teachme.sdui.highlight.Highlights
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Highlights of the open lesson: edits, the write on disk, the debounced send and its retries. */
internal class HighlightsController(
    private val host: LessonStateHost,
    private val highlightSync: HighlightSyncer,
) {
    private val state get() = host.state
    private val scope get() = host.scope
    private val key get() = host.key
    private val isSample get() = host.isSample
    private val dispatchers get() = host.dispatchers
    private val highlightRequests = Channel<Unit>(Channel.CONFLATED)
    var highlightEdits = 0
        private set
    private var highlightDebounce: Job? = null
    private var highlightRetry: Job? = null
    private var highlightIndicator: Job? = null

    /** Paints `[start, end)` of the source of [blockId]'s text; [color] defaults to the current one. */
    fun addHighlight(
        blockId: String,
        start: Int,
        end: Int,
        color: HighlightColor? = null,
    ): HighlightEdit =
        editHighlights {
            Highlights.add(
                it.highlights,
                blockId,
                start,
                end,
                color ?: it.highlightColor,
            )
        }

    /** The eraser: takes `[start, end)` of [blockId] away, keeping what is left on either side. */
    fun eraseHighlight(
        blockId: String,
        start: Int,
        end: Int,
    ): HighlightEdit = editHighlights { Highlights.remove(it.highlights, blockId, start, end) }

    /**
     * "Rimuovi": removes whole the highlight under the source [position]. Returns what was removed (empty when
     * there was nothing), to offer "Annulla" with [restoreHighlights].
     */
    fun removeHighlightAt(
        blockId: String,
        position: Int,
    ): List<Highlight> {
        val ready = state.value as? LessonUiState.Ready ?: return emptyList()
        val hit = ready.highlights.filter { it.blockId == blockId && position >= it.start && position < it.end }
        if (hit.isEmpty()) return emptyList()
        return if (editHighlights { Highlights.removeAt(it.highlights, blockId, position) } ==
            HighlightEdit.Applied
        ) {
            hit
        } else {
            emptyList()
        }
    }

    /** "Annulla" of [removeHighlightAt]: puts [removed] back with their colours. */
    fun restoreHighlights(removed: List<Highlight>): HighlightEdit =
        editHighlights { ready ->
            removed.fold(ready.highlights) { all, h -> Highlights.add(all, h.blockId, h.start, h.end, h.color) }
        }

    /** The learner picked a colour: it is used from the next stroke and remembered on this device. */
    fun setHighlightColor(color: HighlightColor) {
        state.update {
            if (it is LessonUiState.Ready) it.copy(highlightColor = color, eraserMode = false) else it
        }
        scope.launch { highlightSync.setColor(color) }
    }

    /**
     * Sends the highlights now (the screen is paused or closing, "Riprova ora"), without waiting for the debounce.
     * A parked set is only tried again when [retryParked]: on a new edit, on opening and on "Riprova ora".
     */
    fun syncHighlightsNow(retryParked: Boolean = false) {
        highlightDebounce?.cancel()
        scope.launch { attemptHighlights(retryParked) }
    }

    /**
     * Replaces the set with what [change] computes from the state. The set stays as it is when the result is the
     * same, or when it would grow past [MAX_HIGHLIGHTS] (highlight.md section 5).
     */
    private inline fun editHighlights(change: (LessonUiState.Ready) -> List<Highlight>): HighlightEdit {
        var outcome = HighlightEdit.Unchanged
        state.update { current ->
            if (current !is LessonUiState.Ready) return@update current
            val next = change(current)
            when {
                next.size == current.highlights.size && next.toSet() == current.highlights.toSet() -> {
                    outcome = HighlightEdit.Unchanged
                    current
                }
                next.size > MAX_HIGHLIGHTS && next.size > current.highlights.size -> {
                    outcome = HighlightEdit.LimitReached
                    current
                }
                else -> {
                    outcome = HighlightEdit.Applied
                    current.copy(highlights = next)
                }
            }
        }
        if (outcome == HighlightEdit.Applied) {
            highlightEdits++
            highlightRequests.trySend(Unit)
        }
        return outcome
    }

    /** Writes the set on disk after every edit, in order, always the latest one; then waits for the debounce. */
    fun startHighlightPersisting() {
        scope.launch {
            highlightRequests.receiveAsFlow().collect {
                val ready = state.value as? LessonUiState.Ready ?: return@collect
                if (isSample || ready.lesson == null) return@collect
                // A failing disk write must not end this collector: the next edit saves the whole set again.
                try {
                    highlightSync.save(key, ready.highlights.map { it.toRecord() })
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    Log.w("Sdui", "highlights could not be saved on the device for '$key'", e)
                    return@collect
                }
                highlightDebounce?.cancel()
                highlightDebounce =
                    scope.launch {
                        delay(dispatchers.highlightDebounceMs)
                        attemptHighlights()
                    }
            }
        }
    }

    /** One send of the set, and what follows: retries after a failure, the indicator, the end of the lesson. */
    suspend fun attemptHighlights(retryParked: Boolean = false) {
        if (isSample) return
        val result = highlightSync.flush(key, retryParked)
        if (result.discarded.isNotEmpty()) {
            val gone = result.discarded.map { Triple(it.blockId, it.start, it.end) }.toSet()
            Log.w("Sdui", "highlights refused by the backend and dropped: ${result.discarded.size} in '$key'")
            state.update { current ->
                if (current !is LessonUiState.Ready) {
                    current
                } else {
                    current.copy(
                        highlights =
                            current.highlights.filterNot {
                                Triple(it.blockId, it.start, it.end) in
                                    gone
                            },
                    )
                }
            }
        }
        when (result.status) {
            HighlightFlushStatus.Offline -> scheduleHighlightRetries()
            HighlightFlushStatus.Unauthorized -> {
                highlightRetry?.cancel()
                host.setAuthError(true)
            }
            HighlightFlushStatus.Gone -> host.fail(LessonUiState.Gone)
            // Sent, or refused for good: nothing more to retry, and the learner is told nothing.
            HighlightFlushStatus.Done, HighlightFlushStatus.Parked -> {
                highlightRetry?.cancel()
                highlightIndicator?.cancel()
                highlightIndicator = null
                setHighlightSyncFailing(false)
            }
        }
    }

    private fun scheduleHighlightRetries() {
        if (highlightIndicator == null) {
            highlightIndicator =
                scope.launch {
                    delay(dispatchers.highlightIndicatorMs)
                    setHighlightSyncFailing(true)
                }
        }
        if (highlightRetry?.isActive == true) return
        highlightRetry =
            scope.launch {
                for (wait in dispatchers.highlightRetryMs) {
                    delay(wait)
                    if (!highlightSync.hasPending(key)) return@launch
                    attemptHighlights()
                    if (!highlightSync.hasPending(key)) return@launch
                }
            }
    }

    private fun setHighlightSyncFailing(value: Boolean) =
        state.update {
            if (it is LessonUiState.Ready &&
                it.highlightSyncFailing != value
            ) {
                it.copy(highlightSyncFailing = value)
            } else {
                it
            }
        }
}
