package com.teachme.core.designsystem.components

import androidx.compose.animation.core.TweenSpec
import androidx.compose.animation.core.tween
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import com.teachme.core.designsystem.MotionSpec
import com.teachme.core.designsystem.TeachTheme

/** Press/focus state of one interactive component, driven by a single [MutableInteractionSource]. */
class InteractionState(
    val source: MutableInteractionSource,
    val pressed: Boolean,
    val focused: Boolean,
)

@Composable
fun rememberInteractionState(): InteractionState {
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val focused by source.collectIsFocusedAsState()
    return InteractionState(source, pressed, focused)
}

/** Tween for [spec], instantaneous when the user turned animations off. */
@Composable
fun <T> teachTween(spec: MotionSpec): TweenSpec<T> {
    val reduced = TeachTheme.reducedMotion
    return tween(durationMillis = if (reduced) 0 else spec.durationMs, easing = spec.easing)
}

/**
 * Focus ring (tokens: stroke.focus, stroke.focusOffset): drawn outside the component bounds with the
 * same shape. Only drawn while [visible], i.e. when the component is focused (keyboard/switch/D-pad).
 */
@Composable
fun Modifier.focusRing(
    visible: Boolean,
    shape: Shape,
    color: Color = TeachTheme.colors.focus,
): Modifier {
    if (!visible) return this
    val strokeWidth = TeachTheme.stroke.focus
    val offset = TeachTheme.stroke.focusOffset
    return drawBehind {
        val inflate = (offset + strokeWidth / 2).toPx()
        val ringSize = Size(size.width + 2 * inflate, size.height + 2 * inflate)
        val outline = shape.createOutline(ringSize, layoutDirection, this)
        translate(-inflate, -inflate) {
            drawOutline(outline, color, style = Stroke(strokeWidth.toPx()))
        }
    }
}
