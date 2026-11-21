package com.teachme.core.designsystem.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

private const val SKELETON_ROWS = 3
private const val BAR_LONG = 0.6f
private const val BAR_SHORT = 0.35f

/**
 * List skeleton (base-components 10.4): [SKELETON_ROWS] static rows, no shimmer. The block is a single
 * accessibility element described by [description] with state "loading" ([stateText]). [slowText] is
 * shown under it once loading takes long.
 */
@Composable
fun TeachListSkeleton(
    description: String,
    stateText: String,
    modifier: Modifier = Modifier,
    withIcon: Boolean = true,
    slowText: String? = null,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachCard(
            Modifier.clearAndSetSemantics {
                contentDescription = description
                stateDescription = stateText
            },
        ) {
            Column {
                repeat(SKELETON_ROWS) { index ->
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 72.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
                    ) {
                        if (withIcon) {
                            Box(
                                Modifier.size(24.dp).background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.sm),
                            )
                        }
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                            Bar(BAR_LONG)
                            Bar(BAR_SHORT)
                        }
                    }
                    if (index < SKELETON_ROWS - 1) TeachRowDivider()
                }
            }
        }
        if (slowText != null) {
            TeachText(slowText, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
        }
    }
}

@Composable
private fun Bar(fraction: Float) {
    Box(
        Modifier
            .fillMaxWidth(fraction)
            .height(14.dp)
            .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.sm),
    )
}

/**
 * Full-screen error for a failed first load (base-components 10.5): a danger panel with the reason,
 * then the actions ([primary] is the only primary button, [secondary] a text button).
 */
@Composable
fun TeachErrorState(
    title: String,
    body: String,
    modifier: Modifier = Modifier,
    primaryLabel: String? = null,
    onPrimary: (() -> Unit)? = null,
    secondaryLabel: String? = null,
    onSecondary: (() -> Unit)? = null,
) {
    Column(
        modifier = modifier.fillMaxSize().padding(TeachTheme.layout.screenMarginCompact),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        TeachStatusPanel(
            StatusVariant.Danger,
            title,
            body = body,
            modifier = Modifier.widthIn(max = TeachTheme.layout.readingMaxWidth),
        )
        if (primaryLabel != null && onPrimary != null) {
            TeachButton(primaryLabel, onPrimary, size = ButtonSize.Large)
        }
        if (secondaryLabel != null && onSecondary != null) {
            TeachButton(secondaryLabel, onSecondary, variant = ButtonVariant.Text)
        }
    }
}

/** Inline banner for a failed refresh while data is on screen (base-components 10.5). */
@Composable
fun TeachErrorBanner(
    title: String,
    retryLabel: String,
    onRetry: () -> Unit,
    dismissLabel: String,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    TeachStatusPanel(StatusVariant.Danger, title, modifier = modifier) {
        Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            TeachButton(retryLabel, onRetry, variant = ButtonVariant.Text)
            TeachButton(dismissLabel, onDismiss, variant = ButtonVariant.Text)
        }
    }
}

@Composable
private fun StatesSample() {
    Column(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        TeachListSkeleton("Caricamento delle cartelle", "Caricamento", slowText = "Sto ancora caricando…")
        TeachErrorBanner("Non riesco ad aggiornare l'elenco.", "Riprova", {}, "Chiudi", {})
        TeachStatusPanel(
            StatusVariant.Danger,
            "Non riesco a raggiungere il backend",
            body = "Controlla che il dispositivo sia collegato alla rete e che il backend sia acceso.",
        )
        TeachListRow("Lavoro", {}, "Lavoro", leadingIcon = TeachIcons.Folder)
    }
}

@Preview(name = "States light", showBackground = true, heightDp = 700)
@Composable
private fun StatesLightPreview() = TeachTheme(darkTheme = false) { StatesSample() }

@Preview(name = "States dark", showBackground = true, heightDp = 700)
@Composable
private fun StatesDarkPreview() = TeachTheme(darkTheme = true) { StatesSample() }

@Preview(name = "ErrorState", showBackground = true, heightDp = 500)
@Composable
private fun ErrorStatePreview() =
    TeachTheme(darkTheme = false) {
        Box(Modifier.background(TeachTheme.colors.bg)) {
            TeachErrorState(
                "Non riesco a raggiungere il backend",
                "Controlla che il dispositivo sia collegato alla rete e che il backend sia acceso.",
                primaryLabel = "Riprova",
                onPrimary = {},
                secondaryLabel = "Apri le impostazioni",
                onSecondary = {},
            )
        }
    }
