package com.teachme.feature.resources

import androidx.compose.runtime.Composable
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.DocumentBannerStore
import com.teachme.core.model.DocumentRepository
import com.teachme.core.model.DocumentStatus
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LoadError
import com.teachme.core.model.TopicDocument
import com.teachme.core.model.TopicJob
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.UploadSource
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

sealed interface DocumentsLoad {
    data object Loading : DocumentsLoad

    data class Failed(
        val error: LoadError,
    ) : DocumentsLoad

    data object Ready : DocumentsLoad
}

/** The upload as the section shows it (resources.md 13.4 and 13.6). */
sealed interface UploadUi {
    data object Idle : UploadUi

    /** From the tap on the file until the `202`: the local row "Carico il file…". */
    data class Sending(
        val name: String,
    ) : UploadUi

    /** No row was created; [error] decides the panel. */
    data class Failed(
        val error: UploadError,
    ) : UploadUi
}

enum class DeleteError { Network, Busy, Other }

sealed interface DeleteUi {
    data object Idle : DeleteUi

    data class Deleting(
        val id: String,
    ) : DeleteUi

    data class Failed(
        val id: String,
        val error: DeleteError,
    ) : DeleteUi
}

/** What the screen reader is told when an import ends; [id] makes two equal announcements distinct. */
sealed interface DocumentAnnouncement {
    val id: Int

    data class Read(
        val title: String,
        val pages: Int?,
        override val id: Int,
    ) : DocumentAnnouncement

    data class Failed(
        val failure: DocumentFailure,
        override val id: Int,
    ) : DocumentAnnouncement
}

data class DocumentsUiState(
    val load: DocumentsLoad = DocumentsLoad.Loading,
    val documents: List<TopicDocument> = emptyList(),
    /** The newest `document_import` job: the phase of the document being read, and what "Riprova" retries. */
    val importJob: TopicJob? = null,
    /** Import jobs that name their document (`document_id`), by document id. */
    val jobs: Map<String, TopicJob> = emptyMap(),
    val upload: UploadUi = UploadUi.Idle,
    val delete: DeleteUi = DeleteUi.Idle,
    /** "Riprova" on a failed import is in flight. */
    val retrying: Boolean = false,
    /** Documents for which the banner "Ho letto il tuo PDF" was dismissed (kept on the device). */
    val dismissedBanner: Set<String> = emptySet(),
    val announcement: DocumentAnnouncement? = null,
    /** The Topic's content language: a document in the same language needs no label. */
    val topicLanguage: TopicLanguage = TopicLanguage.It,
)

sealed interface DocumentsEffect {
    /** An import succeeded; the sources are reloaded and a toast says so. */
    data class Imported(
        val title: String,
        val pages: Int?,
    ) : DocumentsEffect

    data object Removed : DocumentsEffect

    /** The Topic does not exist any more (404 on upload). */
    data object TopicGone : DocumentsEffect

    data object RetryFailed : DocumentsEffect
}

private const val RELOAD_ATTEMPTS = 3
private const val RELOAD_BASE_DELAY_MS = 1_000L
private const val STREAM_GRACE_MS = 3_000L
private const val POLL_INTERVAL_MS = 15_000L
private const val NANOS_PER_MS = 1_000_000L

/**
 * The PDFs of one Topic: list, upload, import progress, removal and the banner "Ho letto il tuo PDF". Fed by REST and
 * by the `job_updated` events that the chat forwards through [onEvent]; while a document is being read, a slow poll
 * covers a stream that went quiet (there is no `GET /jobs/{id}`).
 */
