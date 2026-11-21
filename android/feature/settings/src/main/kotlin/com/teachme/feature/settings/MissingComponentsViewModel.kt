package com.teachme.feature.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ComponentRequestGroup
import com.teachme.core.model.ComponentRequestRepository
import com.teachme.core.model.LoadError
import com.teachme.core.model.toLoadError
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class MissingComponentsUiState(
    /** Null while the first page loads. */
    val groups: List<ComponentRequestGroup>? = null,
    val loadError: LoadError? = null,
    val refreshing: Boolean = false,
    val nextCursor: String? = null,
    val pageLoading: Boolean = false,
    val pageFailed: Boolean = false,
)

/** Loads `GET /component-requests` on every open (no cache) and follows the cursor on demand. */
@HiltViewModel
class MissingComponentsViewModel
    @Inject
    constructor(
        private val repository: ComponentRequestRepository,
    ) : ViewModel() {
        private val _state = MutableStateFlow(MissingComponentsUiState())
        val state: StateFlow<MissingComponentsUiState> = _state.asStateFlow()

        init {
            refresh()
        }

        /** First page: replaces the list. Keeps the old list on screen while refreshing. */
        fun refresh() {
            if (_state.value.refreshing) return
            _state.update { it.copy(refreshing = true, loadError = null, pageFailed = false) }
            viewModelScope.launch {
                when (val result = repository.list(null)) {
                    is ApiResult.Success ->
                        _state.update {
                            it.copy(
                                groups = result.value.items,
                                nextCursor = result.value.nextCursor,
                                refreshing = false,
                                pageLoading = false,
                            )
                        }
                    is ApiResult.Failure ->
                        _state.update { it.copy(loadError = result.failure.toLoadError(), refreshing = false) }
                }
            }
        }

        fun loadMore() {
            val current = _state.value
            val cursor = current.nextCursor ?: return
            if (current.pageLoading || current.refreshing || current.groups == null) return
            _state.update { it.copy(pageLoading = true, pageFailed = false) }
            viewModelScope.launch {
                when (val result = repository.list(cursor)) {
                    is ApiResult.Success ->
                        _state.update {
                            it.copy(
                                groups = it.groups.orEmpty() + result.value.items,
                                nextCursor = result.value.nextCursor,
                                pageLoading = false,
                            )
                        }
                    is ApiResult.Failure -> _state.update { it.copy(pageLoading = false, pageFailed = true) }
                }
            }
        }
    }
