package com.teachme.feature.lesson

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonSize
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.GlossaryTerm
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.logic.LessonSummary
import com.teachme.sdui.logic.OpenStatus
import com.teachme.sdui.logic.SectionResult

/**
 * The summary after "Completa lezione" (docs/design/screens/lesson.md section 7): the local part (what you did, what to
 * review) is drawn at once; the block "Cosa hai dimostrato" ([CompletionBlock]) follows the closing job and grows at the
 * bottom. Words of competence; no score, no percentage, no red. "Da rivedere" is information, not blame.
 */
@Composable
fun LessonSummaryView(
    objective: String,
    summary: LessonSummary,
    completion: CompletionUi,
    closedElsewhere: Boolean,
    onBackToTopic: () -> Unit,
    /** [emptyRegister] is true when asked from the empty "Cosa hai dimostrato" block (a different question). */
    onAskTutor: (emptyRegister: Boolean) -> Unit,
    onReview: (String) -> Unit,
    onReread: () -> Unit,
    onOpenTerm: (GlossaryTerm) -> Unit,
    onSeeRegister: () -> Unit,
    onRetrySend: () -> Unit,
    onRetryJob: () -> Unit,
    modifier: Modifier = Modifier,
    originOf: (String?) -> LessonOrigin? = { null },
    /** First source taken from the learner's PDF and how many more there are (lesson.md 3.3). */
    pdfTitle: String? = null,
    pdfMore: Int = 0,
) {
    val colors = TeachTheme.colors
    Column(
        modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = screenMargin(), vertical = TeachTheme.spacing.lg),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(
            Modifier.widthIn(max = TeachTheme.layout.readingMaxWidth).fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.lg),
        ) {
            if (closedElsewhere) {
                TeachStatusPanel(StatusVariant.Info, stringResource(R.string.lesson_closed_elsewhere))
            }
            TeachText(
                stringResource(R.string.lesson_summary_title),
                Modifier.semantics { heading() },
                style = TeachTheme.typography.ui.title1,
            )
            if (objective.isNotBlank()) {
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
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
                            stringResource(R.string.lesson_summary_objective),
                            style = TeachTheme.typography.ui.label,
                            color = colors.inkSoft,
                        )
                    }
                    InlineText(objective, TeachTheme.typography.reading.body, interactive = false)
                    pdfTitle?.let { PdfLine(it, pdfMore) }
                }
            }
            if (summary.sections.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                    TeachText(
                        stringResource(R.string.lesson_summary_done_title),
                        Modifier.semantics { heading() },
                        style = TeachTheme.typography.ui.title3,
                    )
                    summary.sections.forEach { item ->
                        val words =
                            stringResource(
                                when (item.result) {
                                    SectionResult.CorrectFirstTry -> R.string.lesson_summary_first_try
                                    SectionResult.CorrectSecondTry -> R.string.lesson_summary_second_try
                                    SectionResult.Review -> R.string.lesson_summary_review
                                },
                            )
                        val title = item.title ?: stringResource(R.string.lesson_section_intro)
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .heightIn(min = TeachTheme.layout.minTouchTarget)
                                .semantics(mergeDescendants = true) {
                                    contentDescription = "$title: $words"
                                },
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
                        ) {
                            TeachIcon(
                                // Never a red cross: a section to review gets the neutral "partial" mark.
                                if (item.result ==
                                    SectionResult.Review
                                ) {
                                    TeachIcons.MinusCircle
                                } else {
                                    TeachIcons.CheckCircle
                                },
                                contentDescription = null,
                                tint = if (item.result == SectionResult.Review) colors.partial else colors.correct,
                            )
                            Column(Modifier.weight(1f)) {
                                TeachText(title, style = TeachTheme.typography.ui.bodyL)
                                TeachText(words, style = TeachTheme.typography.ui.body, color = colors.inkSoft)
                            }
                        }
                    }
                }
            }
            if (summary.openAnswers.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                    if (summary.sections.isEmpty()) {
                        TeachText(
                            stringResource(R.string.lesson_summary_done_title),
                            Modifier.semantics { heading() },
                            style = TeachTheme.typography.ui.title3,
                        )
                    }
                    summary.openAnswers.forEach { item ->
                        val words =
                            stringResource(
                                when (item.status) {
                                    OpenStatus.Correct -> R.string.lesson_summary_open_correct
                                    OpenStatus.Review -> R.string.lesson_summary_review
                                    OpenStatus.Pending -> R.string.lesson_summary_open_pending
                                },
                            )
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .heightIn(min = TeachTheme.layout.minTouchTarget)
                                .semantics(mergeDescendants = true) { contentDescription = "${item.question}: $words" },
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
                        ) {
                            TeachIcon(
                                when (item.status) {
                                    OpenStatus.Correct -> TeachIcons.CheckCircle
                                    OpenStatus.Review -> TeachIcons.MinusCircle
                                    OpenStatus.Pending -> TeachIcons.Clock
                                },
                                contentDescription = null,
                                tint =
                                    when (item.status) {
                                        OpenStatus.Correct -> colors.correct
                                        OpenStatus.Review -> colors.partial
                                        OpenStatus.Pending -> colors.inkSoft
                                    },
                            )
                            Column(Modifier.weight(1f)) {
                                TeachText(
                                    item.question,
                                    style = TeachTheme.typography.ui.bodyL,
                                    maxLines = 2,
                                    overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
                                )
                                TeachText(words, style = TeachTheme.typography.ui.body, color = colors.inkSoft)
                            }
                        }
                    }
                }
            }
            if (summary.toReview.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                    TeachText(
                        stringResource(R.string.lesson_summary_review_title),
                        Modifier.semantics { heading() },
                        style = TeachTheme.typography.ui.title3,
                    )
                    summary.toReview.forEach { item ->
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = TeachTheme.layout.minTouchTarget),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                        ) {
                            TeachText(
                                item.question,
                                Modifier.weight(1f),
                                style = TeachTheme.typography.ui.body,
                                maxLines = 2,
                                overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
                            )
                            TeachButton(
                                text = stringResource(R.string.lesson_summary_review_action),
                                onClick = { onReview(item.blockId) },
                                variant = ButtonVariant.Text,
                                contentDescription =
                                    stringResource(
                                        R.string.lesson_summary_review_action_a11y,
                                        item.question,
                                    ),
                            )
                        }
                    }
                }
            }
            CompletionBlock(
                completion = completion,
                onOpenTerm = onOpenTerm,
                onSeeRegister = onSeeRegister,
                onAskTutor = { onAskTutor(true) },
                onRetrySend = onRetrySend,
                onRetryJob = onRetryJob,
                originOf = originOf,
            )
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                TeachButton(
                    stringResource(R.string.lesson_back_to_topic),
                    onBackToTopic,
                    Modifier.fillMaxWidth(),
                    size = ButtonSize.Large,
                )
                TeachButton(
                    stringResource(R.string.lesson_ask_tutor),
                    { onAskTutor(false) },
                    Modifier.fillMaxWidth(),
                    variant = ButtonVariant.Secondary,
                )
                TeachButton(
                    stringResource(R.string.lesson_reread),
                    onReread,
                    Modifier.fillMaxWidth(),
                    variant = ButtonVariant.Secondary,
                )
            }
        }
    }
}
