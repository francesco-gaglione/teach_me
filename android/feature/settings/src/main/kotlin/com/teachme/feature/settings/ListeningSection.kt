package com.teachme.feature.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachCard
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachSegmentedSwitch
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.core.model.PlaybackSpeed

/** Taps of the "Ascolto" section. */
data class ListeningActions(
    val onSpeed: (PlaybackSpeed) -> Unit = {},
    val onVoice: (String) -> Unit = {},
    val onSample: (String) -> Unit = {},
)

/** The four speed labels do not fit in a row at 360 dp from this font scale on (lesson-audio.md section 4). */
internal const val SPEED_STACK_FONT_SCALE = 1.3f

internal fun speedStacked(fontScale: Float): Boolean = fontScale >= SPEED_STACK_FONT_SCALE

/** Content of the "Ascolto" card: speed (device) then voice (account). The caller draws the section title. */
@Composable
internal fun ListeningCard(
    state: ListeningUiState,
    actions: ListeningActions,
) {
    TeachCard(Modifier.fillMaxWidth()) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
            SpeedChoice(state, actions)
            VoiceChoice(state, actions)
        }
    }
}

@Composable
private fun SpeedChoice(
    state: ListeningUiState,
    actions: ListeningActions,
) {
    val labels =
        listOf(
            R.string.settings_listen_speed_075,
            R.string.settings_listen_speed_1,
            R.string.settings_listen_speed_125,
            R.string.settings_listen_speed_15,
        ).map { stringResource(it) }
    val descriptions =
        listOf(
            R.string.settings_listen_speed_075_a11y,
            R.string.settings_listen_speed_1_a11y,
            R.string.settings_listen_speed_125_a11y,
            R.string.settings_listen_speed_15_a11y,
        ).map { stringResource(it) }
    TeachText(
        stringResource(R.string.settings_listen_speed),
        style = TeachTheme.typography.ui.label,
        color = TeachTheme.colors.ink,
    )
    TeachSegmentedSwitch(
        labels = labels,
        selectedIndex = state.speed.ordinal,
        onSelect = { actions.onSpeed(PlaybackSpeed.entries[it]) },
        descriptions = descriptions,
        stacked = speedStacked(LocalDensity.current.fontScale),
    )
    TeachText(
        stringResource(R.string.settings_listen_speed_note),
        style = TeachTheme.typography.ui.bodyS,
        color = TeachTheme.colors.inkSoft,
    )
}

@Composable
private fun VoiceChoice(
    state: ListeningUiState,
    actions: ListeningActions,
) {
    Column(
        Modifier.padding(top = TeachTheme.spacing.sm),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        TeachText(
            stringResource(R.string.settings_listen_voice),
            style = TeachTheme.typography.ui.label,
            color = TeachTheme.colors.ink,
        )
        Column(Modifier.selectableGroup()) {
            state.voices.forEach { voice ->
                VoiceRow(
                    voice = voice,
                    selected = voice.id == state.selectedVoice,
                    saving = voice.id == state.savingVoice,
                    onSelect = { actions.onVoice(voice.id) },
                    onSample = { actions.onSample(voice.id) },
                )
            }
        }
        TeachText(
            stringResource(R.string.settings_listen_voice_note),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

/**
 * One voice: the row selects it, the "Prova" button next to it only plays the sample. Selected is shown by the radio
 * icon, an `accentSoft` fill and the spoken state, not by color alone.
 */
@Composable
private fun VoiceRow(
    voice: VoiceRowUi,
    selected: Boolean,
    saving: Boolean,
    onSelect: () -> Unit,
    onSample: () -> Unit,
) {
    val colors = TeachTheme.colors
    val shape = TeachTheme.shapes.md
    val interaction = rememberInteractionState()
    val savingText = stringResource(R.string.settings_listen_saving)
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = TeachTheme.layout.choiceMinHeight)
            .background(if (selected) colors.accentSoft else androidx.compose.ui.graphics.Color.Transparent, shape),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier
                .weight(1f)
                .heightIn(min = TeachTheme.layout.choiceMinHeight)
                .focusRing(interaction.focused, shape)
                .selectable(
                    selected = selected,
                    interactionSource = interaction.source,
                    indication = null,
                    role = Role.RadioButton,
                    onClick = onSelect,
                ).semantics { if (saving) stateDescription = savingText }
                .padding(start = TeachTheme.spacing.sm),
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TeachIcon(
                if (selected) TeachIcons.CircleDot else TeachIcons.Circle,
                contentDescription = null,
                tint = if (selected) colors.accent else colors.border,
            )
            TeachText(voice.label, style = TeachTheme.typography.ui.bodyL, color = colors.ink)
        }
        SampleButton(voice, onSample)
    }
}

