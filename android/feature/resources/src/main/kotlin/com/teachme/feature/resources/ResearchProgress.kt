package com.teachme.feature.resources

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachSpinner
import com.teachme.core.designsystem.components.TeachSurface
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.model.JobStatus
import com.teachme.core.model.TopicJob
import kotlinx.coroutines.delay
import java.time.Instant

private val PHASE_MIN_HEIGHT = 40.dp
private val DETAIL_INDENT = 28.dp
private const val CLOCK_TICK_MS = 30_000L

private fun ResearchPhase.label(): Int =
    when (this) {
        ResearchPhase.Planning -> R.string.res_phase_planning
        ResearchPhase.Searching -> R.string.res_phase_searching
        ResearchPhase.Reading -> R.string.res_phase_reading
        ResearchPhase.Evaluating -> R.string.res_phase_evaluating
        ResearchPhase.Saving -> R.string.res_phase_saving
    }

/**
 * The progress of the search (resources.md 4.2): five fixed phases in words, one of them current, no percentage and no
 * bar. A step this app does not know shows a single spinner with the backend's own sentence.
 */
@Composable
internal fun ResearchProgress(
    job: TopicJob?,
    completing: Boolean,
    refreshing: Boolean,
    missionWhy: String?,
    modifier: Modifier = Modifier,
) {
    val now by produceState(Instant.now(), job?.id) {
        while (true) {
            delay(CLOCK_TICK_MS)
            value = Instant.now()
        }
    }
    val phases = phaseStates(job, completing)
    val unknownStep =
        job != null &&
            job.status == JobStatus.Running &&
            !completing &&
            ResearchPhase.fromStep(job.progress?.step) == null
    val padding = if (currentWindowWidthClass() == WindowWidthClass.Compact) TeachTheme.spacing.md else 20.dp
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
        TeachText(
            stringResource(if (refreshing) R.string.res_progress_title_refresh else R.string.res_progress_title),
            Modifier.semantics { heading() },
            style = TeachTheme.typography.ui.title2,
        )
        if (!missionWhy.isNullOrBlank()) {
            TeachText(
                stringResource(R.string.res_progress_mission, missionWhy),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkSoft,
                maxLines = WHY_LINES,
                overflow = TextOverflow.Ellipsis,
            )
        }
        TeachSurface(
            Modifier.fillMaxWidth(),
            border = BorderStroke(TeachTheme.stroke.hairline, TeachTheme.colors.line),
        ) {
            Column(
                Modifier.fillMaxWidth().padding(padding),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
            ) {
                if (unknownStep) {
                    PhaseRow(
                        stringResource(R.string.res_phase_generic),
                        PhaseState.Current,
                        job.progress?.message,
                    )
                } else {
                    phases.forEach { (phase, state) ->
                        val detail =
                            when {
                                state != PhaseState.Current -> null
                                job?.status == JobStatus.Queued || job == null ->
                                    stringResource(
                                        R.string.res_phase_queued,
                                    )
                                else -> job.progress?.message?.takeIf { it.isNotBlank() }
                            }
                        PhaseRow(stringResource(phase.label()), state, detail)
                    }
                }
            }
        }
        TeachText(
            stringResource(if (isSlow(job, now)) R.string.res_progress_footer_slow else R.string.res_progress_footer),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
        PhaseAnnouncer(job, completing || job?.status == JobStatus.Succeeded)
    }
}

private const val WHY_LINES = 3

@Composable
private fun PhaseRow(
    label: String,
    state: PhaseState,
    detail: String?,
) {
    val colors = TeachTheme.colors
    val stateText =
        stringResource(
            when (state) {
                PhaseState.Done -> R.string.res_phase_done
                PhaseState.Current -> R.string.res_phase_current
                PhaseState.Todo -> R.string.res_phase_todo
            },
        )
    Column(
        Modifier
            .fillMaxWidth()
            .heightIn(min = PHASE_MIN_HEIGHT)
            .semantics(mergeDescendants = true) { stateDescription = stateText },
        verticalArrangement = Arrangement.Center,
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            val size = TeachTheme.layout.iconSizeSmall
            when (state) {
                PhaseState.Done -> TeachIcon(TeachIcons.Check, null, tint = colors.correct, size = size)
                PhaseState.Current -> TeachSpinner(size = size)
                PhaseState.Todo -> TeachIcon(TeachIcons.Circle, null, tint = colors.inkMuted, size = size)
            }
            TeachText(
                label,
                style =
                    when (state) {
                        PhaseState.Current -> TeachTheme.typography.ui.bodyL
                        else -> TeachTheme.typography.ui.body
                    },
                color =
                    when (state) {
                        PhaseState.Done -> colors.inkSoft
                        PhaseState.Current -> colors.ink
                        PhaseState.Todo -> colors.inkMuted
                    },
            )
        }
        if (detail != null) {
            TeachText(
                detail,
                Modifier.padding(start = DETAIL_INDENT),
                style = TeachTheme.typography.ui.bodyS,
                color = colors.inkSoft,
            )
        }
    }
}

/**
 * Invisible polite live region: it speaks only the change of phase ("Ricerca: cerco sul web."), never the details or
 * the time, and never moves the focus (resources.md 8.4).
 */
@Composable
private fun PhaseAnnouncer(
    job: TopicJob?,
    finished: Boolean,
) {
    if (finished) return
    val phase = ResearchPhase.fromStep(job?.progress?.step) ?: return
    val text =
        stringResource(R.string.res_announce_phase, stringResource(phase.label()).replaceFirstChar { it.lowercase() })
    key(phase) {
        Box(
            Modifier.size(1.dp).semantics {
                liveRegion = LiveRegionMode.Polite
                contentDescription = text
            },
        )
    }
}

@Composable
private fun ProgressSamples() {
    Column(
        Modifier.padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.lg),
    ) {
        ResearchProgress(PreviewData.job(JobStatus.Queued), false, false, PreviewData.WHY)
        ResearchProgress(
            PreviewData.job(JobStatus.Running, "searching", "Ho consultato 14 risultati"),
            false,
            true,
            PreviewData.WHY,
        )
        ResearchProgress(
            PreviewData.job(JobStatus.Running, "future_step", "Leggo doc.rust-lang.org"),
            false,
            false,
            null,
        )
        ResearchProgress(PreviewData.job(JobStatus.Succeeded), true, false, PreviewData.WHY)
    }
}

@Preview(name = "Progress light", showBackground = true, widthDp = 640, heightDp = 1400)
@Composable
private fun ProgressLightPreview() = TeachTheme(darkTheme = false) { ProgressSamples() }

@Preview(name = "Progress dark", showBackground = true, widthDp = 640, heightDp = 1400)
@Composable
private fun ProgressDarkPreview() = TeachTheme(darkTheme = true) { ProgressSamples() }

@Preview(name = "Progress compact font 2.0", showBackground = true, widthDp = 360, heightDp = 2600, fontScale = 2f)
@Composable
private fun ProgressLargePreview() = TeachTheme(darkTheme = false) { ProgressSamples() }
