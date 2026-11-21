package com.teachme.core.designsystem.components

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import com.teachme.core.designsystem.LocalIsDark
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

/** One entry of a [TeachActionMenu]. A [destructive] entry is drawn in `danger` and should be last. */
data class ActionMenuItem(
    val label: String,
    @DrawableRes val icon: Int,
    val onClick: () -> Unit,
    val destructive: Boolean = false,
)

/** Below the anchor, right edges aligned; flips above when there is no room (base-components 10.2). */
private object BelowAnchorEnd : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val x =
            (anchorBounds.right - popupContentSize.width).coerceIn(
                0,
                (windowSize.width - popupContentSize.width).coerceAtLeast(0),
            )
        val below = anchorBounds.bottom
        val y =
            if (below + popupContentSize.height <=
                windowSize.height
            ) {
                below
            } else {
                (anchorBounds.top - popupContentSize.height).coerceAtLeast(0)
            }
        return IntOffset(x, y)
    }
}

/**
 * Action menu (base-components 10.2). Place it in the same `Box` as its anchor (an icon button or a
 * row); it opens below the anchor. At most 4 entries. Initial focus on the first entry; Back or a tap
 * outside call [onDismiss].
 */
@Composable
fun TeachActionMenu(
    expanded: Boolean,
    onDismiss: () -> Unit,
    title: String,
    items: List<ActionMenuItem>,
) {
    if (!expanded) return
    val shape = TeachTheme.shapes.md
    val shadow = TeachTheme.elevation.raised
    val alpha = if (LocalIsDark.current) shadow.alphaDark else shadow.alphaLight
    val firstFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { firstFocus.requestFocus() } }
    Popup(
        popupPositionProvider = BelowAnchorEnd,
        onDismissRequest = onDismiss,
        properties = PopupProperties(focusable = true),
    ) {
        Column(
            modifier =
                Modifier
                    .shadow(
                        shadow.blur,
                        shape,
                        ambientColor = Color.Black.copy(alpha),
                        spotColor = Color.Black.copy(alpha),
                    ).background(TeachTheme.colors.surfaceRaised, shape)
                    .widthIn(min = 200.dp, max = 280.dp)
                    .padding(vertical = TeachTheme.spacing.xs)
                    .semantics { paneTitle = title },
        ) {
            items.forEachIndexed { index, item ->
                if (item.destructive && index > 0) TeachRowDivider(Modifier.padding(end = 0.dp))
                MenuEntry(item, if (index == 0) Modifier.focusRequester(firstFocus) else Modifier) {
                    onDismiss()
                    item.onClick()
                }
            }
        }
    }
}

@Composable
private fun MenuEntry(
    item: ActionMenuItem,
    modifier: Modifier,
    onClick: () -> Unit,
) {
    val state = rememberInteractionState()
    val color = if (item.destructive) TeachTheme.colors.danger else TeachTheme.colors.ink
    Row(
        modifier =
            modifier
                .fillMaxWidth()
                .heightIn(min = TeachTheme.layout.minTouchTarget)
                .background(if (state.pressed) TeachTheme.colors.surfacePressed else Color.Transparent)
                .focusRing(state.focused, TeachTheme.shapes.sm)
                .clickable(interactionSource = state.source, indication = null, role = Role.Button, onClick = onClick)
                .padding(horizontal = TeachTheme.spacing.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        TeachIcon(item.icon, contentDescription = null, tint = color)
        TeachText(item.label, style = TeachTheme.typography.ui.body, color = color)
    }
}

@Composable
private fun MenuSample() {
    // Previews cannot show popups; the entries are rendered inline.
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        Column(
            Modifier
                .widthIn(min = 200.dp, max = 280.dp)
                .background(TeachTheme.colors.surfaceRaised, TeachTheme.shapes.md)
                .padding(vertical = TeachTheme.spacing.xs),
        ) {
            MenuEntry(ActionMenuItem("Rinomina", TeachIcons.Pencil, {}), Modifier) {}
            TeachRowDivider()
            MenuEntry(ActionMenuItem("Elimina", TeachIcons.Trash, {}, destructive = true), Modifier) {}
        }
    }
}

@Preview(name = "Menu light", showBackground = true)
@Composable
private fun MenuLightPreview() = TeachTheme(darkTheme = false) { MenuSample() }

@Preview(name = "Menu dark", showBackground = true)
@Composable
private fun MenuDarkPreview() = TeachTheme(darkTheme = true) { MenuSample() }
