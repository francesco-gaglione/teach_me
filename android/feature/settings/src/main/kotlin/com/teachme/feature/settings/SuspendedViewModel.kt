package com.teachme.feature.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.LoadError
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.toLoadError
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
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject

internal const val SUSPENDED_PAGE_SIZE = 50
internal const val SUSPENDED_SLOW_MS = 8_000L
internal const val SUSPENDED_TIMEOUT_MS = 10_000L

/** What a row is doing: waiting, sending the reactivation, or showing that the last attempt failed. */
enum class RowPhase { Idle, Busy, Failed }

data class SuspendedRow(
    val card: ReviewCard,
    val phase: RowPhase = RowPhase.Idle,
)

data class SuspendedUiState(
    /** Null while the first page loads (or failed: see [loadError]). */
    val rows: List<SuspendedRow>? = null,
    /** `suspended_count`, kept up to date locally after each reactivation or undo. */
    val count: Int = 0,
    val loadError: LoadError? = null,
    val slowLoading: Boolean = false,
    val nextCursor: String? = null,
    val pageLoading: Boolean = false,
    val pageFailed: Boolean = false,
) {
    /** Nothing to show and nothing more to load: the empty state. */
    val isEmpty: Boolean
        get() = rows != null && rows.isEmpty() && nextCursor == null && !pageLoading && !pageFailed
}

/** One-shot messages for the screen's toast (suspended.md section 3). */
sealed interface SuspendedEffect {
    /**
     * The card is back in the review. [dueInSeconds] is `due - now`: 0 or less means it is already due, null that the
     * backend did not say.
     */
    data class Reactivated(
        val cardId: String,
        val dueInSeconds: Long?,
    ) : SuspendedEffect

    /** `409 card_superseded`: the row left, nothing to undo. */
    data object Superseded : SuspendedEffect

    /** "Annulla" failed: the card stays reactivated. */
    data object UndoFailed : SuspendedEffect

    /** "Annulla" got `404`: the card does not exist any more. */
    data object UndoGone : SuspendedEffect

    /** "Annulla" got `409 card_superseded`: a newer version replaced the card. */
    data object UndoSuperseded : SuspendedEffect

    /**
     * A row left the list: TalkBack focus goes to the row [rowId] (the one that followed it) or, when null, to the
     * title of the top bar (suspended.md section 3). Sent before the toast message.
     */
    data class MoveFocus(
        val rowId: String?,
    ) : SuspendedEffect
}

