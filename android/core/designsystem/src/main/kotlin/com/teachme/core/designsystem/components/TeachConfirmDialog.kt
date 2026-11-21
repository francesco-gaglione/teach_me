package com.teachme.core.designsystem.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.teachme.core.designsystem.LocalIsDark
import com.teachme.core.designsystem.OVERLAY_OPACITY
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.currentWindowWidthClass

internal const val LARGE_FONT_SCALE = 1.3f

/** An action failure shown inside a dialog (base-components section 8, error slot). */
data class DialogErrorPanel(
    val title: String,
    val body: String,
    val actionLabel: String? = null,
    val onAction: (() -> Unit)? = null,
)

/**
 * Shared frame of the app's dialogs: scrim, raised panel, scroll, `paneTitle`. While [blocked] the scrim
 * tap and Back are ignored (an action is in flight). Content goes in a spaced column.
 */
@Composable
internal fun TeachDialogShell(
    title: String,
    onDismissRequest: () -> Unit,
    blocked: Boolean,
    dismissOnScrimTap: Boolean,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = TeachTheme.colors
    val shape = TeachTheme.shapes.lg
    val shadow = TeachTheme.elevation.overlay
    val shadowAlpha = if (LocalIsDark.current) shadow.alphaDark else shadow.alphaLight
    Dialog(
        onDismissRequest = { if (!blocked) onDismissRequest() },
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        Box(
            modifier =
                Modifier
                    .fillMaxSize()
                    .background(colors.overlay.copy(alpha = OVERLAY_OPACITY))
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = { if (dismissOnScrimTap && !blocked) onDismissRequest() },
                    ).windowInsetsPadding(WindowInsets.safeDrawing),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                modifier =
                    Modifier
                        .padding(TeachTheme.spacing.lg)
                        .widthIn(
                            max =
                                if (currentWindowWidthClass() == WindowWidthClass.Compact) {
                                    TeachTheme.layout.dialogWidth
                                } else {
                                    TeachTheme.layout.dialogWidthWide
                                },
                        ).shadow(
                            shadow.blur,
                            shape,
                            ambientColor = Color.Black.copy(shadowAlpha),
                            spotColor = Color.Black.copy(shadowAlpha),
                        ).background(colors.surfaceRaised, shape)
                        // Swallow taps on the panel so they do not reach the scrim.
                        .clickable(
                            interactionSource = remember { MutableInteractionSource() },
                            indication = null,
                            onClick = {},
                        ).verticalScroll(rememberScrollState())
                        .padding(TeachTheme.spacing.lg)
                        .semantics { paneTitle = title },
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
                content = content,
            )
        }
    }
}

/** Danger panel with an optional text button, for failed dialog actions. */
@Composable
internal fun DialogErrorContent(error: DialogErrorPanel) {
    TeachStatusPanel(StatusVariant.Danger, error.title, body = error.body) {
        if (error.actionLabel != null && error.onAction != null) {
            TeachButton(error.actionLabel, error.onAction, variant = ButtonVariant.Text)
        }
    }
}

/**
 * Confirmation dialog (base-components section 8). Initial focus goes to the safe action
 * ([dismissLabel]); scrim tap, Back and the dismiss button all call [onDismiss]. Labels are verbs,
 * never "Yes/No/OK". With [destructive] the confirm button uses the destructive style.
 *
 * Error slot: when the confirmed action fails the dialog stays open and shows [error] above the actions.
 * While [loading] the confirm button shows [loadingLabel], Cancel is disabled and scrim/Back are ignored.
 */
@Composable
fun TeachConfirmDialog(
    title: String,
    body: String,
    confirmLabel: String,
    dismissLabel: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
    destructive: Boolean = false,
    loading: Boolean = false,
    loadingLabel: String = confirmLabel,
    error: DialogErrorPanel? = null,
) {
    val stacked = LocalDensity.current.fontScale > LARGE_FONT_SCALE
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }

    TeachDialogShell(title, onDismiss, blocked = loading, dismissOnScrimTap = true) {
        TeachText(title, style = TeachTheme.typography.ui.title2)
        TeachText(body, style = TeachTheme.typography.ui.bodyL)
        if (error != null) DialogErrorContent(error)
        val confirmVariant = if (destructive) ButtonVariant.Destructive else ButtonVariant.Primary
        val confirmText = if (loading) loadingLabel else confirmLabel
        if (stacked) {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                TeachButton(
                    confirmText,
                    onConfirm,
                    Modifier.fillMaxWidth(),
                    variant = confirmVariant,
                    loading = loading,
                )
                TeachButton(
                    dismissLabel,
                    onDismiss,
                    Modifier.fillMaxWidth().focusRequester(focus),
                    variant = ButtonVariant.Text,
                    enabled = !loading,
                )
            }
        } else {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs, Alignment.End),
            ) {
                TeachButton(
                    dismissLabel,
                    onDismiss,
                    Modifier.focusRequester(focus),
                    variant = ButtonVariant.Text,
                    enabled = !loading,
                )
                TeachButton(confirmText, onConfirm, variant = confirmVariant, loading = loading)
            }
        }
    }
}

@Preview(name = "Dialog light", showBackground = true)
@Composable
private fun DialogLightPreview() =
    TeachTheme(darkTheme = false) {
        TeachConfirmDialog("Uscire senza salvare?", "Le modifiche andranno perse.", "Esci", "Resta", {}, {}, true)
    }

@Preview(name = "Dialog dark", showBackground = true)
@Composable
private fun DialogDarkPreview() =
    TeachTheme(darkTheme = true) {
        TeachConfirmDialog("Uscire senza salvare?", "Le modifiche andranno perse.", "Esci", "Resta", {}, {}, true)
    }
