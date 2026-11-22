package com.teachme.feature.lesson

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachDisclosure
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachRowDivider
import com.teachme.core.designsystem.components.TeachSpinner
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachSurface
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.JobError
import com.teachme.core.model.JobProgress
import com.teachme.core.model.JobStatus
import com.teachme.core.model.TopicJob
import com.teachme.core.model.isLlmBudgetCode
import com.teachme.core.model.jobBudgetReset
import com.teachme.core.model.llmKeyVariantOfCode
import com.teachme.core.ui.LlmBudgetPanel
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyNotice
import kotlinx.coroutines.delay
import java.time.Instant

private const val SLOW_AFTER_MS = 60_000L

/** Which fixed sentence a `lesson_completion` step gets (lesson.md section 7.2, state A). */
internal fun completionStepText(step: String?): Int =
    when (step) {
        "grading" -> R.string.completion_step_grading
        "saving" -> R.string.completion_step_saving
        else -> R.string.completion_step_recording
    }

/** Title and body of a failed closing job, by `error.code` (lesson.md section 7.2, state D). */
internal fun completionFailure(code: String?): Pair<Int, Int> =
    when (code) {
        "invalid_output" -> R.string.completion_failed_invalid_title to R.string.completion_failed_invalid_body
        "upstream_failed" -> R.string.completion_failed_upstream_title to R.string.completion_failed_upstream_body
        "rate_limited" -> R.string.completion_failed_rate_title to R.string.completion_failed_rate_body
        "timeout" -> R.string.completion_failed_timeout_title to R.string.completion_failed_timeout_body
        "interrupted" -> R.string.completion_failed_interrupted_title to R.string.completion_failed_interrupted_body
        else -> R.string.completion_failed_generic_title to R.string.completion_failed_generic_body
    }

/**
 * The block "Cosa hai dimostrato" of the summary (lesson.md section 7.2): what the closing job is doing, then what it
 * produced. It grows at the bottom of the summary, so it never moves what is being read.
 */
@Composable
fun CompletionBlock(
    completion: CompletionUi,
    onOpenTerm: (GlossaryTerm) -> Unit,
    onSeeRegister: () -> Unit,
    onAskTutor: () -> Unit,
    onRetrySend: () -> Unit,
    onRetryJob: () -> Unit,
    modifier: Modifier = Modifier,
    originOf: (String?) -> LessonOrigin? = { null },
) {
    if (completion == CompletionUi.None || completion == CompletionUi.Sending) return
    val padding = if (currentWindowWidthClass() == WindowWidthClass.Compact) TeachTheme.spacing.md else 20.dp
    val pane = stringResource(R.string.completion_title)
    TeachSurface(
        modifier.fillMaxWidth().semantics { paneTitle = pane },
        border = BorderStroke(TeachTheme.stroke.hairline, TeachTheme.colors.line),
    ) {
        Column(
            Modifier.fillMaxWidth().padding(padding),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            TeachText(pane, Modifier.semantics { heading() }, style = TeachTheme.typography.ui.title3)
            when (completion) {
                is CompletionUi.NeedsSend -> NeedsSend(completion.retrying, onRetrySend)
                is CompletionUi.Running -> Running(completion.job)
                is CompletionUi.Done -> Done(completion, onOpenTerm, onSeeRegister, onAskTutor, originOf)
                is CompletionUi.Failed -> Failed(completion, onRetryJob)
                else -> Unit
            }
            Announcer(completion)
        }
    }
}

@Composable
private fun NeedsSend(
    retrying: Boolean,
    onRetry: () -> Unit,
) {
    TeachStatusPanel(
        StatusVariant.Info,
        stringResource(R.string.completion_needs_send_title),
        body = stringResource(R.string.completion_needs_send_body),
    )
    TeachButton(
        stringResource(if (retrying) R.string.completion_needs_send_retrying else R.string.completion_needs_send_retry),
        onRetry,
        variant = ButtonVariant.Text,
        loading = retrying,
    )
}