/** Loads `GET /reviews/suspended` on every open (no cache) and reactivates cards one tap at a time. */
@HiltViewModel
class SuspendedViewModel
    @Inject
    constructor(
        private val repository: ReviewRepository,
        private val clock: AppClock,
    ) : ViewModel() {
        private val _state = MutableStateFlow(SuspendedUiState())
        val state: StateFlow<SuspendedUiState> = _state.asStateFlow()

        private val effectChannel = Channel<SuspendedEffect>(Channel.BUFFERED)
        val effects: Flow<SuspendedEffect> = effectChannel.receiveAsFlow()

        private var loadJob: Job? = null
        private var pageJob: Job? = null

        /** Cards reactivated in this session: a page fetched in the meantime must not bring them back. */
        private val reactivated = HashSet<String>()

        init {
            refresh()
        }

        /** First page: starts over with the skeleton. Also the "Riprova" of the load error. */
        fun refresh() {
            loadJob?.cancel()
            pageJob?.cancel()
            _state.value = SuspendedUiState()
            loadJob =
                viewModelScope.launch {
                    val slow =
                        launch {
                            delay(SUSPENDED_SLOW_MS)
                            _state.update { it.copy(slowLoading = true) }
                        }
                    val result =
                        withTimeoutOrNull(SUSPENDED_TIMEOUT_MS) { repository.suspended(SUSPENDED_PAGE_SIZE, null) }
                    slow.cancel()
                    when (result) {
                        is ApiResult.Success ->
                            _state.value =
                                SuspendedUiState(
                                    rows =
                                        result.value.items
                                            .filter { it.id !in reactivated }
                                            .map { SuspendedRow(it) },
                                    count = result.value.suspendedCount,
                                    nextCursor = result.value.nextCursor,
                                )
                        is ApiResult.Failure -> failLoad(result.failure.toLoadError())
                        null -> failLoad(LoadError.Network)
                    }
                }
        }

        private fun failLoad(error: LoadError) = _state.update { it.copy(loadError = error, slowLoading = false) }

        /** The next page, when the end of the list comes into view. */
        fun loadMore() {
            val current = _state.value
            val cursor = current.nextCursor ?: return
            if (current.pageLoading || current.rows == null || loadJob?.isActive == true) return
            _state.update { it.copy(pageLoading = true, pageFailed = false) }
            pageJob =
                viewModelScope.launch {
                    when (val result = repository.suspended(SUSPENDED_PAGE_SIZE, cursor)) {
                        is ApiResult.Success ->
                            _state.update {
                                // A refresh cancels this job, but a page that already arrived must still not be appended
                                // to a list that moved on.
                                if (it.nextCursor != cursor) return@update it
                                val known = it.rows.orEmpty().mapTo(HashSet()) { row -> row.card.id }
                                it.copy(
                                    rows =
                                        it.rows.orEmpty() +
                                            result.value.items
                                                .filter { c -> c.id !in known && c.id !in reactivated }
                                                .map(::SuspendedRow),
                                    nextCursor = result.value.nextCursor,
                                    pageLoading = false,
                                )
                            }
                        is ApiResult.Failure -> _state.update { it.copy(pageLoading = false, pageFailed = true) }
                    }
                }
        }

        /** "Riattiva" (and "Riprova" of a failed row): one request, no confirmation, no offline queue. */
        fun reactivate(cardId: String) {
            val row = _state.value.rows?.firstOrNull { it.card.id == cardId } ?: return
            if (row.phase == RowPhase.Busy) return
            setPhase(cardId, RowPhase.Busy)
            viewModelScope.launch {
                when (val result = repository.unsuspend(cardId)) {
                    is ApiResult.Success -> {
                        val dueIn = result.value.due?.let { it.epochSecond - clock.nowMs() / MS_PER_SECOND }
                        reactivated += cardId
                        remove(cardId)
                        effectChannel.send(SuspendedEffect.Reactivated(cardId, dueIn))
                    }
                    is ApiResult.Failure ->
                        when (result.failure) {
                            ApiFailure.ReviewConflict.CardSuperseded -> {
                                reactivated += cardId
                                remove(cardId)
                                effectChannel.send(SuspendedEffect.Superseded)
                            }
                            // Gone in the meantime: leave quietly.
                            ApiFailure.NotFound -> {
                                reactivated += cardId
                                remove(cardId)
                            }
                            else -> setPhase(cardId, RowPhase.Failed)
                        }
                }
            }
        }

        /** "Annulla" in the toast: suspends the card again; its row comes back on top. */
        fun undo(cardId: String) {
            viewModelScope.launch {
                when (val result = repository.suspend(cardId)) {
                    is ApiResult.Success -> {
                        reactivated -= cardId
                        if (_state.value.rows == null) {
                            // The first load is still running (or failed): its answer may predate the undo. Start over.
                            refresh()
                        } else {
                            _state.update {
                                val rest = it.rows.orEmpty()
                                val present = rest.any { row -> row.card.id == cardId }
                                it.copy(
                                    rows =
                                        listOf(SuspendedRow(result.value)) +
                                            rest.filterNot { row -> row.card.id == cardId },
                                    // Idempotent: a row that was already there is already counted.
                                    count = if (present) it.count else it.count + 1,
                                )
                            }
                        }
                    }
                    is ApiResult.Failure ->
                        effectChannel.send(
                            when (result.failure) {
                                ApiFailure.NotFound -> SuspendedEffect.UndoGone
                                ApiFailure.ReviewConflict.CardSuperseded -> SuspendedEffect.UndoSuperseded
                                else -> SuspendedEffect.UndoFailed
                            },
                        )
                }
            }
        }

        private fun setPhase(
            cardId: String,
            phase: RowPhase,
        ) = _state.update { s ->
            s.copy(rows = s.rows?.map { if (it.card.id == cardId) it.copy(phase = phase) else it })
        }

        private fun remove(cardId: String) {
            val rows = _state.value.rows.orEmpty()
            val index = rows.indexOfFirst { it.card.id == cardId }
            if (index >= 0) effectChannel.trySend(SuspendedEffect.MoveFocus(rows.getOrNull(index + 1)?.card?.id))
            _state.update { s ->
                s.copy(rows = s.rows?.filterNot { it.card.id == cardId }, count = (s.count - 1).coerceAtLeast(0))
            }
            // Every loaded row is gone but the backend has more: do not show "empty" over a longer list.
            if (_state.value.rows?.isEmpty() == true) loadMore()
        }

        private companion object {
            const val MS_PER_SECOND = 1_000L
        }
    }
