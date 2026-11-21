package com.teachme.feature.review

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.LoadError
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.REVIEW_SESSION_SIZE
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.ReviewCardStatus
import com.teachme.core.model.ReviewFlushResult
import com.teachme.core.model.ReviewFlushStatus
import com.teachme.core.model.ReviewRating
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.ReviewSyncer
import com.teachme.core.model.toFormError
import com.teachme.core.model.toLoadError
import com.teachme.core.ui.DialogUi
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.time.Instant
import javax.inject.Inject

/** Intervals and limits of the review screen (review.md); tests swap them for virtual-time friendly values. */
open class ReviewTimings
    @Inject
    constructor() {
        open val sessionSize: Int = REVIEW_SESSION_SIZE

        /** "Sto ancora caricando…" appears after this long. */
        open val slowMs: Long = 8_000L

        /** The session load gives up after this long (review.md section 11). */
        open val loadTimeoutMs: Long = 10_000L

        /** Votes of an earlier session are sent before the due list is read: at most this wait. */
        open val flushWaitStartMs: Long = 2_000L

        /** Votes are sent before the end of the session is shown: at most this wait (review.md section 7). */
        open val flushWaitEndMs: Long = 3_000L

        /** How long a notice without an action stays in the `UndoRow`. */
        open val noticeMs: Long = 4_000L

        /** The suspension notice is longer, so it stays longer (review.md section 6.1). */
        open val suspendedNoticeMs: Long = 6_000L

        /** Wait before the queue is sent again after a failure; doubles up to [retryMaxMs]. */
        open val retryStartMs: Long = 10_000L
        open val retryMaxMs: Long = 300_000L

        /** The backend refuses to undo a vote older than this (reviews.md section 5). */
        open val undoWindowMs: Long = 10 * 60_000L

        /** "Voti inviati." stays in its live region this long. */
        open val sentAnnounceMs: Long = 3_000L
    }

private const val DEFAULT_UNDO_VISIBLE_MS = 5_000L

enum class ReviewPhase { Loading, Failed, Empty, Card, Finished, TopicGone }

/** Keys of the filtered session in the route (the field names of `ReviewRoute`, read from [SavedStateHandle]). */
object ReviewArgs {
    const val TOPIC_ID = "topicId"
    const val TOPIC_TITLE = "topicTitle"

    /** The card on screen and whether its back is up: lets a session survive the death of the process. */
    const val CARD_ID = "reviewCardId"
    const val REVEALED = "reviewRevealed"
}

/** What the `UndoRow` says (review.md section 5). */
sealed interface ReviewNotice {
    /** The last vote, with the "Annulla" action. */
    data class Voted(
        val rating: ReviewRating,
        val intervalSeconds: Long?,
    ) : ReviewNotice

    /** `409` (`already_reviewed`, `card_suspended`) on an immediate send: the card was already graded or suspended. */
    data object AlreadyChanged : ReviewNotice

    /** The backend refused the undo (`409` `undo_expired`: older than 10 minutes; `nothing_to_undo`). */
    data object UndoGone : ReviewNotice

    /** The undo did not leave the device (no network, bad token); the vote stays applied. */
    data object UndoOffline : ReviewNotice

    data object Suspended : ReviewNotice
}

/** A vote given in this session, kept for the end-of-session summary and for undo. */
data class SessionVote(
    val card: ReviewCard,
    val rating: ReviewRating,
    /** The interval shown on the button, in seconds. */
    val intervalSeconds: Long?,
    val queued: QueuedReview,
)

/** Totals of the end of the session; null until the backend has answered. */
data class FinishData(
    /** Cards still due, net of the votes still queued. */
    val residual: Int? = null,
    val nextDueAt: Instant? = null,
)