@Composable
private fun SampleButton(
    voice: VoiceRowUi,
    onClick: () -> Unit,
) {
    val (textRes, a11yRes, icon) =
        when (voice.sample) {
            SampleState.Ready ->
                Triple(
                    R.string.settings_listen_try,
                    R.string.settings_listen_try_a11y,
                    TeachIcons.Volume2,
                )
            SampleState.Playing ->
                Triple(
                    R.string.settings_listen_stop,
                    R.string.settings_listen_stop_a11y,
                    TeachIcons.Square,
                )
            SampleState.Preparing ->
                Triple(R.string.settings_listen_preparing, R.string.settings_listen_preparing_a11y, TeachIcons.Clock)
            SampleState.Unavailable ->
                Triple(
                    R.string.settings_listen_unavailable,
                    R.string.settings_listen_unavailable_a11y,
                    TeachIcons.MinusCircle,
                )
        }
    val enabled = voice.sample == SampleState.Ready || voice.sample == SampleState.Playing
    Box(Modifier.padding(horizontal = TeachTheme.spacing.xxs)) {
        TeachButton(
            text = stringResource(textRes),
            onClick = onClick,
            variant = ButtonVariant.Text,
            enabled = enabled,
            leadingIcon = icon,
            contentDescription = stringResource(a11yRes, voice.label),
        )
    }
}

// region Previews

private val previewVoices =
    listOf(
        VoiceRowUi("vivian", "Vivian", SampleState.Ready),
        VoiceRowUi("serena", "Serena", SampleState.Preparing),
        VoiceRowUi("aiden", "Aiden", SampleState.Unavailable),
        VoiceRowUi("ono", "Ono", SampleState.Ready),
    )
private val previewListening =
    ListeningUiState(visible = true, voices = previewVoices, selectedVoice = "vivian")

@Composable
private fun PreviewHost(
    state: ListeningUiState,
    dark: Boolean = false,
) {
    TeachTheme(darkTheme = dark) {
        Box(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
            ListeningCard(state, ListeningActions())
        }
    }
}

@Preview(name = "Listening", showBackground = true)
@Composable
private fun PreviewListening() = PreviewHost(previewListening)

@Preview(name = "Listening dark", showBackground = true)
@Composable
private fun PreviewListeningDark() = PreviewHost(previewListening, dark = true)

@Preview(name = "Listening playing, 1.5x", showBackground = true)
@Composable
private fun PreviewListeningPlaying() =
    PreviewHost(
        previewListening.copy(
            speed = PlaybackSpeed.X15,
            voices = previewVoices.mapIndexed { i, v -> if (i == 3) v.copy(sample = SampleState.Playing) else v },
        ),
    )

@Preview(name = "Listening saving", showBackground = true)
@Composable
private fun PreviewListeningSaving() = PreviewHost(previewListening.copy(selectedVoice = "ono", savingVoice = "ono"))

@Preview(name = "Listening font 1.5 (stacked)", showBackground = true, fontScale = 1.5f)
@Composable
private fun PreviewListeningLargeFont() = PreviewHost(previewListening)

@Preview(name = "Listening font 1.5 dark", showBackground = true, fontScale = 1.5f)
@Composable
private fun PreviewListeningLargeFontDark() = PreviewHost(previewListening, dark = true)

// endregion
