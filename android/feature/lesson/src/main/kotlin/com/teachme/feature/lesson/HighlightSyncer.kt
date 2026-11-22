package com.teachme.feature.lesson

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ExpectedUserScope
import com.teachme.core.model.HighlightRecord
import com.teachme.core.model.HighlightStore
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.StoredHighlights
import com.teachme.core.model.UserScopes
import com.teachme.sdui.highlight.Highlight
import com.teachme.sdui.highlight.HighlightColor
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.util.concurrent.atomic.AtomicInteger
import javax.inject.Inject
import javax.inject.Singleton

/** How many entries one flush drops after `422` before it gives up on the set (highlight.md section 11). */
internal const val MAX_DISCARDS = 3

/** Why a flush stopped (highlight.md section 11). */
enum class HighlightFlushStatus {
    /** Nothing is waiting for the lesson any more. */
    Done,

    /** No network, timeout or a server problem: the set stays `dirty` and is sent later. */
    Offline,

    /**
     * The backend refused the set for good (`422` that could not be fixed by dropping entries, or an unexpected `400`):
     * it is kept and shown, but not retried until the next edit or the next opening of the lesson.
     */
    Parked,

    /** `404`: the lesson does not exist any more; what was stored for it is gone. */
    Gone,

    /** `401`: the session ended. The set is kept (it belongs to its user) and not retried by itself. */
    Unauthorized,
}

data class HighlightFlushResult(
    val status: HighlightFlushStatus,
    /** Entries dropped after a `422` (also dropped from the stored set): the screen must not show them any more. */
    val discarded: List<HighlightRecord>,
)

internal fun Highlight.toRecord() = HighlightRecord(blockId, start, end, color.wire)

/** An unknown colour is yellow (lessons.md section 10). */
internal fun HighlightRecord.toHighlight() = Highlight(blockId, start, end, HighlightColor.fromWire(color))

/**
 * Taken with [HighlightSyncer.stamp] before asking the backend for a lesson: tells [HighlightSyncer.resolve] whether a
 * `PUT` could have run while the answer was on its way (then the answer may be older than the stored set).
 */
class FetchStamp internal constructor(
    internal val epoch: Int,
    internal val putInFlight: Boolean,
)

/**
 * Keeps the highlights of a lesson on the device and sends the whole set to the backend (lessons.md section 10,
 * highlight.md section 11). The stored set is the truth while it is `dirty`. A request carries a snapshot; when it
 * succeeds `dirty` is cleared only if the stored set is still that snapshot, so an edit made meanwhile is sent next.
 * One flush at a time.
 */
