package com.teachme.feature.lesson

import androidx.compose.runtime.Composable
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.JobStatus
import com.teachme.core.model.KeyNotice
import com.teachme.core.model.KeyNoticeSource
import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.LessonStateStore
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.LoadError
import com.teachme.core.model.NoLlmKeyMonitor
import com.teachme.core.model.TopicJob
import com.teachme.core.model.toLoadError
import com.teachme.sdui.logic.LessonSavedState
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

sealed interface LessonsLoad {
    data object Loading : LessonsLoad

    data class Failed(
        val error: LoadError,
    ) : LessonsLoad

    data object Ready : LessonsLoad
}

/** What the screen reader is told when a generation ends; [id] makes two equal announcements distinct. */
data class GenerationAnnouncement(
    val ready: Boolean,
    val title: String?,
    val id: Int,
)

data class LessonsUiState(
    val load: LessonsLoad = LessonsLoad.Loading,
    /** `number` descending, as the backend sends them. */
    val lessons: List<Lesson> = emptyList(),
    val nextCursor: String? = null,
    val pageLoading: Boolean = false,
    val pageFailed: Boolean = false,
    /** The current `lesson_generation` job: the first of the list of jobs of that kind. */
    val job: TopicJob? = null,
    /**
     * Every `lesson_generation` job seen (first page of the list plus events), by id: the cards of the chat follow
     * their own job, which may no longer be the current one.
     */
    val generationJobs: Map<String, TopicJob> = emptyMap(),
    /** The latest closing job of each lesson, to flag "registro da riprovare" in the list. */
    val completionJobs: Map<String, TopicJob> = emptyMap(),
    /** "Genera lezione" is in flight. */
    val starting: Boolean = false,
    /** "Riprova" is in flight. */
    val retrying: Boolean = false,
    /** The last start or retry could not be sent. */
    val actionError: LoadError? = null,
    /** The last start or retry was refused because of the LLM key (llm-key.md section 5): nothing was created. */
    val keyNotice: KeyNotice? = null,
    /** The lesson that has just been generated, marked "Nuova" until the tab is left. */
    val newLessonId: String? = null,
    val announcement: GenerationAnnouncement? = null,
    /** "k di N" sections of the lessons started on this device (local state, never the backend's). */
    val sectionProgress: Map<String, Pair<Int, Int>> = emptyMap(),
) {
    fun lesson(id: String?): Lesson? = lessons.firstOrNull { it.id == id }

    /** A generation job by id, null when it is not among the loaded ones. */
    fun generationJob(id: String): TopicJob? = generationJobs[id] ?: job?.takeIf { it.id == id }
}

/** Upserts [job] by id; an older state of the same job (replayed event, late answer) never replaces a newer one. */
internal fun Map<String, TopicJob>.upsertJob(job: TopicJob): Map<String, TopicJob> {
    val known = this[job.id]
    return if (known == null || job.supersedes(known)) this + (job.id to job) else this
}

sealed interface LessonsEffect {
    /** A generation ended well: the Topic screen shows a toast unless the learner is already looking at the lessons. */
    data class Ready(
        val number: Int?,
        /** The job that ended: the chat card of the same job already says it (lessons.md section 12). */
        val jobId: String? = null,
    ) : LessonsEffect

    data class Failed(
        val jobId: String? = null,
    ) : LessonsEffect
}

private const val STREAM_GRACE_MS = 3_000L
private const val RELOAD_RETRY_MS = 3_000L

/**
 * Lessons of one Topic and their generation. Fed by REST (`GET .../lessons`, `GET .../jobs`) and by the `job_updated` events
 * that the chat forwards through [onEvent]; events upsert the job by id and are safe to replay. When a generation
 * succeeds the first page is fetched again (lessons.md section 8).
 */
