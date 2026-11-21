package com.teachme.feature.lesson

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.CardVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachCard
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.model.JobError
import com.teachme.core.model.JobProgress
import com.teachme.core.model.JobResult
import com.teachme.core.model.JobStatus
import com.teachme.core.model.TopicJob
import com.teachme.core.model.llmKeyVariantOfCode
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyNotice
import kotlinx.coroutines.delay
import java.time.Instant

/** Below this width, or with a larger font, the action goes under the text (chat.md design 10b.4). */
private const val STACK_FONT_SCALE = 1.3f
private const val STEP_COALESCE_MS = 1_000L
private val ActionMinWidth = 160.dp

/**
 * The card of a `lesson_request` part (chat.md design 10b): a reference to the generation job [jobId], whose state it
 * follows through [state] (the same one of the Lessons tab: REST list plus `job_updated`). [announces] is true on one
 * card per job, the most recent in the chat. "Apri lezione" opens the lesson; "Riprova" retries the job (only the last
 * one of the Topic has it).
 */
@Composable
fun LessonRequestCard(
    jobId: String,
    stateProvider: () -> LessonsUiState,
    announces: Boolean,
    onOpenLesson: (String) -> Unit,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // [stateProvider] reads snapshot state: each card derives only what it shows, so a `job_updated` of another job
    // leaves this card (and the chat row around it) alone.
    val cardState by remember(jobId) {
        derivedStateOf {
            val s = stateProvider()
            lessonRequestState(s.generationJob(jobId), s.job?.id) { s.lesson(it)?.title }
        }
    }
    val retrying by remember { derivedStateOf { stateProvider().retrying } }
    val networkError by remember { derivedStateOf { stateProvider().actionError != null } }
    val keyAgain by remember { derivedStateOf { stateProvider().keyNotice?.again ?: 0 } }
    LessonRequestCardContent(
        cardState,
        jobId,
        retrying = retrying,
        networkError = networkError,
        keyAgain = keyAgain,
        announces = announces,
        onOpenLesson = onOpenLesson,
        onRetry = onRetry,
        modifier = modifier,
    )
}

@Composable
internal fun LessonRequestCardContent(
    state: LessonRequestState,
    jobId: String,
    retrying: Boolean,
    networkError: Boolean,
    keyAgain: Int,
    announces: Boolean,
    onOpenLesson: (String) -> Unit,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
    forceStacked: Boolean? = null,
) {
    val stacked =
        forceStacked ?: (
            currentWindowWidthClass() != WindowWidthClass.Expanded || LocalDensity.current.fontScale > STACK_FONT_SCALE
        )
    val paneTitle = stringResource(R.string.lessonreq_pane_title)
    val inProgress = stringResource(R.string.lessonreq_state_in_progress)
    val fade = if (TeachTheme.reducedMotion) snap() else tween<Float>(TeachTheme.motion.fastMs)
    Box(
        modifier.fillMaxWidth().semantics {
            this.paneTitle = paneTitle
            if (state.inProgress) stateDescription = inProgress
        },
    ) {
        // The card changes in place with a short fade; queued and running are the same block (only the line changes).
        // The outgoing content keeps showing the state it had (the wrapper carries it); equality is by kind, so a step
        // change inside the same kind does not fade, and the content of the current kind reads the latest state.
        val kind = state.kind()
        Crossfade(targetState = KindState(kind, state), animationSpec = fade, label = "lesson-request") { held ->
            val shown = if (held.kind == kind) state else held.state
            Box(Modifier.fillMaxWidth()) {
                CardBody(shown, jobId, retrying, networkError, keyAgain, stacked, onOpenLesson, onRetry)
            }
        }
        LessonRequestAnnouncer(state, announces)
    }
}

/** A card state tagged with its kind; two are equal when the kind is, whatever the state they carry. */
private class KindState(
    val kind: Int,
    val state: LessonRequestState,
) {
    override fun equals(other: Any?) = other is KindState && other.kind == kind

    override fun hashCode() = kind
}

