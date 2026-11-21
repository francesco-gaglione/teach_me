package com.teachme.feature.lesson

import android.util.Log
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ExpectedUserScope
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.StoredStrokes
import com.teachme.core.model.StrokeRecord
import com.teachme.core.model.StrokeStore
import com.teachme.core.model.UserScopes
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkWidth
import com.teachme.sdui.ink.Stroke
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.IOException
import java.util.concurrent.atomic.AtomicInteger
import javax.inject.Inject
import javax.inject.Singleton

/** The tool of the palette (handwriting.md section 10); the highlighter is the default. */
enum class PenTool(
    val wire: String,
) {
    HIGHLIGHTER("highlighter"),
    PEN("pen"),
    ;

    companion object {
        fun fromWire(value: String?): PenTool = entries.firstOrNull { it.wire == value } ?: HIGHLIGHTER
    }
}

/** Why a flush stopped (handwriting.md section 9). */
enum class StrokeFlushStatus {
    /** Nothing is waiting for the lesson any more. */
    Done,

    /** No network, timeout or a server problem: the set stays `dirty` and is sent later. */
    Offline,

    /**
     * The backend refused the set for good (`413`, a `422` without an index or after too many drops, or an unexpected
     * `400`): it is kept and shown, but not retried until the next edit or the next opening of the lesson.
     */
    Parked,

    /** `404`: the lesson does not exist any more; what was stored for it is gone. */
    Gone,

    /** `401`: the session ended. The set is kept (it belongs to its user) and not retried by itself. */
    Unauthorized,
}

data class StrokeFlushResult(
    val status: StrokeFlushStatus,
    /** Strokes dropped after a `422` (also dropped from the stored set): the screen must not show them any more. */
    val discarded: List<StrokeRecord>,
    /** The backend answered `413` for this very flush (the set is too big to be saved): the queue says so. */
    val tooLarge: Boolean = false,
)

internal fun Stroke.toRecord() = StrokeRecord(id, blockId, color.wire, width.wire, points)

/** An unknown colour is ink and an unknown width is medium (lessons.md section 11). */
internal fun StrokeRecord.toStroke() = Stroke(id, blockId, InkColor.fromWire(color), InkWidth.fromWire(width), points)

/**
 * Keeps the handwritten notes of a lesson on the device and sends the whole set to the backend (lessons.md section 11,
 * handwriting.md section 9). Same rules as [HighlightSyncer]: the stored set is the truth while it is `dirty`; a request
 * carries a snapshot and `dirty` is cleared only if the stored set is still that snapshot; a parked set is not sent
 * again by pause or close; one flush at a time; a server set read around a `PUT` never replaces the local one.
 */
