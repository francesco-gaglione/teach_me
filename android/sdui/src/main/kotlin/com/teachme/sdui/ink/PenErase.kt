package com.teachme.sdui.ink

import androidx.compose.ui.input.pointer.PointerType

/**
 * The one rule of "the pen erases now", shared by the notes and the highlighter (handwriting.md section 3.4): the
 * physical eraser end, the stylus side button, or the stylus tip while the "Gomma" switch is on. A finger or a mouse
 * never erases with this rule.
 */
fun penErases(
    type: PointerType,
    sideButton: Boolean,
    eraserSwitch: Boolean,
): Boolean =
    type == PointerType.Eraser ||
        sideButton ||
        (type == PointerType.Stylus && eraserSwitch)