private fun LessonRequestState.kind(): Int =
    when (this) {
        LessonRequestState.Unknown -> 0
        LessonRequestState.Queued, is LessonRequestState.Running -> 1
        is LessonRequestState.Ready -> 2
        is LessonRequestState.Failed -> if (canRetry) 3 else 4
    }

@Composable
private fun CardBody(
    state: LessonRequestState,
    jobId: String,
    retrying: Boolean,
    networkError: Boolean,
    keyAgain: Int,
    stacked: Boolean,
    onOpenLesson: (String) -> Unit,
    onRetry: () -> Unit,
) {
    val colors = TeachTheme.colors
    when (state) {
        LessonRequestState.Unknown ->
            TeachCard(Modifier.fillMaxWidth(), CardVariant.Static) {
                Row(
                    Modifier.fillMaxWidth().padding(TeachTheme.spacing.md),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
                ) {
                    TeachIcon(TeachIcons.BookOpen, null, tint = colors.inkMuted, size = TeachTheme.layout.iconSizeSmall)
                    TeachText(
                        stringResource(R.string.lessonreq_unknown),
                        style = TeachTheme.typography.ui.label,
                        color = colors.inkSoft,
                    )
                }
            }
        LessonRequestState.Queued ->
            ProgressPanel {
                Line(stringResource(R.string.lessons_phase_queued), colors.inkSoft)
            }
        is LessonRequestState.Running ->
            ProgressPanel {
                when {
                    state.repairing -> {
                        Line(stringResource(R.string.lessons_phase_repairing), colors.ink)
                        TeachText(
                            stringResource(R.string.lessons_repair_detail),
                            style = TeachTheme.typography.ui.bodyS,
                            color = colors.inkSoft,
                        )
                    }
                    state.phase != null -> Line(stringResource(phaseLabel(state.phase)), colors.ink)
                    else -> Line(state.message ?: stringResource(R.string.lessons_phase_generic), colors.ink)
                }
            }
        is LessonRequestState.Ready -> {
            val number = state.number
            val title =
                if (number != null) {
                    pluralStringResource(R.plurals.lessonreq_ready_title, number, number)
                } else {
                    stringResource(R.string.lessonreq_ready_title_plain)
                }
            val lessonId = state.lessonId
            val description =
                when {
                    number != null && state.title != null ->
                        stringResource(R.string.lessonreq_open_a11y_title, number, state.title)
                    number != null -> stringResource(R.string.lessonreq_open_a11y_number, number)
                    else -> stringResource(R.string.lessonreq_open)
                }
            PanelWithAction(
                StatusVariant.Correct,
                title,
                stacked,
                extra = { state.title?.let { Line(it, colors.ink) } },
                action =
                    lessonId?.let { id ->
                        { buttonModifier ->
                            TeachButton(
                                stringResource(R.string.lessonreq_open),
                                { onOpenLesson(id) },
                                buttonModifier,
                                leadingIcon = TeachIcons.ArrowRight,
                                contentDescription = description,
                            )
                        }
                    },
            )
        }
        is LessonRequestState.Failed ->
            if (!state.canRetry) {
                PanelWithAction(
                    StatusVariant.Danger,
                    stringResource(R.string.lessonreq_old_failed_title),
                    stacked,
                    extra = { Line(stringResource(R.string.lessonreq_old_failed_body), colors.ink) },
                    action = null,
                )
            } else {
                val keyVariant = state.code.llmKeyVariantOfCode()
                if (keyVariant != null) {
                    // The key, not the work, is the problem: same rules of "Riprova" as the Lessons tab (llm-key.md).
                    LlmKeyNotice(
                        keyVariant,
                        LlmKeyContext.LessonGenerationJob,
                        jobId,
                        onRetry = onRetry,
                        retrying = retrying,
                        again = keyAgain,
                    )
                } else {
                    val (title, body) = generationFailure(state.code)
                    val retryLabel = stringResource(R.string.lessonreq_retry_a11y)
                    PanelWithAction(
                        StatusVariant.Danger,
                        stringResource(title),
                        stacked,
                        extra = {
                            Line(stringResource(body), colors.ink)
                            if (networkError) {
                                Box(
                                    Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                                ) {
                                    TeachText(
                                        stringResource(R.string.lessons_start_network_error),
                                        style = TeachTheme.typography.ui.bodyS,
                                        color = colors.inkSoft,
                                    )
                                }
                            }
                        },
                        action = { buttonModifier ->
                            TeachButton(
                                stringResource(if (retrying) R.string.lessons_retrying else R.string.lessons_retry),
                                onRetry,
                                buttonModifier,
                                variant = ButtonVariant.Secondary,
                                loading = retrying,
                                leadingIcon = TeachIcons.RotateCcw,
                                contentDescription = retryLabel,
                            )
                        },
                    )
                }
            }
    }
}

