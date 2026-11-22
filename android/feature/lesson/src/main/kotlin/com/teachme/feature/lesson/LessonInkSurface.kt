package com.teachme.feature.lesson

import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity

/**
 * The handwriting surface of the screen: settings in, geometry of the blocks out of the list state, edits to the view
 * model. [left] and [width] are the column in the pixels of the box that holds the list.
 */
@Composable
internal fun rememberInkSurface(
    ready: LessonUiState.Ready,
    vm: LessonViewModel,
    listState: LazyListState,
    items: List<LessonItem>,
    audio: LessonAudioUiState,
    left: Float,
    width: Float,
    areaRight: Float,
    onLimit: () -> Unit,
): InkSurface {
    val density = LocalDensity.current
    val pads = items.indices.map { with(density) { blockTopPadding(items, it, audio).toPx() } }
    val itemsNow = rememberUpdatedState(items)
    val padsNow = rememberUpdatedState(pads)
    val leftNow = rememberUpdatedState(left)
    val widthNow = rememberUpdatedState(width)
    val rowSizes = remember(listState) { InkRowSizes() }
    val layout =
        remember(listState) {
            derivedStateOf {
                computeInkLayout(
                    listState.layoutInfo,
                    itemsNow.value,
                    padsNow.value,
                    leftNow.value,
                    widthNow.value,
                    rowSizes,
                )
            }
        }
    val surface = remember(vm) { InkSurface(write = vm::writeStroke, erase = vm::eraseStrokes) }
    surface.layout = { layout.value }
    val strokes = ready.ink.set.strokes
    SideEffect {
        surface.penWrites = ready.penTool == PenTool.PEN
        surface.eraserMode = ready.eraserMode
        surface.color = ready.inkColor
        surface.width = ready.inkWidth
        surface.strokes = strokes
        surface.areaRight = areaRight
        surface.onLimit = onLimit
    }
    return surface
}

/** The keyboard shortcuts of the notes (handwriting.md section 6.2). */
internal enum class InkShortcut { UNDO, REDO }

/** Ctrl+Z undoes, Ctrl+Shift+Z and Ctrl+Y redo; anything else, and Z or Y alone, is no shortcut. */
internal fun inkShortcut(e: androidx.compose.ui.input.key.KeyEvent): InkShortcut? {
    if (e.type != KeyEventType.KeyDown || !e.isCtrlPressed || e.isAltPressed) return null
    return when {
        e.key == Key.Z && !e.isShiftPressed -> InkShortcut.UNDO
        e.key == Key.Z || e.key == Key.Y -> InkShortcut.REDO
        else -> null
    }
}
