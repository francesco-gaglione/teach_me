package com.teachme.feature.home

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.DueTopic
import com.teachme.core.model.FlashInbox
import com.teachme.core.model.FlashKind
import com.teachme.core.model.Folder
import com.teachme.core.model.FolderRepository
import com.teachme.core.model.LessonInProgress
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.LibraryStateRepository
import com.teachme.core.model.LoadError
import com.teachme.core.model.NotificationPermissionAsker
import com.teachme.core.model.ReviewReminderGate
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.ReviewSyncer
import com.teachme.core.model.toFormError
import com.teachme.core.model.toLoadError
import com.teachme.core.ui.DialogUi
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
import javax.inject.Inject

internal const val SLOW_LOADING_MS = 8_000L

/** Home waits at most this long for the queued review grades to be sent before it counts what is due (review.md 8). */
internal const val REVIEW_FLUSH_WAIT_MS = 2_000L
internal const val FOLDER_NAME_MAX = 80
private const val SELECTED_KEY = "selected_folder"
private const val BY_USER_KEY = "selected_by_user"

/** The open dialog of the Home screen; text typed in it is kept by the dialog itself. */
sealed interface HomeDialog {
    val ui: DialogUi

    data class Create(
        override val ui: DialogUi = DialogUi(),
    ) : HomeDialog

    data class Rename(
        val folder: Folder,
        override val ui: DialogUi = DialogUi(),
    ) : HomeDialog

    data class Delete(
        val folder: Folder,
        override val ui: DialogUi = DialogUi(),
    ) : HomeDialog
}

/** The "Ripasso di oggi" entry of Home (home.md, "Ripasso di oggi"). */
sealed interface HomeReview {
    /** Nothing to say: still loading, no card was ever made, or all are suspended. */
    data object Hidden : HomeReview

    /** [dueCount] cards are due, net of the grades still waiting in the queue. */
    data class Due(
        val dueCount: Int,
        /** Topics with due cards, for the filter chips; they show only with 2 or more. */
        val topics: List<DueTopic> = emptyList(),
    ) : HomeReview

    /** Nothing due, but some card comes later: one discreet line, not a card. */
    data class Upcoming(
        val nextDueAt: Instant,
    ) : HomeReview

    /** The check failed and there is nothing recent to show. */
    data object Failed : HomeReview
}

data class HomeUiState(
    /** Null until the first successful load. */
    val folders: List<Folder>? = null,
    val loading: Boolean = true,
    val slowLoading: Boolean = false,
    /** Failure of the first load (no data to show): full-screen error. */
    val loadError: LoadError? = null,
    /** Failure of a refresh while data is visible: banner. */
    val refreshFailed: Boolean = false,
    val dialog: HomeDialog? = null,
    /** Folder shown in the detail pane of the list-detail layout (expanded); null when there are no folders. */
    val selectedFolderId: String? = null,
    /** True once the user picked or created a folder (an automatic selection does not count). */
    val selectionByUser: Boolean = false,
    val review: HomeReview = HomeReview.Hidden,
    /** Topic picked in the review filter chips; null is "Tutti". Never remembered: it lives only in this state. */
    val reviewTopicId: String? = null,
    /** The lesson to resume (home.md, "Riprendi la lezione"); null while loading, when there is none or on error. */
    val resume: LessonInProgress? = null,
)

/** Toast texts, resolved by the screen from resources. */
sealed interface HomeMessage {
    data class Created(
        val name: String,
    ) : HomeMessage

    data class Renamed(
        val name: String,
    ) : HomeMessage

    data class Deleted(
        val name: String,
    ) : HomeMessage

    data object FolderGone : HomeMessage
}

sealed interface HomeEffect {
    data class Toast(
        val message: HomeMessage,
    ) : HomeEffect

    data class ScrollTo(
        val folderId: String,
    ) : HomeEffect

    /** The folder with this id was deleted or renamed away: panes showing it must drop it. */
    data class FolderRemoved(
        val folderId: String,
    ) : HomeEffect

    /** The review card shows due cards for the first time: the screen asks for the notification permission (ADR 0012). */
    data object AskNotificationPermission : HomeEffect
}

