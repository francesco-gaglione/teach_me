package com.teachme.feature.lesson

import com.teachme.core.model.JobStatus
import com.teachme.core.model.TopicJob

/** What the `lesson_request` card of the chat shows, derived from its job (chat.md design 10b.3). */
sealed interface LessonRequestState {
    /** The job is not among the loaded ones (older pages), or is of a kind or status this app does not know. */
    data object Unknown : LessonRequestState

    data object Queued : LessonRequestState

    /** [phase] is null for a step this app does not know: only the spinner and [message] are shown. */
    data class Running(
        val phase: GenerationPhase?,
        val repairing: Boolean,
        val message: String?,
    ) : LessonRequestState

    /** [lessonId] and [number] come from `result`; [title] only when the lesson is already in the loaded list. */
    data class Ready(
        val lessonId: String?,
        val number: Int?,
        val title: String?,
    ) : LessonRequestState

    /** [canRetry] is true only when the job is the last `lesson_generation` of the Topic. */
    data class Failed(
        val code: String?,
        val canRetry: Boolean,
    ) : LessonRequestState

    /** Queued or running: the card is one live status line. */
    val inProgress: Boolean get() = this is Queued || this is Running
}

/**
 * The state of a card from its [job] (null when not loaded). [latestGenerationId] is the id of the last
 * `lesson_generation` job of the Topic; [lessonTitle] resolves a lesson id to its title when already loaded.
 */
fun lessonRequestState(
    job: TopicJob?,
    latestGenerationId: String?,
    lessonTitle: (String) -> String?,
): LessonRequestState {
    if (job == null || !job.isLessonGeneration) return LessonRequestState.Unknown
    return when (job.status) {
        JobStatus.Queued -> LessonRequestState.Queued
        JobStatus.Running ->
            LessonRequestState.Running(
                phase = GenerationPhase.fromStep(job.progress?.step),
                repairing = job.isRepair(),
                message = job.progress?.message?.takeIf { it.isNotBlank() },
            )
        JobStatus.Succeeded ->
            LessonRequestState.Ready(
                lessonId = job.result?.lessonId,
                number = job.result?.lessonNumber,
                title = job.result?.lessonId?.let(lessonTitle),
            )
        JobStatus.Failed -> LessonRequestState.Failed(job.error?.code, canRetry = job.id == latestGenerationId)
        JobStatus.Unknown -> LessonRequestState.Unknown
    }
}

/** What a screen reader is told when the card moves from one state to another (chat.md design 10b.6). */
sealed interface LessonRequestSpeech {
    data class Step(
        val phase: GenerationPhase,
        val repairing: Boolean,
    ) : LessonRequestSpeech

    data class Ready(
        val number: Int?,
    ) : LessonRequestSpeech

    data class Failed(
        val canRetry: Boolean,
    ) : LessonRequestSpeech
}

/**
 * The announcement for [previous] to [current], or null. Only changes seen while the card is on screen are spoken: from
 * an unknown or an already final state nothing is said (history), and `progress.message` never is.
 */
fun lessonRequestSpeech(
    previous: LessonRequestState,
    current: LessonRequestState,
): LessonRequestSpeech? {
    if (!previous.inProgress) return null
    return when (current) {
        is LessonRequestState.Running ->
            current.phase
                ?.takeIf {
                    previous !is LessonRequestState.Running ||
                        it != previous.phase ||
                        current.repairing != previous.repairing
                }?.let { LessonRequestSpeech.Step(it, current.repairing) }
        is LessonRequestState.Ready -> LessonRequestSpeech.Ready(current.number)
        is LessonRequestState.Failed -> LessonRequestSpeech.Failed(current.canRetry)
        else -> null
    }
}

/** The state without what must not restart a pending announcement (the free text of the progress). */
internal fun LessonRequestState.stage(): LessonRequestState =
    if (this is LessonRequestState.Running) copy(message = null) else this