@Singleton
class HighlightSyncer
    @Inject
    constructor(
        private val lessons: LessonRepository,
        private val store: HighlightStore,
        private val scopes: UserScopes,
    ) {
        private val lock = Mutex()

        // Counts the starts and the ends of every `PUT`, so a lesson fetched while one was out can be recognised.
        private val putEpoch = AtomicInteger()
        private val putsInFlight = AtomicInteger()

        /** Call right before fetching the lesson, and hand the result to [resolve] with the fetched set. */
        fun stamp() = FetchStamp(putEpoch.get(), putsInFlight.get() > 0)

        /**
         * The set to show when the lesson was fetched: the stored one if it has unsent changes, else [server]'s, which
         * becomes the stored one (so the lesson opens with it offline).
         *
         * A [server] set read while a `PUT` was in flight, or before one that ended since [stamp], can be older than
         * what that `PUT` stored: it is never trusted over the local set (which then is the sent one, no longer
         * `dirty`) and is not written. The next opening reads the backend again.
         */
        suspend fun resolve(
            lessonId: String,
            server: List<HighlightRecord>,
            stamp: FetchStamp,
        ): List<HighlightRecord> {
            var shown = server
            store.update(lessonId) { old ->
                val stale = stamp.putInFlight || stamp.epoch != putEpoch.get() || putsInFlight.get() > 0
                if (old != null && (old.dirty || stale)) {
                    shown = old.items
                    old
                } else {
                    StoredHighlights(lessonId, server, dirty = false)
                }
            }
            return shown
        }

        /** The set to show when the lesson could not be fetched: whatever this device has. */
        suspend fun local(lessonId: String): List<HighlightRecord> = store.get(lessonId)?.items.orEmpty()

        /** An edit: [items] is now the truth and waits to be sent. Persisted before it returns. */
        suspend fun save(
            lessonId: String,
            items: List<HighlightRecord>,
        ) = store.update(lessonId) { StoredHighlights(lessonId, items, dirty = true) }

        suspend fun hasPending(lessonId: String): Boolean = store.get(lessonId)?.dirty == true

        suspend fun color(): HighlightColor = HighlightColor.fromWire(store.color())

        suspend fun setColor(color: HighlightColor) = store.setColor(color.wire)

        /** How many lessons have changes waiting to be sent (the exit warning counts them). */
        suspend fun pendingLessons(): Int = store.dirtyLessons().size

        /** Sends the sets of every lesson that has changes waiting (app start, back in the foreground). */
        suspend fun flushAll() {
            for (id in store.dirtyLessons()) {
                if (flush(id).status == HighlightFlushStatus.Unauthorized) return
            }
        }

        /** Sends the set of [lessonId] if it has unsent changes. A parked set is only sent when [retryParked]. */
        suspend fun flush(
            lessonId: String,
            retryParked: Boolean = false,
        ): HighlightFlushResult =
            lock.withLock {
                val discarded = mutableListOf<HighlightRecord>()
                val owner =
                    scopes.current() ?: return@withLock HighlightFlushResult(HighlightFlushStatus.Done, discarded)
                var status = HighlightFlushStatus.Done
                while (true) {
                    if (scopes.current() != owner) {
                        status = HighlightFlushStatus.Offline
                        break
                    }
                    val entry = store.get(lessonId)
                    if (entry == null || !entry.dirty) break
                    if (entry.parked && !retryParked) {
                        status = HighlightFlushStatus.Parked
                        break
                    }
                    val sent = entry.items
                    putEpoch.incrementAndGet()
                    putsInFlight.incrementAndGet()
                    val response =
                        try {
                            withContext(ExpectedUserScope(owner)) { lessons.putHighlights(lessonId, sent) }
                        } finally {
                            putsInFlight.decrementAndGet()
                            putEpoch.incrementAndGet()
                        }
                    if (scopes.current() != owner) {
                        status = HighlightFlushStatus.Offline
                        break
                    }
                    val failure =
                        when (response) {
                            is ApiResult.Success -> {
                                store.update(lessonId) { now ->
                                    if (now != null &&
                                        now.items == sent
                                    ) {
                                        now.copy(dirty = false, parked = false)
                                    } else {
                                        now
                                    }
                                }
                                null
                            }
                            is ApiResult.Failure -> response.failure
                        } ?: continue
                    when (failure) {
                        ApiFailure.NotFound -> {
                            store.update(lessonId) { null }
                            status = HighlightFlushStatus.Gone
                        }
                        ApiFailure.Unauthorized -> status = HighlightFlushStatus.Unauthorized
                        is ApiFailure.Validation -> {
                            // The entry is found in the snapshot that was sent, not in what is stored now.
                            val bad = badIndex(failure)?.let { sent.getOrNull(it) }
                            if (bad != null && discarded.size < MAX_DISCARDS) {
                                discarded += bad
                                store.update(lessonId) { now ->
                                    now?.copy(
                                        items =
                                            now.items.filterNot {
                                                it.blockId == bad.blockId && it.start == bad.start && it.end == bad.end
                                            },
                                    )
                                }
                                continue
                            }
                            status = park(lessonId, sent)
                        }
                        // Not answers this endpoint gives: a bug of the app. Never retried in a loop.
                        is ApiFailure.BadRequest, ApiFailure.InvalidId -> status = park(lessonId, sent)
                        // No network, a server problem, a limit: the set stays dirty and is sent later.
                        ApiFailure.Conflict,
                        is ApiFailure.Network,
                        is ApiFailure.Unexpected,
                        is ApiFailure.RateLimited,
                        ApiFailure.Unavailable,
                        ApiFailure.InvalidCredentials,
                        ApiFailure.LlmKeyMissing,
                        ApiFailure.LlmKeyRejected,
                        ApiFailure.LlmCreditExhausted,
                        is ApiFailure.LlmBudgetExceeded,
                        ApiFailure.StorageQuotaExceeded,
                        is ApiFailure.ReviewConflict,
                        -> status = HighlightFlushStatus.Offline
                    }
                    break
                }
                HighlightFlushResult(status, discarded)
            }

        /** Marks the snapshot as refused for good; an edit made meanwhile has a different set and stays unparked. */
        private suspend fun park(
            lessonId: String,
            sent: List<HighlightRecord>,
        ): HighlightFlushStatus {
            store.update(lessonId) { now -> if (now != null && now.items == sent) now.copy(parked = true) else now }
            return HighlightFlushStatus.Parked
        }

        /** `highlights[3].start` -> 3. */
        private fun badIndex(failure: ApiFailure.Validation): Int? =
            failure.details.firstNotNullOfOrNull { d ->
                d.field
                    ?.let(INDEX::find)
                    ?.groupValues
                    ?.get(1)
                    ?.toIntOrNull()
            }

        private companion object {
            val INDEX = Regex("""highlights\[(\d+)]""")
        }
    }
