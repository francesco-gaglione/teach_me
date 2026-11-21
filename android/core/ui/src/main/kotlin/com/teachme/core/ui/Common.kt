package com.teachme.core.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.components.ChipVariant
import com.teachme.core.designsystem.components.TeachErrorState
import com.teachme.core.designsystem.components.TeachStatusChip
import com.teachme.core.model.FeatureFlags
import com.teachme.core.model.LoadError
import com.teachme.core.model.TopicState

/** "Nessun argomento" / "1 argomento" / "n argomenti". */
@Composable
fun topicCountText(count: Int): String =
    if (count == 0) {
        stringResource(R.string.ui_topic_count_none)
    } else {
        pluralStringResource(R.plurals.ui_topic_count, count, count)
    }

private data class StateLook(
    val textRes: Int,
    val icon: Int,
    val variant: ChipVariant,
)

private fun TopicState.look(
    researchAvailable: Boolean,
    researchFailed: Boolean,
): StateLook =
    when (this) {
        TopicState.New, TopicState.Interviewing ->
            StateLook(R.string.ui_state_define_mission, TeachIcons.MessageCircle, ChipVariant.Neutral)
        TopicState.MissionProposed ->
            StateLook(R.string.ui_state_confirm_mission, TeachIcons.Flag, ChipVariant.Info)
        // Without the source research the chip states a fact, not a promise (mission.md design section 5).
        TopicState.Researching ->
            if (researchAvailable && researchFailed) {
                // The error has its own panel in the Fonti tab: the chip stays neutral (resources.md 4.4).
                StateLook(R.string.ui_state_research_retry, TeachIcons.AlertCircle, ChipVariant.Neutral)
            } else if (researchAvailable) {
                StateLook(R.string.ui_state_researching, TeachIcons.Search, ChipVariant.Neutral)
            } else {
                StateLook(R.string.ui_state_mission_confirmed, TeachIcons.CheckCircle, ChipVariant.Neutral)
            }
        TopicState.Ready -> StateLook(R.string.ui_state_ready, TeachIcons.CheckCircle, ChipVariant.Accent)
        TopicState.Unknown -> StateLook(R.string.ui_state_unknown, TeachIcons.HelpCircle, ChipVariant.Neutral)
    }

/** The state in words, for the chip and for accessibility descriptions. */
@Composable
fun topicStateText(
    state: TopicState,
    researchAvailable: Boolean = FeatureFlags.ResearchAvailable,
    researchFailed: Boolean = false,
): String = stringResource(state.look(researchAvailable, researchFailed).textRes)

/** "stato: definiamo la mission" (lowercase first letter, used in content descriptions). */
@Composable
fun topicStateDescription(
    state: TopicState,
    researchAvailable: Boolean = FeatureFlags.ResearchAvailable,
    researchFailed: Boolean = false,
): String =
    stringResource(
        R.string.ui_state_prefix,
        topicStateText(state, researchAvailable, researchFailed).replaceFirstChar { it.lowercase() },
    )

@Composable
fun TopicStateChip(
    state: TopicState,
    modifier: Modifier = Modifier,
    decorative: Boolean = true,
    researchAvailable: Boolean = FeatureFlags.ResearchAvailable,
    researchFailed: Boolean = false,
) {
    val look = state.look(researchAvailable, researchFailed)
    TeachStatusChip(
        text = stringResource(look.textRes),
        icon = look.icon,
        modifier = modifier,
        variant = look.variant,
        mergeIntoParent = decorative,
    )
}

/** Full-screen error of a failed first load, with the actions that fit the cause. */
@Composable
fun LoadErrorState(
    error: LoadError,
    onRetry: () -> Unit,
    onOpenSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    when (error) {
        // A 401 never gets its own message (login.md): the sign-in screen opens over this one. If this screen is what
        // the user comes back to, the retry reloads it.
        LoadError.Network, LoadError.Unauthorized ->
            TeachErrorState(
                title = stringResource(R.string.ui_load_network_title),
                body = stringResource(R.string.ui_load_network_body),
                modifier = modifier,
                primaryLabel = stringResource(R.string.ui_load_retry),
                onPrimary = onRetry,
                secondaryLabel = stringResource(R.string.ui_load_open_settings),
                onSecondary = onOpenSettings,
            )
        LoadError.Server ->
            TeachErrorState(
                title = stringResource(R.string.ui_load_server_title),
                body = stringResource(R.string.ui_load_server_body),
                modifier = modifier,
                primaryLabel = stringResource(R.string.ui_load_retry),
                onPrimary = onRetry,
            )
    }
}