private fun phaseLabel(phase: GenerationPhase): Int =
    when (phase) {
        GenerationPhase.Planning -> R.string.lessons_phase_planning
        GenerationPhase.Reading -> R.string.lessons_phase_reading
        GenerationPhase.Writing -> R.string.lessons_phase_writing
        GenerationPhase.Checking -> R.string.lessons_phase_checking
        GenerationPhase.Saving -> R.string.lessons_phase_saving
    }

@Composable
private fun Line(
    text: String,
    color: androidx.compose.ui.graphics.Color,
) = TeachText(text, style = TeachTheme.typography.ui.body, color = color)

@Composable
private fun ProgressPanel(lines: @Composable ColumnScope.() -> Unit) {
    // The panel's own live region is off: the announcements are the one node of LessonRequestAnnouncer.
    TeachStatusPanel(
        StatusVariant.Progress,
        stringResource(R.string.lessons_preparing_title),
        announce = false,
        extra = lines,
    )
}

/** A status panel with one action: at the end of the row when there is room, otherwise under the text, full width. */
@Composable
private fun PanelWithAction(
    variant: StatusVariant,
    title: String,
    stacked: Boolean,
    extra: @Composable ColumnScope.() -> Unit,
    action: (@Composable (Modifier) -> Unit)?,
) {
    when {
        action == null -> TeachStatusPanel(variant, title, announce = false, extra = extra)
        stacked ->
            TeachStatusPanel(variant, title, announce = false, extra = {
                extra()
                Box(Modifier.padding(top = TeachTheme.spacing.xs)) { action(Modifier.fillMaxWidth()) }
            })
        else ->
            TeachStatusPanel(
                variant,
                title,
                announce = false,
                extra = extra,
                trailing = { action(Modifier.widthIn(min = ActionMinWidth)) },
            )
    }
}

/**
 * The one invisible polite live region of the card (chat.md design 10b.6): says the change of step (coalesced, so a fast
 * run of steps is one sentence), the end, and the failure; never on first composition, never when [enabled] is false.
 */
