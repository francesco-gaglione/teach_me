package com.teachme.core.designsystem.components

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

/** Colors of a [TeachStatusChip] (base-components 10.3). */
enum class ChipVariant { Neutral, Info, Accent, Warning }

/**
 * Non-interactive status chip: icon + word, never color alone. It is not a separate accessibility
 * target: rows that contain it describe the state in their own `contentDescription`.
 */
@Composable
fun TeachStatusChip(
    text: String,
    @DrawableRes icon: Int,
    modifier: Modifier = Modifier,
    variant: ChipVariant = ChipVariant.Neutral,
    mergeIntoParent: Boolean = true,
) {
    val colors = TeachTheme.colors
    val (fill, content) =
        when (variant) {
            ChipVariant.Neutral -> colors.surfaceSunken to colors.inkSoft
            ChipVariant.Info -> colors.infoSoft to colors.info
            ChipVariant.Accent -> colors.accentSoft to colors.accent
            ChipVariant.Warning -> colors.warningSoft to colors.warning
        }
    Row(
        modifier =
            modifier
                .background(fill, TeachTheme.shapes.full)
                .heightIn(min = 28.dp)
                .padding(horizontal = TeachTheme.spacing.xs, vertical = TeachTheme.spacing.xxs)
                .then(if (mergeIntoParent) Modifier.clearAndSetSemantics { } else Modifier),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
    ) {
        TeachIcon(icon, contentDescription = null, tint = content, size = TeachTheme.layout.iconSizeSmall)
        TeachText(text, style = TeachTheme.typography.ui.labelS, color = content)
    }
}

@Composable
private fun ChipSample() {
    Column(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        TeachStatusChip("Definiamo la mission", TeachIcons.MessageCircle)
        TeachStatusChip("Conferma la mission", TeachIcons.Flag, variant = ChipVariant.Info)
        TeachStatusChip("Pronto per le lezioni", TeachIcons.CheckCircle, variant = ChipVariant.Accent)
    }
}

@Preview(name = "Chips light", showBackground = true)
@Composable
private fun ChipLightPreview() = TeachTheme(darkTheme = false) { ChipSample() }

@Preview(name = "Chips dark", showBackground = true)
@Composable
private fun ChipDarkPreview() = TeachTheme(darkTheme = true) { ChipSample() }

@Preview(name = "Chips font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun ChipLargePreview() = TeachTheme(darkTheme = false) { ChipSample() }