@Singleton
class StrokeSyncer
    @Inject
    constructor(
        private val lessons: LessonRepository,
        private val store: StrokeStore,
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
         * becomes the stored one (so the lesson opens with it offline). A [server] set read while a `PUT` was in
         * flight, or before one that ended since [stamp], is never trusted over the local set and is not written.
         */
        suspend fun resolve(
            lessonId: String,
            server: List<StrokeRecord>,
            stamp: FetchStamp,
        ): List<StrokeRecord> {
            var shown = server
            try {
                store.update(lessonId) { old ->
                    val stale = stamp.putInFlight || stamp.epoch != putEpoch.get() || putsInFlight.get() > 0
                    when {
                        old != null && (old.dirty || stale) -> {
                            shown = old.items
                            old
                        }
                        // A server set read around a `PUT` may be older than what was just sent: shown, not kept.
                        old == null && stale -> null
                        else -> StoredStrokes(lessonId, server, dirty = false)
                    }
                }
            } catch (e: IOException) {
                // The copy for offline use could not be written; the lesson still opens with what was read.
                Log.w("Sdui", "notes of '$lessonId' could not be written on the device", e)
            }
            return shown
        }

        /** The set to show when the lesson could not be fetched: whatever this device has. */
        suspend fun local(lessonId: String): List<StrokeRecord> = store.get(lessonId)?.items.orEmpty()

        /** An edit: [items] is now the truth and waits to be sent. Persisted before it returns. */
        suspend fun save(
            lessonId: String,
            items: List<StrokeRecord>,
        ) = store.update(lessonId) { StoredStrokes(lessonId, items, dirty = true) }

        suspend fun hasPending(lessonId: String): Boolean = store.get(lessonId)?.dirty == true

        suspend fun tool(): PenTool = PenTool.fromWire(store.penTool())

        suspend fun setTool(tool: PenTool) = store.setPenTool(tool.wire)

        suspend fun inkColor(): InkColor = InkColor.fromWire(store.inkColor())

        suspend fun setInkColor(color: InkColor) = store.setInkColor(color.wire)

        suspend fun width(): InkWidth = InkWidth.fromWire(store.penWidth())

        suspend fun setWidth(width: InkWidth) = store.setPenWidth(width.wire)

        /** How many lessons have notes waiting to be sent (the exit warning counts them). */
        suspend fun pendingLessons(): Int = store.dirtyLessons().size

        /** Sends the set of every lesson that has changes waiting (app start, back in the foreground). */
        suspend fun flushAll() {
            for (id in store.dirtyLessons()) {
                if (flush(id).status == StrokeFlushStatus.Unauthorized) return
            }
        }

        /** Sends the set of [lessonId] if it has unsent changes. A parked set is only sent when [retryParked]. */
        suspend fun flush(
            lessonId: String,
            retryParked: Boolean = false,
        ): StrokeFlushResult =
            lock.withLock {
                val discarded = mutableListOf<StrokeRecord>()
                val owner =
                    scopes.current() ?: return@withLock StrokeFlushResult(StrokeFlushStatus.Done, discarded)
                var status = StrokeFlushStatus.Done
                var tooLarge = false
                try {
                    while (true) {
                        if (scopes.current() != owner) {
                            status = StrokeFlushStatus.Offline
                            break
                        }
                        val entry = store.get(lessonId)
                        if (entry == null || !entry.dirty) break
                        if (entry.parked && !retryParked) {
                            status = StrokeFlushStatus.Parked
                            break
                        }
                        val sent = entry.items
                        putEpoch.incrementAndGet()
                        putsInFlight.incrementAndGet()
                        val response =
                            try {
                                withContext(ExpectedUserScope(owner)) { lessons.putStrokes(lessonId, sent) }
                            } finally {
                                putsInFlight.decrementAndGet()
                                putEpoch.incrementAndGet()
                            }
                        if (scopes.current() != owner) {
                            status = StrokeFlushStatus.Offline
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
                                status = StrokeFlushStatus.Gone
                            }
                            ApiFailure.Unauthorized -> status = StrokeFlushStatus.Unauthorized
                            is ApiFailure.Validation -> {
                                // The stroke is found in the snapshot that was sent, not in what is stored now.
                                val bad = badIndex(failure)?.let { sent.getOrNull(it) }
                                if (bad != null && discarded.size < MAX_DISCARDS) {
                                    discarded += bad
                                    store.update(
                                        lessonId,
                                    ) { now -> now?.copy(items = now.items.filterNot { it.id == bad.id }) }
                                    continue
                                }
                                status = park(lessonId, sent)
                            }
                            // Not answers this endpoint gives: a bug of the app. Never retried in a loop.
                            is ApiFailure.BadRequest, ApiFailure.InvalidId -> status = park(lessonId, sent)
                            is ApiFailure.Unexpected ->
                                status =
                                    if (failure.httpStatus == HTTP_PAYLOAD_TOO_LARGE) {
                                        tooLarge = true
                                        park(lessonId, sent)
                                    } else {
                                        StrokeFlushStatus.Offline
                                    }
                            // No network, a server problem: the set stays dirty and is sent later.
                            ApiFailure.Conflict,
                            is ApiFailure.Network,
                            is ApiFailure.RateLimited,
                            ApiFailure.Unavailable,
                            ApiFailure.InvalidCredentials,
                            ApiFailure.LlmKeyMissing,
                            ApiFailure.LlmKeyRejected,
                            ApiFailure.LlmCreditExhausted,
                            is ApiFailure.ReviewConflict,
                            -> status = StrokeFlushStatus.Offline
                        }
                        break
                    }
                } catch (e: IOException) {
                    // The disk refused a write (full, not writable): the same as a send that failed. What is stored
                    // stays `dirty`, so the next try sends it again.
                    Log.w("Sdui", "notes of '$lessonId' could not be written on the device", e)
                    status = StrokeFlushStatus.Offline
                }
                StrokeFlushResult(status, discarded, tooLarge)
            }

        /** Marks the snapshot as refused for good; an edit made meanwhile has a different set and stays unparked. */
        private suspend fun park(
            lessonId: String,
            sent: List<StrokeRecord>,
        ): StrokeFlushStatus {
            store.update(lessonId) { now -> if (now != null && now.items == sent) now.copy(parked = true) else now }
            return StrokeFlushStatus.Parked
        }

        /** `strokes[3].points` -> 3. A bare `strokes` (too many strokes or points) has no index. */
        private fun badIndex(failure: ApiFailure.Validation): Int? =
            failure.details.firstNotNullOfOrNull { d ->
                d.field
                    ?.let(INDEX::find)
                    ?.groupValues
                    ?.get(1)
                    ?.toIntOrNull()
            }

        private companion object {
            val INDEX = Regex("""strokes\[(\d+)]""")
            const val HTTP_PAYLOAD_TOO_LARGE = 413
        }
    }
