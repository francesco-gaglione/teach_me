package com.teachme.feature.mission

import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.DialogErrorPanel
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachConfirmDialog
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.model.Preference
import com.teachme.core.model.PreferenceScope

private val ROW_MIN_HEIGHT = 56.dp
private val SKELETON_BAR = 14.dp

/**
 * Preferences of the Topic (mission.md design section 4.4): the Tutor saves them, the user can only read and
 * delete them. [titleFocus] receives the focus after a deletion. Sections are independent of the mission.
 */
@Composable
fun PreferencesSection(
    state: MissionUiState,
    onRetry: () -> Unit,
    onDelete: (Preference) -> Unit,
    modifier: Modifier = Modifier,
    titleFocus: FocusRequester = FocusRequester(),
) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachText(
            stringResource(R.string.mission_prefs_title),
            Modifier.semantics { heading() }.focusRequester(titleFocus).focusable(),
            style = TeachTheme.typography.ui.title3,
        )
        TeachText(
            stringResource(R.string.mission_prefs_intro),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
        when (state.prefsLoad) {
            PreferencesLoad.Loading -> PreferencesSkeleton()
            PreferencesLoad.Failed ->
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                    TeachText(
                        stringResource(R.string.mission_prefs_failed),
                        style = TeachTheme.typography.ui.bodyS,
                        color = TeachTheme.colors.inkSoft,
                    )
                    TeachButton(
                        stringResource(R.string.mission_prefs_retry),
                        onRetry,
                        variant = ButtonVariant.Text,
                    )
                }
            PreferencesLoad.Ready -> PreferenceGroups(state.preferences, onDelete)
        }
    }
}

@Composable
private fun PreferenceGroups(
    preferences: List<Preference>,
    onDelete: (Preference) -> Unit,
) {
    if (preferences.isEmpty()) {
        TeachText(
            stringResource(R.string.mission_prefs_empty),
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkSoft,
        )
        return
    }
    val topic = preferences.filter { it.scope == PreferenceScope.Topic }
    val global = preferences.filter { it.scope == PreferenceScope.Global }
    if (topic.isNotEmpty()) {
        PreferenceGroup(stringResource(R.string.mission_prefs_topic), topic, onDelete)
    }
    if (global.isNotEmpty()) {
        PreferenceGroup(stringResource(R.string.mission_prefs_global), global, onDelete)
        TeachText(
            stringResource(R.string.mission_prefs_global_hint),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

@Composable
private fun PreferenceGroup(
    label: String,
    items: List<Preference>,
    onDelete: (Preference) -> Unit,
) {
    Column(Modifier.padding(top = TeachTheme.spacing.xs)) {
        TeachText(
            label,
            Modifier.semantics { heading() },
            style = TeachTheme.typography.ui.label,
            color = TeachTheme.colors.inkSoft,
        )
        items.forEachIndexed { index, preference ->
            if (index >
                0
            ) {
                Box(Modifier.fillMaxWidth().height(TeachTheme.stroke.hairline).background(TeachTheme.colors.line))
            }
            PreferenceRow(preference, { onDelete(preference) })
        }
    }
}

/** Text (selectable, wraps without ellipsis) and a 48 dp trash button; TalkBack also gets an "Elimina" action. */
@Composable
fun PreferenceRow(
    preference: Preference,
    onDelete: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val deleteLabel = stringResource(R.string.mission_pref_delete_action)
    val description =
        stringResource(R.string.mission_pref_delete_description, preference.text.ellipsize(PREFERENCE_QUOTE_MAX))
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = ROW_MIN_HEIGHT)
            .semantics {
                customActions =
                    listOf(
                        CustomAccessibilityAction(deleteLabel) {
                            onDelete()
                            true
                        },
                    )
            },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        SelectionContainer(Modifier.weight(1f)) {
            TeachText(
                preference.text,
                Modifier.padding(vertical = TeachTheme.spacing.xs),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.ink,
            )
        }
        TeachIconButton(TeachIcons.Trash, description, onDelete)
    }
}

@Composable
private fun PreferencesSkeleton() {
    val loading = stringResource(R.string.mission_prefs_loading)
    Column(
        Modifier.fillMaxWidth().semantics { stateDescription = loading },
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        listOf(0.8f, 0.55f).forEach { fraction ->
            Box(
                Modifier
                    .fillMaxWidth(fraction)
                    .height(SKELETON_BAR)
                    .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.sm),
            )
        }
    }
}

/** "Eliminare questa preferenza?" (base-components section 8): "Annulla" has the initial focus. */
@Composable
fun DeletePreferenceDialog(
    dialog: PreferenceDialog,
    topicTitle: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val text = dialog.preference.text
    val body =
        if (dialog.preference.scope == PreferenceScope.Topic) {
            stringResource(R.string.mission_delete_body_topic, text, topicTitle)
        } else {
            // Visual design bolds "nessun argomento"; the dialog component takes a plain string (proposal in the report).
            stringResource(R.string.mission_delete_body_global, text)
        }
    TeachConfirmDialog(
        title = stringResource(R.string.mission_delete_title),
        body = body,
        confirmLabel = stringResource(R.string.mission_delete_confirm),
        dismissLabel = stringResource(R.string.mission_delete_cancel),
        onConfirm = onConfirm,
        onDismiss = onDismiss,
        destructive = true,
        loading = dialog.submitting,
        loadingLabel = stringResource(R.string.mission_delete_loading),
        error =
            if (dialog.failed) {
                DialogErrorPanel(
                    stringResource(R.string.mission_delete_error_title),
                    stringResource(R.string.mission_delete_error_body),
                )
            } else {
                null
            },
    )
}

@Composable
private fun PreferencesPreviewContent(state: MissionUiState) {
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        PreferencesSection(state, {}, {})
    }
}

private val previewPrefs = PreviewData.prefTopic to PreviewData.prefGlobal

@Preview(name = "Preferences light", showBackground = true, widthDp = 300, heightDp = 600)
@Composable
private fun PreferencesLightPreview() =
    TeachTheme(darkTheme = false) {
        PreferencesPreviewContent(
            MissionUiState(
                prefsLoad = PreferencesLoad.Ready,
                preferences = listOf(previewPrefs.first, previewPrefs.second),
            ),
        )
    }

@Preview(name = "Preferences dark empty", showBackground = true, widthDp = 300, heightDp = 400)
@Composable
private fun PreferencesDarkPreview() =
    TeachTheme(darkTheme = true) { PreferencesPreviewContent(MissionUiState(prefsLoad = PreferencesLoad.Ready)) }