data class ReviewUiState(
    val phase: ReviewPhase = ReviewPhase.Loading,
    val slowLoading: Boolean = false,
    val loadError: LoadError? = null,
    /** `next_due_at` when nothing is due. */
    val emptyNextDueAt: Instant? = null,
    val cards: List<ReviewCard> = emptyList(),
    val index: Int = 0,
    val revealed: Boolean = false,
    val votes: List<SessionVote> = emptyList(),
    /** Votes dropped because the card had changed (`409`). */
    val conflicts: Int = 0,
    val notice: ReviewNotice? = null,
    /** Changes with every notice, so the screen restarts the timer of the row. */
    val noticeToken: Int = 0,
    /** Until when the last vote can be undone (epoch ms); null when there is nothing to undo. */
    val undoableUntilMs: Long? = null,
    val pendingCount: Int = 0,
    val unauthorized: Boolean = false,
    /** "Voti inviati." live region is on. */
    val sentAnnouncement: Boolean = false,
    /** The suspend dialog, null when closed. */
    val suspendDialog: DialogUi? = null,
    val finish: FinishData = FinishData(),
    /** Title of the topic this session is limited to (review.md section 7.3); null for the whole review. */
    val topicTitle: String? = null,
) {
    val current: ReviewCard? get() = if (phase == ReviewPhase.Card) cards.getOrNull(index) else null
    val total: Int get() = cards.size
}

