package com.teachme.feature.folder

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.Flash
import com.teachme.core.model.FlashInbox
import com.teachme.core.model.FlashKind
import com.teachme.core.model.Folder
import com.teachme.core.model.FolderRepository
import com.teachme.core.model.LoadError
import com.teachme.core.model.Topic
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.TopicRepository
import com.teachme.core.model.toFormError
import com.teachme.core.model.toLoadError
import com.teachme.core.ui.DialogUi
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.supervisorScope
import java.util.Locale

internal const val SLOW_LOADING_MS = 8_000L
internal const val FOLDER_NAME_MAX = 80
internal const val TOPIC_TITLE_MAX = 120

sealed interface FolderDialog {
    val ui: DialogUi

    data class CreateTopic(
        /** Content language chosen for the new Topic (ADR 0019); sent in the create request. */
        val language: TopicLanguage = TopicLanguage.It,
        override val ui: DialogUi = DialogUi(),
    ) : FolderDialog

    data class RenameTopic(
        val topic: Topic,
        override val ui: DialogUi = DialogUi(),
    ) : FolderDialog

    data class DeleteTopic(
        val topic: Topic,
        override val ui: DialogUi = DialogUi(),
    ) : FolderDialog

    data class RenameFolder(
        override val ui: DialogUi = DialogUi(),
    ) : FolderDialog

    data class DeleteFolder(
        override val ui: DialogUi = DialogUi(),
    ) : FolderDialog
}

data class FolderUiState(
    /** Known immediately from navigation, updated after a rename or a load. */
    val folderName: String,
    /** Authoritative count from `GET /folders/{id}` (the list on screen may be incomplete). */
    val topicCount: Int? = null,
    val topics: List<Topic>? = null,
    val loading: Boolean = true,
    val slowLoading: Boolean = false,
    val loadError: LoadError? = null,
    val refreshFailed: Boolean = false,
    val dialog: FolderDialog? = null,
)

sealed interface FolderMessage {
    data class TopicCreated(
        val topic: Topic,
    ) : FolderMessage

    data class TopicRenamed(
        val title: String,
    ) : FolderMessage

    data class TopicDeleted(
        val title: String,
    ) : FolderMessage

    data object TopicGone : FolderMessage

    data class FolderRenamed(
        val name: String,
    ) : FolderMessage
}

sealed interface FolderEffect {
    data class Toast(
        val message: FolderMessage,
    ) : FolderEffect

    data class ScrollTo(
        val topicId: String,
    ) : FolderEffect

    /** The folder is gone (deleted here, or elsewhere): leave the screen and tell the previous one. */
    data class Leave(
        val flash: Flash,
    ) : FolderEffect

    /** Something changed that other panes show (topic count, name). */
    data object Changed : FolderEffect

    data class TopicRemoved(
        val topicId: String,
    ) : FolderEffect
}

