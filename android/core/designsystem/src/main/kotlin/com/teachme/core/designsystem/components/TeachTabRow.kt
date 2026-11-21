package com.teachme.core.designsystem.components

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme

private val TAB_MIN_WIDTH = 72.dp
private val INDICATOR_HEIGHT = 3.dp

/**
 * Tab row (base-components section 7): scrollable, left aligned, 48 dp tabs, the selected one in `ink`
 * with a 3 dp accent underline (text and line, not color alone). Arrow keys move the focus between
 * tabs; Enter or Space select. Switching tabs does not animate the content. [startPadding] aligns the
 * first label with the screen margin while the bottom border spans the whole width.
 */
@Composable
fun TeachTabRow(
    labels: List<String>,
    selectedIndex: Int,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
    startPadding: Dp = 0.dp,
) {
    Column(modifier.fillMaxWidth()) {
        Row(
            Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .selectableGroup()
                .padding(start = startPadding),
        ) {
            labels.forEachIndexed { index, label ->
                TabItem(label, selected = index == selectedIndex, onClick = { onSelect(index) })
            }
        }
        Box(Modifier.fillMaxWidth().height(TeachTheme.stroke.hairline).background(TeachTheme.colors.line))
    }
}

@Composable
private fun TabItem(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.sm
    Box(
        Modifier
            .sizeIn(minWidth = TAB_MIN_WIDTH, minHeight = TeachTheme.layout.minTouchTarget)
            .focusRing(state.focused, shape)
            .background(if (state.pressed) TeachTheme.colors.surfacePressed else Color.Transparent, shape)
            .selectable(
                selected = selected,
                interactionSource = state.source,
                indication = null,
                role = Role.Tab,
                onClick = onClick,
            ).padding(horizontal = TeachTheme.spacing.md),
        contentAlignment = Alignment.Center,
    ) {
        Column(Modifier.width(IntrinsicSize.Max), horizontalAlignment = Alignment.CenterHorizontally) {
            Box(Modifier.height(TeachTheme.spacing.xs))
            TeachText(
                label,
                Modifier.padding(vertical = TeachTheme.spacing.xxs),
                style = TeachTheme.typography.ui.label,
                color = if (selected) TeachTheme.colors.ink else TeachTheme.colors.inkSoft,
            )
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(INDICATOR_HEIGHT)
                    .background(if (selected) TeachTheme.colors.accent else Color.Transparent),
            )
        }
    }
}

@Composable
private fun TabRowSample() {
    Column(Modifier.background(TeachTheme.colors.bg)) {
        TeachTabRow(listOf("Chat", "Mission"), 0, {}, startPadding = TeachTheme.spacing.md)
        TeachTabRow(listOf("Chat", "Mission"), 1, {}, startPadding = TeachTheme.spacing.md)
    }
}

@Preview(name = "Tabs light", showBackground = true, widthDp = 360)
@Composable
private fun TabRowLightPreview() = TeachTheme(darkTheme = false) { TabRowSample() }

@Preview(name = "Tabs dark", showBackground = true, widthDp = 360)
@Composable
private fun TabRowDarkPreview() = TeachTheme(darkTheme = true) { TabRowSample() }

@Preview(name = "Tabs font 2.0", showBackground = true, widthDp = 360, fontScale = 2f)
@Composable
private fun TabRowLargePreview() = TeachTheme(darkTheme = false) { TabRowSample() }
