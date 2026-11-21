package com.teachme.core.designsystem.components

import androidx.annotation.DrawableRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import com.teachme.core.designsystem.LocalContentColor
import com.teachme.core.designsystem.TeachTheme

/** Icon tinted with [tint] (default: the content color). Decorative icons pass a null description. */
@Composable
fun TeachIcon(
    @DrawableRes icon: Int,
    contentDescription: String?,
    modifier: Modifier = Modifier,
    tint: Color = Color.Unspecified,
    size: Dp = TeachTheme.layout.iconSize,
) {
    val resolved = if (tint != Color.Unspecified) tint else LocalContentColor.current
    Image(
        painter = painterResource(icon),
        contentDescription = contentDescription,
        modifier = modifier.size(size),
        colorFilter = ColorFilter.tint(resolved),
    )
}

/**
 * Icon button: 48 x 48 dp target, shape md, pressed = fill change. The description is mandatory.
 * With [toggled] non-null it behaves as a switch (e.g. show/hide token) and reports its state to
 * accessibility services; [onClick] is then called on every change.
 */
@Composable
fun TeachIconButton(
    @DrawableRes icon: Int,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    tint: Color = TeachTheme.colors.inkSoft,
    toggled: Boolean? = null,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    val fill by animateColorAsState(
        targetValue = if (state.pressed && enabled) colors.surfacePressed else Color.Transparent,
        animationSpec = teachTween(TeachTheme.motion.press),
        label = "icon-button-fill",
    )
    val minTarget = TeachTheme.layout.minTouchTarget
    Box(
        modifier =
            modifier
                .sizeIn(minWidth = minTarget, minHeight = minTarget)
                .focusRing(state.focused, shape)
                .background(fill, shape)
                .then(
                    if (toggled != null) {
                        Modifier.toggleable(
                            value = toggled,
                            interactionSource = state.source,
                            indication = null,
                            enabled = enabled,
                            role = Role.Switch,
                            onValueChange = { onClick() },
                        )
                    } else {
                        Modifier.clickable(
                            interactionSource = state.source,
                            indication = null,
                            enabled = enabled,
                            role = Role.Button,
                            onClick = onClick,
                        )
                    },
                ),
        contentAlignment = Alignment.Center,
    ) {
        TeachIcon(icon, contentDescription, tint = if (enabled) tint else colors.disabledInk)
    }
}
