package com.teachme.feature.lesson

import android.util.Log
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkEdit
import com.teachme.sdui.ink.InkHistory
import com.teachme.sdui.ink.InkOperation
import com.teachme.sdui.ink.InkWidth
import com.teachme.sdui.ink.Stroke
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** Handwritten notes of the open lesson: edits, the write on disk, the debounced send and its retries. */
internal class LessonInkNotes(
    private val host: LessonStateHost,
    private val strokeSync: StrokeSyncer,
) {
    private val state get() = host.state
    private val scope get() = host.scope
    private val key get() = host.key
    private val isSample get() = host.isSample
    private val dispatchers get() = host.dispatchers
    private val inkRequests = Channel<Unit>(Channel.CONFLATED)
    var inkEdits = 0
        private set
    private var inkDebounce: Job? = null
    private var inkRetry: Job? = null
    private var inkIndicator: Job? = null

    // The set on screen could not be written on the device (disk full): it stays in memory and is written again
    // by the next edit and by every retry. Never sent before it is on disk.
    @Volatile private var inkUnsaved = false
    private val inkSaveLock = Mutex()

    /** A finished pen stroke goes on top of the others. [InkEdit.Rejected] says which limit it would break. */
    fun writeStroke(stroke: Stroke): InkEdit = editInk { it.write(stroke) }

    /** One eraser pass over the strokes [ids] (those of `InkEraser.touched`): one undoable operation. */
    fun eraseStrokes(ids: Collection<String>): InkEdit = editInk { it.erase(ids) }

    /**
     * Takes away all the notes of [blockId] as one undoable pass (the TalkBack action, handwriting.md section 10).
     * Returns the operation that was recorded, to hand to [undoNotesClear]; null when the block had no notes.
     */
    fun clearBlockNotes(blockId: String): InkOperation? {
        val ready = state.value as? LessonUiState.Ready ?: return null
        val ids =
            ready.ink.set.strokes
                .filter { it.blockId == blockId }
                .map { it.id }
        if (ids.isEmpty() || eraseStrokes(ids) !is InkEdit.Applied) return null
        return (state.value as? LessonUiState.Ready)?.ink?.lastOperation
    }

    /** "Annulla" of [clearBlockNotes]: only while that pass is still the last operation, so nothing else is undone. */
    fun undoNotesClear(operation: InkOperation): Boolean {
        val ready = state.value as? LessonUiState.Ready ?: return false
        return ready.ink.lastOperation == operation && undoInk() is InkEdit.Applied
    }

    fun undoInk(): InkEdit = editInk { it.undo() }

    fun redoInk(): InkEdit = editInk { it.redo() }

    /** Changing the tool switches the eraser off (handwriting.md 3.4); picking the tool already active changes nothing. */
    fun setPenTool(tool: PenTool) {
        state.update {
            if (it is LessonUiState.Ready && it.penTool != tool) it.copy(penTool = tool, eraserMode = false) else it
        }
        scope.launch { strokeSync.setTool(tool) }
    }

    /** Choosing a colour or a width means "I want to write": the eraser goes off even if the choice is the same. */
    fun setInkColor(color: InkColor) {
        state.update { if (it is LessonUiState.Ready) it.copy(inkColor = color, eraserMode = false) else it }
        scope.launch { strokeSync.setInkColor(color) }
    }

    fun setInkWidth(width: InkWidth) {
        state.update { if (it is LessonUiState.Ready) it.copy(inkWidth = width, eraserMode = false) else it }
        scope.launch { strokeSync.setWidth(width) }
    }

    /** The "Gomma" switch. Not saved on the device; Undo/Redo do not touch it. */
    fun setEraserMode(on: Boolean) {
        state.update { if (it is LessonUiState.Ready && it.eraserMode != on) it.copy(eraserMode = on) else it }
    }

    /**
     * Sends the notes now (the screen is paused or closing, "Riprova ora"), without waiting for the debounce.
     * A parked set is only tried again when [retryParked]: on a new edit, on opening and on "Riprova ora".
     */
    fun syncInkNow(retryParked: Boolean = false) {
        inkDebounce?.cancel()
        scope.launch { attemptInk(retryParked) }
    }

    /** Applies [change] to the history; only an applied edit is saved and sent. */
    private inline fun editInk(change: (InkHistory) -> InkEdit): InkEdit {
        var outcome: InkEdit = InkEdit.NoOp
        state.update { current ->
            if (current !is LessonUiState.Ready) return@update current
            outcome = change(current.ink)
            when (val edit = outcome) {
                is InkEdit.Applied -> current.copy(ink = edit.history)
                else -> current
            }
        }
        if (outcome is InkEdit.Applied) {
            inkEdits++
            inkRequests.trySend(Unit)
        }
        return outcome
    }

    /** Writes the set on disk after every edit, in order, always the latest one; then waits for the debounce. */
    fun startInkPersisting() {
        scope.launch {
            inkRequests.receiveAsFlow().collect {
                if (!saveInk()) {
                    // Not on disk: the row of the notes appears and the retries write the set again.
                    scheduleInkRetries()
                    return@collect
                }
                inkDebounce?.cancel()
                inkDebounce =
                    scope.launch {
                        delay(dispatchers.inkDebounceMs)
                        attemptInk()
                    }
            }
        }
    }

    /**
     * Writes the set on screen on the device; false if the disk refused it (the set stays in memory, [inkUnsaved]).
     * Serialized, and the set is read inside the lock, so an older snapshot never overwrites a newer one.
     */
    private suspend fun saveInk(): Boolean =
        inkSaveLock.withLock {
            val ready = state.value as? LessonUiState.Ready ?: return@withLock true
            if (isSample || ready.lesson == null) return@withLock true
            try {
                strokeSync.save(
                    key,
                    ready.ink.set.strokes
                        .map { it.toRecord() },
                )
                inkUnsaved = false
                true
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.w("Sdui", "notes could not be saved on the device for '$key'", e)
                inkUnsaved = true
                false
            }
        }

    /** One send of the notes, and what follows: retries after a failure, the indicator, the end of the lesson. */
    suspend fun attemptInk(retryParked: Boolean = false) {
        if (isSample) return
        // A set that never reached the disk is written first; if the disk still refuses, this is a failed send.
        val result =
            if (inkUnsaved && !saveInk()) {
                StrokeFlushResult(StrokeFlushStatus.Offline, emptyList())
            } else {
                strokeSync.flush(key, retryParked)
            }
        if (result.discarded.isNotEmpty()) {
            val gone = result.discarded.map { it.id }.toSet()
            Log.w("Sdui", "notes refused by the backend and dropped: ${result.discarded.size} in '$key'")
            state.update { current ->
                if (current is LessonUiState.Ready) current.copy(ink = current.ink.without(gone)) else current
            }
        }
        // A parked set that was not sent again says nothing new: the row stays as it was.
        if (result.tooLarge) {
            setInkTooLarge(true)
        } else if (result.status != StrokeFlushStatus.Parked) {
            setInkTooLarge(false)
        }
        when (result.status) {
            StrokeFlushStatus.Offline -> scheduleInkRetries()
            StrokeFlushStatus.Unauthorized -> {
                inkRetry?.cancel()
                host.setAuthError(true)
            }
            StrokeFlushStatus.Gone -> host.fail(LessonUiState.Gone)
            // Sent, or refused for good: nothing more to retry, and the learner is told nothing.
            StrokeFlushStatus.Done, StrokeFlushStatus.Parked -> {
                inkRetry?.cancel()
                inkIndicator?.cancel()
                inkIndicator = null
                setInkSyncFailing(false)
            }
        }
    }

    private fun scheduleInkRetries() {
        if (inkIndicator == null) {
            inkIndicator =
                scope.launch {
                    delay(dispatchers.highlightIndicatorMs)
                    setInkSyncFailing(true)
                }
        }
        if (inkRetry?.isActive == true) return
        inkRetry =
            scope.launch {
                for (wait in dispatchers.highlightRetryMs) {
                    delay(wait)
                    if (!inkWaiting()) return@launch
                    attemptInk()
                    if (!inkWaiting()) return@launch
                }
            }
    }

    private suspend fun inkWaiting() = inkUnsaved || strokeSync.hasPending(key)

    private fun setInkTooLarge(value: Boolean) =
        state.update {
            if (it is LessonUiState.Ready && it.inkTooLarge != value) it.copy(inkTooLarge = value) else it
        }

    private fun setInkSyncFailing(value: Boolean) =
        state.update {
            if (it is LessonUiState.Ready && it.inkSyncFailing != value) it.copy(inkSyncFailing = value) else it
        }
}

/**
 * The history without the strokes [ids] (the backend refused them): they are gone from the set and from the stacks, so
 * an Undo or Redo can never bring them back.
 */
internal fun InkHistory.without(ids: Set<String>): InkHistory {
    fun List<InkOperation>.clean() =
        mapNotNull { op ->
            when (op) {
                is InkOperation.Write -> op.takeIf { it.entry.stroke.id !in ids }
                is InkOperation.Erase ->
                    op.entries.filter { it.stroke.id !in ids }.takeIf { it.isNotEmpty() }?.let(
                        InkOperation::Erase,
                    )
            }
        }
    return InkHistory(set.remove(ids), undoStack.clean(), redoStack.clean())
}