@Composable
private fun Running(job: TopicJob?) {
    val slow by produceState(false, job?.id) {
        value = false
        delay(SLOW_AFTER_MS)
        value = true
    }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        TeachSpinner(size = TeachTheme.layout.iconSizeSmall)
        TeachText(
            stringResource(completionStepText(job?.progress?.step)),
            style = TeachTheme.typography.ui.body,
        )
    }
    job?.progress?.message?.takeIf { it.isNotBlank() }?.let {
        TeachText(it, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
    }
    TeachText(
        stringResource(if (slow) R.string.completion_running_slow else R.string.completion_running_footer),
        style = TeachTheme.typography.ui.bodyS,
        color = TeachTheme.colors.inkSoft,
    )
}

@Composable
private fun Done(
    done: CompletionUi.Done,
    onOpenTerm: (GlossaryTerm) -> Unit,
    onSeeRegister: () -> Unit,
    onAskTutor: () -> Unit,
    originOf: (String?) -> LessonOrigin?,
) {
    val colors = TeachTheme.colors
    if (done.isEmpty) {
        TeachText(stringResource(R.string.completion_empty_title), style = TeachTheme.typography.ui.body)
        TeachText(
            stringResource(R.string.completion_empty_body),
            style = TeachTheme.typography.ui.body,
            color = colors.inkSoft,
        )
        TeachButton(stringResource(R.string.lesson_ask_tutor), onAskTutor, variant = ButtonVariant.Secondary)
        return
    }
    if (done.records.isNotEmpty()) {
        done.records.forEachIndexed { i, record ->
            if (i > 0) TeachRowDivider(Modifier.padding(vertical = TeachTheme.spacing.xs))
            RecordCard(record, framed = false, origin = originOf(record.lessonId))
        }
    }
    if (done.terms.isNotEmpty()) {
        TeachText(
            stringResource(R.string.completion_terms_title),
            style = TeachTheme.typography.ui.label,
            color = colors.inkSoft,
        )
        TermPills(done.terms, onOpenTerm)
    }
    if (done.reviewCardCount > 0) {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            TeachIcon(TeachIcons.Layers, null, tint = colors.inkSoft)
            Column {
                TeachText(
                    pluralStringResource(R.plurals.completion_cards, done.reviewCardCount, done.reviewCardCount),
                    style = TeachTheme.typography.ui.body,
                    color = colors.inkSoft,
                )
                TeachText(
                    pluralStringResource(R.plurals.completion_cards_note, done.reviewCardCount),
                    style = TeachTheme.typography.ui.bodyS,
                    color = colors.inkSoft,
                )
            }
        }
    }
    TeachButton(stringResource(R.string.completion_see_register), onSeeRegister, variant = ButtonVariant.Text)
}