@HiltViewModel(assistedFactory = LessonsViewModel.Factory::class)
class LessonsViewModel
    @AssistedInject
    constructor(
        @Assisted private val topicId: String,
        private val repository: LessonRepository,
        private val store: LessonStateStore,
        /** Told when a request meets an exhausted credit (the key notice is born here, not in the panel). */
        private val llmKey: LlmKeyMonitor = NoLlmKeyMonitor,
    ) : ViewModel() {
        @AssistedFactory
        interface Factory {
            fun create(topicId: String): LessonsViewModel
        }

        private val _state = MutableStateFlow(LessonsUiState())
        val state: StateFlow<LessonsUiState> = _state.asStateFlow()

        private val effectChannel = Channel<LessonsEffect>(Channel.BUFFERED)
        val effects: Flow<LessonsEffect> = effectChannel.receiveAsFlow()

        private var fetchedOnce = false
        private var loadJob: Job? = null
        private var pageJob: Job? = null
        private var announcementCounter = 0

        /** The first load rides on the stream opening (a resync); this covers a stream that never opens. */
        fun loadIfStreamSilent() {
            if (fetchedOnce) return
            viewModelScope.launch {
                delay(STREAM_GRACE_MS)
                if (!fetchedOnce) load()
            }
        }

        fun retryLoad() {
            _state.update { it.copy(load = LessonsLoad.Loading) }
            load()
        }

        /** An event of the Topic stream (forwarded by the chat). Events of other types are ignored. */
        fun onEvent(event: ChatEvent) {
            when (event) {
                is ChatEvent.JobUpdated -> onJob(event.job)
                // Sent on every (re)connection, so the list is also fresh when the app comes back.
                ChatEvent.ResyncRequired -> load()
                else -> Unit
            }
        }

        /** The tab was left: the "Nuova" mark goes away (topic.md F3.C). */
        fun markSeen() = _state.update { if (it.newLessonId == null) it else it.copy(newLessonId = null) }

        private fun load(retryOnce: Boolean = false) {
            fetchedOnce = true
            loadJob?.cancel()
            loadJob =
                viewModelScope.launch {
                    // After a generation the new lesson must show up: one more try a few seconds later if this fails.
                    if (!loadNow() && retryOnce) {
                        delay(RELOAD_RETRY_MS)
                        loadNow()
                    }
                }
        }

        /** Reads lessons and jobs once; false when the lessons could not be read. */
        private suspend fun loadNow(): Boolean =
            coroutineScope {
                run {
                    val lessons = async { repository.listLessons(topicId) }
                    val jobs = async { repository.listJobs(topicId) }
                    val lessonsResult = lessons.await()
                    val jobsResult = jobs.await()
                    val progress =
                        (lessonsResult as? ApiResult.Success)
                            ?.value
                            ?.items
                            .orEmpty()
                            .let { localProgress(it) }
                    when (lessonsResult) {
                        is ApiResult.Success -> {
                            val page = lessonsResult.value
                            _state.update { s ->
                                val withJobs =
                                    if (jobsResult is ApiResult.Success) applyJobs(s, jobsResult.value) else s
                                withJobs.copy(
                                    sectionProgress = progress,
                                    load = LessonsLoad.Ready,
                                    lessons = mergeFirstPage(s.lessons, page.items),
                                    nextCursor =
                                        if (s.lessons.size >
                                            page.items.size
                                        ) {
                                            s.nextCursor
                                        } else {
                                            page.nextCursor
                                        },
                                    pageFailed = false,
                                )
                            }
                        }
                        // A failed refresh keeps what is on screen; only the first load shows an error.
                        is ApiResult.Failure ->
                            _state.update {
                                if (it.load ==
                                    LessonsLoad.Ready
                                ) {
                                    it
                                } else {
                                    it.copy(load = LessonsLoad.Failed(lessonsResult.failure.toLoadError()))
                                }
                            }
                    }
                    lessonsResult is ApiResult.Success
                }
            }

        /** Reads the local reading position of the lessons in progress; a damaged state is just absent. */
        private suspend fun localProgress(lessons: List<Lesson>): Map<String, Pair<Int, Int>> =
            lessons
                .filter { it.status == LessonStatus.InProgress }
                .mapNotNull { lesson ->
                    val saved =
                        runCatching { LessonSavedState.decode(store.load(lesson.id)) }.getOrNull()
                            ?: return@mapNotNull null
                    if (saved.sectionNumber > 0 &&
                        saved.sectionTotal > 0
                    ) {
                        lesson.id to (saved.sectionNumber to saved.sectionTotal)
                    } else {
                        null
                    }
                }.toMap()

        /** The refreshed first page replaces the head of the list; pages already loaded after it are kept. */
        private fun mergeFirstPage(
            old: List<Lesson>,
            first: List<Lesson>,
        ): List<Lesson> {
            if (old.isEmpty() || first.isEmpty()) return first
            val ids = first.map { it.id }.toSet()
            // Lessons after the first page of the old list stay (older numbers); the head is replaced.
            val tail = old.drop(first.size).filter { it.id !in ids && it.number < first.last().number }
            return first + tail
        }

        private fun applyJobs(
            s: LessonsUiState,
            jobs: List<TopicJob>,
        ): LessonsUiState {
            val generation = jobs.firstOrNull { it.isLessonGeneration }
            val completions =
                jobs
                    .filter { it.isLessonCompletion && it.lessonId != null }
                    .groupBy { it.lessonId!! }
                    .mapValues { (_, list) -> list.maxBy { it.createdAt } }
            val previous = s.job
            // The REST list is the server's truth: it overwrites, whatever the rank (a retry from another client).
            return s.copy(
                generationJobs =
                    jobs.filter { it.isLessonGeneration }.fold(s.generationJobs) { m, j -> m + (j.id to j) },
                job = generation ?: previous,
                completionJobs = completions,
            )
        }

        fun loadMore() {
            val cursor = _state.value.nextCursor ?: return
            if (_state.value.pageLoading || pageJob?.isActive == true) return
            _state.update { it.copy(pageLoading = true, pageFailed = false) }
            pageJob =
                viewModelScope.launch {
                    when (val result = repository.listLessons(topicId, cursor)) {
                        is ApiResult.Success ->
                            _state.update {
                                val known = it.lessons.map { l -> l.id }.toSet()
                                it.copy(
                                    lessons = it.lessons + result.value.items.filter { l -> l.id !in known },
                                    nextCursor = result.value.nextCursor,
                                    pageLoading = false,
                                )
                            }
                        is ApiResult.Failure -> _state.update { it.copy(pageLoading = false, pageFailed = true) }
                    }
                }
        }

        private fun onJob(job: TopicJob) {
            if (job.topicId != topicId) return
            if (job.isLessonCompletion) {
                val lessonId = job.lessonId ?: return
                _state.update { s ->
                    val known = s.completionJobs[lessonId]
                    if (known != null && known.id != job.id && job.createdAt < known.createdAt) {
                        s
                    } else {
                        s.copy(completionJobs = s.completionJobs + (lessonId to job))
                    }
                }
                return
            }
            if (!job.isLessonGeneration) return
            _state.update { it.copy(generationJobs = it.generationJobs.upsertJob(job)) }
            val previous = _state.value.job
            if (previous != null && !job.supersedes(previous)) return
            val justSucceeded =
                job.status == JobStatus.Succeeded && !(previous?.id == job.id && previous.status == job.status)
            val justFailed = job.status == JobStatus.Failed && previous?.id == job.id && previous.isActive
            _state.update {
                it.copy(
                    job = job,
                    actionError = if (job.isActive) null else it.actionError,
                    starting = if (job.isActive) false else it.starting,
                    retrying = if (job.isActive) false else it.retrying,
                    newLessonId = if (justSucceeded) job.result?.lessonId ?: it.newLessonId else it.newLessonId,
                    announcement =
                        if (justSucceeded || justFailed) {
                            GenerationAnnouncement(justSucceeded, null, ++announcementCounter)
                        } else {
                            it.announcement
                        },
                )
            }
            if (justSucceeded) {
                load(retryOnce = true)
                effectChannel.trySend(LessonsEffect.Ready(job.result?.lessonNumber, job.id))
            } else if (justFailed) {
                effectChannel.trySend(LessonsEffect.Failed(job.id))
            }
        }

        /** "Genera lezione" (`POST /topics/{id}/lessons`). */
        fun generate() = act { repository.startGeneration(topicId) }

        /** "Riprova": the failed job goes back in the queue (`POST .../jobs/{id}/retry`). */
        fun retry() {
            val job = _state.value.job ?: return
            if (job.status != JobStatus.Failed) return
            act(retrying = true) { repository.retryJob(topicId, job.id) }
        }

        private val notices = KeyNoticeSource(llmKey)

        private fun act(
            retrying: Boolean = false,
            call: suspend () -> ApiResult<TopicJob>,
        ) {
            val s = _state.value
            if (s.starting || s.retrying) return
            _state.update {
                it.copy(
                    starting = !retrying,
                    retrying = retrying,
                    actionError = null,
                    // The exhausted-credit panel stays while the action goes out again (llm-key.md 9.1).
                    keyNotice = it.keyNotice?.takeIf { notice -> notice.variant == LlmKeyVariant.Credit },
                )
            }
            viewModelScope.launch {
                when (val result = call()) {
                    is ApiResult.Success -> {
                        val job = result.value
                        // The same guard as an event: a late REST answer never takes back what the stream already said.
                        _state.update {
                            val known = it.job
                            it.copy(
                                job = if (known == null || job.supersedes(known)) job else known,
                                generationJobs =
                                    if (job.isLessonGeneration) {
                                        it.generationJobs.upsertJob(
                                            job,
                                        )
                                    } else {
                                        it.generationJobs
                                    },
                                starting = false,
                                retrying = false,
                                keyNotice = null,
                            )
                        }
                    }
                    is ApiResult.Failure -> {
                        val keyNotice = notices.of(result)
                        _state.update { it.copy(starting = false, retrying = false, keyNotice = keyNotice) }
                        // With a key notice the panel says it.
                        val moved = result.failure == ApiFailure.Conflict || result.failure == ApiFailure.NotFound
                        if (keyNotice == null && moved) {
                            // Another client already started it (or the state moved): look at the truth, in silence.
                            load()
                        } else if (keyNotice == null) {
                            _state.update { it.copy(actionError = result.failure.toLoadError()) }
                        }
                    }
                }
            }
        }
    }

/** The ViewModel of the lessons of [topicId]; the Topic screen shares it between the tab and the context pane. */
@Composable
fun rememberLessonsViewModel(topicId: String): LessonsViewModel =
    hiltViewModel<LessonsViewModel, LessonsViewModel.Factory>(key = "lessons-$topicId") { it.create(topicId) }
