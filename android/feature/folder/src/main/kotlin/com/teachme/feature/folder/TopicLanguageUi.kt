package com.teachme.feature.folder

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachSegmentedSwitch
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.model.TopicLanguage

/** Name of a content language in its own language, so it reads the same under any interface language. */
@Composable
internal fun topicLanguageName(language: TopicLanguage): String =
    stringResource(
        when (language) {
            TopicLanguage.It -> R.string.folder_language_it
            TopicLanguage.En -> R.string.folder_language_en
            TopicLanguage.Unknown -> R.string.folder_language_unknown
        },
    )

/** Choice of the content language of a new Topic, shown inside the create dialog (ADR 0019). */
@Composable
internal fun ColumnScope.TopicLanguagePicker(
    selected: TopicLanguage,
    onSelect: (TopicLanguage) -> Unit,
    enabled: Boolean = true,
) {
    val languages = TopicLanguage.selectable
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachText(
            stringResource(R.string.folder_language_label),
            style = TeachTheme.typography.ui.label,
            color = TeachTheme.colors.inkSoft,
        )
        TeachSegmentedSwitch(
            labels = languages.map { topicLanguageName(it) },
            selectedIndex = languages.indexOf(selected),
            onSelect = { if (enabled) onSelect(languages[it]) },
        )
    }
}

@Preview(name = "Language picker light", showBackground = true)
@Composable
private fun LanguagePickerLightPreview() =
    TeachTheme(darkTheme = false) { Column { TopicLanguagePicker(TopicLanguage.It, {}) } }

@Preview(name = "Language picker dark, English", showBackground = true)
@Composable
private fun LanguagePickerDarkPreview() =
    TeachTheme(darkTheme = true) { Column { TopicLanguagePicker(TopicLanguage.En, {}) } }
