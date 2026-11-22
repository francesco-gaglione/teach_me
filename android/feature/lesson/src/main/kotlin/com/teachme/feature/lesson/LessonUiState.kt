package com.teachme.feature.lesson

import com.teachme.core.designsystem.TeachHandwriting
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.KeyNotice
import com.teachme.core.model.LearningRecord
import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.MAX_HIGHLIGHTS
import com.teachme.core.model.QualityWarning
import com.teachme.core.model.TopicJob
import com.teachme.sdui.highlight.Highlight
import com.teachme.sdui.highlight.HighlightColor
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkHistory
import com.teachme.sdui.ink.InkWidth
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.LessonProgress
import com.teachme.sdui.logic.LessonSavedState
import com.teachme.sdui.logic.LessonSections
import com.teachme.sdui.model.LessonDocument
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import java.time.Instant
import javax.inject.Inject

/**
 * Where document parsing runs (off the main thread), the clock and the intervals of the background work; tests swap it
 * for an immediate dispatcher and a fake clock.
 */
open class LessonDispatchers
    @Inject
    constructor() {
        open val parse: CoroutineDispatcher = Dispatchers.Default

        open fun nowMs(): Long = System.currentTimeMillis()

        /** Polling of a running `lesson_completion` job when the Topic stream is not available (lessons.md section 8). */
        open val pollMs: Long = POLL_MS

        /** Pause before the attempt queue is sent again after a failure. */
        open val retryMs: Long = RETRY_MS

        /** Quiet time after the last highlight edit before the set is sent (highlight.md section 11). */
        open val highlightDebounceMs: Long = HIGHLIGHT_DEBOUNCE_MS

        /** Waits before each new try of a failed highlight send; after the last one only a resume or "Riprova ora" tries. */
        open val highlightRetryMs: List<Long> = HIGHLIGHT_RETRY_MS

        /** How long highlight sends must keep failing before the TopBar indicator is shown. */
        open val highlightIndicatorMs: Long = HIGHLIGHT_INDICATOR_MS

        /** Quiet time after the last note edit before the set is sent (handwriting.md section 9); the retries and the indicator are the highlights'. */
        open val inkDebounceMs: Long = INK_DEBOUNCE_MS
    }

private const val POLL_MS = 3_000L
private const val RETRY_MS = 15_000L
private const val HIGHLIGHT_DEBOUNCE_MS = 1_500L
private val HIGHLIGHT_RETRY_MS = listOf(5_000L, 15_000L, 60_000L)
private const val HIGHLIGHT_INDICATOR_MS = 30_000L
private val INK_DEBOUNCE_MS = TeachHandwriting.syncDebounceMs.toLong()
internal const val PERSIST_DEBOUNCE_MS = 300L
internal const val LESSON_FIXTURE_PREFIX = "fixture/"
internal const val DOC_PREFIX = "doc/"

/** The lesson the learner is closing, as the backend sees it (lesson.md section 7.2). */
sealed interface CompletionUi {
    /** Nothing to show: the lesson is still being read. */
    data object None : CompletionUi

    /** "Completa lezione" is sending the answers (button in `loading`). */
    data object Sending : CompletionUi

    /** Completed here, but the answers or the closing could not be sent yet. */
    data class NeedsSend(
        val retrying: Boolean = false,
    ) : CompletionUi

    /** `lesson_completion` queued or running ([job] is null just after the closing, before the first poll). */
    data class Running(
        val job: TopicJob?,
    ) : CompletionUi

    data class Done(
        val records: List<LearningRecord>,
        val terms: List<GlossaryTerm>,
        val reviewCardCount: Int,
    ) : CompletionUi {
        /** Legitimate outcome, not an error: nothing was demonstrated this time (lesson.md state C). */
        val isEmpty: Boolean get() = records.isEmpty() && terms.isEmpty()
    }

    data class Failed(
        val job: TopicJob,
        val retrying: Boolean = false,
        val networkError: Boolean = false,
        /** How many times "Riprova" met an exhausted credit again: the panel announces it (llm-key.md 9.1). */
        val creditAgain: Int = 0,
        /**
         * What a "Riprova" was refused with, when it differs from what the job says (a credit job whose retry meets a
         * refused key, or the other way round): the panel shows this variant, as a new notice.
         */
        val retryNotice: KeyNotice? = null,
        /** A "Riprova" refused for the daily LLM budget: when it comes back (llm-key.md 10.1); the later reset wins. */
        val budgetResetAt: Instant? = null,
    ) : CompletionUi
}

/** What an edit of the highlights did (the screen shows a toast for [LimitReached]). */
enum class HighlightEdit {
    /** The set changed and will be sent. */
    Applied,

