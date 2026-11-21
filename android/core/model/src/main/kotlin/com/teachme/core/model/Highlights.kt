package com.teachme.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * A highlight as it travels (lessons.md section 10): `[start, end)` in UTF-16 units of the source of the `text` of
 * block [blockId]. [color] stays the wire string here; the lesson feature reads it as a colour (unknown means yellow).
 */
@Serializable
data class HighlightRecord(
    @SerialName("block_id") val blockId: String,
    val start: Int,
    val end: Int,
    val color: String,
)

/**
 * What this device keeps of the highlights of one lesson. [dirty] means [items] were not sent yet: they are the truth
 * until the backend has them (an empty dirty set is "erased everything, not sent yet"). [parked] means the backend
 * refused the set for good: it is not retried by itself until the next edit or the next time the lesson is opened.
 */
@Serializable
data class StoredHighlights(
    val lessonId: String,
    val items: List<HighlightRecord>,
    val dirty: Boolean,
    val parked: Boolean = false,
)

/**
 * Persistent, per user (ADR 0008): the highlights waiting to be sent, and the colour the pen used last. Survives
 * restarts. Every change goes through [update], which is one atomic read-modify-write.
 */
interface HighlightStore {
    suspend fun get(lessonId: String): StoredHighlights?

    /** Applies [transform] to the entry of [lessonId] (null when none); a null result removes the entry. */
    suspend fun update(
        lessonId: String,
        transform: (StoredHighlights?) -> StoredHighlights?,
    )

    /** Lessons with highlights not sent yet. */
    suspend fun dirtyLessons(): List<String>

    /** The last colour chosen, as a wire string, or null if never chosen. */
    suspend fun color(): String?

    suspend fun setColor(color: String)
}

/** Most highlights of one lesson (lessons.md section 10). */
const val MAX_HIGHLIGHTS = 500
