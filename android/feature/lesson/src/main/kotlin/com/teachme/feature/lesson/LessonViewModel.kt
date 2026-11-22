package com.teachme.feature.lesson

import android.util.Log
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.AttemptQueue
import com.teachme.core.model.KeyNoticeSource
import com.teachme.core.model.LessonDocumentSource
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.LessonStateStore
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LessonView
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.NoLlmKeyMonitor
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.TopicTabInbox
import com.teachme.core.model.TopicTabNames
import com.teachme.sdui.highlight.Highlight
import com.teachme.sdui.highlight.HighlightColor
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkEdit
import com.teachme.sdui.ink.InkHistory
import com.teachme.sdui.ink.InkOperation
import com.teachme.sdui.ink.InkSet
import com.teachme.sdui.ink.InkWidth
import com.teachme.sdui.ink.Stroke
import com.teachme.sdui.logic.AttemptRestore
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.LessonProgress
import com.teachme.sdui.logic.LessonSavedState
import com.teachme.sdui.logic.SectionBuilder
import com.teachme.sdui.model.LessonDocument
import com.teachme.sdui.model.LessonParser
import com.teachme.sdui.model.ParseResult
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import javax.inject.Inject

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
        syncer: AttemptSyncer,
        private val highlightSync: HighlightSyncer,
        private val strokeSync: StrokeSyncer,
        private val queue: AttemptQueue,
        private val tabInbox: TopicTabInbox,
        /** Told when the Grader evaluated an answer: the service works, so a remembered credit or key problem is stale. */
        llmKey: LlmKeyMonitor = NoLlmKeyMonitor,
        clock: AppClock = AppClock { System.currentTimeMillis() },
    ) : ViewModel() {
        /** Key of the lesson, from the navigation argument of the same name: the lesson id, or a sample's key. */
        val key: String = handle.get<String>("key").orEmpty()

        private val _state = MutableStateFlow<LessonUiState>(LessonUiState.Loading)
        val state: StateFlow<LessonUiState> = _state.asStateFlow()

        private val persistRequests = Channel<Unit>(Channel.CONFLATED)
        private val host = LessonStateHost(key, viewModelScope, dispatchers, _state, persistRequests)
        private val isSample = host.isSample
        private val highlights = HighlightsController(host, highlightSync)
        private val inkNotes = LessonInkNotes(host, strokeSync)
        private val submitter: AttemptSubmitter =
            AttemptSubmitter(host, syncer, llmKey) { completion.onClosedElsewhere() }
        private val completion: LessonCompletion =
            LessonCompletion(
                host,
                lessons,
                queue,
                clock,
                KeyNoticeSource(llmKey),
                submitter,
                highlights,
                inkNotes,
                highlightSync,
                strokeSync,
            )

        init {
            startPersisting()
            highlights.startHighlightPersisting()
            inkNotes.startInkPersisting()
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
                            host.fromCache = true
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
            host.exercises = sections.sections.flatMap { it.exercises }.associateBy { it.id }
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
                saved = AttemptRestore.apply(saved, host.exercises.values.toList(), view.attempts, unsent)
                if (view.lesson.status == LessonStatus.Completed) {
                    saved = saved.copy(revealedCount = sections.sections.size, completed = true)
                }
            }
            val shown =
                when {
                    view == null -> emptyList()
                    // Opened from the copy on the device: whatever this device holds is all there is.
                    host.fromCache -> highlightSync.local(key)
                    else -> highlightSync.resolve(key, view.highlights, checkNotNull(stamp))
                }
            val shownInk =
                when {
                    view == null -> emptyList()
                    host.fromCache -> strokeSync.local(key)
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
                    completion = if (view != null) completion.completionOf(view, saved) else CompletionUi.None,
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
                viewModelScope.launch { highlights.attemptHighlights(retryParked = true) }
                viewModelScope.launch { inkNotes.attemptInk(retryParked = true) }
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
                val status = submitter.flush()
                if (ready.saved.completed && ready.lesson?.status != LessonStatus.Completed) {
                    // Closing with answers still queued would make them fail with 409: only after a clean flush.
                    if (status == FlushStatus.Done) completion.closeOnBackend()
                } else {
                    completion.followCompletion()
                }
            }
        }

        /** The screen came back to the foreground: send what is queued, grade pending answers once, follow the job. */
        fun onResume() {
            val ready = _state.value as? LessonUiState.Ready ?: return
            if (ready.lesson == null) return
            viewModelScope.launch {
                // Opened from the copy on the device: the backend's attempts and status are read as soon as it answers.
                highlights.attemptHighlights()
                inkNotes.attemptInk()
                if (host.fromCache) completion.refreshFromBackend()
                submitter.gradePendingOnce()
                val status = submitter.flush()
                if (status == FlushStatus.Done &&
                    ready.saved.completed &&
                    (ready.completion as? CompletionUi.NeedsSend) != null
                ) {
                    completion.closeOnBackend()
                }
                completion.followCompletion()
            }
        }

        /** "Riprova ora" of the queue popover. */
        fun retryQueueNow() {
            viewModelScope.launch { submitter.flush() }
            // The learner asks for it: a set the backend refused is tried once more.
            highlights.syncHighlightsNow(retryParked = true)
            inkNotes.syncInkNow(retryParked = true)
        }

        // --- state changes by the learner ---------------------------------------------------------------------------

        private inline fun mutate(transform: (LessonUiState.Ready) -> LessonUiState.Ready) = host.mutate(transform)

        fun updateExercise(
            blockId: String,
            transform: (ExerciseState) -> ExerciseState,
        ) = submitter.updateExercise(blockId, transform)

        /** True for exercises of a section the learner already left with "Continua". */
        fun isLocked(blockId: String): Boolean {
            val ready = _state.value as? LessonUiState.Ready ?: return false
            val section = ready.sections.sections.firstOrNull { s -> s.items.any { it.id == blockId } } ?: return false
            return LessonProgress.isLocked(section.index, ready.saved.revealedCount)
        }

        fun reevaluate(blockId: String) = submitter.reevaluate(blockId)

        /** "Continua" / "Completa lezione": see [LessonCompletion.continueLesson]. */
        fun continueLesson() = completion.continueLesson()

        fun retryCompletion() = completion.retryCompletion()

        fun retryCompletionJob() = completion.retryCompletionJob()

        // --- highlights and handwritten notes -----------------------------------------------------------------------

        fun addHighlight(
            blockId: String,
            start: Int,
            end: Int,
            color: HighlightColor? = null,
        ): HighlightEdit = highlights.addHighlight(blockId, start, end, color)

        fun eraseHighlight(
            blockId: String,
            start: Int,
            end: Int,
        ): HighlightEdit = highlights.eraseHighlight(blockId, start, end)

        fun removeHighlightAt(
            blockId: String,
            position: Int,
        ): List<Highlight> = highlights.removeHighlightAt(blockId, position)

        fun restoreHighlights(removed: List<Highlight>): HighlightEdit = highlights.restoreHighlights(removed)

        fun setHighlightColor(color: HighlightColor) = highlights.setHighlightColor(color)

        fun syncHighlightsNow(retryParked: Boolean = false) = highlights.syncHighlightsNow(retryParked)

        fun writeStroke(stroke: Stroke): InkEdit = inkNotes.writeStroke(stroke)

        fun eraseStrokes(ids: Collection<String>): InkEdit = inkNotes.eraseStrokes(ids)

        fun clearBlockNotes(blockId: String): InkOperation? = inkNotes.clearBlockNotes(blockId)

        fun undoNotesClear(operation: InkOperation): Boolean = inkNotes.undoNotesClear(operation)

        fun undoInk(): InkEdit = inkNotes.undoInk()

        fun redoInk(): InkEdit = inkNotes.redoInk()

        fun setPenTool(tool: PenTool) = inkNotes.setPenTool(tool)

        fun setInkColor(color: InkColor) = inkNotes.setInkColor(color)

        fun setInkWidth(width: InkWidth) = inkNotes.setInkWidth(width)

        fun setEraserMode(on: Boolean) = inkNotes.setEraserMode(on)

        fun syncInkNow(retryParked: Boolean = false) = inkNotes.syncInkNow(retryParked)

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

/** What a block without `lang` is written in: the Topic language, or the device language when this app does not know it. */
internal fun contentLanguageOf(language: TopicLanguage): String =
    language.wire ?: java.util.Locale
        .getDefault()
        .language
        .ifBlank { "it" }
