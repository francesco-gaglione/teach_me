package com.teachme.core.designsystem.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.teachme.core.designsystem.LocalIsDark
import com.teachme.core.designsystem.OVERLAY_OPACITY
import com.teachme.core.designsystem.TeachRadii
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.currentWindowWidthClass

private const val SHEET_MAX_HEIGHT_FRACTION = 0.9f

/**
 * Bottom sheet (base-components section 9): scrim at the overlay opacity, raised panel anchored to the
 * bottom with rounded top corners, handle, at most 90 % of the height, scrolling content. Full width on
 * compact windows, `dialogWidthWide` and centered otherwise. Back, Esc and a tap on the scrim call
 * [onDismissRequest]. [title] is the pane title announced by screen readers.
 */
@Composable
fun TeachBottomSheet(
    title: String,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = TeachTheme.colors
    val radius = TeachRadii().lg
    val shape = RoundedCornerShape(topStart = radius, topEnd = radius)
    val shadow = TeachTheme.elevation.overlay
    val shadowAlpha = if (LocalIsDark.current) shadow.alphaDark else shadow.alphaLight
    val compact = currentWindowWidthClass() == WindowWidthClass.Compact
    Dialog(
        onDismissRequest = onDismissRequest,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        BoxWithConstraints(
            Modifier
                .fillMaxSize()
                .background(colors.overlay.copy(alpha = OVERLAY_OPACITY))
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = onDismissRequest,
                ).windowInsetsPadding(WindowInsets.safeDrawing),
            contentAlignment = Alignment.BottomCenter,
        ) {
            Column(
                modifier =
                    modifier
                        .then(
                            if (compact) {
                                Modifier.fillMaxWidth()
                            } else {
                                Modifier.widthIn(
                                    max = TeachTheme.layout.dialogWidthWide,
                                )
                            },
                        ).heightIn(max = maxHeight * SHEET_MAX_HEIGHT_FRACTION)
                        .shadow(
                            shadow.blur,
                            shape,
                            ambientColor = Color.Black.copy(shadowAlpha),
                            spotColor = Color.Black.copy(shadowAlpha),
                        ).background(colors.surfaceRaised, shape)
                        .clickable(
                            interactionSource = remember { MutableInteractionSource() },
                            indication = null,
                            onClick = {},
                        ).semantics { paneTitle = title },
            ) {
                Box(
                    Modifier
                        .align(Alignment.CenterHorizontally)
                        .padding(vertical = TeachTheme.spacing.sm)
                        .size(width = SHEET_HANDLE_WIDTH, height = SHEET_HANDLE_HEIGHT)
                        .background(colors.border, TeachTheme.shapes.full),
                )
                Column(
                    Modifier
                        .verticalScroll(rememberScrollState())
                        .padding(
                            start = TeachTheme.spacing.lg,
                            end = TeachTheme.spacing.lg,
                            bottom = TeachTheme.spacing.lg,
                        ),
                    verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
                    content = content,
                )
            }
        }
    }
}

private val SHEET_HANDLE_WIDTH = 40.dp
private val SHEET_HANDLE_HEIGHT = 4.dp

@Preview(name = "Sheet light", showBackground = true)
@Composable
private fun SheetLightPreview() =
    TeachTheme(darkTheme = false) {
        TeachBottomSheet("Termine", {}) { TeachText("ownership", style = TeachTheme.typography.ui.title2) }
    }

@Preview(name = "Sheet dark", showBackground = true)
@Composable
private fun SheetDarkPreview() =
    TeachTheme(darkTheme = true) {
        TeachBottomSheet("Termine", {}) { TeachText("ownership", style = TeachTheme.typography.ui.title2) }
    }
