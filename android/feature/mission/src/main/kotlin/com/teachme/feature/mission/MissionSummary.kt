package com.teachme.feature.mission

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachDisclosure
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.model.Mission
import com.teachme.core.model.asStartingPoint

private val NUMBER_COLUMN = 28.dp
private val BULLET_COLUMN = 20.dp
private val TAG_HEIGHT = 20.dp

/** The topic name quoted in the invitation is cut at this many characters (mission.md design section 4.6). */
private const val INVITE_TITLE_MAX = 40

/** The "Nuovo" pill (base-components section 13). A word, never just a color. */
@Composable
fun DiffTag(modifier: Modifier = Modifier) {
    Row(
        modifier
            .heightIn(min = TAG_HEIGHT)
            .background(TeachTheme.colors.infoSoft, TeachTheme.shapes.full)
            .padding(horizontal = TeachTheme.spacing.xs)
            .clearAndSetSemantics { },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TeachText(
            stringResource(R.string.mission_diff_new),
            style = TeachTheme.typography.ui.labelS,
            color = TeachTheme.colors.info,
        )
    }
}

@Composable
private fun SectionLabel(
    text: String,
    modifier: Modifier = Modifier,
    tag: Boolean = false,
    description: String? = null,
) {
    Row(
        modifier.semantics(mergeDescendants = true) {
            heading()
            if (description != null) contentDescription = description
        },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        TeachText(
            text,
            style = TeachTheme.typography.ui.label,
            color = TeachTheme.colors.inkSoft,
            modifier = if (description != null) Modifier.clearAndSetSemantics { } else Modifier,
        )
        if (tag) DiffTag()
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CriterionRow(
    index: Int,
    total: Int,
    text: String,
    isNew: Boolean,
) {
    val newSuffix = stringResource(R.string.mission_diff_new_suffix)
    val base = stringResource(R.string.mission_criterion_description, index + 1, total, text)
    Row(Modifier.clearAndSetSemantics { contentDescription = if (isNew) base + newSuffix else base }) {
        TeachText(
            "${index + 1}",
            Modifier.width(NUMBER_COLUMN),
            style = TeachTheme.typography.ui.labelL,
            color = TeachTheme.colors.inkSoft,
        )
        FlowRow(
            Modifier.weight(1f),
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
            itemVerticalAlignment = Alignment.CenterVertically,
        ) {
            TeachText(text, style = TeachTheme.typography.ui.bodyL, color = TeachTheme.colors.ink)
            if (isNew) DiffTag()
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun BulletRow(
    text: String,
    description: String,
    isNew: Boolean,
) {
    val newSuffix = stringResource(R.string.mission_diff_new_suffix)
    Row(Modifier.clearAndSetSemantics { contentDescription = if (isNew) description + newSuffix else description }) {
        TeachText(
            stringResource(R.string.mission_bullet),
            Modifier.width(BULLET_COLUMN),
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkSoft,
        )
        FlowRow(
            Modifier.weight(1f),
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
            itemVerticalAlignment = Alignment.CenterVertically,
        ) {
            TeachText(text, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.ink)
            if (isNew) DiffTag()
        }
    }
}

/** "Tolto dalla versione N": the entries of the confirmed version that the proposal drops (icon + strike). */
@Composable
private fun RemovedSection(diff: MissionDiff) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    val title = stringResource(R.string.mission_diff_removed, diff.previousVersion)
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
        TeachText(
            title,
            Modifier.semantics { heading() },
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkMuted,
        )
        val items: @Composable () -> Unit = {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                diff.removed.forEach { item ->
                    val description = stringResource(R.string.mission_diff_removed_item, diff.previousVersion, item)
                    Row(
                        Modifier.clearAndSetSemantics { contentDescription = description },
                        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                    ) {
                        TeachIcon(
                            TeachIcons.Minus,
                            null,
                            tint = TeachTheme.colors.inkMuted,
                            size = TeachTheme.layout.iconSizeSmall,
                        )
                        TeachText(
                            item,
                            style =
                                TeachTheme.typography.ui.bodyS
                                    .copy(textDecoration = TextDecoration.LineThrough),
                            color = TeachTheme.colors.inkMuted,
                        )
                    }
                }
            }
        }
        if (diff.removed.size > 2) {
            TeachDisclosure(
                pluralStringResource(R.plurals.mission_diff_show_removed, diff.removed.size, diff.removed.size),
                expanded,
                { expanded = !expanded },
            ) { items() }
        } else {
            items()
        }
    }
}

/** The invitation of section 4.6: [topicTitle] names the subject, [onWrite] goes to the chat composer. */
@Immutable
class StartingPointInvite(
    val topicTitle: String,
    val onWrite: () -> Unit,
)

/**
 * The "Punto di partenza" section (mission.md design section 2.5): plain prose, never truncated, one node for
 * screen readers. [change] drives the "Nuovo" pill, the closed "Com'era nella versione N" and the defensive line.
 * Without a text it shows the [invite] when there is one, otherwise nothing.
 */
@Composable
private fun StartingPointSection(
    text: String?,
    change: StartingPointChange,
    previousVersion: Int,
    invite: StartingPointInvite?,
) {
    val label = stringResource(R.string.mission_section_starting_point)
    when {
        change is StartingPointChange.Removed ->
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachIcon(
                    TeachIcons.Minus,
                    null,
                    tint = TeachTheme.colors.inkMuted,
                    size = TeachTheme.layout.iconSizeSmall,
                )
                TeachText(
                    stringResource(R.string.mission_starting_point_removed, previousVersion),
                    style = TeachTheme.typography.ui.bodyS,
                    color = TeachTheme.colors.inkMuted,
                )
            }
        text != null -> {
            val isNew = change is StartingPointChange.Added || change is StartingPointChange.Changed
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                SectionLabel(
                    label,
                    tag = isNew,
                    description = if (isNew) stringResource(R.string.mission_diff_why_description, label) else null,
                )
                TeachText(text, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.ink)
                if (change is StartingPointChange.Changed) {
                    var expanded by rememberSaveable { mutableStateOf(false) }
                    val before =
                        stringResource(R.string.mission_starting_point_before, previousVersion, change.previous)
                    TeachDisclosure(
                        stringResource(R.string.mission_starting_point_was, previousVersion),
                        expanded,
                        { expanded = !expanded },
                    ) {
                        Row(
                            Modifier.fillMaxWidth().clearAndSetSemantics { contentDescription = before },
                            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                        ) {
                            TeachIcon(
                                TeachIcons.History,
                                null,
                                tint = TeachTheme.colors.inkMuted,
                                size = TeachTheme.layout.iconSizeSmall,
                            )
                            TeachText(
                                change.previous,
                                style = TeachTheme.typography.ui.bodyS,
                                color = TeachTheme.colors.inkMuted,
                            )
                        }
                    }
                }
            }
        }
        invite != null ->
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                SectionLabel(label)
                TeachText(
                    stringResource(
                        R.string.mission_starting_point_invite,
                        invite.topicTitle.ellipsize(INVITE_TITLE_MAX),
                    ),
                    style = TeachTheme.typography.ui.body,
                    color = TeachTheme.colors.inkSoft,
                )
                TeachButton(
                    stringResource(R.string.mission_starting_point_write),
                    invite.onWrite,
                    variant = ButtonVariant.Text,
                    contentDescription =
                        stringResource(
                            R.string.mission_starting_point_write_description,
                            invite.topicTitle.ellipsize(INVITE_TITLE_MAX),
                        ),
                )
            }
    }
}

