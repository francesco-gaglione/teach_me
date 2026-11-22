package com.teachme.core.designsystem.components

import androidx.annotation.DrawableRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.R
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

enum class ButtonVariant { Primary, Secondary, Text, Destructive }

enum class ButtonSize { Normal, Large }

private val MIN_BUTTON_WIDTH = 64.dp

/**
 * Button (base-components section 1). At most one Primary per screen. A disabled button must have its
 * reason explained next to it by the caller. [loading] shows a spinner, reads as busy and ignores taps.
 * [contentDescription] gives screen readers a name with context ("Conferma la mission per Rust") that
 * still starts with the visible label (WCAG 2.5.3).
 */
@Composable
fun TeachButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    variant: ButtonVariant = ButtonVariant.Primary,
    size: ButtonSize = ButtonSize.Normal,
    enabled: Boolean = true,
    loading: Boolean = false,
    @DrawableRes leadingIcon: Int? = null,
    contentDescription: String? = null,
    /** Read by screen readers while the button is disabled (not loading), e.g. "Disponibile oggi alle 01:00". */
    disabledStateDescription: String? = null,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    val inactive = !enabled || loading
    val busyDescription = stringResource(R.string.teach_state_busy)

    val restFill =
        when {
            inactive && variant != ButtonVariant.Text -> colors.disabledFill
            inactive -> Color.Transparent
            variant == ButtonVariant.Primary -> colors.accent
            variant == ButtonVariant.Text -> Color.Transparent
            else -> colors.surface
        }
    val pressedFill =
        when (variant) {
            ButtonVariant.Primary -> colors.accentPressed
            else -> colors.surfacePressed
        }
    val contentColor =
        when {
            inactive -> colors.disabledInk
            variant == ButtonVariant.Primary -> colors.onAccent
            variant == ButtonVariant.Text -> colors.accent
            variant == ButtonVariant.Destructive -> colors.danger
            else -> colors.ink
        }
    val borderColor =
        when {
            inactive -> null
            variant == ButtonVariant.Secondary -> colors.border
            variant == ButtonVariant.Destructive -> colors.danger
            else -> null
        }
    val fill by animateColorAsState(
        targetValue = if (state.pressed && !inactive) pressedFill else restFill,
        animationSpec = teachTween(TeachTheme.motion.press),
        label = "button-fill",
    )
    val height = if (size == ButtonSize.Large) TeachTheme.layout.controlHeightLarge else TeachTheme.layout.controlHeight
    val horizontal = if (variant == ButtonVariant.Text) TeachTheme.spacing.sm else 20.dp

    Row(
        modifier =
            modifier
                .focusRing(state.focused, shape)
                .clip(shape)
                .background(fill, shape)
                .then(
                    if (borderColor != null) {
                        Modifier.border(BorderStroke(TeachTheme.stroke.control, borderColor), shape)
                    } else {
                        Modifier
                    },
                ).semantics {
                    if (!enabled) disabled()
                    if (loading) {
                        stateDescription = busyDescription
                    } else if (!enabled && disabledStateDescription != null) {
                        stateDescription = disabledStateDescription
                    }
                    if (contentDescription != null) this.contentDescription = contentDescription
                }.clickable(
                    interactionSource = state.source,
                    indication = null,
                    enabled = !inactive,
                    role = Role.Button,
                    onClick = onClick,
                ).heightIn(min = height)
                .widthIn(min = MIN_BUTTON_WIDTH)
                .padding(horizontal = horizontal, vertical = TeachTheme.spacing.xs),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (loading) {
            TeachSpinner(color = contentColor, size = TeachTheme.layout.iconSizeSmall)
        } else if (leadingIcon != null) {
            TeachIcon(
                leadingIcon,
                contentDescription = null,
                tint = contentColor,
                size = TeachTheme.layout.iconSizeSmall,
            )
        }
        TeachText(
            text = text,
            modifier = if (contentDescription != null) Modifier.clearAndSetSemantics { } else Modifier,
            style = TeachTheme.typography.ui.label,
            color = contentColor,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun ButtonsSample() {
    Column(
        modifier =
            Modifier
                .background(TeachTheme.colors.bg)
                .padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        TeachButton("Primario", {}, size = ButtonSize.Large, leadingIcon = TeachIcons.Plus)
        TeachButton("Secondario", {}, variant = ButtonVariant.Secondary)
        TeachButton("Testuale", {}, variant = ButtonVariant.Text)
        TeachButton("Elimina", {}, variant = ButtonVariant.Destructive)
        TeachButton("Disabilitato", {}, enabled = false)
        TeachButton("Provo…", {}, variant = ButtonVariant.Secondary, loading = true)
    }
}

@Preview(name = "Buttons light", showBackground = true)
@Composable
private fun ButtonsLightPreview() = TeachTheme(darkTheme = false) { ButtonsSample() }

@Preview(name = "Buttons dark", showBackground = true)
@Composable
private fun ButtonsDarkPreview() = TeachTheme(darkTheme = true) { ButtonsSample() }

@Preview(name = "Buttons font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun ButtonsLargeFontPreview() = TeachTheme(darkTheme = false) { ButtonsSample() }
