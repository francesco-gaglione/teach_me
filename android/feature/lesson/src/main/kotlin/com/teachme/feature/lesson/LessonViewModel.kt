package com.teachme.feature.lesson

import android.util.Log
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.designsystem.TeachHandwriting
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AttemptQueue
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.JobStatus
import com.teachme.core.model.KeyNotice
import com.teachme.core.model.KeyNoticeSource
import com.teachme.core.model.LearningRecord
import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonDocumentSource
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.LessonStateStore
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LessonView
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.MAX_HIGHLIGHTS
import com.teachme.core.model.NoLlmKeyMonitor
import com.teachme.core.model.QualityWarning
import com.teachme.core.model.TopicJob
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.TopicTabInbox
import com.teachme.core.model.TopicTabNames
import com.teachme.core.model.llmKeyVariant
import com.teachme.core.model.llmKeyVariantOfCode
import com.teachme.sdui.highlight.Highlight
import com.teachme.sdui.highlight.HighlightColor
import com.teachme.sdui.highlight.Highlights
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkEdit
import com.teachme.sdui.ink.InkHistory
import com.teachme.sdui.ink.InkOperation
import com.teachme.sdui.ink.InkSet
import com.teachme.sdui.ink.InkWidth
import com.teachme.sdui.ink.Stroke
import com.teachme.sdui.logic.AttemptFactory
import com.teachme.sdui.logic.AttemptRestore
import com.teachme.sdui.logic.ExerciseReducers
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.LessonProgress
import com.teachme.sdui.logic.LessonSavedState
import com.teachme.sdui.logic.LessonSections
import com.teachme.sdui.logic.SectionBuilder
import com.teachme.sdui.logic.toLocalOutcome
import com.teachme.sdui.model.Exercise
import com.teachme.sdui.model.LessonDocument
import com.teachme.sdui.model.LessonParser
import com.teachme.sdui.model.OpenAnswer
import com.teachme.sdui.model.ParseResult
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
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
private const val PERSIST_DEBOUNCE_MS = 300L
private const val FIXTURE_PREFIX = "fixture/"
private const val DOC_PREFIX = "doc/"

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