@HiltViewModel
class HomeViewModel
    @Inject
    constructor(
        private val repository: FolderRepository,
        private val libraryState: LibraryStateRepository,
        private val reviews: ReviewRepository,
        private val lessons: LessonRepository,
        private val reviewSyncer: ReviewSyncer,
        private val flashInbox: FlashInbox,
        private val permissionAsker: NotificationPermissionAsker,
        private val reminderGate: ReviewReminderGate,
        private val savedState: SavedStateHandle,
    ) : ViewModel() {
        private val _state =
            MutableStateFlow(
                HomeUiState(
                    selectedFolderId = savedState.get<String>(SELECTED_KEY),
                    selectionByUser = savedState.get<Boolean>(BY_USER_KEY) ?: false,
                ),
            )
        val state: StateFlow<HomeUiState> = _state.asStateFlow()

        private val effectChannel = Channel<HomeEffect>(Channel.BUFFERED)

        // An ask is in the channel and not launched yet: do not queue a second one.
        private var askPending = false
        val effects: Flow<HomeEffect> = effectChannel.receiveAsFlow()

        private var loadJob: Job? = null
        private var slowJob: Job? = null
        private var reviewJob: Job? = null
        private var resumeJob: Job? = null

        /** Called on every `ON_START`: shows a pending flash message and reloads (silently if data is visible). */
        fun onStart() {
            flashInbox.take(FlashKind.FolderGone, FlashKind.FolderDeleted)?.let { flash ->
                when (flash.kind) {
                    FlashKind.FolderGone -> effectChannel.trySend(HomeEffect.Toast(HomeMessage.FolderGone))
                    FlashKind.FolderDeleted ->
                        effectChannel.trySend(HomeEffect.Toast(HomeMessage.Deleted(flash.name.orEmpty())))
                    else -> Unit
                }
            }
            // Back in the foreground the filter is "Tutti" again (home.md, interleaving).
            _state.update { it.copy(reviewTopicId = null) }
            load()
            loadReview()
            loadResume()
        }

        /** Failure or empty answer: no card and no banner, the card is not critical data. */
        private fun loadResume() {
            resumeJob?.cancel()
            resumeJob =
                viewModelScope.launch {
                    // A failed lookup keeps the card already shown instead of clearing it.
                    val result = lessons.lastInProgress() as? ApiResult.Success ?: return@launch
                    _state.update { it.copy(resume = result.value) }
                }
        }

        fun retry() = load()

        /** "Riprova" of the line that says the review could not be checked. */
        fun retryReview() = loadReview()

        /** Chip tap: [topicId] null is "Tutti". Works on the data already loaded, no request. */
        fun selectReviewTopic(topicId: String?) {
            _state.update { state ->
                val topics = (state.review as? HomeReview.Due)?.topics.orEmpty()
                state.copy(reviewTopicId = topicId.takeIf { id -> topics.any { it.topicId == id } })
            }
        }

        /**
         * Sends the queued grades (waiting 2 s at most), then asks what is due and shows it net of the grades still
         * queued, so cards just reviewed never look due. Independent of the folders: neither waits for the other.
         */
        private fun loadReview() {
            reviewJob?.cancel()
            // Back from a session the old count would be wrong for a moment: show nothing until the new one is in.
            _state.update { it.copy(review = HomeReview.Hidden) }
            reviewJob =
                viewModelScope.launch {
                    reviewSyncer.flushWithin(viewModelScope, REVIEW_FLUSH_WAIT_MS)
                    when (val result = reviews.due(1)) {
                        is ApiResult.Success -> {
                            val due = (result.value.dueCount - reviewSyncer.pending().size).coerceAtLeast(0)
                            val next = result.value.nextDueAt
                            val review =
                                when {
                                    due >= 1 -> HomeReview.Due(due, result.value.topics)
                                    next != null -> HomeReview.Upcoming(next)
                                    else -> HomeReview.Hidden
                                }
                            _state.update { state ->
                                val topics = (review as? HomeReview.Due)?.topics.orEmpty()
                                // A topic that is no longer due (all reviewed, or deleted) silently goes back to "Tutti".
                                val keep = state.reviewTopicId?.takeIf { id -> topics.any { it.topicId == id } }
                                state.copy(review = review, reviewTopicId = keep)
                            }
                            // At most once ever, and never at start: only when the card has due cards to show.
                            // Marked as asked only when the screen really launches the dialog ([onNotificationPermissionRequested]).
                            if (review is HomeReview.Due && !askPending && permissionAsker.shouldAskFromCard()) {
                                askPending = true
                                effectChannel.trySend(HomeEffect.AskNotificationPermission)
                            }
                        }
                        is ApiResult.Failure ->
                            _state.update {
                                // An expired token is already said by the list.
                                if (result.failure is ApiFailure.Unauthorized) {
                                    it
                                } else {
                                    it.copy(review = HomeReview.Failed)
                                }
                            }
                    }
                }
        }

        /** The screen is launching the permission dialog now: only here the ask counts as used up. */
        fun onNotificationPermissionRequested() {
            askPending = false
            viewModelScope.launch { permissionAsker.markAsked() }
        }

        /** The permission dialog closed: with a grant the hourly check can start now. */
        fun onNotificationPermissionResult() {
            viewModelScope.launch { reminderGate.sync() }
        }

        /** Silent reload requested by the host (something changed in another pane). */
        fun reload() = load()

        /** The user picked [id] in the folder list (or created it): remember it, also across restarts. */
        fun select(id: String) {
            setSelection(id, byUser = true)
            viewModelScope.launch { libraryState.setLastFolderId(id) }
        }

        /**
         * The folder [id] was deleted from the detail pane. Selection moves to the next folder, else the
         * previous, and the list is refreshed.
         */
        fun folderRemoved(
            id: String,
            message: HomeMessage? = null,
        ) {
            cancelPendingLoad()
            _state.update { s ->
                val list = s.folders ?: return@update s
                val remaining = list.filterNot { it.id == id }
                val sel = if (s.selectedFolderId == id) neighbour(list, id) else s.selectedFolderId
                s.copy(folders = remaining, selectedFolderId = sel)
            }
            persistSelection()
            message?.let { effectChannel.trySend(HomeEffect.Toast(it)) }
            load()
        }

        private fun neighbour(
            list: List<Folder>,
            id: String,
        ): String? {
            val index = list.indexOfFirst { it.id == id }
            return list.getOrNull(index + 1)?.id ?: list.getOrNull(index - 1)?.id
        }

        private fun setSelection(
            id: String?,
            byUser: Boolean,
        ) {
            savedState[SELECTED_KEY] = id
            savedState[BY_USER_KEY] = byUser
            _state.update { it.copy(selectedFolderId = id, selectionByUser = byUser) }
        }

        /** Saves the current selection (also across restarts), e.g. after it moved because of a delete. */
        private fun persistSelection() {
            val id = _state.value.selectedFolderId
            savedState[SELECTED_KEY] = id
            viewModelScope.launch { libraryState.setLastFolderId(id) }
        }

        /**
         * A successful mutation makes any list request still in flight stale (it may have been served
         * before the change): drop it, its answer must not overwrite the newer local state.
         */
        private fun cancelPendingLoad() {
            loadJob?.cancel()
            slowJob?.cancel()
        }

        /** Keeps a valid selection: the current one, else the last used one, else the first folder. */
        private suspend fun resolveSelection(list: List<Folder>) {
            val current = _state.value.selectedFolderId
            if (list.isEmpty()) {
                if (current != null) setSelection(null, byUser = false)
                return
            }
            if (current != null && list.any { it.id == current }) return
            val last = libraryState.lastFolderId()?.takeIf { id -> list.any { it.id == id } }
            setSelection(last ?: list.first().id, byUser = false)
        }

        fun dismissRefreshError() = _state.update { it.copy(refreshFailed = false) }

        private fun load() {
            loadJob?.cancel()
            slowJob?.cancel()
            loadJob =
                viewModelScope.launch {
                    val hadData = _state.value.folders != null
                    if (!hadData) {
                        _state.update { it.copy(loading = true, loadError = null, slowLoading = false) }
                        slowJob =
                            launch {
                                delay(SLOW_LOADING_MS)
                                _state.update { it.copy(slowLoading = true) }
                            }
                    }
                    when (val result = repository.listFolders()) {
                        is ApiResult.Success -> {
                            _state.update {
                                it.copy(
                                    folders = result.value,
                                    loading = false,
                                    loadError = null,
                                    refreshFailed = false,
                                    slowLoading = false,
                                )
                            }
                            resolveSelection(result.value)
                        }
                        is ApiResult.Failure ->
                            _state.update {
                                if (hadData) {
                                    it.copy(refreshFailed = true, loading = false)
                                } else {
                                    it.copy(
                                        loadError = result.failure.toLoadError(),
                                        loading = false,
                                        slowLoading = false,
                                    )
                                }
                            }
                    }
                    slowJob?.cancel()
                }
        }

        // --- dialogs -------------------------------------------------------------------------

        fun openCreate() = _state.update { it.copy(dialog = HomeDialog.Create()) }

        fun openRename(folder: Folder) = _state.update { it.copy(dialog = HomeDialog.Rename(folder)) }

        fun openDelete(folder: Folder) = _state.update { it.copy(dialog = HomeDialog.Delete(folder)) }

        fun dismissDialog() {
            if (_state.value.dialog
                    ?.ui
                    ?.submitting == true
            ) {
                return
            }
            _state.update { it.copy(dialog = null) }
        }

        /** The user edited the text: a server error about the old text no longer applies. */
        fun onDialogTextEdited() = updateDialogUi { it.copy(error = null) }

        fun create(name: String) {
            val dialog = _state.value.dialog as? HomeDialog.Create ?: return
            if (dialog.ui.submitting) return
            updateDialogUi { it.copy(submitting = true, error = null) }
            viewModelScope.launch {
                when (val result = repository.createFolder(name.trim())) {
                    is ApiResult.Success -> {
                        val folder = result.value
                        cancelPendingLoad()
                        _state.update { it.copy(folders = it.folders.orEmpty() + folder, dialog = null) }
                        select(folder.id)
                        effectChannel.send(HomeEffect.Toast(HomeMessage.Created(folder.name)))
                        effectChannel.send(HomeEffect.ScrollTo(folder.id))
                    }
                    is ApiResult.Failure ->
                        updateDialogUi {
                            it.copy(
                                submitting = false,
                                error = result.failure.toFormError("name"),
                            )
                        }
                }
            }
        }

        fun rename(name: String) {
            val dialog = _state.value.dialog as? HomeDialog.Rename ?: return
            if (dialog.ui.submitting) return
            val newName = name.trim()
            if (newName == dialog.folder.name) {
                _state.update { it.copy(dialog = null) }
                return
            }
            updateDialogUi { it.copy(submitting = true, error = null) }
            viewModelScope.launch {
                when (val result = repository.renameFolder(dialog.folder.id, newName)) {
                    is ApiResult.Success -> {
                        val updated = result.value
                        cancelPendingLoad()
                        _state.update { s ->
                            s.copy(folders = s.folders?.map { if (it.id == updated.id) updated else it }, dialog = null)
                        }
                        effectChannel.send(HomeEffect.Toast(HomeMessage.Renamed(updated.name)))
                    }
                    is ApiResult.Failure ->
                        if (result.failure is ApiFailure.NotFound) {
                            _state.update { it.copy(dialog = null) }
                            effectChannel.send(HomeEffect.Toast(HomeMessage.FolderGone))
                            effectChannel.send(HomeEffect.FolderRemoved(dialog.folder.id))
                            load()
                        } else {
                            updateDialogUi { it.copy(submitting = false, error = result.failure.toFormError("name")) }
                        }
                }
            }
        }

        fun confirmDelete() {
            val dialog = _state.value.dialog as? HomeDialog.Delete ?: return
            if (dialog.ui.submitting) return
            updateDialogUi { it.copy(submitting = true, error = null) }
            viewModelScope.launch {
                val result = repository.deleteFolder(dialog.folder.id)
                // 404 means it is already gone: same outcome as a successful delete.
                if (result is ApiResult.Success ||
                    (result as? ApiResult.Failure)?.failure is ApiFailure.NotFound
                ) {
                    cancelPendingLoad()
                    _state.update { s ->
                        val list = s.folders.orEmpty()
                        val selected =
                            if (s.selectedFolderId ==
                                dialog.folder.id
                            ) {
                                neighbour(list, dialog.folder.id)
                            } else {
                                s.selectedFolderId
                            }
                        s.copy(
                            folders = list.filterNot { it.id == dialog.folder.id },
                            dialog = null,
                            selectedFolderId = selected,
                        )
                    }
                    persistSelection()
                    effectChannel.send(HomeEffect.Toast(HomeMessage.Deleted(dialog.folder.name)))
                    effectChannel.send(HomeEffect.FolderRemoved(dialog.folder.id))
                } else if (result is ApiResult.Failure) {
                    updateDialogUi { it.copy(submitting = false, error = result.failure.toFormError("name")) }
                }
            }
        }

        private fun updateDialogUi(transform: (DialogUi) -> DialogUi) {
            _state.update { s ->
                val d = s.dialog ?: return@update s
                s.copy(
                    dialog =
                        when (d) {
                            is HomeDialog.Create -> d.copy(ui = transform(d.ui))
                            is HomeDialog.Rename -> d.copy(ui = transform(d.ui))
                            is HomeDialog.Delete -> d.copy(ui = transform(d.ui))
                        },
                )
            }
        }
    }
