package com.teachme.feature.mission

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachSpinner
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.model.ResearchPointer
import com.teachme.core.model.ResearchStatus

private val SPINNER_SMALL = 16.dp

// Pointers to the research outside the Fonti tab (resources.md section 7): one sentence and a button that changes
// tab, never a copy of the progress. All of them are silent when there is nothing to say.

/** Under "Confermata il ..." in the confirmed mission card of the chat. */
@Composable
internal fun ResearchCardNote(
    research: ResearchPointer,
    missionVersion: Int,
    onOpenResources: () -> Unit,
) {
    when (research.status) {
        ResearchStatus.None -> Unit
        ResearchStatus.InProgress ->
            PointerText(
                stringResource(R.string.mission_research_card_progress),
                stringResource(R.string.mission_research_see_progress),
                onOpenResources,
            )
        ResearchStatus.Failed ->
            PointerText(
                stringResource(R.string.mission_research_card_failed),
                stringResource(R.string.mission_research_go_sources),
                onOpenResources,
            )
        ResearchStatus.Ready ->
            if (research.isStaleFor(missionVersion)) {
                PointerText(
                    stringResource(R.string.mission_research_stale, research.missionVersion ?: 0),
                    stringResource(R.string.mission_research_go_sources),
                    onOpenResources,
                )
            } else {
                val count = research.resourceCount
                PointerText(
                    if (count != null && count > 0) {
                        pluralStringResource(R.plurals.mission_research_card_found, count, count)
                    } else {
                        stringResource(R.string.mission_research_card_done)
                    },
                    stringResource(R.string.mission_research_see_sources),
                    onOpenResources,
                )
            }
    }
}

@Composable
private fun PointerText(
    text: String,
    action: String,
    onClick: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
        TeachText(text, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
        TeachButton(action, onClick, variant = ButtonVariant.Text)
    }
}

/** In the context pane of the chat (expanded): one quiet line with a text button, never a primary button. */
@Composable
fun ResearchContextNote(
    research: ResearchPointer,
    onOpenResources: () -> Unit,
    modifier: Modifier = Modifier,
) {
    when (research.status) {
        ResearchStatus.None -> Unit
        ResearchStatus.InProgress ->
            Column(modifier, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                ) {
                    TeachSpinner(size = SPINNER_SMALL)
                    TeachText(
                        stringResource(R.string.mission_research_context_progress),
                        style = TeachTheme.typography.ui.bodyS,
                        color = TeachTheme.colors.inkSoft,
                    )
                }
                TeachButton(
                    stringResource(R.string.mission_research_see_progress),
                    onOpenResources,
                    variant = ButtonVariant.Text,
                )
            }
        ResearchStatus.Failed ->
            Column(modifier, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                TeachText(
                    stringResource(R.string.mission_research_card_failed),
                    style = TeachTheme.typography.ui.bodyS,
                    color = TeachTheme.colors.inkSoft,
                )
                TeachButton(
                    stringResource(R.string.mission_research_go_sources),
                    onOpenResources,
                    variant = ButtonVariant.Text,
                )
            }
        ResearchStatus.Ready -> {
            val count = research.resourceCount
            Column(modifier, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                if (count != null && count > 0) {
                    TeachText(
                        pluralStringResource(R.plurals.mission_research_context_ready, count, count),
                        style = TeachTheme.typography.ui.bodyS,
                        color = TeachTheme.colors.inkSoft,
                    )
                }
                TeachButton(
                    stringResource(R.string.mission_research_see_sources),
                    onOpenResources,
                    variant = ButtonVariant.Text,
                )
            }
        }
    }
}

/** "Cosa succede ora" in the Mission tab: replaces the honest note of phase 3; empty once the sources are current. */
@Composable
internal fun ResearchTabNote(
    research: ResearchPointer,
    currentVersion: Int,
    onOpenResources: () -> Unit,
) {
    when {
        research.status == ResearchStatus.InProgress ->
            TabNote(
                stringResource(R.string.mission_research_tab_title),
                stringResource(R.string.mission_research_tab_body),
                stringResource(R.string.mission_research_see_progress),
                onOpenResources,
            )
        research.status == ResearchStatus.Failed ->
            TabNote(
                stringResource(R.string.mission_research_tab_failed_title),
                stringResource(R.string.mission_research_tab_failed_body),
                stringResource(R.string.mission_research_go_sources),
                onOpenResources,
            )
        research.isStaleFor(currentVersion) ->
            TabNote(
                null,
                stringResource(R.string.mission_research_stale, research.missionVersion ?: 0),
                stringResource(R.string.mission_research_go_sources),
                onOpenResources,
            )
    }
}

@Composable
private fun TabNote(
    title: String?,
    body: String,
    action: String,
    onClick: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        if (title != null) {
            TeachText(title, Modifier.semantics { heading() }, style = TeachTheme.typography.ui.title3)
        }
        TeachText(body, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
        TeachButton(action, onClick, variant = ButtonVariant.Text)
    }
}