/**
 * Read-only sections of a mission (base-components section 13): why, numbered success criteria, constraints,
 * out of scope; empty sections are left out. With [highlight] the criteria sit on `accentSoft` (the thing to
 * judge), otherwise on `surfaceSunken`. With a [diff] new entries carry a "Nuovo" pill and removed ones are listed.
 * [showWhy] false and [whyMaxLines] serve the compact confirmed card; [showStartingPoint] false leaves the starting
 * point out of it. A [startingPointInvite] shows the invitation of section 4.6 when the mission has no starting point.
 */
@Composable
fun MissionSummary(
    mission: Mission,
    modifier: Modifier = Modifier,
    highlight: Boolean = true,
    diff: MissionDiff? = null,
    showWhy: Boolean = true,
    whyMaxLines: Int = Int.MAX_VALUE,
    showSecondary: Boolean = true,
    showStartingPoint: Boolean = true,
    startingPointInvite: StartingPointInvite? = null,
) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md)) {
        if (showWhy) {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                val whyLabel = stringResource(R.string.mission_section_why)
                SectionLabel(
                    whyLabel,
                    tag = diff?.whyChanged == true,
                    description =
                        if (diff?.whyChanged ==
                            true
                        ) {
                            stringResource(R.string.mission_diff_why_description, whyLabel)
                        } else {
                            null
                        },
                )
                TeachText(
                    mission.why,
                    style = TeachTheme.typography.ui.bodyL,
                    color = TeachTheme.colors.ink,
                    maxLines = whyMaxLines,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (showStartingPoint) {
            StartingPointSection(
                mission.startingPoint.asStartingPoint(),
                diff?.startingPoint ?: StartingPointChange.Unchanged,
                diff?.previousVersion ?: 0,
                startingPointInvite,
            )
        }
        if (showSecondary) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .background(
                        if (highlight) TeachTheme.colors.accentSoft else TeachTheme.colors.surfaceSunken,
                        TeachTheme.shapes.sm,
                    ).padding(TeachTheme.spacing.md),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachText(
                    stringResource(R.string.mission_section_criteria),
                    Modifier.semantics { heading() },
                    style = TeachTheme.typography.ui.title3,
                    color = TeachTheme.colors.ink,
                )
                mission.successCriteria.forEachIndexed { index, text ->
                    CriterionRow(index, mission.successCriteria.size, text, diff?.newCriteria?.contains(index) == true)
                }
            }
            if (mission.constraints.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                    SectionLabel(stringResource(R.string.mission_section_constraints))
                    mission.constraints.forEachIndexed { index, text ->
                        BulletRow(
                            text,
                            stringResource(R.string.mission_constraint_description, text),
                            diff?.newConstraints?.contains(index) == true,
                        )
                    }
                }
            }
            if (mission.outOfScope.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                    SectionLabel(stringResource(R.string.mission_section_out_of_scope))
                    mission.outOfScope.forEachIndexed { index, text ->
                        BulletRow(
                            text,
                            stringResource(R.string.mission_out_of_scope_description, text),
                            diff?.newOutOfScope?.contains(index) == true,
                        )
                    }
                }
            }
            if (diff != null && diff.removed.isNotEmpty()) RemovedSection(diff)
        }
    }
}

