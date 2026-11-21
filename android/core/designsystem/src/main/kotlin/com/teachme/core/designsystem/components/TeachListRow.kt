package com.teachme.core.designsystem.components

import androidx.annotation.DrawableRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

/**
 * List row (base-components section 4). The title area is one `Role.Button` target described by
 * [contentDescription]; [trailing] (e.g. a "more" icon button) is a separate target. [onLongClick] and
 * [customActions] expose the same secondary actions to touch and TalkBack. Put rows inside a
 * [TeachListCard]; the divider is drawn by the container.
 */
@Composable
fun TeachListRow(
    title: String,
    onClick: () -> Unit,
    contentDescription: String,
    modifier: Modifier = Modifier,
    @DrawableRes leadingIcon: Int? = null,
    subtitle: String? = null,
    supporting: (@Composable () -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
    onLongClick: (() -> Unit)? = null,
    customActions: List<CustomAccessibilityAction> = emptyList(),
    selected: Boolean = false,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val fill by animateColorAsState(
        targetValue =
            when {
                state.pressed -> colors.surfacePressed
                selected -> colors.accentSoft
                else -> androidx.compose.ui.graphics.Color.Transparent
            },
        animationSpec = teachTween(TeachTheme.motion.press),
        label = "row-fill",
    )
    Row(
        modifier =
            modifier
                .fillMaxWidth()
                .background(fill)
                .then(
                    if (selected) {
                        // Not color alone: a 4 dp bar on the leading edge plus a heavier title (home.md, Expanded).
                        Modifier.drawBehind { drawRect(colors.accent, size = Size(4.dp.toPx(), size.height)) }
                    } else {
                        Modifier
                    },
                ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier =
                Modifier
                    .weight(1f)
                    .heightIn(min = if (subtitle != null || supporting != null) 72.dp else 56.dp)
                    .focusRing(state.focused, TeachTheme.shapes.md)
                    .combinedClickable(
                        interactionSource = state.source,
                        indication = null,
                        role = Role.Button,
                        onClick = onClick,
                        onLongClick = onLongClick,
                    ).semantics(mergeDescendants = true) {
                        this.contentDescription = contentDescription
                        this.selected = selected
                        if (customActions.isNotEmpty()) this.customActions = customActions
                    }.padding(
                        start = TeachTheme.spacing.md,
                        top = TeachTheme.spacing.xs,
                        bottom = TeachTheme.spacing.xs,
                    ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
        ) {
            if (leadingIcon != null) {
                TeachIcon(leadingIcon, contentDescription = null, tint = colors.inkSoft)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                TeachText(
                    title,
                    style =
                        if (selected) {
                            TeachTheme.typography.ui.title3
                                .copy(fontWeight = FontWeight.SemiBold)
                        } else {
                            TeachTheme.typography.ui.title3
                        },
                )
                if (subtitle != null) {
                    TeachText(subtitle, style = TeachTheme.typography.ui.body, color = colors.inkSoft)
                }
                supporting?.invoke()
            }
        }
        if (trailing != null) {
            Box(Modifier.padding(horizontal = TeachTheme.spacing.xxs)) { trailing() }
        } else {
            Box(Modifier.padding(end = TeachTheme.spacing.md))
        }
    }
}

/** Static card holding [TeachListRow]s separated by hairlines (the radius belongs to the container). */
@Composable
fun TeachListCard(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    TeachCard(modifier = modifier.fillMaxWidth()) { Box { content() } }
}

/** Hairline separator between rows, inset 16 dp. */
@Composable
fun TeachRowDivider(modifier: Modifier = Modifier) {
    Box(
        modifier
            .fillMaxWidth()
            .padding(start = TeachTheme.spacing.md)
            .heightIn(min = TeachTheme.stroke.hairline)
            .background(TeachTheme.colors.line),
    )
}

@Composable
private fun RowsSample() {
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        TeachCard {
            Column {
                TeachListRow(
                    title = "Lavoro",
                    onClick = {},
                    contentDescription = "Lavoro, 3 argomenti",
                    leadingIcon = TeachIcons.Folder,
                    subtitle = "3 argomenti",
                    trailing = { TeachIconButton(TeachIcons.MoreVertical, "Altre azioni per Lavoro", {}) },
                )
                TeachRowDivider()
                TeachListRow(
                    title = "Rust",
                    onClick = {},
                    contentDescription = "Rust, stato: definiamo la mission",
                    supporting = { TeachStatusChip("Definiamo la mission", TeachIcons.MessageCircle) },
                    trailing = { TeachIconButton(TeachIcons.MoreVertical, "Altre azioni per Rust", {}) },
                )
            }
        }
    }
}

@Preview(name = "Rows light", showBackground = true)
@Composable
private fun RowsLightPreview() = TeachTheme(darkTheme = false) { RowsSample() }

@Preview(name = "Rows dark", showBackground = true)
@Composable
private fun RowsDarkPreview() = TeachTheme(darkTheme = true) { RowsSample() }

@Preview(name = "Rows font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun RowsLargePreview() = TeachTheme(darkTheme = false) { RowsSample() }