@Composable
private fun LessonRequestAnnouncer(
    state: LessonRequestState,
    enabled: Boolean,
) {
    var previous by remember { mutableStateOf(state) }
    var speech by remember { mutableStateOf<LessonRequestSpeech?>(null) }
    var counter by remember { mutableIntStateOf(0) }
    LaunchedEffect(state.stage(), enabled) {
        val before = previous
        previous = state
        val said = lessonRequestSpeech(before, state)
        if (said == null || !enabled) return@LaunchedEffect
        if (said is LessonRequestSpeech.Step) delay(STEP_COALESCE_MS)
        speech = said
        counter++
    }
    val said = speech ?: return
    val text =
        when (said) {
            is LessonRequestSpeech.Step ->
                stringResource(
                    R.string.lessonreq_announce_step,
                    stringResource(
                        when {
                            said.repairing -> R.string.lessonreq_verb_repairing
                            else ->
                                when (said.phase) {
                                    GenerationPhase.Planning -> R.string.lessonreq_verb_planning
                                    GenerationPhase.Reading -> R.string.lessonreq_verb_reading
                                    GenerationPhase.Writing -> R.string.lessonreq_verb_writing
                                    GenerationPhase.Checking -> R.string.lessonreq_verb_checking
                                    GenerationPhase.Saving -> R.string.lessonreq_verb_saving
                                }
                        },
                    ),
                )
            is LessonRequestSpeech.Ready ->
                if (said.number != null) {
                    pluralStringResource(R.plurals.lessonreq_announce_ready, said.number, said.number)
                } else {
                    stringResource(R.string.lessonreq_announce_ready_plain)
                }
            is LessonRequestSpeech.Failed ->
                stringResource(
                    if (said.canRetry) R.string.lessonreq_announce_failed_retry else R.string.lessonreq_announce_failed,
                )
        }
    key(counter) {
        Box(
            Modifier.size(1.dp).semantics {
                liveRegion = LiveRegionMode.Polite
                contentDescription = text
            },
        )
    }
}

// --- previews: one per state, light and dark, compact and expanded ------------------------------------------------------

private fun previewJob(
    status: JobStatus,
    step: String? = null,
    message: String? = null,
    done: Int? = null,
    error: JobError? = null,
    result: JobResult? = null,
) = TopicJob(
    "j",
    "t",
    "lesson_generation",
    status,
    step?.let { JobProgress(it, message, done, 3) },
    error,
    result,
    1,
    Instant.EPOCH,
    Instant.EPOCH,
    null,
)

@Composable
private fun CardSamples(forceStacked: Boolean) {
    val samples =
        listOf(
            lessonRequestState(previewJob(JobStatus.Queued), "j") { null },
            lessonRequestState(previewJob(JobStatus.Running, "writing", "Scrivo la sezione 2"), "j") { null },
            lessonRequestState(previewJob(JobStatus.Running, "writing", "Correggo la lezione", done = 2), "j") { null },
            lessonRequestState(previewJob(JobStatus.Running, "polishing", "Rifinisco"), "j") { null },
            lessonRequestState(
                previewJob(JobStatus.Succeeded, result = JobResult(lessonId = "l3", lessonNumber = 3)),
                "j",
            ) { "Lifetimes in pratica" },
            lessonRequestState(
                previewJob(JobStatus.Failed, error = JobError("invalid_output", "3 documenti non validi")),
                "j",
            ) { null },
            lessonRequestState(previewJob(JobStatus.Failed, error = JobError("timeout", "")), "newer") { null },
            lessonRequestState(null, "j") { null },
        )
    Column(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        samples.forEach {
            LessonRequestCardContent(it, "j", false, false, 0, false, {}, {}, forceStacked = forceStacked)
        }
    }
}

@Preview(name = "Lesson request compact light", showBackground = true, widthDp = 360, heightDp = 1500)
@Composable
private fun LessonRequestCompactLightPreview() = TeachTheme(darkTheme = false) { CardSamples(true) }

@Preview(name = "Lesson request compact dark", showBackground = true, widthDp = 360, heightDp = 1500)
@Composable
private fun LessonRequestCompactDarkPreview() = TeachTheme(darkTheme = true) { CardSamples(true) }

@Preview(name = "Lesson request expanded", showBackground = true, widthDp = 600, heightDp = 1100)
@Composable
private fun LessonRequestExpandedPreview() = TeachTheme(darkTheme = false) { CardSamples(false) }

@Preview(name = "Lesson request font 2.0", showBackground = true, widthDp = 360, heightDp = 2200, fontScale = 2f)
@Composable
private fun LessonRequestLargePreview() = TeachTheme(darkTheme = false) { CardSamples(true) }
