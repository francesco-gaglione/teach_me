package com.teachme.core.designsystem.components

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme

private const val SWEEP_DEGREES = 270f
private const val FULL_TURN = 360f
private const val TURN_MS = 900
private val SPINNER_STROKE = 2.5.dp

/**
 * Indeterminate ring (base-components section 9). Always shown next to a text that says what is
 * happening. With "reduce animations" it is static.
 */
@Composable
fun TeachSpinner(
    modifier: Modifier = Modifier,
    color: Color = TeachTheme.colors.accent,
    size: Dp = TeachTheme.layout.iconSize,
) {
    val reduced = TeachTheme.reducedMotion
    val angle =
        if (reduced) {
            0f
        } else {
            rememberInfiniteTransition(label = "spinner")
                .animateFloat(
                    initialValue = 0f,
                    targetValue = FULL_TURN,
                    animationSpec = infiniteRepeatable(tween(TURN_MS, easing = LinearEasing), RepeatMode.Restart),
                    label = "spinner-angle",
                ).value
        }
    Canvas(modifier.size(size)) {
        val strokePx = SPINNER_STROKE.toPx()
        drawArc(
            color = color,
            startAngle = angle,
            sweepAngle = SWEEP_DEGREES,
            useCenter = false,
            topLeft = Offset(strokePx / 2, strokePx / 2),
            size = Size(this.size.width - strokePx, this.size.height - strokePx),
            style = Stroke(strokePx, cap = StrokeCap.Round),
        )
    }
}
