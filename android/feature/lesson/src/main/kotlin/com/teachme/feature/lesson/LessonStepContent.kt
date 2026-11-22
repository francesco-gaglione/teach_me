package com.teachme.feature.lesson

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.key
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.ui.LlmBudgetPanel
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyHint
import com.teachme.core.ui.LlmKeyNotice
import com.teachme.sdui.highlight.inkNotesA11y
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.Paragraph
import com.teachme.sdui.render.BlockGap
import com.teachme.sdui.render.DefaultBlockRegistry
import com.teachme.sdui.render.dp

private val REVEAL_SLIDE = 24.dp

/** One row of the lesson list: banner, objective, a block, the quality note or the "Continua" bar. */
@Composable
internal fun LessonItemRow(
    index: Int,
    items: List<LessonItem>,
    ready: LessonUiState.Ready,
    audio: LessonAudioUiState,
    qualityNote: QualityNote,
    qualityFocus: FocusRequester,
    qualityFocusKey: String?,
    blockLabel: (String) -> String,
    goToBlock: (String) -> Unit,
    missingCount: Int,
    onContinue: () -> Unit,
    onLockedTap: () -> Unit,
) {
    val item = items[index]
    // A control row below the previous block takes 16 dp of the gap (lesson-audio.md section 1).
    when (item) {
        LessonItem.NewerCatalogBanner ->
            TeachStatusPanel(
                StatusVariant.Info,
                stringResource(R.string.lesson_newer_catalog_title),
                body = stringResource(R.string.lesson_newer_catalog_body),
                modifier = Modifier.padding(bottom = TeachTheme.layout.blockGap),
            )
        is LessonItem.Objective -> ObjectiveLine(item)
        is LessonItem.BlockItem ->
            Box(
                Modifier
                    .padding(top = blockTopPadding(items, index, audio))
                    .sectionReveal(ready.justRevealed == item.sectionIndex)
                    .then(
                        if (qualityFocusKey == item.key) {
                            Modifier.focusRequester(qualityFocus).focusable()
                        } else {
                            Modifier
                        },
                    )
                    // The text blocks say their notes together with their highlights.
                    .then(
                        if (item.block is Paragraph || item.block is Callout) {
                            Modifier
                        } else {
                            Modifier.inkNotesA11y(item.block.id)
                        },
                    ),
            ) { AudioListenable(item.block) { DefaultBlockRegistry.Render(item.block) } }
        LessonItem.QualityNote ->
            QualityNoteCard(qualityNote, blockLabel, goToBlock)
        is LessonItem.Continue ->
            Column(
                Modifier.padding(top = BlockGap.Block.dp()),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
            ) {
                // The refusal of "Completa lezione" (blocking), or the proactive suggestion.
                val notice = ready.keyNotice
                if (notice != null) {
                    LlmKeyNotice(
                        notice.variant,
                        LlmKeyContext.CompleteLesson,
                        notice.id,
                        onRetry = onContinue,
                        retrying = ready.completing,
                        again = notice.again,
                    )
                } else if (ready.budgetResetAt != null) {
                    LlmBudgetPanel(ready.budgetResetAt)
                } else if (ready.isLastSection && missingCount == 0) {
                    LlmKeyHint(LlmKeyContext.CompleteLessonHint)
                }
                ContinueBar(
                    isLast = ready.isLastSection,
                    missing = missingCount,
                    busy = ready.completing,
                    budgetResetAt = ready.budgetResetAt,
                    onContinue = onContinue,
                    onLockedTap = onLockedTap,
                )
            }
    }
}

/** The objective under the title: what the learner will be able to do, and how long it takes. */
@Composable
private fun ObjectiveLine(item: LessonItem.Objective) {
    val colors = TeachTheme.colors
    Column(
        Modifier.padding(top = TeachTheme.layout.blockGapTight).fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            TeachIcon(
                TeachIcons.Target,
                contentDescription = null,
                tint = colors.inkSoft,
                size = TeachTheme.layout.iconSizeSmall,
            )
            TeachText(
                stringResource(R.string.lesson_objective_label),
                style = TeachTheme.typography.ui.label,
                color = colors.inkSoft,
            )
        }
        InlineText(item.text, TeachTheme.typography.reading.body, interactive = true)
        if (item.minutes > 0) {
            TeachText(
                pluralStringResource(R.plurals.lesson_minutes, item.minutes, item.minutes),
                style = TeachTheme.typography.ui.body,
                color = colors.inkSoft,
            )
        }
        item.pdfTitle?.let { PdfLine(it, item.pdfMore) }
    }
}

/** "Dal tuo PDF: ..." under the title: makes it plain that the lesson follows the learner's document (lesson.md 3.3). */
@Composable
internal fun PdfLine(
    title: String,
    more: Int,
    modifier: Modifier = Modifier,
) {
    val colors = TeachTheme.colors
    Row(
        modifier,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
    ) {
        TeachIcon(
            TeachIcons.FileText,
            contentDescription = null,
            tint = colors.inkSoft,
            size = TeachTheme.layout.iconSizeSmall,
            modifier = Modifier.padding(top = TeachTheme.spacing.xxs),
        )
        TeachText(
            if (more > 0) {
                stringResource(R.string.lesson_pdf_line_more, title, more)
            } else {
                stringResource(R.string.lesson_pdf_line, title)
            },
            style = TeachTheme.typography.ui.bodyS,
            color = colors.inkSoft,
        )
    }
}

/** Fade + slide (at most 24 dp) of a freshly revealed section; instant with "reduce animations". */
@Composable
internal fun Modifier.sectionReveal(enabled: Boolean): Modifier {
    if (!enabled) return this
    val progress = remember { Animatable(0f) }
    val spec = TeachTheme.motion.sectionReveal
    val reduced = TeachTheme.reducedMotion
    LaunchedEffect(Unit) { progress.animateTo(1f, tween(if (reduced) 0 else spec.durationMs, easing = spec.easing)) }
    val slide = with(LocalDensity.current) { REVEAL_SLIDE.toPx() }
    return graphicsLayer {
        alpha = progress.value
        translationY = (1f - progress.value) * slide
    }
}

/** The gap above the block row [index] (a control row below the previous block takes 16 dp of it). */
@Composable
internal fun blockTopPadding(
    items: List<LessonItem>,
    index: Int,
    audio: LessonAudioUiState,
): Dp {
    val item = items[index] as? LessonItem.BlockItem ?: return 0.dp
    val before = (items.getOrNull(index - 1) as? LessonItem.BlockItem)?.block
    val trim = if (before != null && hasControlRowBelow(before, audio)) TeachTheme.spacing.md else 0.dp
    return (item.gap.dp() - trim).coerceAtLeast(0.dp)
}
