package com.teachme.feature.lesson

import com.teachme.sdui.logic.LessonSavedState
import com.teachme.sdui.logic.LessonSections
import com.teachme.sdui.model.Block
import com.teachme.sdui.model.LessonDocument
import com.teachme.sdui.render.BlockGap
import com.teachme.sdui.render.gapBetween

/** One row of the lesson column. [key] is stable across recompositions and restores. */
sealed interface LessonItem {
    val key: String

    /** Banner for a lesson that uses a newer catalog than this app. */
    data object NewerCatalogBanner : LessonItem {
        override val key = "banner"
    }

    /**
     * [ordinal] is the position among the block rows: blocks without an id (the app tolerates them as
     * neutral rows) get a synthetic key, and the `b:` prefix keeps every key clear of the reserved ones.
     */
    data class BlockItem(
        val block: Block,
        val sectionIndex: Int,
        val gap: BlockGap,
        val ordinal: Int = 0,
    ) : LessonItem {
        override val key: String get() = if (block.id.isEmpty()) "b#$ordinal" else blockKey(block.id)
    }

    /** The lesson objective and duration, drawn by the app right after the title (not a block). */
    data class Objective(
        val text: String,
        val minutes: Int,
        /** Title of the first source taken from the learner's PDF, if the lesson has any (lesson.md 3.3). */
        val pdfTitle: String? = null,
        /** How many more PDF sources there are after [pdfTitle]. */
        val pdfMore: Int = 0,
    ) : LessonItem {
        override val key = "objective"
    }

    /** "Nota sulla lezione" (quality-warnings.md section 3): after the last block, before the "Continua" bar. */
    data object QualityNote : LessonItem {
        override val key = "quality_note"
    }

    /** The "Continua" bar at the end of the current section. */
    data class Continue(
        val sectionIndex: Int,
    ) : LessonItem {
        override val key: String get() = "continue_$sectionIndex"
    }
}

/** List row key of the block with this [id] (see [LessonItem.BlockItem.key]). */
fun blockKey(id: String): String = LessonSavedState.BLOCK_KEY_PREFIX + id

/** Flattens the revealed sections into the rows of the column. Pure and cheap. */
fun buildLessonItems(
    document: LessonDocument,
    sections: LessonSections,
    revealedCount: Int,
    showContinue: Boolean = true,
    showQualityNote: Boolean = false,
): List<LessonItem> {
    val items = mutableListOf<LessonItem>()
    if (document.newerCatalog) items += LessonItem.NewerCatalogBanner
    var previous: Block? = null
    var afterObjective = false
    var ordinal = 0
    val revealed = sections.sections.take(revealedCount.coerceAtLeast(1))
    for (section in revealed) {
        var first = true
        for (block in section.items) {
            // The h2 that opens a revealed section sits right under the previous "Continua" bar: it gets the section rhythm.
            val gap =
                when {
                    afterObjective -> BlockGap.Block
                    previous == null -> BlockGap.None
                    first && section.index > 0 -> BlockGap.Section
                    else -> gapBetween(previous, block, document.titleBlockId)
                }
            afterObjective = false
            items += LessonItem.BlockItem(block, section.index, gap, ordinal++)
            if (block.id == document.titleBlockId && document.objective.isNotBlank()) {
                val pdf = document.sources.filter { it.isUserPdf }
                items +=
                    LessonItem.Objective(
                        document.objective,
                        document.estimatedMinutes,
                        pdfTitle = pdf.firstOrNull()?.title,
                        pdfMore = (pdf.size - 1).coerceAtLeast(0),
                    )
                afterObjective = true
            }
            previous = block
            first = false
        }
    }
    if (showQualityNote && revealed.isNotEmpty()) items += LessonItem.QualityNote
    if (showContinue && revealed.isNotEmpty()) items += LessonItem.Continue(revealed.last().index)
    return items
}

/** Index in [items] of the first row of [sectionIndex], or -1. */
fun firstItemOfSection(
    items: List<LessonItem>,
    sectionIndex: Int,
): Int = items.indexOfFirst { it is LessonItem.BlockItem && it.sectionIndex == sectionIndex }

/** Index in [items] of the row with [key], or -1. */
fun indexOfKey(
    items: List<LessonItem>,
    key: String,
): Int = items.indexOfFirst { it.key == key }
