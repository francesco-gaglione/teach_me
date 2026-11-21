package com.teachme.feature.lesson

import com.teachme.sdui.model.Exercise

/**
 * One step of the continuous reading, in document order (audio.md section 6). Only blocks of the revealed sections are
 * slots: reading never reveals a section.
 */
sealed interface ReadingSlot {
    val blockId: String

    /** A block that may have a clip; the reading skips it when it has none. */
    data class Read(
        override val blockId: String,
    ) : ReadingSlot

    /** A required exercise not answered yet: the reading stops before it and never reads it. */
    data class Gate(
        override val blockId: String,
    ) : ReadingSlot
}

/**
 * The slots of the rows on screen. Exercises are never read: a required one still open is a [ReadingSlot.Gate], one that
 * is optional or already answered is not a slot at all (the reading goes past it).
 */
fun readingSlots(
    items: List<LessonItem>,
    isAnswered: (String) -> Boolean,
): List<ReadingSlot> =
    items.mapNotNull { item ->
        val block = (item as? LessonItem.BlockItem)?.block ?: return@mapNotNull null
        when {
            block.id.isEmpty() -> null
            block is Exercise -> if (block.required && !isAnswered(block.id)) ReadingSlot.Gate(block.id) else null
            else -> ReadingSlot.Read(block.id)
        }
    }

/**
 * Where "Ascolta la lezione" starts: the first slot at or after the row [firstVisibleIndex], that is the first readable
 * block visible at the top (audio.md section 6). Null when no slot is left below.
 */
fun firstSlotAtOrAfter(
    items: List<LessonItem>,
    slots: List<ReadingSlot>,
    firstVisibleIndex: Int,
): String? {
    val ids = slots.mapTo(HashSet()) { it.blockId }
    return items
        .drop(firstVisibleIndex.coerceAtLeast(0))
        .firstNotNullOfOrNull { (it as? LessonItem.BlockItem)?.block?.id?.takeIf { id -> id in ids } }
}

/** True when the row spanning [offset] to [offset] + [size] lies entirely inside [windowStart], [windowEnd]. */
fun isFullyVisible(
    offset: Int,
    size: Int,
    windowStart: Int,
    windowEnd: Int,
): Boolean = offset >= windowStart && offset + size <= windowEnd