@HiltViewModel(assistedFactory = DocumentsViewModel.Factory::class)
class DocumentsViewModel
    @AssistedInject
    constructor(
        @Assisted private val topicId: String,
        private val documents: DocumentRepository,
        private val bannerStore: DocumentBannerStore,
    ) : ViewModel() {
        @AssistedFactory
        interface Factory {
            fun create(topicId: String): DocumentsViewModel
        }

        private val _state = MutableStateFlow(DocumentsUiState())
        val state: StateFlow<DocumentsUiState> = _state.asStateFlow()

        private val effectChannel = Channel<DocumentsEffect>(Channel.BUFFERED)
        val effects: Flow<DocumentsEffect> = effectChannel.receiveAsFlow()

        /** Monotonic millisecond clock; replaced in tests. */
        internal var clock: () -> Long = { System.nanoTime() / NANOS_PER_MS }

        /** Replaced in tests: `Long.MAX_VALUE` turns the poll off. */
        internal var pollIntervalMs: Long = POLL_INTERVAL_MS

        private var fetchedOnce = false
        private var loadJob: Job? = null
        private var uploadJob: Job? = null
        private var reloadJob: Job? = null
        private var pollJob: Job? = null
        private var flushJob: Job? = null
        private var pendingProgress: TopicJob? = null
        private var lastApplied = Long.MIN_VALUE
        private var announcementCounter = 0
        private var lastSource: UploadSource? = null

        /** Ended jobs already followed by a reload: each is handled once, until it runs again (retry keeps the id). */
        private val handledTerminalJobs = mutableSetOf<String>()

        init {
            viewModelScope.launch {
                val dismissed = bannerStore.dismissed()
                _state.update { it.copy(dismissedBanner = it.dismissedBanner + dismissed) }
            }
        }

        /** The first load normally rides on the stream opening; this covers a stream that never opens. */
        fun loadIfStreamSilent() {
            if (fetchedOnce) return
            viewModelScope.launch {
                delay(STREAM_GRACE_MS)
                if (!fetchedOnce) {
                    fetchedOnce = true
                    load()
                }
            }
        }

        fun retryLoad() {
            _state.update { it.copy(load = DocumentsLoad.Loading) }
            load()
        }

        fun onEvent(event: ChatEvent) {
            when (event) {
                is ChatEvent.JobUpdated ->
                    if (event.job.isDocumentImport &&
                        event.job.topicId == topicId
                    ) {
                        onJob(event.job)
                    }
                ChatEvent.ResyncRequired -> {
                    fetchedOnce = true
                    load()
                }
                else -> Unit
            }
        }

        private fun load() {
            loadJob?.cancel()
            loadJob =
                viewModelScope.launch {
                    when (val result = documents.list(topicId)) {
                        is ApiResult.Success -> {
                            applyDocuments(result.value)
                            followJob(result.value)
                        }
                        // A failed refresh keeps what is on screen; only the first load shows an error.
                        is ApiResult.Failure ->
                            _state.update {
                                if (it.load == DocumentsLoad.Ready) {
                                    it
                                } else {
                                    it.copy(load = DocumentsLoad.Failed(result.failure.toLoadError()))
                                }
                            }
                    }
                }
        }

        private fun applyDocuments(list: List<TopicDocument>) {
            _state.update { it.copy(load = DocumentsLoad.Ready, documents = list) }
            schedulePoll(list)
        }

        /** A document that is not ready needs its job: the phase while it is read, the id for "Riprova" if it failed. */
        private suspend fun followJob(list: List<TopicDocument>) {
            if (list.none { it.status != DocumentStatus.Ready }) return
            val result = documents.importJobs(topicId)
            // Oldest first, so the newest ends up as the current one.
            if (result is ApiResult.Success) result.value.asReversed().forEach(::applyJob)
        }

        private fun schedulePoll(list: List<TopicDocument>) {
            pollJob?.cancel()
            if (pollIntervalMs == Long.MAX_VALUE || list.none { it.status == DocumentStatus.Importing }) return
            pollJob =
                viewModelScope.launch {
                    delay(pollIntervalMs)
                    load()
                }
        }

        private fun onJob(job: TopicJob) {
            val previous = _state.value.importJob
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
            val before = _state.value
            val documentId = job.documentId
            if (documentId != null) {
                val known = before.jobs[documentId]
                if (known == null || job.supersedes(known)) {
                    _state.update { it.copy(jobs = it.jobs + (documentId to job)) }
                }
            }
            val previous = before.importJob
            if (previous != null && !job.supersedes(previous)) return
            val watched = previous?.id == job.id && previous.isActive
            _state.update { it.copy(importJob = job) }
            val terminal = job.status == JobStatus.Succeeded || job.status == JobStatus.Failed
            // A job that names its document makes the list stale only if that document still shows as importing:
            // an old failed job next to a new import must not trigger a reload (it would loop on every load).
            val listIsStale =
                before.documents.any {
                    it.status == DocumentStatus.Importing && (documentId == null || it.id == documentId)
                }
            if (terminal && (watched || listIsStale) && handledTerminalJobs.add(job.id)) {
                reloadAfterTerminal(job)
            } else if (!terminal) {
                handledTerminalJobs.remove(job.id)
            }
        }

        /** The import ended: read the list again (a couple of retries), then tell the learner what happened. */
        private fun reloadAfterTerminal(job: TopicJob) {
            reloadJob?.cancel()
            reloadJob =
                viewModelScope.launch {
                    var wait = RELOAD_BASE_DELAY_MS
                    repeat(RELOAD_ATTEMPTS) { attempt ->
                        val result = documents.list(topicId)
                        if (result is ApiResult.Success &&
                            result.value.none { it.status == DocumentStatus.Importing }
                        ) {
                            applyDocuments(result.value)
                            announceEnd(job, result.value)
                            return@launch
                        }
                        if (attempt < RELOAD_ATTEMPTS - 1) {
                            delay(wait)
                            wait *= 2
                        }
                    }
                    // Gave up: the next resync or poll reads the list again.
                    load()
                }
        }

        private fun announceEnd(
            job: TopicJob,
            list: List<TopicDocument>,
        ) {
            if (job.status == JobStatus.Succeeded) {
                val document =
                    list.firstOrNull { it.id == job.result?.documentId && it.status == DocumentStatus.Ready }
                        ?: list.firstOrNull { it.status == DocumentStatus.Ready }
                        ?: return
                val pages = document.pageCount ?: job.result?.pageCount
                _state.update {
                    it.copy(
                        announcement = DocumentAnnouncement.Read(document.title, pages, ++announcementCounter),
                    )
                }
                effectChannel.trySend(DocumentsEffect.Imported(document.title, pages))
            } else {
                val code = list.firstOrNull { it.status == DocumentStatus.Failed }?.error?.code ?: job.error?.code
                val failure = DocumentFailure.fromCode(code)
                _state.update { it.copy(announcement = DocumentAnnouncement.Failed(failure, ++announcementCounter)) }
            }
        }

        // Upload

        /**
         * Sends [source]. A file over the limit is refused here, without a request. [source] is kept so that "Riprova"
         * after a network error sends the same file again.
         */
        fun upload(source: UploadSource) {
            val current = _state.value
            if (current.upload is UploadUi.Sending) return
            if (uploadAvailability(current.documents, uploading = false) != UploadAvailability.Available) return
            lastSource = source
            if (isOverSizeLimit(source.sizeBytes)) {
                _state.update { it.copy(upload = UploadUi.Failed(UploadError.TooLarge)) }
                return
            }
            _state.update { it.copy(upload = UploadUi.Sending(source.displayName)) }
            uploadJob =
                viewModelScope.launch {
                    when (val result = documents.upload(topicId, source)) {
                        is ApiResult.Success -> {
                            val created = result.value
                            _state.update {
                                it.copy(
                                    upload = UploadUi.Idle,
                                    documents =
                                        listOf(created.document) +
                                            it.documents.filterNot { d -> d.id == created.document.id },
                                    importJob = created.job,
                                    jobs =
                                        it.jobs +
                                            (created.document.id to created.job.copy(documentId = created.document.id)),
                                )
                            }
                            lastSource = null
                            schedulePoll(_state.value.documents)
                        }
                        is ApiResult.Failure -> onUploadFailed(result.failure)
                    }
                }
        }

        private fun onUploadFailed(failure: ApiFailure) {
            when (val action = uploadFailureAction(failure)) {
                is UploadFailureAction.Show -> _state.update { it.copy(upload = UploadUi.Failed(action.error)) }
                UploadFailureAction.Reload -> {
                    _state.update { it.copy(upload = UploadUi.Idle) }
                    lastSource = null
                    load()
                }
                UploadFailureAction.TopicGone -> {
                    _state.update { it.copy(upload = UploadUi.Idle) }
                    effectChannel.trySend(DocumentsEffect.TopicGone)
                }
                UploadFailureAction.Silent -> _state.update { it.copy(upload = UploadUi.Idle) }
            }
        }

        /** "Riprova" after a network error: the same file, without opening the picker again. */
        fun retryUpload() {
            val source = lastSource ?: return
            if (_state.value.upload !is UploadUi.Failed) return
            _state.update { it.copy(upload = UploadUi.Idle) }
            upload(source)
        }

        /** "Annulla" while sending, or "Chiudi" on the error panel. */
        fun cancelUpload() {
            val sending = _state.value.upload is UploadUi.Sending
            uploadJob?.cancel()
            uploadJob = null
            lastSource = null
            _state.update { it.copy(upload = UploadUi.Idle) }
            // The server may have received the whole file before the cancel: look at the truth.
            if (sending) load()
        }

        // Removal and retry

        /** Removes [documentId] (after the dialog, or straight from a failed row). */
        fun delete(documentId: String) {
            if (_state.value.delete is DeleteUi.Deleting) return
            _state.update { it.copy(delete = DeleteUi.Deleting(documentId)) }
            viewModelScope.launch {
                when (val result = documents.delete(documentId)) {
                    is ApiResult.Success -> removed(documentId)
                    is ApiResult.Failure ->
                        when (result.failure) {
                            // Already gone elsewhere: the goal is reached.
                            ApiFailure.NotFound -> removed(documentId)
                            ApiFailure.Conflict -> failedDelete(documentId, DeleteError.Busy)
                            is ApiFailure.Network -> failedDelete(documentId, DeleteError.Network)
                            else -> failedDelete(documentId, DeleteError.Other)
                        }
                }
            }
        }

        private fun removed(documentId: String) {
            _state.update {
                it.copy(delete = DeleteUi.Idle, documents = it.documents.filterNot { d -> d.id == documentId })
            }
            effectChannel.trySend(DocumentsEffect.Removed)
        }

        private fun failedDelete(
            documentId: String,
            error: DeleteError,
        ) = _state.update { it.copy(delete = DeleteUi.Failed(documentId, error)) }

        /** The dialog was closed: its error goes away with it. */
        fun clearDeleteError() =
            _state.update { if (it.delete is DeleteUi.Failed) it.copy(delete = DeleteUi.Idle) else it }

        /** "Riprova" on a document that failed for a reason that is not the file's (`POST .../jobs/{id}/retry`). */
        fun retryImport(documentId: String) {
            val state = _state.value
            if (state.retrying) return
            val document = state.documents.firstOrNull { it.id == documentId } ?: return
            val job = retryableJob(document, state.documents, jobFor(document, state.jobs, state.importJob)) ?: return
            _state.update { it.copy(retrying = true) }
            viewModelScope.launch {
                when (val result = documents.retryJob(topicId, job.id)) {
                    is ApiResult.Success -> {
                        handledTerminalJobs.remove(job.id)
                        lastApplied = Long.MIN_VALUE
                        _state.update { it.copy(retrying = false) }
                        applyJob(
                            result.value.let { j ->
                                if (j.documentId ==
                                    null
                                ) {
                                    j.copy(documentId = documentId)
                                } else {
                                    j
                                }
                            },
                        )
                        // The document goes back to importing with no error.
                        load()
                    }
                    is ApiResult.Failure -> {
                        _state.update { it.copy(retrying = false) }
                        if (result.failure == ApiFailure.Conflict || result.failure == ApiFailure.NotFound) {
                            load()
                        } else {
                            effectChannel.trySend(DocumentsEffect.RetryFailed)
                        }
                    }
                }
            }
        }

        // Banner

        /** "Non ora", or the research was started: the banner stays away for these documents, also after a restart. */
        fun dismissBanner(documentIds: Collection<String>) {
            if (documentIds.isEmpty()) return
            _state.update { it.copy(dismissedBanner = it.dismissedBanner + documentIds) }
            viewModelScope.launch { bannerStore.dismiss(documentIds) }
        }
    }

/** The ViewModel of the PDFs of [topicId]; the Topic screen shares it with the Fonti tab. */
@Composable
fun rememberDocumentsViewModel(topicId: String): DocumentsViewModel =
    hiltViewModel<DocumentsViewModel, DocumentsViewModel.Factory>(key = "documents-$topicId") { it.create(topicId) }

/** The callbacks of the section, bound to [viewModel]. */
fun documentsActions(
    viewModel: DocumentsViewModel,
    onOpenSettings: () -> Unit,
): DocumentsActions =
    DocumentsActions(
        onUpload = viewModel::upload,
        onRetryLoad = viewModel::retryLoad,
        onRetryUpload = viewModel::retryUpload,
        onCancelUpload = viewModel::cancelUpload,
        onDelete = viewModel::delete,
        onClearDeleteError = viewModel::clearDeleteError,
        onRetryImport = viewModel::retryImport,
        onDismissBanner = viewModel::dismissBanner,
        onOpenSettings = onOpenSettings,
    )
