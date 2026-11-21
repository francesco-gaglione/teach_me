package com.teachme.feature.lesson

import androidx.compose.runtime.Composable
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LearningRecord
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.LoadError
import com.teachme.core.model.RecordStatus
import com.teachme.core.model.toLoadError
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** The two views of the tab "Cosa sai" (progress.md section 2). */
enum class ProgressSection { Register, Glossary }

/** One list of the tab: what is known, whether it is loading, and why a refresh failed. */
data class ListState<T>(
    val items: List<T>? = null,
    val loading: Boolean = true,
    /** The first load failed: the error screen. */
    val error: LoadError? = null,
    /** A refresh failed while [items] are on screen: "Elenco dell'ultimo aggiornamento." */
    val stale: Boolean = false,
)

data class ProgressUiState(
    val section: ProgressSection = ProgressSection.Register,
    val records: ListState<LearningRecord> = ListState(),
    val glossary: ListState<GlossaryTerm> = ListState(),
    /** Counts up when a closing job succeeded and both lists were refreshed: "Registro aggiornato." */
    val updatedSignal: Int = 0,
) {
    val activeRecords: List<LearningRecord> get() = records.items.orEmpty().filter { it.status == RecordStatus.Active }
    val supersededRecords: List<LearningRecord> get() =
        records.items.orEmpty().filter {
            it.status ==
                RecordStatus.Superseded
        }
}

/**
 * Learning records and glossary of one Topic (progress.md). Read again when the tab opens, when the app comes back to the
 * foreground (the Topic screen calls [refresh]) and when a `lesson_completion` job of the Topic succeeds.
 */
@HiltViewModel(assistedFactory = ProgressViewModel.Factory::class)
class ProgressViewModel
    @AssistedInject
    constructor(
        @Assisted private val topicId: String,
        private val repository: LessonRepository,
    ) : ViewModel() {
        @AssistedFactory
        interface Factory {
            fun create(topicId: String): ProgressViewModel
        }

        private val _state = MutableStateFlow(ProgressUiState())
        val state: StateFlow<ProgressUiState> = _state.asStateFlow()

        private var recordsJob: Job? = null
        private var glossaryJob: Job? = null

        fun selectSection(section: ProgressSection) = _state.update { it.copy(section = section) }

        /** Reads both lists; a failure keeps what is on screen. */
        fun refresh() {
            loadRecords()
            loadGlossary()
        }

        fun retryRecords() {
            _state.update { it.copy(records = it.records.copy(loading = true, error = null)) }
            loadRecords()
        }

        fun retryGlossary() {
            _state.update { it.copy(glossary = it.glossary.copy(loading = true, error = null)) }
            loadGlossary()
        }

        fun onEvent(event: ChatEvent) {
            if (event is ChatEvent.JobUpdated) {
                val job = event.job
                if (job.topicId == topicId && job.isLessonCompletion && job.status == JobStatus.Succeeded) {
                    refresh()
                    _state.update { it.copy(updatedSignal = it.updatedSignal + 1) }
                }
            }
        }

        private fun loadRecords() {
            recordsJob?.cancel()
            recordsJob =
                viewModelScope.launch {
                    when (val result = repository.listRecords(topicId)) {
                        is ApiResult.Success ->
                            _state.update { it.copy(records = ListState(result.value, loading = false)) }
                        is ApiResult.Failure ->
                            _state.update {
                                val old = it.records
                                it.copy(
                                    records =
                                        if (old.items != null) {
                                            old.copy(loading = false, stale = true)
                                        } else {
                                            ListState(null, loading = false, error = result.failure.toLoadError())
                                        },
                                )
                            }
                    }
                }
        }

        private fun loadGlossary() {
            glossaryJob?.cancel()
            glossaryJob =
                viewModelScope.launch {
                    when (val result = repository.listGlossary(topicId)) {
                        is ApiResult.Success ->
                            _state.update { it.copy(glossary = ListState(result.value, loading = false)) }
                        is ApiResult.Failure ->
                            _state.update {
                                val old = it.glossary
                                it.copy(
                                    glossary =
                                        if (old.items != null) {
                                            old.copy(loading = false, stale = true)
                                        } else {
                                            ListState(null, loading = false, error = result.failure.toLoadError())
                                        },
                                )
                            }
                    }
                }
        }
    }

/** The ViewModel of "Cosa sai" of [topicId]; the Topic screen shares it with the lessons context pane. */
@Composable
fun rememberProgressViewModel(topicId: String): ProgressViewModel =
    hiltViewModel<ProgressViewModel, ProgressViewModel.Factory>(key = "progress-$topicId") { it.create(topicId) }
