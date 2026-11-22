package com.teachme.feature.lesson

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.AttemptQueue
import com.teachme.core.model.JobStatus
import com.teachme.core.model.KeyNoticeSource
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LessonView
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.budgetResetAt
import com.teachme.core.model.llmKeyVariant
import com.teachme.core.model.llmKeyVariantOfCode
import com.teachme.sdui.ink.InkHistory
import com.teachme.sdui.ink.InkSet
import com.teachme.sdui.logic.AttemptRestore
import com.teachme.sdui.logic.LessonSavedState
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** The closing of the lesson: "Completa lezione", the backend job that follows, and reading the lesson again. */
internal class LessonCompletion(
    private val host: LessonStateHost,
    private val lessons: LessonRepository,
    private val queue: AttemptQueue,
    private val clock: AppClock,
    private val notices: KeyNoticeSource,
    private val submitter: AttemptSubmitter,
    private val highlights: HighlightsController,
    private val inkNotes: LessonInkNotes,
    private val highlightSync: HighlightSyncer,
    private val strokeSync: StrokeSyncer,
) {
    private val state get() = host.state
    private val scope get() = host.scope
    private val key get() = host.key
    private val isSample get() = host.isSample
    private val dispatchers get() = host.dispatchers
    private var pollJob: Job? = null

    /** The lesson turned out to be closed already: show the real state and follow its job. */
    suspend fun onClosedElsewhere() {
        markCompleted()
        refreshFromBackend(closedElsewhere = true)
        followCompletion()
    }

    fun continueLesson() {
        val ready = state.value as? LessonUiState.Ready ?: return
        if (!ready.isUnlocked() || ready.completing) return
        if (!ready.isLastSection) {
            host.mutate {
                val next = it.saved.revealedCount
                it.copy(saved = it.saved.copy(revealedCount = next + 1), justRevealed = next)
            }
            return
        }
        if (ready.lesson == null || ready.lesson.status == LessonStatus.Completed) {
            // A sample lesson, or one that was already closed: nothing to send.
            host.mutate { it.copy(saved = it.saved.copy(completed = true), showSummary = true, justRevealed = null) }
            return
        }
        // The exhausted-credit panel stays while "Completa lezione" is sent again (llm-key.md 9.1).
        host.mutate {
            it.copy(
                completion = CompletionUi.Sending,
                justRevealed = null,
                keyNotice = it.keyNotice?.takeIf { notice -> notice.variant == LlmKeyVariant.Credit },
                budgetResetAt = null,
            )
        }
        scope.launch { complete() }
    }

    private suspend fun complete() {
        val flush = submitter.flush()
        when (flush) {
            FlushStatus.Closed, FlushStatus.Gone -> return // handled by submitter.flush()
            FlushStatus.Offline, FlushStatus.Unauthorized -> {
                openSummary(CompletionUi.NeedsSend())
                return
            }
            FlushStatus.Done -> Unit
        }
        closeOnBackend()
    }

    private fun openSummary(completion: CompletionUi) =
        host.mutate {
            it.copy(
                saved = it.saved.copy(completed = true),
                showSummary = true,
                completion = completion,
                keyNotice = null,
                budgetResetAt = null,
            )
        }

    /** `POST /lessons/{id}/complete`; the answers were sent just before. */
    suspend fun closeOnBackend() {
        when (val result = lessons.completeLesson(key)) {
            is ApiResult.Success -> {
                markCompleted()
                openSummary(CompletionUi.Running(result.value))
                followCompletion()
            }
            is ApiResult.Failure ->
                when (result.failure) {
                    // Already closed (another device): treat as done, show the real state.
                    ApiFailure.Conflict -> {
                        markCompleted()
                        openSummary(CompletionUi.Running(null))
                        refreshFromBackend()
                        followCompletion()
                    }
                    ApiFailure.NotFound -> host.fail(LessonUiState.Gone)
                    // No key (or a refused one): the lesson stays open, with a notice instead of the summary, and
                    // the answers stay safe in the queue (llm-key.md section 5).
                    ApiFailure.LlmKeyMissing, ApiFailure.LlmKeyRejected, ApiFailure.LlmCreditExhausted ->
                        host.mutate { it.copy(completion = CompletionUi.None, keyNotice = notices.of(result)) }
                    else -> {
                        val budgetReset = result.failure.budgetResetAt(clock.nowMs())
                        if (budgetReset != null) {
                            // Today's budget is used up: the lesson stays open with the budget panel, and "Completa
                            // lezione" waits for the reset (llm-key.md 10.1). The answers are already safe.
                            host.mutate { it.copy(completion = CompletionUi.None, budgetResetAt = budgetReset) }
                        } else {
                            openSummary(CompletionUi.NeedsSend())
                            submitter.scheduleRetry()
                        }
                    }
                }
        }
    }

    private fun markCompleted() =
        state.update {
            if (it is LessonUiState.Ready && it.lesson != null) {
                it.copy(lesson = it.lesson.copy(status = LessonStatus.Completed))
            } else {
                it
            }
        }

    /** "Riprova ora" in the "risposte da inviare" panel. */
    fun retryCompletion() {
        scope.launch {
            host.mutate { it.copy(completion = CompletionUi.NeedsSend(retrying = true)) }
            when (submitter.flush()) {
                FlushStatus.Done -> closeOnBackend()
                FlushStatus.Offline, FlushStatus.Unauthorized ->
                    host.mutate { it.copy(completion = CompletionUi.NeedsSend()) }
                else -> Unit
            }
        }
    }

    /** "Riprova" of a failed `lesson_completion` job (jobs.md section 3). */
    fun retryCompletionJob() {
        val ready = state.value as? LessonUiState.Ready ?: return
        val failed = ready.completion as? CompletionUi.Failed ?: return
        if (failed.retrying) return
        host.mutate { it.copy(completion = failed.copy(retrying = true, networkError = false)) }
        scope.launch {
            when (val result = lessons.retryJob(failed.job.topicId, failed.job.id)) {
                is ApiResult.Success -> {
                    host.mutate { it.copy(completion = CompletionUi.Running(result.value)) }
                    followCompletion()
                }
                is ApiResult.Failure ->
                    if (result.failure == ApiFailure.Conflict || result.failure == ApiFailure.NotFound) {
                        refreshFromBackend()
                        followCompletion()
                    } else if (result.failure.llmKeyVariant() != null) {
                        // Reported to the monitor by the source, as of now. The panel keeps the variant of the job
                        // only for a credit met again; any other refusal is a new notice (a new phase: the key must change first).
                        val notice = notices.of(result)!!
                        val shown =
                            failed.retryNotice?.variant ?: failed.job.error
                                ?.code
                                .llmKeyVariantOfCode()
                        host.mutate {
                            it.copy(
                                completion =
                                    if (notice.variant == LlmKeyVariant.Credit && shown == LlmKeyVariant.Credit) {
                                        failed.copy(
                                            retrying = false,
                                            networkError = false,
                                            creditAgain = failed.creditAgain + 1,
                                        )
                                    } else {
                                        failed.copy(
                                            retrying = false,
                                            networkError = false,
                                            creditAgain = 0,
                                            retryNotice = notice,
                                        )
                                    },
                            )
                        }
                    } else {
                        val budgetReset = result.failure.budgetResetAt(clock.nowMs())
                        host.mutate {
                            it.copy(
                                completion =
                                    if (budgetReset != null) {
                                        failed.copy(
                                            retrying = false,
                                            networkError = false,
                                            budgetResetAt = budgetReset,
                                        )
                                    } else {
                                        failed.copy(retrying = false, networkError = true)
                                    },
                            )
                        }
                    }
            }
        }
    }

    /** Polls `GET /lessons/{id}` every few seconds while `lesson_completion` is active (lessons.md section 8). */
    fun followCompletion() {
        val ready = state.value as? LessonUiState.Ready ?: return
        val active =
            (ready.completion as? CompletionUi.Running) != null
        if (!active || pollJob?.isActive == true) return
        pollJob =
            scope.launch {
                while (true) {
                    delay(dispatchers.pollMs)
                    val current = state.value as? LessonUiState.Ready ?: return@launch
                    if (current.completion !is CompletionUi.Running) return@launch
                    refreshFromBackend()
                }
            }
    }

    /** Reads the lesson again: completion state, graded answers (the job grades the pending ones). */
    suspend fun refreshFromBackend(closedElsewhere: Boolean = false) {
        val editsBefore = highlights.highlightEdits
        val inkEditsBefore = inkNotes.inkEdits
        val stamp = highlightSync.stamp()
        val inkStamp = strokeSync.stamp()
        when (val result = lessons.getLesson(key)) {
            is ApiResult.Success -> {
                val view = result.value
                // The highlights of the backend only matter the first time after opening from the device's copy;
                // later reads (the polling of a closing job) never replace what the learner is marking.
                val fresh = if (host.fromCache) highlightSync.resolve(key, view.highlights, stamp) else null
                val freshInk = if (host.fromCache) strokeSync.resolve(key, view.strokes, inkStamp) else null
                host.fromCache = false
                val unsent = queue.pending(key).map { it.blockId to it.attemptNumber }.toSet()
                host.mutate { ready ->
                    val restored =
                        AttemptRestore.apply(
                            ready.saved,
                            host.exercises.values.toList(),
                            view.attempts,
                            unsent,
                        )
                    val completed = view.lesson.status == LessonStatus.Completed
                    val base =
                        if (completed) {
                            restored.copy(
                                completed = true,
                                revealedCount = ready.sections.sections.size,
                            )
                        } else {
                            restored
                        }
                    ready.copy(
                        highlights =
                            if (fresh != null && highlights.highlightEdits == editsBefore) {
                                fresh.map { it.toHighlight() }
                            } else {
                                ready.highlights
                            },
                        ink =
                            if (freshInk != null && inkNotes.inkEdits == inkEditsBefore) {
                                InkHistory(InkSet.of(freshInk.map { it.toStroke() }))
                            } else {
                                ready.ink
                            },
                        saved = base,
                        lesson = view.lesson,
                        completion = if (completed) completionOf(view, base) else ready.completion,
                        closedElsewhere = ready.closedElsewhere || closedElsewhere,
                        showSummary = ready.showSummary || (closedElsewhere && completed),
                    )
                }
            }
            is ApiResult.Failure -> if (result.failure is ApiFailure.NotFound) host.fail(LessonUiState.Gone)
        }
    }

    fun completionOf(
        view: LessonView,
        saved: LessonSavedState,
    ): CompletionUi {
        if (view.lesson.status != LessonStatus.Completed) {
            // Closed on this device but the backend never heard of it (offline): to be sent.
            return if (saved.completed) CompletionUi.NeedsSend() else CompletionUi.None
        }
        val completion = view.completion ?: return CompletionUi.Running(null)
        val job = completion.job
        return when {
            job == null || job.isActive || job.status == JobStatus.Unknown -> CompletionUi.Running(job)
            job.status == JobStatus.Failed -> CompletionUi.Failed(job)
            else -> CompletionUi.Done(completion.records, completion.terms, completion.reviewCardCount)
        }
    }
}
