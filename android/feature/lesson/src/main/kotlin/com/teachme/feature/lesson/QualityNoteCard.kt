package com.teachme.feature.lesson

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonSize
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.model.QualityWarning

/**
 * "Nota sulla lezione" (docs/design/quality-warnings.md section 3): one quiet row after the last block, closed by
 * default. Neutral tones on purpose (no warning colour, no toast, never blocks anything). Draws nothing when [note]
 * has no warnings. [onGoToBlock] is only offered for entries that carry a reachable `blockId`.
 */
@Composable
fun QualityNoteCard(
    note: QualityNote,
    blockLabel: (String) -> String,
    onGoToBlock: (String) -> Unit,
    modifier: Modifier = Modifier,
    initiallyOpen: Boolean = false,
) {
    if (note.total == 0) return
    var open by rememberSaveable { mutableStateOf(initiallyOpen) }
    val colors = TeachTheme.colors
    val spacing = TeachTheme.spacing
    val reduced = TeachTheme.reducedMotion
    val spec = TeachTheme.motion.feedbackAppear
    val ms = if (reduced) 0 else spec.durationMs
    val title = stringResource(R.string.lesson_quality_title, note.total)
    val description = pluralStringResource(R.plurals.lesson_quality_a11y, note.total, note.total)
    val state = stringResource(if (open) R.string.lesson_quality_state_open else R.string.lesson_quality_state_closed)
    val action = stringResource(if (open) R.string.lesson_quality_action_close else R.string.lesson_quality_action_open)
    Column(
        modifier
            .fillMaxWidth()
            .padding(top = spacing.lg)
            .clip(TeachTheme.shapes.md)
            .background(colors.surfaceSunken),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .semantics(mergeDescendants = true) {
                    role = Role.Button
                    contentDescription = description
                    stateDescription = state
                    onClick(label = action) {
                        open = !open
                        true
                    }
                }.clickable(role = Role.Button) { open = !open }
                .heightIn(min = TeachTheme.layout.minTouchTarget)
                .padding(horizontal = spacing.md, vertical = spacing.sm),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(spacing.sm),
        ) {
            TeachIcon(TeachIcons.Info, contentDescription = null, tint = colors.inkSoft, size = 20.dp)
            TeachText(
                title,
                Modifier.weight(1f).clearAndSetSemantics { },
                style = TeachTheme.typography.ui.label,
                color = colors.inkSoft,
            )
            TeachIcon(
                if (open) TeachIcons.ChevronUp else TeachIcons.ChevronDown,
                contentDescription = null,
                tint = colors.inkSoft,
                size = 20.dp,
            )
        }
        AnimatedVisibility(
            visible = open,
            enter = fadeIn(tween(ms)) + expandVertically(tween(ms)),
            exit = fadeOut(tween(ms)) + shrinkVertically(tween(ms)),
        ) {
            Column(Modifier.fillMaxWidth().padding(horizontal = spacing.md).padding(bottom = spacing.md)) {
                note.entries.forEachIndexed { index, entry ->
                    Divider()
                    val text =
                        if (entry.count > 1) {
                            entry.message + " · " +
                                pluralStringResource(R.plurals.lesson_quality_points, entry.count, entry.count)
                        } else {
                            entry.message
                        }
                    val last = index == note.entries.lastIndex && note.hidden > 0
                    TeachText(
                        if (last) text + "\n" + stringResource(R.string.lesson_quality_more, note.hidden) else text,
                        Modifier.padding(vertical = spacing.sm),
                        style = TeachTheme.typography.ui.body,
                        color = colors.ink,
                    )
                    val target = entry.blockId
                    if (target != null) {
                        val a11y = stringResource(R.string.lesson_quality_goto_a11y, blockLabel(target))
                        TeachButton(
                            text = stringResource(R.string.lesson_quality_goto),
                            onClick = { onGoToBlock(target) },
                            variant = ButtonVariant.Text,
                            size = ButtonSize.Normal,
                            contentDescription = a11y,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun Divider() {
    Box(Modifier.fillMaxWidth().height(1.dp).background(TeachTheme.colors.line))
}

private val SAMPLE =
    listOf(
        QualityWarning(
            "explanation_run_too_long",
            "p3",
            "Questa parte è lunga prima del primo esercizio. Fermati a ripetere.",
        ),
        QualityWarning(
            "too_many_callouts",
            null,
            "Qui ci sono diverse note evidenziate: leggi con calma quelle importanti.",
        ),
    )

@Composable
private fun QualitySample(initiallyOpen: Boolean) {
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        QualityNoteCard(
            note = buildQualityNote(SAMPLE),
            blockLabel = { "Paragrafo" },
            onGoToBlock = {},
            initiallyOpen = initiallyOpen,
        )
    }
}

@Preview(name = "Quality closed", showBackground = true)
@Composable
private fun QualityClosedPreview() = TeachTheme(darkTheme = false) { QualitySample(false) }

@Preview(name = "Quality open", showBackground = true)
@Composable
private fun QualityOpenPreview() = TeachTheme(darkTheme = false) { QualitySample(true) }

@Preview(name = "Quality open dark", showBackground = true)
@Composable
private fun QualityOpenDarkPreview() = TeachTheme(darkTheme = true) { QualitySample(true) }

@Preview(name = "Quality open font 200%", showBackground = true, fontScale = 2f)
@Composable
private fun QualityOpenBigFontPreview() = TeachTheme(darkTheme = false) { QualitySample(true) }
