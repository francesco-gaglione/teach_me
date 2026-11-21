package com.teachme.feature.folder

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.Flash
import com.teachme.core.model.FlashKind
import com.teachme.core.model.LoadError
import com.teachme.core.model.REVIEW_SESSION_SIZE
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.ReviewSyncer
import com.teachme.core.model.Topic
import com.teachme.core.model.TopicRepository
import com.teachme.core.model.TopicState
import com.teachme.core.model.TopicTabInbox
import com.teachme.core.model.TopicTabNames
import com.teachme.core.model.sessionMinutes
import com.teachme.core.model.toFormError
import com.teachme.core.model.toLoadError
import com.teachme.core.ui.DialogUi
import com.teachme.feature.lesson.TopicReviewEntry
import com.teachme.feature.mission.TopicTab
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

sealed interface TopicDialog {
    val ui: DialogUi

    data class Rename(
        override val ui: DialogUi = DialogUi(),
    ) : TopicDialog

    data class Delete(
        override val ui: DialogUi = DialogUi(),
    ) : TopicDialog
}

data class TopicUiState(
    val title: String,
    /** Null until loaded: the chip is absent while loading (topic.md). */
    val topic: Topic? = null,
    val loading: Boolean = true,
    val loadError: LoadError? = null,
    val dialog: TopicDialog? = null,
    /** The tab chosen in this session; null until the Topic is known (then [initialTab]). */
    val tab: TopicTab? = null,
    /** "Ripassa questo argomento" (topic.md F3.B2): null when nothing is due or the check failed. */
    val review: TopicReviewEntry? = null,
)

/**
 * Which tab opens first (topic.md F1): the chat while the mission is being defined, the sources while they are searched,
 * the lessons once the Topic is ready. The state is the hint for the next step, so every cold opening starts from it.
 */
internal fun initialTab(topic: Topic): TopicTab =
    when (topic.state) {
        TopicState.Researching -> TopicTab.Resources
        TopicState.Ready -> TopicTab.Lessons
        else -> TopicTab.Chat
    }

/** The tab a screen asked for with a name of [TopicTabNames]; unknown names are ignored. */
internal fun tabNamed(name: String?): TopicTab? =
    when (name) {
        TopicTabNames.LESSONS -> TopicTab.Lessons
        TopicTabNames.CHAT -> TopicTab.Chat
        TopicTabNames.PROGRESS -> TopicTab.Progress
        else -> null
    }

sealed interface TopicEffect {
    data class Renamed(
        val title: String,
    ) : TopicEffect

    data class Leave(
        val flash: Flash,
    ) : TopicEffect

    /** Title changed: other panes (the topic list) should reload. */
    data object Changed : TopicEffect
}

/** How long the topic waits for the queued grades before asking what is due (as Home, review.md section 8). */
internal const val REVIEW_FLUSH_WAIT_MS = 2_000L

