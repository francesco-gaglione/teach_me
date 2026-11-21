package com.teachme.sdui.highlight

import androidx.compose.runtime.staticCompositionLocalOf

/**
 * What the lesson screen offers to the text blocks that can be highlighted (`paragraph` and the text of `callout`):
 * the set to draw and the edits the learner makes with the pen or with a tap. Intervals are always in the markdown
 * source of the block's `text` (`lessons.md` section 10); the block converts with its `SourceMap`. The host shows its
 * own feedback (limit toast, "Annulla"), the block never does.
 *
 * [highlights] must be read from Compose state, so a change redraws the blocks that show it.
 */
interface HighlightHost {
    /** The highlights of the lesson shown now. */
    val highlights: List<Highlight>

    /** The colour a new stroke gets. */
    val color: HighlightColor

    /**
     * True while the pen tool writes ink (handwriting.md section 7): the text then leaves the pen to the lesson screen
     * (no highlight, no eraser, no tap on a term or a highlight). The finger is not affected.
     */
    val penWrites: Boolean get() = false

    /**
     * True while the "Gomma" switch is on (handwriting.md section 3.4): the stylus tip then erases like the physical
     * eraser. The finger is not affected.
     */
    val eraserMode: Boolean get() = false

    /** How many handwritten notes (strokes) the lesson has on the top-level block [blockId]: read by the screen reader. */
    fun noteCount(blockId: String): Int = 0

    /** True once the pen was seen on this screen: only then the notes can be changed, also by a screen reader action. */
    val canClearNotes: Boolean get() = false

    /** The screen reader action "Elimina le note a mano del blocco": takes all the notes of [blockId] away. */
    fun clearNotes(blockId: String) = Unit

    /** A pen stroke over [start, end) of the block's source. */
    fun add(
        blockId: String,
        start: Int,
        end: Int,
    )

    /** The eraser (or the pen with its button) passed over [start, end) of the block's source. */
    fun erase(
        blockId: String,
        start: Int,
        end: Int,
    )

    /** "Rimuovi": removes whole the highlight under the source [position]. */
    fun remove(
        blockId: String,
        position: Int,
    )
}

/** No highlighter: nothing is drawn and nothing can be edited (previews, standalone rendering). */
object NoHighlights : HighlightHost {
    override val highlights: List<Highlight> = emptyList()
    override val color: HighlightColor = HighlightColor.YELLOW

    override fun add(
        blockId: String,
        start: Int,
        end: Int,
    ) = Unit

    override fun erase(
        blockId: String,
        start: Int,
        end: Int,
    ) = Unit

    override fun remove(
        blockId: String,
        position: Int,
    ) = Unit
}

val LocalHighlightHost = staticCompositionLocalOf<HighlightHost> { NoHighlights }

/**
 * Marks an `InlineText` as highlightable: its text is the `text` field of the block [blockId]. [onPenContact] runs
 * when the pen touches the text (the block clears a selection of the system, highlight.md section 9).
 */
class HighlightTarget(
    val blockId: String,
    val onPenContact: () -> Unit = {},
)
