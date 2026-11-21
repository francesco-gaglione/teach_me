package com.teachme.feature.chat

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachSpinner
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.model.TopicMission
import com.teachme.feature.mission.MissionChatContext
import com.teachme.feature.mission.ResearchContextNote
import kotlinx.coroutines.delay

private const val SLOW_LOADING_MS = 8_000L
private const val CHIP_FONT_SCALE_COLUMN = 1.3f
private const val CONTEXT_WHY_LINES = 4
private val CONTEXT_NUMBER_COLUMN = 24.dp
private val EMPTY_TOP_PADDING = 48.dp

/**
 * Side panel of the expanded layout (chat.md 2, mission.md design 4.5): a short summary of the mission, never
 * the full view. It scrolls on its own when taller than the window and never holds a primary button.
 */
@Composable
internal fun ContextPane(
    topicTitle: String,
    mission: MissionChatContext,
    onGoToProposal: () -> Unit,
    modifier: Modifier = Modifier,
    /** The chat was opened from this lesson (ask_agent.md): a block "Lezione" comes first. */
    lesson: ChatLessonContext? = null,
    onBackToLesson: (() -> Unit)? = null,
) {
    val current = mission.mission?.current
    val draft = mission.mission?.draft
    Column(
        modifier.width(TeachTheme.layout.contextPaneWidth).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        if (lesson != null && onBackToLesson != null) {
            LessonPaneBlock(lesson, onBackToLesson)
        }
        TeachText(
            stringResource(R.string.chat_context_title),
            Modifier.semantics { heading() },
            style = TeachTheme.typography.ui.title3,
        )
        when {
            current != null -> {
                if (draft != null) {
                    TeachStatusPanel(
                        StatusVariant.Info,
                        stringResource(R.string.chat_context_change_pending),
                        announce = false,
                    ) {
                        TeachButton(
                            stringResource(R.string.chat_context_go_proposal),
                            onGoToProposal,
                            variant = ButtonVariant.Text,
                        )
                    }
                }
                TeachText(
                    stringResource(R.string.chat_context_version, current.version),
                    style = TeachTheme.typography.ui.bodyS,
                    color = TeachTheme.colors.inkSoft,
                )
                TeachText(
                    current.why,
                    style = TeachTheme.typography.ui.body,
                    color = TeachTheme.colors.ink,
                    maxLines = CONTEXT_WHY_LINES,
                    overflow = TextOverflow.Ellipsis,
                )
                TeachText(
                    stringResource(R.string.chat_context_criteria),
                    Modifier.semantics { heading() }.padding(top = TeachTheme.spacing.xs),
                    style = TeachTheme.typography.ui.label,
                    color = TeachTheme.colors.inkSoft,
                )
                current.successCriteria.forEachIndexed { index, text ->
                    val description =
                        stringResource(R.string.chat_context_criterion, index + 1, current.successCriteria.size, text)
                    Row(Modifier.clearAndSetSemantics { contentDescription = description }) {
                        TeachText(
                            "${index + 1}",
                            Modifier.width(CONTEXT_NUMBER_COLUMN),
                            style = TeachTheme.typography.ui.label,
                            color = TeachTheme.colors.inkSoft,
                        )
                        TeachText(text, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.ink)
                    }
                }
                ResearchContextNote(mission.research, mission.onOpenResources)
                TeachButton(
                    stringResource(R.string.chat_context_open_mission),
                    mission.onOpenMission,
                    variant = ButtonVariant.Text,
                )
            }
            draft != null -> {
                TeachText(
                    stringResource(R.string.chat_context_draft_body, draft.version),
                    style = TeachTheme.typography.ui.body,
                    color = TeachTheme.colors.inkSoft,
                )
                TeachButton(
                    stringResource(R.string.chat_context_go_proposal),
                    onGoToProposal,
                    variant = ButtonVariant.Text,
                )
            }
            else ->
                TeachText(
                    stringResource(R.string.chat_context_body, topicTitle),
                    style = TeachTheme.typography.ui.body,
                    color = TeachTheme.colors.inkSoft,
                )
        }
    }
}

