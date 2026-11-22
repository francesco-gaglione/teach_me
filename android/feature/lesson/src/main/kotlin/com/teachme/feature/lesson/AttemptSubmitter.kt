package com.teachme.feature.lesson

import com.teachme.core.model.LlmKeyMonitor
import com.teachme.sdui.logic.AttemptFactory
import com.teachme.sdui.logic.ExerciseReducers
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.toLocalOutcome
import com.teachme.sdui.model.OpenAnswer
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Sends the learner's answers: queue, local-to-server transitions of exercises, retries, answers of the Grader. */
internal class AttemptSubmitter(
    private val host: LessonStateHost,
    private val syncer: AttemptSyncer,
    private val llmKey: LlmKeyMonitor,
    /** The backend says the lesson is already closed (another device). */
    private val onClosed: suspend () -> Unit,
) {
    private val state get() = host.state
    private val scope get() = host.scope
    private val key get() = host.key
    private val isSample get() = host.isSample
    private val dispatchers get() = host.dispatchers
    private var retryJob: Job? = null

    fun updateExercise(
        blockId: String,
        transform: (ExerciseState) -> ExerciseState,
    ) {
        var old = ExerciseState()
        var new = ExerciseState()
        host.mutate {
            old = it.saved.exercise(blockId)
            new = transform(old)
            it.copy(saved = it.saved.withExercise(blockId, new))
        }
        val ready = state.value as? LessonUiState.Ready ?: return
        val block = host.exercises[blockId]
        if (ready.lesson == null || block == null || ready.readOnly) return
        val attempt = AttemptFactory.fromTransition(key, block, old, new, dispatchers.nowMs()) ?: return
        scope.launch {
            syncer.enqueue(attempt)
            flush()
        }
    }

    /** "Valuta ora": the same open answer goes out again so the Grader tries once more (lessons.md section 5.1). */
    fun reevaluate(blockId: String) = reevaluate(blockId, manual = true)

    private fun reevaluate(
        blockId: String,
        manual: Boolean,
    ) {
        val ready = state.value as? LessonUiState.Ready ?: return
        val block = host.exercises[blockId] as? OpenAnswer ?: return
        if (ready.lesson == null || ready.readOnly) return
        val state = ready.exercise(blockId)
        host.mutate {
            it.copy(
                saved = it.saved.withExercise(blockId, ExerciseReducers.evaluating(state).copy(autoRetried = true)),
            )
        }
        val attempt = AttemptFactory.openAnswerAgain(key, block, state, dispatchers.nowMs())
        scope.launch {
            syncer.enqueue(attempt)
            flush(manualBlock = if (manual) blockId else null)
        }
    }

    /** Pending answers get one automatic second chance (lesson.md section 8.1): not at every second, each costs a call. */
    suspend fun gradePendingOnce() {
        val ready = state.value as? LessonUiState.Ready ?: return
        if (ready.readOnly) return
        val pending =
            host.exercises.keys.filter { id ->
                ready.exercise(id).let {
                    it.submitted &&
                        it.serverPending &&
                        !it.autoRetried
                }
            }
        pending.forEach { reevaluate(it, manual = false) }
    }

    /** Empties the queue and applies the answers of the Grader. Returns how it ended. */
    suspend fun flush(manualBlock: String? = null): FlushStatus {
        if (isSample) return FlushStatus.Done
        val result = syncer.flush(key) { saved -> applySaved(saved, manualBlock) }
        applyLeftovers(result, manualBlock)
        when (result.status) {
            FlushStatus.Done -> {
                retryJob?.cancel()
                host.setAuthError(false)
            }
            FlushStatus.Offline -> scheduleRetry()
            // A wrong token never gets better by retrying: stop, keep the queue, and say so.
            FlushStatus.Unauthorized -> host.setAuthError(true)
            FlushStatus.Closed -> onClosed()
            FlushStatus.Gone -> host.fail(LessonUiState.Gone)
        }
        return result.status
    }

    private fun applySaved(
        saved: List<SavedAttempt>,
        manualBlock: String?,
    ) {
        val opens = saved.filter { it.queued.isOpenAnswer }
        if (opens.isEmpty()) return
        // An open answer came back graded: the LLM service works (llm-key.md 9.4).
        val graded =
            opens.filter {
                it.attempt.outcome.toLocalOutcome() != null &&
                    it.attempt.grade?.feedback != null
            }
        if (graded.isNotEmpty()) {
            // Dated by the server's `Date` of the response that carried the grade; the device clock only without it.
            llmKey.noteWorkedAtServerDate(graded.mapNotNull { it.serverDate }.maxOrNull())
        }
        host.mutate { ready ->
            var s = ready.saved
            for (item in opens) {
                val id = item.queued.blockId
                val verdict = item.attempt.outcome.toLocalOutcome()
                val feedback = item.attempt.grade?.feedback
                val next =
                    if (verdict != null && feedback != null) {
                        ExerciseReducers.graded(s.exercise(id), verdict, feedback)
                    } else {
                        ExerciseReducers.registered(
                            s.exercise(id),
                            onServer = true,
                            manualRetry = id == manualBlock,
                        )
                    }
                s = s.withExercise(id, next)
            }
            ready.copy(saved = s)
        }
    }

    /** Open answers this flush could not deliver stop "evaluating": they are only in the queue (or refused). */
    private fun applyLeftovers(
        result: FlushResult,
        manualBlock: String?,
    ) {
        val savedIds = result.saved.map { it.queued.blockId }.toSet()
        val discardedIds =
            result.discarded
                .filter { it.isOpenAnswer }
                .map { it.blockId }
                .toSet()
        val stuck =
            result.attempted
                .filter { it.isOpenAnswer && it.blockId !in savedIds }
                .map { it.blockId }
                .toSet()
        if (stuck.isEmpty() && discardedIds.isEmpty()) return
        host.mutate { ready ->
            var s = ready.saved
            for (id in stuck) {
                val current = s.exercise(id)
                if (!current.evaluating && !current.submitted) continue
                s =
                    s.withExercise(
                        id,
                        if (id in discardedIds) {
                            ExerciseReducers.notEvaluable(current)
                        } else {
                            ExerciseReducers.registered(
                                current,
                                onServer = current.serverPending,
                                manualRetry = false,
                            )
                        },
                    )
            }
            ready.copy(saved = s)
        }
    }

    fun scheduleRetry() {
        if (retryJob?.isActive == true) return
        retryJob =
            scope.launch {
                // Keeps trying, every retryMs, for as long as something is queued and the network is the problem.
                while (syncer.hasPending(key)) {
                    delay(dispatchers.retryMs)
                    if (!syncer.hasPending(key) || flush() != FlushStatus.Offline) break
                }
            }
    }
}
