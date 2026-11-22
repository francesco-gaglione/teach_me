package com.teachme.feature.resources

import androidx.compose.runtime.Composable
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.JobStatus
import com.teachme.core.model.KeyNotice
import com.teachme.core.model.KeyNoticeSource
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.LoadError
import com.teachme.core.model.NoLlmKeyMonitor
import com.teachme.core.model.ResourceRepository
import com.teachme.core.model.ResourcesView
import com.teachme.core.model.TopicJob
import com.teachme.core.model.budgetResetAt
import com.teachme.core.model.toLoadError
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.time.Instant

sealed interface ResourcesLoad {
    data object Loading : ResourcesLoad

    data class Failed(
        val error: LoadError,
    ) : ResourcesLoad

    data object Ready : ResourcesLoad
}

/** What the screen reader is told when the research ends; [id] makes two equal announcements distinct. */
data class ResearchAnnouncement(
    val ready: Boolean,
    val count: Int,
    val id: Int,
)

data class ResourcesUiState(
    val load: ResourcesLoad = ResourcesLoad.Loading,
    val view: ResourcesView? = null,
    /** "Riprova" or "Aggiorna le fonti" is in flight. */
    val acting: Boolean = false,
    /** The last start/retry could not be sent (network or server); the failed panel says so. */
    val actionError: LoadError? = null,
    /** The last start/retry was refused because of the LLM key (llm-key.md section 5): nothing was created. */
    val keyNotice: KeyNotice? = null,
    /** The last start/retry was refused for the daily LLM budget: when it comes back (llm-key.md section 10.1). */
    val budgetResetAt: Instant? = null,
    /** False while the list of a job that has just succeeded is being fetched again. */
    val settled: Boolean = true,
    /** URLs that are new compared with the previous research, marked "Nuova" until the tab is left. */
    val newUrls: Set<String> = emptySet(),
    val announcement: ResearchAnnouncement? = null,
)

sealed interface ResourcesEffect {
    /** The research succeeded; [refresh] is true when there were sources before (an update, not the first search). */
    data class Succeeded(
        val count: Int,
        val refresh: Boolean,
    ) : ResourcesEffect

    data object Failed : ResourcesEffect
}

/** Progress-only changes are applied at most this often (resources.md 4.2). */
internal const val PROGRESS_COALESCE_MS = 1_000L
private const val RELOAD_DELAYS = 2
private const val MAX_STALE_RELOADS = 2
private const val STREAM_GRACE_MS = 3_000L

/**
 * Sources of one Topic and the state of their research. Fed by REST (`GET /resources`) and by the `job_updated` events
 * that the chat forwards through [onEvent]. Events upsert the research job by id and are safe to replay; when the job
 * succeeds the list is fetched again.
 */
