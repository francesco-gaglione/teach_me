package com.teachme.feature.mission

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import com.teachme.core.designsystem.components.TeachTabRow

/** The contents of a Topic screen, in tab order (topic.md F1): Lezioni · Chat · Cosa sai · Fonti · Mission. */
enum class TopicTab { Lessons, Chat, Progress, Resources, Mission }

/** `TabRow` of the Topic screen (topic.md F1), scrollable, shown on every width class. */
@Composable
fun MissionTabs(
    selected: TopicTab,
    onSelect: (TopicTab) -> Unit,
    modifier: Modifier = Modifier,
    startPadding: Dp = Dp.Hairline,
) {
    val tabs = TopicTab.entries
    TeachTabRow(
        labels =
            tabs.map {
                stringResource(
                    when (it) {
                        TopicTab.Lessons -> R.string.mission_tab_lessons
                        TopicTab.Chat -> R.string.mission_tab_chat
                        TopicTab.Progress -> R.string.mission_tab_progress
                        TopicTab.Resources -> R.string.mission_tab_resources
                        TopicTab.Mission -> R.string.mission_tab_mission
                    },
                )
            },
        selectedIndex = tabs.indexOf(selected),
        onSelect = { onSelect(tabs[it]) },
        modifier = modifier,
        startPadding = startPadding,
    )
}
