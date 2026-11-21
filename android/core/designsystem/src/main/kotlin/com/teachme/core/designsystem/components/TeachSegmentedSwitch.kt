package com.teachme.core.designsystem.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

private val SWITCH_MAX_WIDTH = 360.dp

/**
 * Two-way (or more) switch of views (progress.md section 2): equal segments of 48 dp, the selected one with `accentSoft`
 * fill, a 2 dp `accent` border and a check icon (never color alone). Segments are radio buttons of one selectable group.
 * [descriptions] gives each segment a spoken name when the visible label is not enough ("Velocità 1,25 volte"). With
 * [stacked] the segments become one column of rows (lesson-audio.md section 4: four labels do not fit at large fonts).
 */
@Composable
fun TeachSegmentedSwitch(
    labels: List<String>,
    selectedIndex: Int,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
    descriptions: List<String>? = null,
    stacked: Boolean = false,
) {
    val shape = TeachTheme.shapes.md
    val frame =
        modifier
            .widthIn(max = SWITCH_MAX_WIDTH)
            .fillMaxWidth()
            .border(TeachTheme.stroke.control, TeachTheme.colors.border, shape)
            .selectableGroup()
    val segment: @Composable (Modifier, Int) -> Unit = { m, index ->
        Segment(labels[index], descriptions?.getOrNull(index), index == selectedIndex, m) { onSelect(index) }
    }
    if (stacked) {
        Column(frame) { labels.indices.forEach { segment(Modifier.fillMaxWidth(), it) } }
    } else {
        Row(frame.height(TeachTheme.layout.minTouchTarget), horizontalArrangement = Arrangement.Center) {
            labels.indices.forEach { segment(Modifier.weight(1f), it) }
        }
    }
}

@Composable
private fun Segment(
    label: String,
    description: String?,
    selected: Boolean,
    modifier: Modifier,
    onClick: () -> Unit,
) {
    val colors = TeachTheme.colors
    val shape = TeachTheme.shapes.md
    val state = rememberInteractionState()
    Row(
        modifier
            .heightIn(min = TeachTheme.layout.minTouchTarget)
            .focusRing(state.focused, shape)
            .background(
                when {
                    selected -> colors.accentSoft
                    state.pressed -> colors.surfacePressed
                    else -> colors.surface
                },
                shape,
            ).then(
                if (selected) Modifier.border(TeachTheme.stroke.selected, colors.accent, shape) else Modifier,
            ).selectable(
                selected = selected,
                interactionSource = state.source,
                indication = null,
                role = Role.RadioButton,
                onClick = onClick,
            ).then(
                if (description != null) Modifier.semantics { contentDescription = description } else Modifier,
            ).padding(horizontal = TeachTheme.spacing.sm),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (selected) {
            TeachIcon(TeachIcons.Check, null, tint = colors.ink, size = TeachTheme.layout.iconSizeSmall)
        }
        TeachText(
            label,
            style = TeachTheme.typography.ui.label,
            color = if (selected) colors.ink else colors.inkSoft,
        )
    }
}

@Composable
private fun SwitchSample() {
    Row(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        TeachSegmentedSwitch(listOf("Registro", "Glossario"), 0, {})
    }
}

@Preview(name = "Segmented light", showBackground = true)
@Composable
private fun SegmentedLightPreview() = TeachTheme(darkTheme = false) { SwitchSample() }

@Preview(name = "Segmented dark", showBackground = true)
@Composable
private fun SegmentedDarkPreview() = TeachTheme(darkTheme = true) { SwitchSample() }
