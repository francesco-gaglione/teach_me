package com.teachme.core.designsystem.components

import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachColors
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

/** Meaning of a [TeachStatusPanel]: always conveyed by icon + word + color (P8). */
enum class StatusVariant(
    @DrawableRes val defaultIcon: Int?,
) {
    Correct(TeachIcons.CheckCircle),
    Incorrect(TeachIcons.XCircle),
    Partial(TeachIcons.MinusCircle),
    Info(TeachIcons.Info),
    Tip(TeachIcons.Lightbulb),
    Warning(TeachIcons.AlertTriangle),
    Danger(TeachIcons.AlertCircle),

    /** Spinner instead of an icon, neutral fill. */
    Progress(null),
}

private data class StatusColors(
    val accent: Color,
    val soft: Color,
)

private fun StatusVariant.colors(c: TeachColors) =
    when (this) {
        StatusVariant.Correct -> StatusColors(c.correct, c.correctSoft)
        StatusVariant.Incorrect -> StatusColors(c.incorrect, c.incorrectSoft)
        StatusVariant.Partial -> StatusColors(c.partial, c.partialSoft)
        StatusVariant.Info -> StatusColors(c.info, c.infoSoft)
        StatusVariant.Tip -> StatusColors(c.tip, c.tipSoft)
        StatusVariant.Warning -> StatusColors(c.warning, c.warningSoft)
        StatusVariant.Danger -> StatusColors(c.danger, c.dangerSoft)
        StatusVariant.Progress -> StatusColors(c.accent, c.surfaceSunken)
    }

/**
 * Status / outcome panel (base-components section 5): icon + title in the accent color, body in `ink`
 * on the soft fill. Appears with the `feedbackAppear` motion. When [announce] is true it is a polite
 * live region, so screen readers read "title, body" whenever it appears or changes.
 *
 * @param icon overrides the variant's icon (e.g. key-round for an invalid token).
 * @param trailing optional content at the end of the row, centered vertically (e.g. an action on wide windows).
 * @param extra optional extra content under the body (e.g. a details button); kept last so it is the trailing lambda.
 */
@Composable
fun TeachStatusPanel(
    variant: StatusVariant,
    title: String,
    modifier: Modifier = Modifier,
    body: String? = null,
    @DrawableRes icon: Int? = null,
    announce: Boolean = true,
    trailing: (@Composable () -> Unit)? = null,
    extra: @Composable ColumnScope.() -> Unit = {},
) {
    val colors = variant.colors(TeachTheme.colors)
    val spec = TeachTheme.motion.feedbackAppear
    val state = remember { MutableTransitionState(false).apply { targetState = true } }
    val enter =
        if (TeachTheme.reducedMotion) {
            EnterTransition.None
        } else {
            fadeIn(teachTween(spec)) + expandVertically(teachTween(spec))
        }
    AnimatedVisibility(visibleState = state, enter = enter, modifier = modifier) {
        Row(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .background(colors.soft, TeachTheme.shapes.md)
                    .padding(TeachTheme.spacing.md)
                    .semantics(mergeDescendants = true) {
                        if (announce) liveRegion = LiveRegionMode.Polite
                    },
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            val resolvedIcon = icon ?: variant.defaultIcon
            if (resolvedIcon == null) {
                TeachSpinner(color = colors.accent)
            } else {
                TeachIcon(resolvedIcon, contentDescription = null, tint = colors.accent)
            }
            Column(
                Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                val titleColor = if (variant == StatusVariant.Progress) TeachTheme.colors.ink else colors.accent
                TeachText(title, style = TeachTheme.typography.ui.labelL, color = titleColor)
                if (body != null) {
                    TeachText(body, style = TeachTheme.typography.ui.bodyL, color = TeachTheme.colors.ink)
                }
                extra()
            }
            if (trailing != null) {
                Box(Modifier.align(Alignment.CenterVertically)) { trailing() }
            }
        }
    }
}

@Composable
private fun StatusSample() {
    Column(
        modifier =
            Modifier
                .background(TeachTheme.colors.bg)
                .padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        TeachStatusPanel(StatusVariant.Progress, "Mi collego…", body = "Contatto teach.local")
        TeachStatusPanel(StatusVariant.Correct, "Connesso", body = "Versione 0.1.0 · catalogo v1")
        TeachStatusPanel(StatusVariant.Incorrect, "Non proprio", body = "Ecco perché.")
        TeachStatusPanel(
            StatusVariant.Warning,
            "Token non valido",
            body = "Il backend non riconosce questo token.",
            icon = TeachIcons.KeyRound,
        )
        TeachStatusPanel(StatusVariant.Danger, "Errore imprevisto", body = "Il backend ha risposto con il codice 500.")
    }
}

@Preview(name = "Status light", showBackground = true)
@Composable
private fun StatusLightPreview() = TeachTheme(darkTheme = false) { StatusSample() }

@Preview(name = "Status dark", showBackground = true)
@Composable
private fun StatusDarkPreview() = TeachTheme(darkTheme = true) { StatusSample() }

@Preview(name = "Status font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun StatusLargeFontPreview() = TeachTheme(darkTheme = false) { StatusSample() }
