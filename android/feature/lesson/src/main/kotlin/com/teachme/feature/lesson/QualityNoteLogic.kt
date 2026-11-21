package com.teachme.feature.lesson

import com.teachme.core.model.QualityWarning

/** One line of the "Nota sulla lezione" list: a warning, or several of the same code folded into one. */
data class QualityNoteEntry(
    val code: String,
    val message: String,
    /** The block "Vai al blocco" goes to (the first one of the group that has a block), or null. */
    val blockId: String?,
    /** How many warnings this line stands for; more than 1 only for a folded group. */
    val count: Int = 1,
)

/** What the open note lists: at most [QualityNote.MAX_ENTRIES] lines, and how many were left out. */
data class QualityNote(
    val entries: List<QualityNoteEntry>,
    val hidden: Int,
    /** Number of warnings of the lesson, the counter of the closed row. */
    val total: Int,
) {
    companion object {
        const val MAX_ENTRIES = 6

        /** A code repeated more than this many times becomes one line (quality-warnings.md section 3). */
        const val FOLD_ABOVE = 2
    }
}

/**
 * Pure: folds a `code` that appears more than [QualityNote.FOLD_ABOVE] times into one line at the place of its first
 * occurrence (document order is kept), then keeps the first [QualityNote.MAX_ENTRIES] lines. [blockExists] drops the
 * `block_id` of a block the learner cannot reach.
 */
fun buildQualityNote(
    warnings: List<QualityWarning>,
    blockExists: (String) -> Boolean = { true },
): QualityNote {
    val byCode = warnings.groupBy { it.code }
    val seen = mutableSetOf<String>()
    val lines = mutableListOf<QualityNoteEntry>()
    for (w in warnings) {
        val group = byCode.getValue(w.code)

        fun target(list: List<QualityWarning>) = list.firstNotNullOfOrNull { it.blockId?.takeIf(blockExists) }
        if (group.size > QualityNote.FOLD_ABOVE) {
            if (seen.add(w.code)) lines += QualityNoteEntry(w.code, w.message, target(group), group.size)
        } else {
            lines += QualityNoteEntry(w.code, w.message, target(listOf(w)))
        }
    }
    val shown = lines.take(QualityNote.MAX_ENTRIES)
    return QualityNote(shown, lines.size - shown.size, warnings.size)
}