/**
 * Empty chat (chat.md 9.1): invites the user to say why they want to learn the topic. The chips only
 * insert the start of a sentence in the composer, they never send.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ChatEmptyState(
    topicTitle: String,
    showSuggestions: Boolean,
    onSuggestion: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val suggestions =
        listOf(
            stringResource(R.string.chat_suggestion_work),
            stringResource(R.string.chat_suggestion_project),
            stringResource(R.string.chat_suggestion_exam),
        )
    Column(
        modifier.fillMaxWidth().padding(top = EMPTY_TOP_PADDING),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        TeachText(
            stringResource(R.string.chat_empty_title),
            Modifier.semantics { heading() },
            style = TeachTheme.typography.ui.title2,
        )
        TeachText(
            stringResource(R.string.chat_empty_body, topicTitle),
            style = TeachTheme.typography.ui.bodyL,
            color = TeachTheme.colors.ink,
        )
        if (showSuggestions) {
            TeachText(
                stringResource(R.string.chat_empty_examples),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkSoft,
            )
            if (LocalDensity.current.fontScale > CHIP_FONT_SCALE_COLUMN) {
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                    suggestions.forEach { TeachButton(it, { onSuggestion(it) }, variant = ButtonVariant.Secondary) }
                }
            } else {
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                    verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                ) {
                    suggestions.forEach { TeachButton(it, { onSuggestion(it) }, variant = ButtonVariant.Secondary) }
                }
            }
        }
    }
}

/** Skeleton of the first load (chat.md 4.4): three bars, no shimmer. */
@Composable
internal fun ChatSkeleton(modifier: Modifier = Modifier) {
    var slow by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(SLOW_LOADING_MS)
        slow = true
    }
    val description = stringResource(R.string.chat_loading)
    Column(
        modifier.fillMaxWidth().padding(vertical = TeachTheme.spacing.md).semantics { stateDescription = description },
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        Bar(0.7f, Alignment.Start)
        Bar(0.45f, Alignment.End)
        Bar(1f, Alignment.Start)
        if (slow) {
            TeachText(
                stringResource(R.string.chat_loading_long),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
        }
    }
}

@Composable
private fun Bar(
    fraction: Float,
    align: Alignment.Horizontal,
) {
    Column(Modifier.fillMaxWidth().clearAndSetSemantics { }, horizontalAlignment = align) {
        Box(
            Modifier
                .fillMaxWidth(fraction)
                .height(14.dp)
                .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.sm),
        )
    }
}

/** Row at the top of the list while older messages load or fail to load (chat.md 4.4). */
@Composable
internal fun OlderMessagesRow(
    failed: Boolean,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier.fillMaxWidth().padding(bottom = TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        if (failed) {
            TeachText(
                stringResource(R.string.chat_older_failed),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkMuted,
            )
            TeachButton(stringResource(R.string.chat_older_retry), onRetry, variant = ButtonVariant.Text)
        } else {
            Row(
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TeachSpinner(size = TeachTheme.layout.iconSizeSmall)
                TeachText(
                    stringResource(R.string.chat_older_loading),
                    style = TeachTheme.typography.ui.bodyS,
                    color = TeachTheme.colors.inkMuted,
                )
            }
        }
    }
}

@Composable
private fun PanelsPreviewContent() {
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        ChatEmptyState("Rust", showSuggestions = true, onSuggestion = {})
        ContextPane("Rust", previewMissionContext(), {}, Modifier.padding(top = TeachTheme.spacing.lg))
        ContextPane(
            "Rust",
            previewMissionContext(TopicMission(PreviewMissions.confirmed, PreviewMissions.draft)),
            {},
            Modifier.padding(top = TeachTheme.spacing.lg),
        )
        ChatSkeleton()
    }
}

@Preview(name = "Empty light", showBackground = true, widthDp = 360, heightDp = 900)
@Composable
private fun PanelsLightPreview() = TeachTheme(darkTheme = false) { PanelsPreviewContent() }

@Preview(name = "Empty dark", showBackground = true, widthDp = 360, heightDp = 900)
@Composable
private fun PanelsDarkPreview() = TeachTheme(darkTheme = true) { PanelsPreviewContent() }
