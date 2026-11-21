package com.teachme.core.designsystem.components

import androidx.annotation.DrawableRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.LocalIsDark
import com.teachme.core.designsystem.R
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

/**
 * Primary square icon button, 48 x 48 dp (chat "Invia"). [enabled] false paints the disabled fill and
 * the caller explains why; [loading] shows a spinner and ignores taps. [disabledDescription] replaces the
 * description while disabled (e.g. "Invia, scrivi prima un messaggio").
 */
@Composable
fun TeachFilledIconButton(
    @DrawableRes icon: Int,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    loading: Boolean = false,
    disabledDescription: String = contentDescription,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    val inactive = !enabled || loading
    val busy = stringResource(R.string.teach_state_busy)
    val fill by animateColorAsState(
        targetValue =
            when {
                inactive -> colors.disabledFill
                state.pressed -> colors.accentPressed
                else -> colors.accent
            },
        animationSpec = teachTween(TeachTheme.motion.press),
        label = "filled-icon-button",
    )
    val minTarget = TeachTheme.layout.minTouchTarget
    Box(
        modifier =
            modifier
                .sizeIn(minWidth = minTarget, minHeight = minTarget)
                .focusRing(state.focused, shape)
                .background(fill, shape)
                .semantics {
                    this.contentDescription = if (enabled) contentDescription else disabledDescription
                    if (!enabled) disabled()
                    if (loading) stateDescription = busy
                }.clickable(
                    interactionSource = state.source,
                    indication = null,
                    enabled = !inactive,
                    role = Role.Button,
                    onClick = onClick,
                ),
        contentAlignment = Alignment.Center,
    ) {
        if (loading) {
            TeachSpinner(color = colors.disabledInk, size = TeachTheme.layout.iconSizeSmall)
        } else {
            TeachIcon(icon, contentDescription = null, tint = if (enabled) colors.onAccent else colors.disabledInk)
        }
    }
}

/** Floating pill (chat "Nuovi messaggi"): raised surface, border, shadow, icon + word. Announced when it appears. */
@Composable
fun TeachPill(
    text: String,
    @DrawableRes icon: Int,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.full
    val shadow = TeachTheme.elevation.raised
    val alpha = if (LocalIsDark.current) shadow.alphaDark else shadow.alphaLight
    Row(
        modifier =
            modifier
                .shadow(shadow.blur, shape, ambientColor = Color.Black.copy(alpha), spotColor = Color.Black.copy(alpha))
                .focusRing(state.focused, shape)
                .background(if (state.pressed) colors.surfacePressed else colors.surfaceRaised, shape)
                .border(TeachTheme.stroke.control, colors.border, shape)
                .semantics { liveRegion = LiveRegionMode.Polite }
                .clickable(
                    interactionSource = state.source,
                    indication = null,
                    role = Role.Button,
                    onClick = onClick,
                ).heightIn(min = TeachTheme.layout.minTouchTarget)
                .padding(horizontal = TeachTheme.spacing.md),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TeachIcon(icon, contentDescription = null, tint = colors.ink, size = TeachTheme.layout.iconSizeSmall)
        TeachText(text, style = TeachTheme.typography.ui.label, color = colors.ink)
    }
}

@Composable
private fun ChatControlsSample() {
    Row(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TeachFilledIconButton(TeachIcons.Send, "Invia", {})
        TeachFilledIconButton(TeachIcons.Send, "Invia", {}, enabled = false)
        TeachFilledIconButton(TeachIcons.Send, "Invia", {}, loading = true)
        TeachPill("Nuovi messaggi", TeachIcons.ArrowDown, {})
    }
}

@Preview(name = "Chat controls light", showBackground = true)
@Composable
private fun ChatControlsLightPreview() = TeachTheme(darkTheme = false) { ChatControlsSample() }

@Preview(name = "Chat controls dark", showBackground = true)
@Composable
private fun ChatControlsDarkPreview() = TeachTheme(darkTheme = true) { ChatControlsSample() }