@HiltViewModel
class ReviewViewModel
    @Inject
    constructor(
        private val repository: ReviewRepository,
        private val syncer: ReviewSyncer,
        private val clock: AppClock,
        private val timings: ReviewTimings,
        private val savedState: SavedStateHandle,
    ) : ViewModel() {
        /** The topic filter of this session; null after "Ripassa tutto". Kept in the saved state. */
        private var topicId: String? = savedState.get<String>(ReviewArgs.TOPIC_ID)?.takeIf { it.isNotBlank() }
        private var topicTitle: String? =
            savedState.get<String>(ReviewArgs.TOPIC_TITLE)?.takeIf { topicId != null && it.isNotBlank() }

        /** Card to go back to once the due list is loaded after the process was recreated; used once. */
        private var restoreCardId: String? = savedState.get<String>(ReviewArgs.CARD_ID)
        private var restoreRevealed: Boolean = savedState.get<Boolean>(ReviewArgs.REVEALED) == true

        private val _state = MutableStateFlow(ReviewUiState(topicTitle = topicTitle))
        val state: StateFlow<ReviewUiState> = _state.asStateFlow()

        /** How long the "Annulla" row stays: longer with accessibility services (set by the screen). */
        var undoVisibleMs: Long = DEFAULT_UNDO_VISIBLE_MS

        private var loadJob: Job? = null
        private var noticeJob: Job? = null
        private var finishJob: Job? = null
        private var enqueueJob: Job? = null
        private var undoJob: Job? = null
        private var announceJob: Job? = null

        /** Wakes the sync loop: new vote, back in the foreground, "Riprova ora". */
        private val wake = Channel<Unit>(Channel.CONFLATED)

        init {
            viewModelScope.launch { syncLoop() }
            viewModelScope.launch {
                // Only the id of the card and the reveal flag are kept, never the cards themselves.
                _state.collect { s ->
                    if (s.phase == ReviewPhase.Loading && restoreCardId != null) return@collect
                    savedState[ReviewArgs.CARD_ID] = s.current?.id
                    savedState[ReviewArgs.REVEALED] = s.current != null && s.revealed
                }
            }
            viewModelScope.launch {
                var hadPending = false
                syncer.pendingCount().collect { n ->
                    _state.update { it.copy(pendingCount = n) }
                    if (hadPending && n == 0) announceSent()
                    hadPending = n > 0
                }
            }
            start()
        }

        // --- loading ---------------------------------------------------------------------------

        /** Loads a new session of up to 20 cards (also "Riprova" and "Ripassa altre"). */
        fun start() {
            // A new session by hand (retry, "Ripassa altre") never goes back to a saved card.
            if (_state.value.phase != ReviewPhase.Loading || loadJob != null) restoreCardId = null
            loadJob?.cancel()
            finishJob?.cancel()
            noticeJob?.cancel()
            _state.update {
                ReviewUiState(
                    pendingCount = it.pendingCount,
                    unauthorized = it.unauthorized,
                    topicTitle = topicTitle,
                )
            }
            loadJob =
                viewModelScope.launch {
                    val slow =
                        launch {
                            delay(timings.slowMs)
                            _state.update { it.copy(slowLoading = true) }
                        }
                    // The grades of the last session must be at the backend before the due list is read.
                    wake.trySend(Unit)
                    waitForEmptyQueue(timings.flushWaitStartMs)
                    val result =
                        withTimeoutOrNull(timings.loadTimeoutMs) { repository.due(timings.sessionSize, topicId) }
                    slow.cancel()
                    when (result) {
                        is ApiResult.Success -> onLoaded(result.value.items, result.value.nextDueAt)
                        is ApiResult.Failure -> {
                            if (result.failure is ApiFailure.Unauthorized) markUnauthorized()
                            _state.update {
                                if (result.failure is ApiFailure.NotFound && topicId != null) {
                                    // The topic is gone: not a network error (review.md section 7.3).
                                    it.copy(phase = ReviewPhase.TopicGone, topicTitle = null)
                                } else {
                                    it.copy(phase = ReviewPhase.Failed, loadError = result.failure.toLoadError())
                                }
                            }
                        }
                        null -> _state.update { it.copy(phase = ReviewPhase.Failed, loadError = LoadError.Network) }
                    }
                }
        }

        /** "Ripassa tutto" after the topic is gone: a new session without the filter. */
        fun reviewAll() {
            topicId = null
            topicTitle = null
            savedState[ReviewArgs.TOPIC_ID] = null
            savedState[ReviewArgs.TOPIC_TITLE] = null
            start()
        }

        private suspend fun onLoaded(
            items: List<ReviewCard>,
            nextDueAt: Instant?,
        ) {
            // A card whose grade is still waiting in the queue is not shown again.
            val queued = syncer.pending().map { it.cardId }.toSet()
            val cards =
                items
                    .filter { it.status == ReviewCardStatus.Active && it.id !in queued }
                    .take(timings.sessionSize)
            val at = restoreCardId?.let { id -> cards.indexOfFirst { it.id == id } } ?: -1
            val revealed = at >= 0 && restoreRevealed
            restoreCardId = null
            restoreRevealed = false
            _state.update {
                if (cards.isEmpty()) {
                    it.copy(phase = ReviewPhase.Empty, emptyNextDueAt = nextDueAt)
                } else {
                    it.copy(phase = ReviewPhase.Card, cards = cards, index = at.coerceAtLeast(0), revealed = revealed)
                }
            }
        }

        private suspend fun waitForEmptyQueue(maxMs: Long) {
            withTimeoutOrNull(maxMs) { syncer.pendingCount().first { it == 0 } }
        }

        // --- the card --------------------------------------------------------------------------

        fun reveal() {
            _state.update { if (it.phase == ReviewPhase.Card && !it.revealed) it.copy(revealed = true) else it }
        }

        /** Grades the current card. Optimistic: the grade goes to the queue and the next card comes at once. */
        fun rate(rating: ReviewRating) {
            val s = _state.value
            val card = s.current ?: return
            if (!s.revealed) return
            val now = clock.nowMs()
            val queued = QueuedReview(card.id, rating.wire, now, card.reviewCount)
            val interval = card.intervals?.forRating(rating)
            val vote = SessionVote(card, rating, interval, queued)
            val next = s.index + 1
            _state.update {
                it.copy(
                    votes = it.votes + vote,
                    index = next,
                    revealed = false,
                    phase = if (next >= it.cards.size) ReviewPhase.Finished else ReviewPhase.Card,
                    undoableUntilMs = now + timings.undoWindowMs,
                    finish = FinishData(),
                )
            }
            showNotice(ReviewNotice.Voted(rating, interval), undoVisibleMs)
            enqueueJob =
                viewModelScope.launch {
                    syncer.enqueue(queued)
                    wake.trySend(Unit)
                }
            if (next >= s.cards.size) prepareFinish()
        }

        // --- undo ------------------------------------------------------------------------------

        /** True when the last vote can still be undone (the menu entry and Ctrl+Z). */
        fun canUndo(): Boolean {
            val s = _state.value
            return s.votes.isNotEmpty() && (s.undoableUntilMs?.let { clock.nowMs() <= it } == true)
        }

        fun undo() {
            val vote = _state.value.votes.lastOrNull() ?: return
            if (!canUndo()) {
                showNotice(ReviewNotice.UndoGone, timings.noticeMs)
                return
            }
            if (undoJob?.isActive == true) return
            undoJob =
                viewModelScope.launch {
                    enqueueJob?.join()
                    when (syncer.withdraw(vote.queued)) {
                        ReviewSyncer.Withdrawn.FromQueue -> {
                            // Never sent: nothing to ask the backend.
                            restore(vote, vote.card)
                            return@launch
                        }
                        ReviewSyncer.Withdrawn.Dropped -> {
                            // The backend refused that grade: undoing it there would undo someone else's.
                            _state.update { it.copy(undoableUntilMs = null) }
                            showNotice(ReviewNotice.UndoGone, timings.noticeMs)
                            return@launch
                        }
                        ReviewSyncer.Withdrawn.Applied -> Unit
                    }
                    when (val result = repository.undo(vote.card.id)) {
                        is ApiResult.Success -> restore(vote, result.value)
                        is ApiResult.Failure ->
                            when (result.failure) {
                                ApiFailure.ReviewConflict.NothingToUndo,
                                ApiFailure.ReviewConflict.UndoExpired,
                                ApiFailure.Conflict,
                                ApiFailure.NotFound,
                                -> {
                                    _state.update { it.copy(undoableUntilMs = null) }
                                    showNotice(ReviewNotice.UndoGone, timings.noticeMs)
                                }
                                else -> {
                                    if (result.failure is ApiFailure.Unauthorized) markUnauthorized()
                                    showNotice(ReviewNotice.UndoOffline, timings.noticeMs)
                                }
                            }
                    }
                }
        }

        /** Back to the undone card, already on its back: the answer was seen, redoing the front would falsify recall. */
        private fun restore(
            vote: SessionVote,
            card: ReviewCard,
        ) {
            finishJob?.cancel()
            noticeJob?.cancel()
            _state.update { s ->
                val at = s.cards.indexOfFirst { it.id == vote.card.id }
                if (at < 0) return@update s
                s.copy(
                    phase = ReviewPhase.Card,
                    cards = s.cards.toMutableList().also { it[at] = card },
                    index = at,
                    revealed = true,
                    votes = s.votes.filterNot { it.queued.sameReview(vote.queued) },
                    undoableUntilMs = null,
                    notice = null,
                    finish = FinishData(),
                )
            }
        }

        // --- suspend ---------------------------------------------------------------------------

        fun openSuspend() {
            if (_state.value.current != null) _state.update { it.copy(suspendDialog = DialogUi()) }
        }

        fun dismissSuspend() {
            if (_state.value.suspendDialog?.submitting == true) return
            _state.update { it.copy(suspendDialog = null) }
        }

        fun confirmSuspend() {
            val s = _state.value
            val card = s.current ?: return
            if (s.suspendDialog?.submitting != false) return
            _state.update { it.copy(suspendDialog = DialogUi(submitting = true)) }
            viewModelScope.launch {
                val result = repository.suspend(card.id)
                val gone = (result as? ApiResult.Failure)?.failure is ApiFailure.NotFound
                if (result is ApiResult.Success || gone) {
                    onSuspended(card)
                } else if (result is ApiResult.Failure) {
                    _state.update { it.copy(suspendDialog = DialogUi(error = result.failure.toFormError("none"))) }
                }
            }
        }

        private fun onSuspended(card: ReviewCard) {
            var finished = false
            _state.update { s ->
                val cards = s.cards.filterNot { it.id == card.id }
                finished = s.index >= cards.size
                s.copy(
                    cards = cards,
                    revealed = false,
                    suspendDialog = null,
                    phase = if (finished) ReviewPhase.Finished else ReviewPhase.Card,
                )
            }
            showNotice(ReviewNotice.Suspended, timings.suspendedNoticeMs)
            when {
                !finished -> Unit
                _state.value.votes.isEmpty() -> showEmpty()
                else -> prepareFinish()
            }
        }

        /** Nothing was graded and nothing is left: the same state as "nothing due", with the next date. */
        private fun showEmpty() {
            _state.update { it.copy(phase = ReviewPhase.Loading, slowLoading = false) }
            finishJob?.cancel()
            finishJob =
                viewModelScope.launch {
                    val result = repository.due(1, topicId)
                    if (result is ApiResult.Failure && result.failure is ApiFailure.NotFound) {
                        // The topic vanished halfway: the normal end of the session, not "nothing to review".
                        _state.update { it.copy(phase = ReviewPhase.Finished, finish = FinishData()) }
                    } else {
                        val next = (result as? ApiResult.Success)?.value?.nextDueAt
                        _state.update { it.copy(phase = ReviewPhase.Empty, emptyNextDueAt = next) }
                    }
                }
        }

        // --- end of the session ----------------------------------------------------------------

        private fun prepareFinish() {
            finishJob?.cancel()
            finishJob =
                viewModelScope.launch {
                    wake.trySend(Unit)
                    waitForEmptyQueue(timings.flushWaitEndMs)
                    val queued = syncer.pending().size
                    val result = repository.due(1, topicId)
                    if (result is ApiResult.Success) {
                        _state.update {
                            it.copy(
                                finish =
                                    FinishData(
                                        residual = (result.value.dueCount - queued).coerceAtLeast(0),
                                        nextDueAt = result.value.nextDueAt,
                                    ),
                            )
                        }
                    } else if ((result as ApiResult.Failure).failure is ApiFailure.Unauthorized) {
                        // Any other failure (a 404 on the topic included) is silent: the lines just do not show.
                        markUnauthorized()
                    }
                }
        }

        // --- notices ---------------------------------------------------------------------------

        private fun showNotice(
            notice: ReviewNotice,
            ms: Long,
        ) {
            noticeJob?.cancel()
            _state.update { it.copy(notice = notice, noticeToken = it.noticeToken + 1) }
            val token = _state.value.noticeToken
            noticeJob =
                viewModelScope.launch {
                    delay(ms)
                    _state.update { if (it.noticeToken == token) it.copy(notice = null) else it }
                }
        }

        fun dismissNotice() {
            noticeJob?.cancel()
            _state.update { it.copy(notice = null) }
        }

        private fun announceSent() {
            announceJob?.cancel()
            _state.update { it.copy(sentAnnouncement = true) }
            announceJob =
                viewModelScope.launch {
                    delay(timings.sentAnnounceMs)
                    _state.update { it.copy(sentAnnouncement = false) }
                }
        }

        // --- the queue of grades ---------------------------------------------------------------

        /** Back in the foreground, or "Riprova ora": send what is waiting. */
        fun syncNow() {
            // Back after a sign-in over this screen (login.md): a load that ended with a `401` is simply done again.
            val s = _state.value
            if (s.phase == ReviewPhase.Failed && s.loadError == LoadError.Unauthorized) start()
            wake.trySend(Unit)
        }

        private fun markUnauthorized() = _state.update { it.copy(unauthorized = true) }

        /**
         * One loop sends the queue, so a request is never cancelled halfway (a vote the backend applied but the app did
         * not hear about would come back as a false `409`). After a failure it waits 10 s, then twice as long up to 5
         * minutes, unless woken earlier.
         */
        private suspend fun syncLoop() {
            var wait = timings.retryStartMs
            var offline = false
            while (true) {
                // A new grade or "Riprova ora" wakes the loop early and does not make the pause longer.
                val timedOut =
                    if (offline) {
                        withTimeoutOrNull(
                            wait,
                        ) { wake.receive() } == null
                    } else {
                        wake.receive().let { false }
                    }
                val result = syncer.flush()
                handleFlush(result)
                offline = result.status == ReviewFlushStatus.Offline
                wait =
                    when {
                        offline && timedOut -> (wait * 2).coerceAtMost(timings.retryMaxMs)
                        offline -> wait
                        else -> timings.retryStartMs
                    }
            }
        }

        private fun handleFlush(result: ReviewFlushResult) {
            _state.update { s ->
                var next = s.copy(unauthorized = result.status == ReviewFlushStatus.Unauthorized)
                if (result.conflicts.isNotEmpty() || result.discarded.isNotEmpty()) {
                    val dropped = (result.conflicts + result.discarded).map { it.cardId }.toSet()
                    next =
                        next.copy(
                            votes = next.votes.filterNot { it.card.id in dropped },
                            conflicts = next.conflicts + result.conflicts.size,
                            undoableUntilMs =
                                if (next.votes
                                        .lastOrNull()
                                        ?.card
                                        ?.id in dropped
                                ) {
                                    null
                                } else {
                                    next.undoableUntilMs
                                },
                        )
                }
                next
            }
            if (result.conflicts.isNotEmpty()) showNotice(ReviewNotice.AlreadyChanged, timings.noticeMs)
        }
    }