@HiltViewModel(assistedFactory = FolderViewModel.Factory::class)
class FolderViewModel
    @AssistedInject
    constructor(
        @Assisted("id") private val folderId: String,
        @Assisted("name") initialName: String,
        private val folders: FolderRepository,
        private val topics: TopicRepository,
        private val flashInbox: FlashInbox,
    ) : ViewModel() {
        @AssistedFactory
        interface Factory {
            fun create(
                @Assisted("id") folderId: String,
                @Assisted("name") initialName: String,
            ): FolderViewModel
        }

        private val _state = MutableStateFlow(FolderUiState(folderName = initialName))
        val state: StateFlow<FolderUiState> = _state.asStateFlow()

        private val effectChannel = Channel<FolderEffect>(Channel.BUFFERED)
        val effects: Flow<FolderEffect> = effectChannel.receiveAsFlow()

        private var loadJob: Job? = null
        private var slowJob: Job? = null

        /** Called on every `ON_START` and when the host asks for a refresh. */
        fun onStart() {
            flashInbox.take(FlashKind.TopicDeleted, FlashKind.TopicGone)?.let { flash ->
                when (flash.kind) {
                    FlashKind.TopicDeleted ->
                        effectChannel.trySend(FolderEffect.Toast(FolderMessage.TopicDeleted(flash.name.orEmpty())))
                    FlashKind.TopicGone -> effectChannel.trySend(FolderEffect.Toast(FolderMessage.TopicGone))
                    else -> Unit
                }
            }
            load()
        }

        fun retry() = load()

        fun dismissRefreshError() = _state.update { it.copy(refreshFailed = false) }

        /** A successful mutation makes a list request still in flight stale: drop it. */
        private fun cancelPendingLoad() {
            loadJob?.cancel()
            slowJob?.cancel()
        }

        private fun load() {
            loadJob?.cancel()
            slowJob?.cancel()
            loadJob =
                viewModelScope.launch {
                    val hadData = _state.value.topics != null
                    if (!hadData) {
                        _state.update { it.copy(loading = true, loadError = null, slowLoading = false) }
                        slowJob =
                            launch {
                                delay(SLOW_LOADING_MS)
                                _state.update { it.copy(slowLoading = true) }
                            }
                    }
                    val (folderResult, topicsResult) =
                        supervisorScope {
                            val f = async { folders.getFolder(folderId) }
                            val t = async { topics.listTopics(folderId) }
                            f.await() to t.await()
                        }
                    slowJob?.cancel()
                    val gone =
                        (folderResult as? ApiResult.Failure)?.failure is ApiFailure.NotFound ||
                            (topicsResult as? ApiResult.Failure)?.failure is ApiFailure.NotFound
                    if (gone) {
                        effectChannel.send(FolderEffect.Leave(Flash(FlashKind.FolderGone)))
                        return@launch
                    }
                    val failure =
                        (folderResult as? ApiResult.Failure)?.failure ?: (topicsResult as? ApiResult.Failure)?.failure
                    if (failure != null) {
                        _state.update {
                            if (hadData) {
                                it.copy(refreshFailed = true, loading = false)
                            } else {
                                it.copy(loadError = failure.toLoadError(), loading = false, slowLoading = false)
                            }
                        }
                    } else {
                        val folder = (folderResult as ApiResult.Success<Folder>).value
                        val list = (topicsResult as ApiResult.Success<List<Topic>>).value
                        _state.update {
                            it.copy(
                                folderName = folder.name,
                                topicCount = folder.topicCount,
                                topics = list,
                                loading = false,
                                loadError = null,
                                refreshFailed = false,
                                slowLoading = false,
                            )
                        }
                    }
                }
        }

        // --- dialogs -------------------------------------------------------------------------

        fun openCreateTopic(deviceLanguage: String? = Locale.getDefault().language) =
            _state.update {
                it.copy(dialog = FolderDialog.CreateTopic(language = TopicLanguage.defaultFor(deviceLanguage)))
            }

        fun selectCreateLanguage(language: TopicLanguage) =
            _state.update { state ->
                val dialog = state.dialog as? FolderDialog.CreateTopic
                if (dialog == null ||
                    dialog.ui.submitting
                ) {
                    state
                } else {
                    state.copy(dialog = dialog.copy(language = language))
                }
            }

        fun openRenameTopic(topic: Topic) = _state.update { it.copy(dialog = FolderDialog.RenameTopic(topic)) }

        fun openDeleteTopic(topic: Topic) = _state.update { it.copy(dialog = FolderDialog.DeleteTopic(topic)) }

        fun openRenameFolder() = _state.update { it.copy(dialog = FolderDialog.RenameFolder()) }

        fun openDeleteFolder() = _state.update { it.copy(dialog = FolderDialog.DeleteFolder()) }

        fun dismissDialog() {
            if (_state.value.dialog
                    ?.ui
                    ?.submitting == true
            ) {
                return
            }
            _state.update { it.copy(dialog = null) }
        }

        fun onDialogTextEdited() = updateDialogUi { it.copy(error = null) }

        fun createTopic(title: String) {
            val dialog = _state.value.dialog as? FolderDialog.CreateTopic ?: return
            if (dialog.ui.submitting) return
            updateDialogUi { it.copy(submitting = true, error = null) }
            viewModelScope.launch {
                when (val result = topics.createTopic(folderId, title.trim(), dialog.language)) {
                    is ApiResult.Success -> {
                        val topic = result.value
                        cancelPendingLoad()
                        _state.update {
                            it.copy(
                                topics = it.topics.orEmpty() + topic,
                                topicCount = (it.topicCount ?: 0) + 1,
                                dialog = null,
                            )
                        }
                        effectChannel.send(FolderEffect.Toast(FolderMessage.TopicCreated(topic)))
                        effectChannel.send(FolderEffect.ScrollTo(topic.id))
                        effectChannel.send(FolderEffect.Changed)
                    }
                    is ApiResult.Failure ->
                        if (result.failure is ApiFailure.NotFound) {
                            _state.update { it.copy(dialog = null) }
                            effectChannel.send(FolderEffect.Leave(Flash(FlashKind.FolderGone)))
                        } else {
                            updateDialogUi { it.copy(submitting = false, error = result.failure.toFormError("title")) }
                        }
                }
            }
        }

        fun renameTopic(title: String) {
            val dialog = _state.value.dialog as? FolderDialog.RenameTopic ?: return
            if (dialog.ui.submitting) return
            val newTitle = title.trim()
            if (newTitle == dialog.topic.title) {
                _state.update { it.copy(dialog = null) }
                return
            }
            updateDialogUi { it.copy(submitting = true, error = null) }
            viewModelScope.launch {
                when (val result = topics.updateTopic(dialog.topic.id, title = newTitle)) {
                    is ApiResult.Success -> {
                        val updated = result.value
                        cancelPendingLoad()
                        _state.update { s ->
                            s.copy(topics = s.topics?.map { if (it.id == updated.id) updated else it }, dialog = null)
                        }
                        effectChannel.send(FolderEffect.Toast(FolderMessage.TopicRenamed(updated.title)))
                    }
                    is ApiResult.Failure ->
                        if (result.failure is ApiFailure.NotFound) {
                            _state.update { it.copy(dialog = null) }
                            effectChannel.send(FolderEffect.Toast(FolderMessage.TopicGone))
                            effectChannel.send(FolderEffect.TopicRemoved(dialog.topic.id))
                            load()
                        } else {
                            updateDialogUi { it.copy(submitting = false, error = result.failure.toFormError("title")) }
                        }
                }
            }
        }

        fun confirmDeleteTopic() {
            val dialog = _state.value.dialog as? FolderDialog.DeleteTopic ?: return
            if (dialog.ui.submitting) return
            updateDialogUi { it.copy(submitting = true, error = null) }
            viewModelScope.launch {
                val result = topics.deleteTopic(dialog.topic.id)
                val failure = (result as? ApiResult.Failure)?.failure
                if (failure == null || failure is ApiFailure.NotFound) {
                    cancelPendingLoad()
                    // 404: it was already gone, so the count we hold is stale (not ours to decrement): reload.
                    val deleted = failure == null
                    _state.update { s ->
                        s.copy(
                            topics = s.topics?.filterNot { it.id == dialog.topic.id },
                            topicCount = if (deleted) s.topicCount?.let { (it - 1).coerceAtLeast(0) } else s.topicCount,
                            dialog = null,
                        )
                    }
                    effectChannel.send(FolderEffect.Toast(FolderMessage.TopicDeleted(dialog.topic.title)))
                    effectChannel.send(FolderEffect.TopicRemoved(dialog.topic.id))
                    effectChannel.send(FolderEffect.Changed)
                    if (!deleted) load()
                } else {
                    updateDialogUi { it.copy(submitting = false, error = failure.toFormError("title")) }
                }
            }
        }

        fun renameFolder(name: String) {
            val dialog = _state.value.dialog as? FolderDialog.RenameFolder ?: return
            if (dialog.ui.submitting) return
            val newName = name.trim()
            if (newName == _state.value.folderName) {
                _state.update { it.copy(dialog = null) }
                return
            }
            updateDialogUi { it.copy(submitting = true, error = null) }
            viewModelScope.launch {
                when (val result = folders.renameFolder(folderId, newName)) {
                    is ApiResult.Success -> {
                        cancelPendingLoad()
                        _state.update { it.copy(folderName = result.value.name, dialog = null) }
                        effectChannel.send(FolderEffect.Toast(FolderMessage.FolderRenamed(result.value.name)))
                        effectChannel.send(FolderEffect.Changed)
                    }
                    is ApiResult.Failure ->
                        if (result.failure is ApiFailure.NotFound) {
                            _state.update { it.copy(dialog = null) }
                            effectChannel.send(FolderEffect.Leave(Flash(FlashKind.FolderGone)))
                        } else {
                            updateDialogUi { it.copy(submitting = false, error = result.failure.toFormError("name")) }
                        }
                }
            }
        }

        fun confirmDeleteFolder() {
            val dialog = _state.value.dialog as? FolderDialog.DeleteFolder ?: return
            if (dialog.ui.submitting) return
            updateDialogUi { it.copy(submitting = true, error = null) }
            viewModelScope.launch {
                val result = folders.deleteFolder(folderId)
                val failure = (result as? ApiResult.Failure)?.failure
                if (failure == null || failure is ApiFailure.NotFound) {
                    _state.update { it.copy(dialog = null) }
                    effectChannel.send(FolderEffect.Leave(Flash(FlashKind.FolderDeleted, _state.value.folderName)))
                } else {
                    updateDialogUi { it.copy(submitting = false, error = failure.toFormError("name")) }
                }
            }
        }

        private fun updateDialogUi(transform: (DialogUi) -> DialogUi) {
            _state.update { s ->
                val d = s.dialog ?: return@update s
                s.copy(
                    dialog =
                        when (d) {
                            is FolderDialog.CreateTopic -> d.copy(ui = transform(d.ui))
                            is FolderDialog.RenameTopic -> d.copy(ui = transform(d.ui))
                            is FolderDialog.DeleteTopic -> d.copy(ui = transform(d.ui))
                            is FolderDialog.RenameFolder -> d.copy(ui = transform(d.ui))
                            is FolderDialog.DeleteFolder -> d.copy(ui = transform(d.ui))
                        },
                )
            }
        }
    }
