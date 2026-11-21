package com.teachme.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * A handwritten stroke as it travels (lessons.md section 11). [color] and [width] stay wire strings here; the lesson
 * feature reads them as roles (unknown means `ink` / `medium`). [points] is the flat `[x0, y0, x1, y1, ...]` in
 * ten-thousandths of the block width.
 */
@Serializable
data class StrokeRecord(
    val id: String,
    @SerialName("block_id") val blockId: String,
    val color: String,
    val width: String,
    val points: List<Int>,
)

/**
 * What this device keeps of the handwritten notes of one lesson, in drawing order. [dirty] means [items] were not sent
 * yet: they are the truth until the backend has them (an empty dirty set is "erased everything, not sent yet").
 * [parked] means the backend refused the set for good (`413`, or a `422` that dropping strokes could not fix): it is
 * not retried by itself until the next edit or the next time the lesson is opened.
 */
@Serializable
data class StoredStrokes(
    val lessonId: String,
    val items: List<StrokeRecord>,
    val dirty: Boolean,
    val parked: Boolean = false,
)

/**
 * Persistent, per user (ADR 0008): the notes waiting to be sent, and the pen choices of the device (`pen_tool`,
 * `pen_ink_color`, `pen_width`, as wire strings; null when never chosen). Survives restarts. Every change of the sets
 * goes through [update], which is one atomic read-modify-write.
 */
interface StrokeStore {
    suspend fun get(lessonId: String): StoredStrokes?

    /** Applies [transform] to the entry of [lessonId] (null when none); a null result removes the entry. */
    suspend fun update(
        lessonId: String,
        transform: (StoredStrokes?) -> StoredStrokes?,
    )

    /** Lessons with notes not sent yet. */
    suspend fun dirtyLessons(): List<String>

    suspend fun penTool(): String?

    suspend fun setPenTool(tool: String)

    suspend fun inkColor(): String?

    suspend fun setInkColor(color: String)

    suspend fun penWidth(): String?

    suspend fun setPenWidth(width: String)
}