@Composable
private fun SummaryPreviewContent() {
    Column(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.lg),
    ) {
        MissionSummary(PreviewData.draft2)
        MissionSummary(
            PreviewData.draft2,
            highlight = false,
            diff = missionDiff(PreviewData.draft2, PreviewData.confirmed1),
        )
        // Starting point added to a mission saved before: pill, no "before" text.
        MissionSummary(
            PreviewData.draftAddsStart,
            diff = missionDiff(PreviewData.draftAddsStart, PreviewData.confirmedNoStart),
        )
        // Defensive line: the draft lost the starting point.
        MissionSummary(
            PreviewData.confirmedNoStart,
            diff = missionDiff(PreviewData.confirmedNoStart, PreviewData.confirmed1),
        )
        // Invitation of the Mission tab, and the plain version without one.
        MissionSummary(PreviewData.confirmedNoStart, startingPointInvite = StartingPointInvite("Rust") {})
        MissionSummary(PreviewData.confirmedNoStart)
    }
}

@Preview(name = "Summary light", showBackground = true, widthDp = 600, heightDp = 2600)
@Composable
private fun SummaryLightPreview() = TeachTheme(darkTheme = false) { SummaryPreviewContent() }

@Preview(name = "Summary dark", showBackground = true, widthDp = 600, heightDp = 2600)
@Composable
private fun SummaryDarkPreview() = TeachTheme(darkTheme = true) { SummaryPreviewContent() }
