package com.teachme.sdui.highlight

import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import com.teachme.sdui.R

/**
 * What a screen reader hears about the handwritten notes of one top-level block (handwriting.md section 10): the count in
 * the `stateDescription` ("1 nota a mano", joined with the highlights' count by the text blocks) and the custom action
 * that takes them away, only once the pen was seen. The notes themselves are drawings: nothing of them is read.
 */
internal class NotesA11y(
    val count: Int,
    val stateText: String?,
    val clearAction: CustomAccessibilityAction?,
)

@Composable
internal fun rememberNotesA11y(
    host: HighlightHost,
    blockId: String,
): NotesA11y {
    // The count is a derived state, so a block only recomposes when its own notes change.
    val count by remember(host, blockId) { derivedStateOf { host.noteCount(blockId) } }
    val canClear by remember(host) { derivedStateOf { host.canClearNotes } }
    if (count <= 0) return NotesA11y(0, null, null)
    val text = pluralStringResource(R.plurals.sdui_ink_note_count, count, count)
    val label = stringResource(R.string.sdui_ink_clear_action)
    val action =
        if (canClear) {
            CustomAccessibilityAction(label) {
                host.clearNotes(blockId)
                true
            }
        } else {
            null
        }
    return NotesA11y(count, text, action)
}

/**
 * Notes announcement for a top-level block that is not highlightable text (exercises, code, diagrams...): the text
 * blocks add the same to their own node together with the highlights.
 */
@Composable
fun Modifier.inkNotesA11y(blockId: String): Modifier {
    val notes = rememberNotesA11y(LocalHighlightHost.current, blockId)
    if (notes.count == 0) return this
    return semantics {
        stateDescription = notes.stateText.orEmpty()
        notes.clearAction?.let { customActions = listOf(it) }
    }
}