@HiltViewModel(assistedFactory = ResourcesViewModel.Factory::class)
class ResourcesViewModel
    @AssistedInject
    constructor(
        @Assisted private val topicId: String,
        private val resources: ResourceRepository,
        /** Told when a request meets an exhausted credit (the key notice is born here, not in the panel). */
        private val llmKey: LlmKeyMonitor = NoLlmKeyMonitor,
        private val wallClock: AppClock = AppClock { System.currentTimeMillis() },
    ) : ViewModel() {
        @AssistedFactory
        interface Factory {
            fun create(topicId: String): ResourcesViewModel
        }

        private val _state = MutableStateFlow(ResourcesUiState())
        val state: StateFlow<ResourcesUiState> = _state.asStateFlow()

        private val effectChannel = Channel<ResourcesEffect>(Channel.BUFFERED)
        val effects: Flow<ResourcesEffect> = effectChannel.receiveAsFlow()

        /** Monotonic millisecond clock; replaced in tests. */
        internal var clock: () -> Long = { System.nanoTime() / NANOS_PER_MS }

        private var fetchedOnce = false
        private var loadJob: Job? = null
        private var reloadJob: Job? = null
        private var flushJob: Job? = null
        private var pendingProgress: TopicJob? = null
        private var lastApplied = Long.MIN_VALUE
        private var announcementCounter = 0
        private val notices = KeyNoticeSource(llmKey)

        /** Stream events that changed the state: a fetch that started before one may be stale. */
        private var eventCounter = 0

        private fun startLoading() {
            fetchedOnce = true
            load()
        }

        /**
         * The first load normally rides on the stream opening (the chat forwards that as a resync, and every reconnection
         * reloads again, so a screen back in the foreground refreshes too); this covers a stream that never opens.
         */
        fun loadIfStreamSilent() {
            if (fetchedOnce) return
            viewModelScope.launch {
                delay(STREAM_GRACE_MS)
                if (!fetchedOnce) startLoading()
            }
        }

        fun retryLoad() {
            _state.update { it.copy(load = ResourcesLoad.Loading) }
            load()
        }

        /** Reads the list again without touching what is on screen (after a PDF was read: resources.md 13.4). */
        fun refresh() = load()

        /** The tab was left: the "Nuova" marks go away (resources.md 6.2). */
        fun markSeen() = _state.update { if (it.newUrls.isEmpty()) it else it.copy(newUrls = emptySet()) }

        /** True when the last fetch finished after newer events and could not be repeated any more. */
        private var fetchStale = false

        private suspend fun fetch(): ApiResult<ResourcesView> {
            var attempts = 0
            while (true) {
                val epoch = eventCounter
                val result = resources.getResources(topicId)
                if (result is ApiResult.Success && epoch != eventCounter && attempts++ < MAX_STALE_RELOADS) continue
                fetchStale = result is ApiResult.Success && epoch != eventCounter
                return result
            }
        }

        private fun load() {
            loadJob?.cancel()
            loadJob =
                viewModelScope.launch {
                    when (val result = fetch()) {
                        is ApiResult.Success -> applyView(result.value, settled = true, keepNewer = fetchStale)
                        // A failed refresh keeps what is on screen; only the first load shows an error.
                        is ApiResult.Failure ->
                            _state.update {
                                it.copy(
                                    load =
                                        if (it.load != ResourcesLoad.Ready) {
                                            ResourcesLoad.Failed(result.failure.toLoadError())
                                        } else {
                                            it.load
                                        },
                                    // Nobody else is going to settle a list that is still being fetched after a success.
                                    settled = it.settled || reloadJob?.isActive != true,
                                )
                            }
                    }
                }
        }

        /** Installs a freshly fetched [view]; the "Nuova" marks are computed against the list it replaces. */
        private fun applyView(
            view: ResourcesView,
            settled: Boolean,
            keepNewer: Boolean = false,
        ) {
            flushJob?.cancel()
            flushJob = null
            pendingProgress = null
            _state.update { s ->
                val old = s.view
                val current = s.view?.job
                // A fetch that started before newer events must not take the job back (same id: never lower rank).
                val fetched = view.job
                val keepCurrent =
                    keepNewer &&
                        current != null &&
                        fetched != null &&
                        (
                            current.createdAt > fetched.createdAt ||
                                (current.id == fetched.id && current.rank() > fetched.rank())
                        )
                val merged = if (keepCurrent) view.copy(job = current) else view
                val marks =
                    if (old != null && old.research?.jobId != view.research?.jobId) {
                        newUrls(old.resources, view.resources)
                    } else {
                        s.newUrls
                    }
                s.copy(
                    load = ResourcesLoad.Ready,
                    view = merged,
                    settled = settled,
                    newUrls = marks,
                    acting = if (view.job?.isActive == true) false else s.acting,
                )
            }
        }

        /** An event of the Topic stream (forwarded by the chat). Events of other types are ignored. */
        fun onEvent(event: ChatEvent) {
            when (event) {
                is ChatEvent.JobUpdated -> onJob(event.job)
                // Sent on every (re)connection of the stream, so the list is also refreshed when the app comes back.
                ChatEvent.ResyncRequired -> {
                    fetchedOnce = true
                    load()
                }
                else -> Unit
            }
        }

        private fun onJob(job: TopicJob) {
            if (!job.isResearch || job.topicId != topicId) return
            eventCounter++
            val previous = _state.value.view?.job
            // An older job than the one we know never replaces it (a replayed event).
            if (previous != null && previous.id != job.id && job.createdAt < previous.createdAt) return
            val progressOnly =
                previous != null &&
                    previous.id == job.id &&
                    previous.status == job.status &&
                    job.isActive &&
                    previous.progress?.step == job.progress?.step
            if (progressOnly) {
                val wait = PROGRESS_COALESCE_MS - (clock() - lastApplied)
                if (wait > 0) {
                    pendingProgress = job
                    if (flushJob?.isActive != true) {
                        flushJob =
                            viewModelScope.launch {
                                delay(wait)
                                pendingProgress?.let(::applyJob)
                            }
                    }
                    return
                }
            }
            applyJob(job)
        }

        private fun applyJob(job: TopicJob) {
            flushJob?.cancel()
            flushJob = null
            pendingProgress = null
            lastApplied = clock()
            val before = _state.value.view
            val previous = before?.job
            if (previous != null && !job.supersedes(previous)) return
            val hadSources = before?.resources?.isNotEmpty() == true
            val wasActive = previous?.id == job.id && previous.isActive
            val justSucceeded =
                job.status == JobStatus.Succeeded && !(previous?.id == job.id && previous.status == job.status)
            if (before == null) {
                // Not loaded yet: the fetch in flight (or the next one) carries this job.
                return
            }
            _state.update {
                it.copy(
                    view = before.copy(job = job),
                    actionError = if (job.isActive) null else it.actionError,
                    acting = if (job.isActive) false else it.acting,
                    settled = !justSucceeded && it.settled,
                )
            }
            when {
                justSucceeded -> reloadAfterSuccess(job, hadSources)
                job.status == JobStatus.Failed && wasActive -> {
                    announce(ready = false, count = 0)
                    effectChannel.trySend(ResourcesEffect.Failed)
                }
            }
        }

        /** The job succeeded: fetch the new list (a couple of retries), then tell the screen. */
        private fun reloadAfterSuccess(
            job: TopicJob,
            hadSources: Boolean,
        ) {
            reloadJob?.cancel()
            reloadJob =
                viewModelScope.launch {
                    var delayMs = RELOAD_BASE_DELAY_MS
                    repeat(RELOAD_DELAYS + 1) { attempt ->
                        val result = resources.getResources(topicId)
                        if (result is ApiResult.Success) {
                            // The fetch may predate the end of the job; trust the event for the job itself.
                            val view = result.value.let { v -> if (v.job?.id == job.id) v else v.copy(job = job) }
                            applyView(view, settled = true)
                            val count = job.result?.resourceCount ?: view.resources.size
                            announce(ready = true, count = count)
                            effectChannel.send(ResourcesEffect.Succeeded(count, refresh = hadSources))
                            return@launch
                        }
                        if (attempt < RELOAD_DELAYS) {
                            delay(delayMs)
                            delayMs *= 2
                        }
                    }
                    // Gave up: show the old list; the next start or resync fetches again.
                    _state.update { it.copy(settled = true) }
                }
        }

        private fun announce(
            ready: Boolean,
            count: Int,
        ) = _state.update { it.copy(announcement = ResearchAnnouncement(ready, count, ++announcementCounter)) }

        /** "Aggiorna le fonti" / "Cerca di nuovo": a new research (`POST .../resources/research`). */
        fun startResearch() = act { resources.startResearch(topicId) }

        /** "Riprova" on a failed research: the same job goes back in the queue. */
        fun retry() {
            val job = _state.value.view?.job ?: return
            if (job.status != JobStatus.Failed) return
            act { resources.retryJob(topicId, job.id) }
        }

        private fun act(call: suspend () -> ApiResult<TopicJob>) {
            if (_state.value.acting) return
            // The exhausted-credit panel stays while the action goes out again (llm-key.md 9.1).
            _state.update {
                it.copy(
                    acting = true,
                    actionError = null,
                    keyNotice = it.keyNotice?.takeIf { notice -> notice.variant == LlmKeyVariant.Credit },
                )
            }
            viewModelScope.launch {
                when (val result = call()) {
                    is ApiResult.Success -> {
                        val job = result.value
                        eventCounter++
                        if (_state.value.view == null) {
                            _state.update { it.copy(acting = false, keyNotice = null, budgetResetAt = null) }
                            load()
                        } else {
                            // The answer is the job we just created or requeued: show it right away.
                            lastApplied = Long.MIN_VALUE
                            applyJob(job)
                            _state.update { it.copy(acting = false, keyNotice = null, budgetResetAt = null) }
                        }
                    }
                    is ApiResult.Failure ->
                        if (result.failure == ApiFailure.Conflict || result.failure == ApiFailure.NotFound) {
                            // Another client already started it (or the state moved): look at the truth, in silence.
                            _state.update { it.copy(acting = false, keyNotice = null, budgetResetAt = null) }
                            load()
                        } else {
                            val keyNotice = notices.of(result)
                            val budgetReset = result.failure.budgetResetAt(wallClock.nowMs())
                            _state.update {
                                if (keyNotice != null) {
                                    it.copy(acting = false, keyNotice = keyNotice, budgetResetAt = null)
                                } else if (budgetReset != null) {
                                    // The budget panel says it (llm-key.md 10.1): no generic error.
                                    it.copy(acting = false, keyNotice = null, budgetResetAt = budgetReset)
                                } else {
                                    it.copy(
                                        acting = false,
                                        keyNotice = null,
                                        budgetResetAt = null,
                                        actionError = result.failure.toLoadError(),
                                    )
                                }
                            }
                        }
                }
            }
        }
    }

private const val NANOS_PER_MS = 1_000_000L

/** Queued < running < finished: for the same job id the state only moves forward. */
private fun TopicJob.rank(): Int =
    when (status) {
        JobStatus.Queued -> 0
        JobStatus.Succeeded, JobStatus.Failed -> 2
        else -> 1
    }

/**
 * Whether this update of the job may replace [previous]. Another job wins if it is newer; the same job never goes
 * back (a replayed event or a late HTTP answer), except the retry: failed goes back to queued.
 */
internal fun TopicJob.supersedes(previous: TopicJob): Boolean =
    when {
        id != previous.id -> createdAt >= previous.createdAt
        previous.status == JobStatus.Failed && status == JobStatus.Queued -> true
        else -> rank() >= previous.rank()
    }

private const val RELOAD_BASE_DELAY_MS = 1_000L

/** The ViewModel of the sources of [topicId]; the Topic screen shares it between the tab and the pointers. */
@Composable
fun rememberResourcesViewModel(topicId: String): ResourcesViewModel =
    hiltViewModel<ResourcesViewModel, ResourcesViewModel.Factory>(key = "resources-$topicId") { it.create(topicId) }