@Composable
private fun Failed(
    failed: CompletionUi.Failed,
    onRetry: () -> Unit,
) {
    val keyVariant =
        failed.retryNotice?.variant
            ?: failed.job.error
                ?.code
                .llmKeyVariantOfCode()
    if (keyVariant != null) {
        // The key, not the work, is the problem: "Riprova" only once a key has been saved (llm-key.md section 6).
        LlmKeyNotice(
            keyVariant,
            LlmKeyContext.LessonCompletionJob,
            "${failed.job.id}:${failed.retryNotice?.id ?: 0}",
            onRetry = onRetry,
            retrying = failed.retrying,
            again = failed.creditAgain,
        )
        return
    }
    if (failed.job.error
            ?.code
            .isLlmBudgetCode() ||
        failed.budgetResetAt != null
    ) {
        // The closing stopped for today's budget, or "Riprova" was just refused for it (llm-key.md 10.1).
        val resetAt =
            remember(
                failed.job.id,
                failed.job.finishedAt,
                failed.budgetResetAt,
            ) { failed.job.jobBudgetReset(failed.budgetResetAt) }
        LlmBudgetPanel(
            resetAt,
            retryText =
                stringResource(
                    if (failed.retrying) R.string.completion_retrying else R.string.completion_retry,
                ),
            onRetry = onRetry,
            retrying = failed.retrying,
            retryIcon = TeachIcons.RotateCcw,
        )
        return
    }
    val (title, body) = completionFailure(failed.job.error?.code)
    var details by rememberSaveable { mutableStateOf(false) }
    TeachStatusPanel(
        StatusVariant.Danger,
        stringResource(title),
        body =
            if (failed.networkError) {
                stringResource(R.string.completion_network_error)
            } else {
                stringResource(body)
            },
    )
    TeachButton(
        stringResource(if (failed.retrying) R.string.completion_retrying else R.string.completion_retry),
        onRetry,
        variant = ButtonVariant.Secondary,
        loading = failed.retrying,
        leadingIcon = TeachIcons.RotateCcw,
    )
    failed.job.error
        ?.message
        ?.takeIf { it.isNotBlank() }
        ?.let { message ->
            TeachDisclosure(
                title = stringResource(R.string.completion_details),
                expanded = details,
                onToggle = { details = !details },
            ) {
                androidx.compose.foundation.text.selection.SelectionContainer {
                    TeachText(message, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
                }
            }
        }
}

/** Polite, invisible: announces the end of the closing (once) without moving the focus. */
@Composable
private fun Announcer(completion: CompletionUi) {
    val text =
        when (completion) {
            is CompletionUi.Done ->
                if (completion.isEmpty) {
                    stringResource(R.string.completion_announce_empty)
                } else {
                    val records =
                        pluralStringResource(
                            R.plurals.completion_announce_records,
                            completion.records.size,
                            completion.records.size,
                        )
                    val terms =
                        pluralStringResource(
                            R.plurals.completion_announce_terms,
                            completion.terms.size,
                            completion.terms.size,
                        )
                    stringResource(R.string.completion_announce_done, records, terms)
                }
            is CompletionUi.Failed -> stringResource(R.string.completion_announce_failed)
            else -> return
        }
    key(completion::class) {
        Box(
            Modifier.size(1.dp).semantics {
                liveRegion = LiveRegionMode.Polite
                contentDescription = text
            },
        )
    }
}

private val sampleJob =
    TopicJob(
        id = "j",
        topicId = "t",
        kind = "lesson_completion",
        status = JobStatus.Running,
        progress = JobProgress("recording", null),
        error = null,
        result = null,
        attempt = 1,
        createdAt = Instant.EPOCH,
        startedAt = null,
        finishedAt = null,
        lessonId = "l",
    )

@Composable
private fun Samples() {
    val term = GlossaryTerm("t1", "ownership", "ownership", "…", emptyList(), "l", Instant.EPOCH, Instant.EPOCH)
    Column(
        Modifier.padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        CompletionBlock(CompletionUi.NeedsSend(), {}, {}, {}, {}, {})
        CompletionBlock(CompletionUi.Running(sampleJob), {}, {}, {}, {}, {})
        CompletionBlock(CompletionUi.Done(emptyList(), listOf(term), 3), {}, {}, {}, {}, {})
        CompletionBlock(CompletionUi.Done(emptyList(), emptyList(), 0), {}, {}, {}, {}, {})
        CompletionBlock(
            CompletionUi.Failed(sampleJob.copy(status = JobStatus.Failed, error = JobError("upstream_failed", "502"))),
            {},
            {},
            {},
            {},
            {},
        )
    }
}

@Preview(name = "Completion light", showBackground = true, widthDp = 420, heightDp = 1200)
@Composable
private fun CompletionLightPreview() = TeachTheme(darkTheme = false) { Samples() }

@Preview(name = "Completion dark", showBackground = true, widthDp = 420, heightDp = 1200)
@Composable
private fun CompletionDarkPreview() = TeachTheme(darkTheme = true) { Samples() }