    /** Nothing to do: the result would be the same set (for example a stroke inside a highlight of the same colour). */
    Unchanged,

    /** The result would have more than [MAX_HIGHLIGHTS] highlights: nothing changed. */
    LimitReached,
}

/** What the learner asked the Tutor from inside a lesson: the Topic screen opens the chat on it. */
data class LessonAskRequest(
    val topicId: String,
    val lessonId: String,
    val lessonNumber: Int,
    val lessonTitle: String,
    val prompt: String,
)

sealed interface LessonUiState {
    data object Loading : LessonUiState

    /** The document could not be fetched: offer "Riprova". */
    data object LoadFailed : LessonUiState

    /** The document is not usable at all: "Questa lezione non si può aprire". */
    data object Unusable : LessonUiState

    /** `404`: "Questa lezione non esiste più." */
    data object Gone : LessonUiState

    data class Ready(
        val document: LessonDocument,
        val sections: LessonSections,
        val saved: LessonSavedState,
        /** Section index revealed by the last "Continua" (to animate it once), or null. */
        val justRevealed: Int? = null,
        val showSummary: Boolean = false,
        /** The backend lesson; null for a sample lesson of a debug build (no attempts are sent). */
        val lesson: Lesson? = null,
        val completion: CompletionUi = CompletionUi.None,
        /** Attempts waiting in the local queue. */
        val queued: Int = 0,
        /** The lesson was closed from another device (lesson.md section 7.2). */
        val closedElsewhere: Boolean = false,
        /** The session ended (`401`): the queue is kept, and nothing is retried until the user signs in again. */
        val authError: Boolean = false,
        /** "Completa lezione" was refused because of the LLM key (llm-key.md section 5): the lesson is not closed. */
        val keyNotice: KeyNotice? = null,
        /** "Completa lezione" was refused for the daily LLM budget: when it comes back (llm-key.md 10.1); not closed. */
        val budgetResetAt: Instant? = null,
        /** The highlights of the lesson shown now: the local set, which is the truth while it has unsent changes. */
        val highlights: List<Highlight> = emptyList(),
        /** Validator notes of the lesson (lessons.md v1.7); empty when the lesson comes from a sample or an old cache. */
        val qualityWarnings: List<QualityWarning> = emptyList(),
        /** The colour of the next stroke; remembered on the device, the same for every lesson. */
        val highlightColor: HighlightColor = HighlightColor.YELLOW,
        /** Highlight sends keep failing for more than 30 s: the TopBar indicator joins the queue's (highlight.md 11). */
        val highlightSyncFailing: Boolean = false,
        /** The handwritten notes of the lesson with the Undo/Redo stacks of this screen session (not persisted). */
        val ink: InkHistory = InkHistory(),
        /** The tool of the palette; remembered on the device. */
        val penTool: PenTool = PenTool.HIGHLIGHTER,
        val inkColor: InkColor = InkColor.INK,
        val inkWidth: InkWidth = InkWidth.MEDIUM,
        /**
         * The "Gomma" switch: the stylus tip erases for the active tool (handwriting.md 3.4). Never persisted: off every
         * time a lesson opens, kept across a rotation.
         */
        val eraserMode: Boolean = false,
        /** Note sends keep failing for more than 30 s: the TopBar indicator joins the queue's (handwriting.md 9). */
        val inkSyncFailing: Boolean = false,
        /** The backend refused the notes for their size (`413`): they stay here, and the queue says so (handwriting.md 9). */
        val inkTooLarge: Boolean = false,
        /** ISO 639-1 language of the Topic; "it" while unknown (sample lesson, Topic not reachable yet). */
        val contentLanguage: String = "it",
    ) : LessonUiState {
        val canUndoInk: Boolean get() = ink.canUndo

        val canRedoInk: Boolean get() = ink.canRedo

        val newerCatalog: Boolean get() = document.newerCatalog

        /** Closed here or on the backend: exercises without an answer are disabled. */
        val readOnly: Boolean get() = saved.completed || lesson?.status == LessonStatus.Completed

        /** Index of the section "Continua" belongs to: the last revealed one. */
        val currentIndex: Int get() = (saved.revealedCount - 1).coerceIn(0, sections.sections.lastIndex)

        val isLastSection: Boolean get() = currentIndex == sections.sections.lastIndex

        val completing: Boolean get() = completion == CompletionUi.Sending

        fun exercise(id: String): ExerciseState = saved.exercise(id)

        fun isUnlocked(): Boolean = LessonProgress.isUnlocked(sections.sections[currentIndex]) { saved.exercise(it) }
    }
}
