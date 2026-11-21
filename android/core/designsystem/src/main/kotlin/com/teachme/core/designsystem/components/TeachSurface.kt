package com.teachme.core.designsystem.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachTheme

/** Flat container: fill, optional border, shape, and a content color for the children. No elevation. */
@Composable
fun TeachSurface(
    modifier: Modifier = Modifier,
    color: Color = TeachTheme.colors.surface,
    contentColor: Color = TeachTheme.colors.ink,
    shape: Shape = TeachTheme.shapes.md,
    border: BorderStroke? = null,
    content: @Composable () -> Unit,
) {
    Box(
        modifier =
            modifier
                .background(color, shape)
                .then(if (border != null) Modifier.border(border, shape) else Modifier)
                .clip(shape),
    ) {
        ProvideContent(contentColor, content = content)
    }
}

enum class CardVariant { Static, Interactive, Emphasis, Sunken }

/**
 * Card (base-components section 3). [Interactive][CardVariant.Interactive] makes the whole card one
 * button target ([onClick] required); do not nest buttons inside it.
 */
@Composable
fun TeachCard(
    modifier: Modifier = Modifier,
    variant: CardVariant = CardVariant.Static,
    onClick: (() -> Unit)? = null,
    content: @Composable () -> Unit,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    val interactive = variant == CardVariant.Interactive && onClick != null
    val base =
        when (variant) {
            CardVariant.Static, CardVariant.Interactive -> colors.surface
            CardVariant.Emphasis -> colors.accentSoft
            CardVariant.Sunken -> colors.surfaceSunken
        }
    val fill by animateColorAsState(
        targetValue = if (interactive && state.pressed) colors.surfacePressed else base,
        animationSpec = teachTween(TeachTheme.motion.press),
        label = "card-fill",
    )
    val border =
        when (variant) {
            CardVariant.Static, CardVariant.Interactive -> BorderStroke(TeachTheme.stroke.hairline, colors.line)
            else -> null
        }
    val clickable =
        if (interactive) {
            Modifier.clickable(
                interactionSource = state.source,
                indication = null,
                role = Role.Button,
                onClick = onClick,
            )
        } else {
            Modifier
        }
    TeachSurface(
        modifier =
            modifier
                .focusRing(interactive && state.focused, shape)
                .then(clickable),
        color = fill,
        border = border,
        shape = shape,
    ) {
        Box(Modifier.padding(TeachTheme.spacing.md)) { content() }
    }
}

@Composable
private fun CardsSample() {
    Column(
        modifier =
            Modifier
                .background(TeachTheme.colors.bg)
                .padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        TeachCard { TeachText("Card statica", style = TeachTheme.typography.ui.title3) }
        TeachCard(variant = CardVariant.Interactive, onClick = {}) { TeachText("Card interattiva") }
        TeachCard(variant = CardVariant.Emphasis) { TeachText("Card in evidenza") }
        TeachCard(variant = CardVariant.Sunken) { TeachText("Card incassata") }
    }
}

@Preview(name = "Cards light", showBackground = true)
@Composable
private fun CardsLightPreview() = TeachTheme(darkTheme = false) { CardsSample() }

@Preview(name = "Cards dark", showBackground = true)
@Composable
private fun CardsDarkPreview() = TeachTheme(darkTheme = true) { CardsSample() }