@HiltViewModel
class LessonViewModel
    @Inject
    constructor(
        handle: SavedStateHandle,
        private val source: LessonDocumentSource,
        private val store: LessonStateStore,
        private val events: SduiEventLogger,
        private val dispatchers: LessonDispatchers,
        private val lessons: LessonRepository,
        private val syncer: AttemptSyncer,
        private val highlightSync: HighlightSyncer,
        private val strokeSync: StrokeSyncer,
        private val queue: AttemptQueue,
        private val tabInbox: TopicTabInbox,
        /** Told when the Grader evaluated an answer: the service works, so a remembered credit or key problem is stale. */
        private val llmKey: LlmKeyMonitor = NoLlmKeyMonitor,
    ) : ViewModel() {
        /** Key of the lesson, from the navigation argument of the same name: the lesson id, or a sample's key. */
        val key: String = handle.get<String>("key").orEmpty()

        private val isSample = key.startsWith(FIXTURE_PREFIX)

        private val _state = MutableStateFlow<LessonUiState>(LessonUiState.Loading)
        val state: StateFlow<LessonUiState> = _state.asStateFlow()

        private val persistRequests = Channel<Unit>(Channel.CONFLATED)
        private var exercises: Map<String, Exercise> = emptyMap()
        private var retryJob: Job? = null
        private var pollJob: Job? = null
        private var fromCache = false
        private val highlightRequests = Channel<Unit>(Channel.CONFLATED)
        private var highlightEdits = 0
        private var highlightDebounce: Job? = null
        private var highlightRetry: Job? = null
        private var highlightIndicator: Job? = null
        private val inkRequests = Channel<Unit>(Channel.CONFLATED)
        private var inkEdits = 0
        private var inkDebounce: Job? = null
        private var inkRetry: Job? = null
        private var inkIndicator: Job? = null

        // The set on screen could not be written on the device (disk full): it stays in memory and is written again
        // by the next edit and by every retry. Never sent before it is on disk.
        @Volatile private var inkUnsaved = false
        private val inkSaveLock = Mutex()
        private val notices = KeyNoticeSource(llmKey)

        init {
            startPersisting()
            startHighlightPersisting()
            startInkPersisting()
            load()
        }

        @OptIn(FlowPreview::class, ExperimentalCoroutinesApi::class)
        private fun startPersisting() {
            viewModelScope.launch {
                persistRequests.receiveAsFlow().debounce(PERSIST_DEBOUNCE_MS).collect { persistNow() }
            }
        }

        fun retryLoad() {
            _state.value = LessonUiState.Loading
            load()
        }

        // --- loading ------------------------------------------------------------------------------------------------

        private fun load() {
            viewModelScope.launch {
                val view: LessonView?
                val text: String
                var stamp: FetchStamp? = null
                var inkStamp: FetchStamp? = null
                if (isSample) {
                    view = null
                    text = source.load(key) ?: return@launch fail(LessonUiState.LoadFailed)
                } else {
                    stamp = highlightSync.stamp()
                    inkStamp = strokeSync.stamp()
                    when (val result = lessons.getLesson(key)) {
                        is ApiResult.Success -> {
                            view = result.value
                            text = result.value.documentJson
                            store.save(DOC_PREFIX + key, CachedLesson.of(result.value).encode())
                        }
                        is ApiResult.Failure -> {
                            if (result.failure is ApiFailure.NotFound) return@launch fail(LessonUiState.Gone)
                            // Offline: a lesson already downloaded on this device still opens (lesson.md section 8.1).
                            val cached =
                                CachedLesson.decode(store.load(DOC_PREFIX + key))
                                    ?: return@launch fail(LessonUiState.LoadFailed)
                            fromCache = true
                            view = cached.toView()
                            text = cached.document
                        }
                    }
                }
                val parsed = withContext(dispatchers.parse) { LessonParser.parse(text) }
                when (parsed) {
                    is ParseResult.Unusable -> {
                        Log.w("Sdui", "lesson '$key' cannot be opened: ${parsed.reason}")
                        _state.value = LessonUiState.Unusable
                    }
                    is ParseResult.Usable -> {
                        parsed.events.forEach(events::record)
                        open(parsed.document, text, view, stamp, inkStamp)
                    }
                }
            }
        }

        private fun fail(state: LessonUiState) {
            _state.value = state
        }

        private suspend fun open(
            document: LessonDocument,
            text: String,
            view: LessonView?,
            stamp: FetchStamp?,
            inkStamp: FetchStamp?,
        ) {
            val sections = SectionBuilder.build(document)
            exercises = sections.sections.flatMap { it.exercises }.associateBy { it.id }
            val ids =
                sections.sections
                    .flatMap { it.items }
                    .map { it.id }
                    .toSet()
            val hash = LessonSavedState.hashOf(text)
            val stored =
                try {
                    LessonSavedState.decode(store.load(key))
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    Log.w("Sdui", "saved state of '$key' unreadable (${e::class.simpleName}): starting fresh")
                    LessonSavedState()
                }
            // A different document under the same key: old answers and progress would not mean anything.
            var saved =
                if (stored.docHash ==
                    hash
                ) {
                    stored.prunedTo(ids, sections.sections.size)
                } else {
                    LessonSavedState(docHash = hash)
                }
            // Nothing is "being evaluated" after a restart: the queue sends it again.
            saved = saved.copy(exercises = saved.exercises.mapValues { it.value.copy(evaluating = false) })
            if (view != null) {
                val unsent = queue.pending(key).map { it.blockId to it.attemptNumber }.toSet()
                saved = AttemptRestore.apply(saved, exercises.values.toList(), view.attempts, unsent)
                if (view.lesson.status == LessonStatus.Completed) {
                    saved = saved.copy(revealedCount = sections.sections.size, completed = true)
                }
            }
            val shown =
                when {
                    view == null -> emptyList()
                    // Opened from the copy on the device: whatever this device holds is all there is.
                    fromCache -> highlightSync.local(key)
                    else -> highlightSync.resolve(key, view.highlights, checkNotNull(stamp))
                }
            val shownInk =
                when {
                    view == null -> emptyList()
                    fromCache -> strokeSync.local(key)
                    else -> strokeSync.resolve(key, view.strokes, checkNotNull(inkStamp))
                }
            val ready =
                LessonUiState.Ready(
                    document = document,
                    sections = sections,
                    saved = saved,
                    showSummary = saved.completed,
                    lesson = view?.lesson,
                    contentLanguage = contentLanguageOf(view?.language ?: TopicLanguage.It),
                    qualityWarnings = view?.qualityWarnings.orEmpty(),
                    completion = if (view != null) completionOf(view, saved) else CompletionUi.None,
                    highlights = shown.map { it.toHighlight() },
                    highlightColor = highlightSync.color(),
                    ink = InkHistory(InkSet.of(shownInk.map { it.toStroke() })),
                    penTool = strokeSync.tool(),
                    inkColor = strokeSync.inkColor(),
                    inkWidth = strokeSync.width(),
                )
            _state.value = ready
            if (view != null) {
                watchQueue()
                background(ready)
                // Unsent highlights go out when the lesson opens, a refused (parked) set gets one more try.
                viewModelScope.launch { attemptHighlights(retryParked = true) }
                viewModelScope.launch { attemptInk(retryParked = true) }
            }
        }

        private fun completionOf(
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

        private fun watchQueue() {
            viewModelScope.launch {
                queue.count(key).collect { n ->
                    _state.update { if (it is LessonUiState.Ready && it.queued != n) it.copy(queued = n) else it }
                }
            }
        }

        /** What happens right after the lesson opens: send the queue, finish a closing, follow a running job. */
        private fun background(ready: LessonUiState.Ready) {
            viewModelScope.launch {
                val status = flush()
                if (ready.saved.completed && ready.lesson?.status != LessonStatus.Completed) {
                    // Closing with answers still queued would make them fail with 409: only after a clean flush.
                    if (status == FlushStatus.Done) closeOnBackend()
                } else {
                    followCompletion()
                }
            }
        }

        /** The screen came back to the foreground: send what is queued, grade pending answers once, follow the job. */
        fun onResume() {
            val ready = _state.value as? LessonUiState.Ready ?: return
            if (ready.lesson == null) return
            viewModelScope.launch {
                // Opened from the copy on the device: the backend's attempts and status are read as soon as it answers.
                attemptHighlights()
                attemptInk()
                if (fromCache) refreshFromBackend()
                gradePendingOnce()
                val status = flush()
                if (status == FlushStatus.Done &&
                    ready.saved.completed &&
                    (ready.completion as? CompletionUi.NeedsSend) != null
                ) {
                    closeOnBackend()
                }
                followCompletion()
            }
        }

        /** "Riprova ora" of the queue popover. */
        fun retryQueueNow() {
            viewModelScope.launch { flush() }
            // The learner asks for it: a set the backend refused is tried once more.
            syncHighlightsNow(retryParked = true)
            syncInkNow(retryParked = true)
        }

        // --- state changes by the learner ---------------------------------------------------------------------------

        private inline fun mutate(transform: (LessonUiState.Ready) -> LessonUiState.Ready) {
            _state.update { current -> if (current is LessonUiState.Ready) transform(current) else current }
            persistRequests.trySend(Unit)
        }

        fun updateExercise(
            blockId: String,
            transform: (ExerciseState) -> ExerciseState,
        ) {
            var old = ExerciseState()
            var new = ExerciseState()
            mutate {
                old = it.saved.exercise(blockId)
                new = transform(old)
                it.copy(saved = it.saved.withExercise(blockId, new))
            }
            val ready = _state.value as? LessonUiState.Ready ?: return
            val block = exercises[blockId]
            if (ready.lesson == null || block == null || ready.readOnly) return
            val attempt = AttemptFactory.fromTransition(key, block, old, new, dispatchers.nowMs()) ?: return
            viewModelScope.launch {
                syncer.enqueue(attempt)
                flush()
            }
        }

        /** True for exercises of a section the learner already left with "Continua". */
        fun isLocked(blockId: String): Boolean {
            val ready = _state.value as? LessonUiState.Ready ?: return false
            val section = ready.sections.sections.firstOrNull { s -> s.items.any { it.id == blockId } } ?: return false
            return LessonProgress.isLocked(section.index, ready.saved.revealedCount)
        }

        /** "Valuta ora": the same open answer goes out again so the Grader tries once more (lessons.md section 5.1). */
        fun reevaluate(blockId: String) = reevaluate(blockId, manual = true)

        private fun reevaluate(
            blockId: String,
            manual: Boolean,
        ) {
            val ready = _state.value as? LessonUiState.Ready ?: return
            val block = exercises[blockId] as? OpenAnswer ?: return
            if (ready.lesson == null || ready.readOnly) return
            val state = ready.exercise(blockId)
            mutate {
                it.copy(
                    saved = it.saved.withExercise(blockId, ExerciseReducers.evaluating(state).copy(autoRetried = true)),
                )
            }
            val attempt = AttemptFactory.openAnswerAgain(key, block, state, dispatchers.nowMs())
            viewModelScope.launch {
                syncer.enqueue(attempt)
                flush(manualBlock = if (manual) blockId else null)
            }
        }

        /** Pending answers get one automatic second chance (lesson.md section 8.1): not at every second, each costs a call. */
        private suspend fun gradePendingOnce() {
            val ready = _state.value as? LessonUiState.Ready ?: return
            if (ready.readOnly) return
            val pending =
                exercises.keys.filter { id ->
                    ready.exercise(id).let {
                        it.submitted &&
                            it.serverPending &&
                            !it.autoRetried
                    }
                }
            pending.forEach { reevaluate(it, manual = false) }
        }

        /**
         * "Continua" / "Completa lezione". Does nothing while a required exercise is missing (the screen
         * scrolls to it instead). On the last section it sends the answers and closes the lesson.
         */
        fun continueLesson() {
            val ready = _state.value as? LessonUiState.Ready ?: return
            if (!ready.isUnlocked() || ready.completing) return
            if (!ready.isLastSection) {
                mutate {
                    val next = it.saved.revealedCount
                    it.copy(saved = it.saved.copy(revealedCount = next + 1), justRevealed = next)
                }
                return
            }
            if (ready.lesson == null || ready.lesson.status == LessonStatus.Completed) {
                // A sample lesson, or one that was already closed: nothing to send.
                mutate { it.copy(saved = it.saved.copy(completed = true), showSummary = true, justRevealed = null) }
                return
            }
            // The exhausted-credit panel stays while "Completa lezione" is sent again (llm-key.md 9.1).
            mutate {
                it.copy(
                    completion = CompletionUi.Sending,
                    justRevealed = null,
                    keyNotice = it.keyNotice?.takeIf { notice -> notice.variant == LlmKeyVariant.Credit },
                )
            }
            viewModelScope.launch { complete() }
        }

        private suspend fun complete() {
            val flush = flush()
            when (flush) {
                FlushStatus.Closed, FlushStatus.Gone -> return // handled by flush()
                FlushStatus.Offline, FlushStatus.Unauthorized -> {
                    openSummary(CompletionUi.NeedsSend())
                    return
                }
                FlushStatus.Done -> Unit
            }
            closeOnBackend()
        }

        private fun openSummary(completion: CompletionUi) =
            mutate {
                it.copy(
                    saved = it.saved.copy(completed = true),
                    showSummary = true,
                    completion = completion,
                    keyNotice = null,
                )
            }

        /** `POST /lessons/{id}/complete`; the answers were sent just before. */
        private suspend fun closeOnBackend() {
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
                        ApiFailure.NotFound -> fail(LessonUiState.Gone)
                        // No key (or a refused one): the lesson stays open, with a notice instead of the summary, and
                        // the answers stay safe in the queue (llm-key.md section 5).
                        ApiFailure.LlmKeyMissing, ApiFailure.LlmKeyRejected, ApiFailure.LlmCreditExhausted ->
                            mutate { it.copy(completion = CompletionUi.None, keyNotice = notices.of(result)) }
                        else -> {
                            openSummary(CompletionUi.NeedsSend())
                            scheduleRetry()
                        }
                    }
            }
        }

        private fun markCompleted() =
            _state.update {
                if (it is LessonUiState.Ready && it.lesson != null) {
                    it.copy(lesson = it.lesson.copy(status = LessonStatus.Completed))
                } else {
                    it
                }
            }

        /** "Riprova ora" in the "risposte da inviare" panel. */
        fun retryCompletion() {
            viewModelScope.launch {
                mutate { it.copy(completion = CompletionUi.NeedsSend(retrying = true)) }
                when (flush()) {
                    FlushStatus.Done -> closeOnBackend()
                    FlushStatus.Offline, FlushStatus.Unauthorized ->
                        mutate { it.copy(completion = CompletionUi.NeedsSend()) }
                    else -> Unit
                }
            }
        }

        /** "Riprova" of a failed `lesson_completion` job (jobs.md section 3). */
        fun retryCompletionJob() {
            val ready = _state.value as? LessonUiState.Ready ?: return
            val failed = ready.completion as? CompletionUi.Failed ?: return
            if (failed.retrying) return
            mutate { it.copy(completion = failed.copy(retrying = true, networkError = false)) }
            viewModelScope.launch {
                when (val result = lessons.retryJob(failed.job.topicId, failed.job.id)) {
                    is ApiResult.Success -> {
                        mutate { it.copy(completion = CompletionUi.Running(result.value)) }
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
                            mutate {
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
                            mutate { it.copy(completion = failed.copy(retrying = false, networkError = true)) }
                        }
                }
            }
        }

        // --- sending ------------------------------------------------------------------------------------------------

        /** Empties the queue and applies the answers of the Grader. Returns how it ended. */
        private suspend fun flush(manualBlock: String? = null): FlushStatus {
            if (isSample) return FlushStatus.Done
            val result = syncer.flush(key) { saved -> applySaved(saved, manualBlock) }
            applyLeftovers(result, manualBlock)
            when (result.status) {
                FlushStatus.Done -> {
                    retryJob?.cancel()
                    setAuthError(false)
                }
                FlushStatus.Offline -> scheduleRetry()
                // A wrong token never gets better by retrying: stop, keep the queue, and say so.
                FlushStatus.Unauthorized -> setAuthError(true)
                FlushStatus.Closed -> {
                    markCompleted()
                    refreshFromBackend(closedElsewhere = true)
                    followCompletion()
                }
                FlushStatus.Gone -> fail(LessonUiState.Gone)
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
            mutate { ready ->
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
            mutate { ready ->
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

        private fun setAuthError(value: Boolean) =
            _state.update { if (it is LessonUiState.Ready && it.authError != value) it.copy(authError = value) else it }

        private fun scheduleRetry() {
            if (retryJob?.isActive == true) return
            retryJob =
                viewModelScope.launch {
                    // Keeps trying, every retryMs, for as long as something is queued and the network is the problem.
                    while (syncer.hasPending(key)) {
                        delay(dispatchers.retryMs)
                        if (!syncer.hasPending(key) || flush() != FlushStatus.Offline) break
                    }
                }
        }

        // --- highlights ---------------------------------------------------------------------------------------------

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
            val ready = _state.value as? LessonUiState.Ready ?: return emptyList()
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
            _state.update {
                if (it is LessonUiState.Ready) it.copy(highlightColor = color, eraserMode = false) else it
            }
            viewModelScope.launch { highlightSync.setColor(color) }
        }

        /**
         * Sends the highlights now (the screen is paused or closing, "Riprova ora"), without waiting for the debounce.
         * A parked set is only tried again when [retryParked]: on a new edit, on opening and on "Riprova ora".
         */
        fun syncHighlightsNow(retryParked: Boolean = false) {
            highlightDebounce?.cancel()
            viewModelScope.launch { attemptHighlights(retryParked) }
        }

        /**
         * Replaces the set with what [change] computes from the state. The set stays as it is when the result is the
         * same, or when it would grow past [MAX_HIGHLIGHTS] (highlight.md section 5).
         */
        private inline fun editHighlights(change: (LessonUiState.Ready) -> List<Highlight>): HighlightEdit {
            var outcome = HighlightEdit.Unchanged
            _state.update { current ->
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
        private fun startHighlightPersisting() {
            viewModelScope.launch {
                highlightRequests.receiveAsFlow().collect {
                    val ready = _state.value as? LessonUiState.Ready ?: return@collect
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
                        viewModelScope.launch {
                            delay(dispatchers.highlightDebounceMs)
                            attemptHighlights()
                        }
                }
            }
        }

        /** One send of the set, and what follows: retries after a failure, the indicator, the end of the lesson. */
        private suspend fun attemptHighlights(retryParked: Boolean = false) {
            if (isSample) return
            val result = highlightSync.flush(key, retryParked)
            if (result.discarded.isNotEmpty()) {
                val gone = result.discarded.map { Triple(it.blockId, it.start, it.end) }.toSet()
                Log.w("Sdui", "highlights refused by the backend and dropped: ${result.discarded.size} in '$key'")
                _state.update { current ->
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
                    setAuthError(true)
                }
                HighlightFlushStatus.Gone -> fail(LessonUiState.Gone)
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
                    viewModelScope.launch {
                        delay(dispatchers.highlightIndicatorMs)
                        setHighlightSyncFailing(true)
                    }
            }
            if (highlightRetry?.isActive == true) return
            highlightRetry =
                viewModelScope.launch {
                    for (wait in dispatchers.highlightRetryMs) {
                        delay(wait)
                        if (!highlightSync.hasPending(key)) return@launch
                        attemptHighlights()
                        if (!highlightSync.hasPending(key)) return@launch
                    }
                }
        }

        private fun setHighlightSyncFailing(value: Boolean) =
            _state.update {
                if (it is LessonUiState.Ready &&
                    it.highlightSyncFailing != value
                ) {
                    it.copy(highlightSyncFailing = value)
                } else {
                    it
                }
            }

        // --- handwritten notes --------------------------------------------------------------------------------------

        /** A finished pen stroke goes on top of the others. [InkEdit.Rejected] says which limit it would break. */
        fun writeStroke(stroke: Stroke): InkEdit = editInk { it.write(stroke) }

        /** One eraser pass over the strokes [ids] (those of `InkEraser.touched`): one undoable operation. */
        fun eraseStrokes(ids: Collection<String>): InkEdit = editInk { it.erase(ids) }

        /**
         * Takes away all the notes of [blockId] as one undoable pass (the TalkBack action, handwriting.md section 10).
         * Returns the operation that was recorded, to hand to [undoNotesClear]; null when the block had no notes.
         */
        fun clearBlockNotes(blockId: String): InkOperation? {
            val ready = _state.value as? LessonUiState.Ready ?: return null
            val ids =
                ready.ink.set.strokes
                    .filter { it.blockId == blockId }
                    .map { it.id }
            if (ids.isEmpty() || eraseStrokes(ids) !is InkEdit.Applied) return null
            return (_state.value as? LessonUiState.Ready)?.ink?.lastOperation
        }

        /** "Annulla" of [clearBlockNotes]: only while that pass is still the last operation, so nothing else is undone. */
        fun undoNotesClear(operation: InkOperation): Boolean {
            val ready = _state.value as? LessonUiState.Ready ?: return false
            return ready.ink.lastOperation == operation && undoInk() is InkEdit.Applied
        }

        fun undoInk(): InkEdit = editInk { it.undo() }

        fun redoInk(): InkEdit = editInk { it.redo() }

        /** Changing the tool switches the eraser off (handwriting.md 3.4); picking the tool already active changes nothing. */
        fun setPenTool(tool: PenTool) {
            _state.update {
                if (it is LessonUiState.Ready && it.penTool != tool) it.copy(penTool = tool, eraserMode = false) else it
            }
            viewModelScope.launch { strokeSync.setTool(tool) }
        }

        /** Choosing a colour or a width means "I want to write": the eraser goes off even if the choice is the same. */
        fun setInkColor(color: InkColor) {
            _state.update { if (it is LessonUiState.Ready) it.copy(inkColor = color, eraserMode = false) else it }
            viewModelScope.launch { strokeSync.setInkColor(color) }
        }

        fun setInkWidth(width: InkWidth) {
            _state.update { if (it is LessonUiState.Ready) it.copy(inkWidth = width, eraserMode = false) else it }
            viewModelScope.launch { strokeSync.setWidth(width) }
        }

        /** The "Gomma" switch. Not saved on the device; Undo/Redo do not touch it. */
        fun setEraserMode(on: Boolean) {
            _state.update { if (it is LessonUiState.Ready && it.eraserMode != on) it.copy(eraserMode = on) else it }
        }

        /**
         * Sends the notes now (the screen is paused or closing, "Riprova ora"), without waiting for the debounce.
         * A parked set is only tried again when [retryParked]: on a new edit, on opening and on "Riprova ora".
         */
        fun syncInkNow(retryParked: Boolean = false) {
            inkDebounce?.cancel()
            viewModelScope.launch { attemptInk(retryParked) }
        }

        /** Applies [change] to the history; only an applied edit is saved and sent. */
        private inline fun editInk(change: (InkHistory) -> InkEdit): InkEdit {
            var outcome: InkEdit = InkEdit.NoOp
            _state.update { current ->
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
        private fun startInkPersisting() {
            viewModelScope.launch {
                inkRequests.receiveAsFlow().collect {
                    if (!saveInk()) {
                        // Not on disk: the row of the notes appears and the retries write the set again.
                        scheduleInkRetries()
                        return@collect
                    }
                    inkDebounce?.cancel()
                    inkDebounce =
                        viewModelScope.launch {
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
                val ready = _state.value as? LessonUiState.Ready ?: return@withLock true
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
        private suspend fun attemptInk(retryParked: Boolean = false) {
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
                _state.update { current ->
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
                    setAuthError(true)
                }
                StrokeFlushStatus.Gone -> fail(LessonUiState.Gone)
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
                    viewModelScope.launch {
                        delay(dispatchers.highlightIndicatorMs)
                        setInkSyncFailing(true)
                    }
            }
            if (inkRetry?.isActive == true) return
            inkRetry =
                viewModelScope.launch {
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
            _state.update {
                if (it is LessonUiState.Ready && it.inkTooLarge != value) it.copy(inkTooLarge = value) else it
            }

        private fun setInkSyncFailing(value: Boolean) =
            _state.update {
                if (it is LessonUiState.Ready && it.inkSyncFailing != value) it.copy(inkSyncFailing = value) else it
            }

        // --- the closing job ----------------------------------------------------------------------------------------

        /** Polls `GET /lessons/{id}` every few seconds while `lesson_completion` is active (lessons.md section 8). */
        private fun followCompletion() {
            val ready = _state.value as? LessonUiState.Ready ?: return
            val active =
                (ready.completion as? CompletionUi.Running) != null
            if (!active || pollJob?.isActive == true) return
            pollJob =
                viewModelScope.launch {
                    while (true) {
                        delay(dispatchers.pollMs)
                        val current = _state.value as? LessonUiState.Ready ?: return@launch
                        if (current.completion !is CompletionUi.Running) return@launch
                        refreshFromBackend()
                    }
                }
        }

        /** Reads the lesson again: completion state, graded answers (the job grades the pending ones). */
        private suspend fun refreshFromBackend(closedElsewhere: Boolean = false) {
            val editsBefore = highlightEdits
            val inkEditsBefore = inkEdits
            val stamp = highlightSync.stamp()
            val inkStamp = strokeSync.stamp()
            when (val result = lessons.getLesson(key)) {
                is ApiResult.Success -> {
                    val view = result.value
                    // The highlights of the backend only matter the first time after opening from the device's copy;
                    // later reads (the polling of a closing job) never replace what the learner is marking.
                    val fresh = if (fromCache) highlightSync.resolve(key, view.highlights, stamp) else null
                    val freshInk = if (fromCache) strokeSync.resolve(key, view.strokes, inkStamp) else null
                    fromCache = false
                    val unsent = queue.pending(key).map { it.blockId to it.attemptNumber }.toSet()
                    mutate { ready ->
                        val restored =
                            AttemptRestore.apply(
                                ready.saved,
                                exercises.values.toList(),
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
                                if (fresh != null && highlightEdits == editsBefore) {
                                    fresh.map { it.toHighlight() }
                                } else {
                                    ready.highlights
                                },
                            ink =
                                if (freshInk != null && inkEdits == inkEditsBefore) {
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
                is ApiResult.Failure -> if (result.failure is ApiFailure.NotFound) fail(LessonUiState.Gone)
            }
        }

        // --- other ----------------------------------------------------------------------------------------------------

        fun dismissSummary() = mutate { it.copy(showSummary = false) }

        /** "Torna all'argomento" asks the Topic screen to open on the lessons tab (lesson.md section 7.3). */
        fun requestLessonsTab() {
            val lesson = (_state.value as? LessonUiState.Ready)?.lesson ?: return
            tabInbox.post(lesson.topicId, TopicTabNames.LESSONS)
        }

        /** "Vedi tutto nel registro": the Topic screen opens on "Cosa sai". */
        fun requestProgressTab() {
            val lesson = (_state.value as? LessonUiState.Ready)?.lesson ?: return
            tabInbox.post(lesson.topicId, TopicTabNames.PROGRESS)
        }

        /** Context for the chat when the learner asks the Tutor from this lesson; null for a sample lesson. */
        fun askRequest(prompt: String): LessonAskRequest? {
            val lesson = (_state.value as? LessonUiState.Ready)?.lesson ?: return null
            return LessonAskRequest(lesson.topicId, lesson.id, lesson.number, lesson.title, prompt)
        }

        fun animationShown() {
            _state.update {
                if (it is LessonUiState.Ready &&
                    it.justRevealed != null
                ) {
                    it.copy(justRevealed = null)
                } else {
                    it
                }
            }
        }

        fun termOpened(termKey: String) =
            mutate {
                if (termKey in
                    it.saved.openedTerms
                ) {
                    it
                } else {
                    it.copy(saved = it.saved.copy(openedTerms = it.saved.openedTerms + termKey))
                }
            }

        fun setDisclosure(
            section: String,
            open: Boolean,
        ) = mutate { it.copy(saved = it.saved.copy(disclosures = it.saved.disclosures + (section to open))) }

        fun saveScroll(
            blockId: String?,
            offsetPx: Int,
        ) = mutate { it.copy(saved = it.saved.copy(scrollBlockId = blockId, scrollOffsetPx = offsetPx)) }

        /** Writes the state now. Called when leaving the screen; the store keeps the write alive. */
        fun flushState() = persistNow()

        private fun persistNow() {
            val ready = _state.value as? LessonUiState.Ready ?: return
            val position =
                ready.saved.copy(
                    sectionNumber =
                        ready.sections.sections
                            .getOrNull(ready.currentIndex)
                            ?.number ?: 0,
                    sectionTotal = ready.sections.numberedCount,
                )
            store.save(key, position.encode())
        }

        override fun onCleared() {
            persistNow()
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

/** What a block without `lang` is written in: the Topic language, or the device language when this app does not know it. */
internal fun contentLanguageOf(language: TopicLanguage): String =
    language.wire ?: java.util.Locale
        .getDefault()
        .language
        .ifBlank { "it" }