@HiltViewModel(assistedFactory = TopicViewModel.Factory::class)
class TopicViewModel
    @AssistedInject
    constructor(
        @Assisted("id") private val topicId: String,
        @Assisted("name") initialTitle: String,
        private val topics: TopicRepository,
        private val tabInbox: TopicTabInbox,
        private val reviews: ReviewRepository,
        private val reviewSyncer: ReviewSyncer,
    ) : ViewModel() {
        @AssistedFactory
        interface Factory {
            fun create(
                @Assisted("id") topicId: String,
                @Assisted("name") initialTitle: String,
            ): TopicViewModel
        }

        private val _state = MutableStateFlow(TopicUiState(title = initialTitle))
        val state: StateFlow<TopicUiState> = _state.asStateFlow()

        private val effectChannel = Channel<TopicEffect>(Channel.BUFFERED)
        val effects: Flow<TopicEffect> = effectChannel.receiveAsFlow()

        private var loadJob: Job? = null
        private var reviewJob: Job? = null

        fun onStart() {
            // A screen above this one (the lesson) may have asked for a tab ("Torna all'argomento", "Vedi tutto nel registro").
            tabNamed(tabInbox.take(topicId))?.let { requested -> _state.update { it.copy(tab = requested) } }
            load()
            loadReview()
        }

        /** Asked at every opening and return to the foreground; failures and zero cards show nothing, silently. */
        private fun loadReview() {
            reviewJob?.cancel()
            reviewJob =
                viewModelScope.launch {
                    // As on Home: send the queued grades first (2 s at most), so cards just reviewed never look due.
                    reviewSyncer.flushWithin(viewModelScope, REVIEW_FLUSH_WAIT_MS)
                    val result = reviews.due(1, topicId) as? ApiResult.Success
                    val queued = reviewSyncer.pending().size
                    val due = result?.let { (it.value.dueCount - queued).coerceAtLeast(0) } ?: 0
                    val session = due.coerceAtMost(REVIEW_SESSION_SIZE)
                    val entry = if (due >= 1) TopicReviewEntry(session, sessionMinutes(due)) else null
                    _state.update { it.copy(review = entry) }
                }
        }

        fun retry() = load()

        private fun load() {
            loadJob?.cancel()
            loadJob =
                viewModelScope.launch {
                    val hadData = _state.value.topic != null
                    if (!hadData) _state.update { it.copy(loading = true, loadError = null) }
                    when (val result = topics.getTopic(topicId)) {
                        is ApiResult.Success ->
                            _state.update {
                                it.copy(
                                    topic = result.value,
                                    title = result.value.title,
                                    loading = false,
                                    loadError = null,
                                    tab = it.tab ?: initialTab(result.value),
                                )
                            }
                        is ApiResult.Failure ->
                            if (result.failure is ApiFailure.NotFound) {
                                effectChannel.send(TopicEffect.Leave(Flash(FlashKind.TopicGone)))
                            } else if (!hadData) {
                                _state.update { it.copy(loadError = result.failure.toLoadError(), loading = false) }
                            } else {
                                _state.update { it.copy(loading = false) }
                            }
                    }
                }
        }

        /** The Topic changed on the chat stream (state, rename or move elsewhere): mirror it in title and chip. */
        fun onTopicFromChat(updated: Topic) {
            val current = _state.value
            if (updated.id != topicId || current.topic == updated) return
            val titleChanged = current.title != updated.title
            _state.update { it.copy(topic = updated, title = updated.title, tab = it.tab ?: initialTab(updated)) }
            if (titleChanged) effectChannel.trySend(TopicEffect.Changed)
        }

        /** The user picked a tab: remembered for the session; the content below keeps its state. */
        fun selectTab(tab: TopicTab) = _state.update { it.copy(tab = tab) }

        fun openRename() = _state.update { it.copy(dialog = TopicDialog.Rename()) }

        fun openDelete() = _state.update { it.copy(dialog = TopicDialog.Delete()) }

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

        fun rename(title: String) {
            val dialog = _state.value.dialog as? TopicDialog.Rename ?: return
            if (dialog.ui.submitting) return
            val newTitle = title.trim()
            if (newTitle == _state.value.title) {
                _state.update { it.copy(dialog = null) }
                return
            }
            updateDialogUi { it.copy(submitting = true, error = null) }
            viewModelScope.launch {
                when (val result = topics.updateTopic(topicId, title = newTitle)) {
                    is ApiResult.Success -> {
                        loadJob?.cancel()
                        _state.update { it.copy(topic = result.value, title = result.value.title, dialog = null) }
                        effectChannel.send(TopicEffect.Renamed(result.value.title))
                        effectChannel.send(TopicEffect.Changed)
                    }
                    is ApiResult.Failure ->
                        if (result.failure is ApiFailure.NotFound) {
                            _state.update { it.copy(dialog = null) }
                            effectChannel.send(TopicEffect.Leave(Flash(FlashKind.TopicGone)))
                        } else {
                            updateDialogUi { it.copy(submitting = false, error = result.failure.toFormError("title")) }
                        }
                }
            }
        }

        fun confirmDelete() {
            val dialog = _state.value.dialog as? TopicDialog.Delete ?: return
            if (dialog.ui.submitting) return
            updateDialogUi { it.copy(submitting = true, error = null) }
            viewModelScope.launch {
                val failure = (topics.deleteTopic(topicId) as? ApiResult.Failure)?.failure
                if (failure == null || failure is ApiFailure.NotFound) {
                    loadJob?.cancel()
                    val title = _state.value.title
                    _state.update { it.copy(dialog = null) }
                    effectChannel.send(TopicEffect.Leave(Flash(FlashKind.TopicDeleted, title)))
                } else {
                    updateDialogUi { it.copy(submitting = false, error = failure.toFormError("title")) }
                }
            }
        }

        private fun updateDialogUi(transform: (DialogUi) -> DialogUi) {
            _state.update { s ->
                val d = s.dialog ?: return@update s
                s.copy(
                    dialog =
                        when (d) {
                            is TopicDialog.Rename -> d.copy(ui = transform(d.ui))
                            is TopicDialog.Delete -> d.copy(ui = transform(d.ui))
                        },
                )
            }
        }
    }
