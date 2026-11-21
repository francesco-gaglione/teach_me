package com.teachme.core.designsystem.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.LocalIsDark
import com.teachme.core.designsystem.TeachTheme
import kotlinx.coroutines.delay

private const val TOAST_MS = 4_000L
private const val TOAST_WITH_ACTION_MS = 6_000L

/** Message shown by [TeachToastHost]. */
@Stable
class TeachToastState {
    internal var message by mutableStateOf<String?>(null)
    internal var actionLabel by mutableStateOf<String?>(null)
    internal var onAction by mutableStateOf<(() -> Unit)?>(null)

    /** Shows [text] for 4 s (6 s with an action) and suspends until it disappears. */
    suspend fun show(
        text: String,
        actionLabel: String? = null,
        onAction: (() -> Unit)? = null,
    ) {
        message = text
        this.actionLabel = actionLabel
        this.onAction = onAction
        delay(if (actionLabel != null) TOAST_WITH_ACTION_MS else TOAST_MS)
        message = null
    }

    /** Shows [text] with no action for [durationMs], for a message that needs longer than 4 s to be read. */
    suspend fun showFor(
        text: String,
        durationMs: Long,
    ) {
        message = text
        actionLabel = null
        onAction = null
        delay(durationMs)
        message = null
    }

    /** Hides the message now. */
    fun dismiss() {
        message = null
    }
}

/**
 * Toast (base-components section 9). Not for errors that need action: use [TeachStatusPanel].
 * The shadow approximates the `raised` token (blur used as elevation).
 */
@Composable
fun TeachToastHost(
    state: TeachToastState,
    modifier: Modifier = Modifier,
) {
    val text = state.message
    Box(
        modifier = modifier.semantics { liveRegion = LiveRegionMode.Polite },
    ) {
        if (text != null) {
            ToastContent(text, state.actionLabel, state.onAction)
        }
    }
}

@Composable
private fun ToastContent(
    text: String,
    actionLabel: String?,
    onAction: (() -> Unit)?,
) {
    val colors = TeachTheme.colors
    val shape = TeachTheme.shapes.md
    val shadow = TeachTheme.elevation.raised
    val alpha = if (LocalIsDark.current) shadow.alphaDark else shadow.alphaLight
    Row(
        modifier =
            Modifier
                .padding(TeachTheme.spacing.md)
                .fillMaxWidth()
                .shadow(shadow.blur, shape, ambientColor = Color.Black.copy(alpha), spotColor = Color.Black.copy(alpha))
                .background(colors.toastBg, shape)
                .heightIn(min = TeachTheme.layout.minTouchTarget)
                .padding(start = TeachTheme.spacing.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        TeachText(text, Modifier.weight(1f), style = TeachTheme.typography.ui.bodyL, color = colors.toastInk)
        if (actionLabel != null && onAction != null) {
            TeachButton(actionLabel, onAction, variant = ButtonVariant.Text)
        } else {
            Box(Modifier.padding(end = TeachTheme.spacing.md))
        }
    }
}

@Preview(name = "Toast light", showBackground = true)
@Composable
private fun ToastLightPreview() = TeachTheme(darkTheme = false) { ToastContent("Impostazioni salvate", null, null) }

@Preview(name = "Toast dark", showBackground = true)
@Composable
private fun ToastDarkPreview() = TeachTheme(darkTheme = true) { ToastContent("Impostazioni salvate", "Annulla", {}) }
