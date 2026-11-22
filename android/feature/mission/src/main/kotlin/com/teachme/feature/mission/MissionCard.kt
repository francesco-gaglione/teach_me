package com.teachme.feature.mission

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachDisclosure
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachSurface
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.model.KeyNotice
import com.teachme.core.model.MessagePart
import com.teachme.core.model.Mission
import com.teachme.core.model.ResearchPointer
import com.teachme.core.model.ResearchStatus
import com.teachme.core.model.TopicMission
import com.teachme.core.ui.LlmBudgetPanel
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyNotice
import com.teachme.core.ui.llmBudgetAvailability
import com.teachme.core.ui.rememberBudgetRenewed
import java.time.Instant
import java.time.LocalDate

private const val STACKED_FONT_SCALE = 1.3f
private val STACKED_MAX_WIDTH = 360.dp
private const val CONFIRMED_WHY_LINES = 3

/**
 * The proposal card (docs/design/screens/mission.md section 2), used in the chat and in the Mission tab.
 * Stateless: [state] is computed by [missionCardState] from the data. "Modifica" opens nothing, it hands
 * the focus to the composer ([onEdit]); the only decision the card takes is [onConfirm].
 *
 * @param confirmedAt when the confirmed version was confirmed (the card's [mission] is a draft snapshot).
 * @param previous the confirmed version it replaces, for the "Nuovo" / "Tolto" marks.
 * @param research what to say about the source research under "Confermata il ..." (resources.md section 7).
 */
@Composable
fun MissionCard(
    mission: Mission,
    state: MissionCardState,
    topicTitle: String,
    replacesVersion: Int?,
    confirmedAt: Instant?,
    diff: MissionDiff?,
    previousConfirmedAt: Instant?,
    research: ResearchPointer,
    onConfirm: () -> Unit,
    onEdit: () -> Unit,
    onOpenMission: () -> Unit,
    onOpenResources: () -> Unit,
    modifier: Modifier = Modifier,
    today: LocalDate = remember { LocalDate.now() },
    /** The confirmation was refused because of the LLM key: the error slot holds the notice (mission.md, llm-key.md). */
    keyNotice: KeyNotice? = null,
    /** The confirmation was refused because the daily LLM budget is used up: when it comes back (llm-key.md 10.1). */
    budgetResetAt: Instant? = null,
) {
    val colors = TeachTheme.colors
    val padding = if (currentWindowWidthClass() == WindowWidthClass.Compact) TeachTheme.spacing.md else 20.dp
    val hot = state != MissionCardState.Confirmed && state != MissionCardState.Superseded
    val paneTitle = stringResource(R.string.mission_pane_title, mission.version)
    var showSuperseded by rememberSaveable { mutableStateOf(false) }

    // After a confirmation the focus follows the card to its band, otherwise it would vanish with the button.
    val bandFocus = remember { FocusRequester() }
    var previousState by remember { mutableStateOf(state) }
    var justConfirmed by remember { mutableStateOf(false) }
    LaunchedEffect(state) {
        if (previousState == MissionCardState.Confirming && state == MissionCardState.Confirmed) {
            justConfirmed = true
            runCatching { bandFocus.requestFocus() }
        }
        previousState = state
    }

    TeachSurface(
        modifier = modifier.fillMaxWidth().semantics { this.paneTitle = paneTitle },
        color = colors.surface,
        border =
            BorderStroke(
                if (hot) TeachTheme.stroke.control else TeachTheme.stroke.hairline,
                if (hot) colors.border else colors.line,
            ),
        shape = TeachTheme.shapes.md,
    ) {
        Column(Modifier.fillMaxWidth()) {
            Band(
                state = state,
                version = mission.version,
                isChange = replacesVersion != null,
                announce = justConfirmed,
                modifier = Modifier.focusRequester(bandFocus).focusable(),
            )
            Column(
                Modifier.fillMaxWidth().padding(padding),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
            ) {
                when (state) {
                    MissionCardState.Confirmed ->
                        ConfirmedBody(mission, confirmedAt, research, today, onOpenMission, onOpenResources)
                    MissionCardState.Superseded ->
                        SupersededBody(mission, showSuperseded) { showSuperseded = !showSuperseded }
                    else ->
                        OpenBody(
                            mission,
                            state,
                            topicTitle,
                            replacesVersion,
                            previousConfirmedAt,
                            diff,
                            today,
                            onConfirm,
                            onEdit,
                            keyNotice,
                            budgetResetAt,
                        )
                }
            }
        }
    }
}

@Composable
private fun Band(
    state: MissionCardState,
    version: Int,
    isChange: Boolean,
    announce: Boolean,
    modifier: Modifier = Modifier,
) {
    val colors = TeachTheme.colors
    val (fill, content, icon) =
        when (state) {
            MissionCardState.Confirmed -> Triple(colors.correctSoft, colors.correct, TeachIcons.CheckCircle)
            MissionCardState.Superseded -> Triple(colors.surfaceSunken, colors.inkSoft, TeachIcons.History)
            else -> Triple(colors.infoSoft, colors.info, TeachIcons.Flag)
        }
    val base =
        stringResource(if (isChange) R.string.mission_band_change else R.string.mission_band_proposal)
    val text =
        when (state) {
            MissionCardState.Confirmed -> stringResource(R.string.mission_band_confirmed)
            MissionCardState.Superseded -> stringResource(R.string.mission_band_superseded)
            else -> if (version > 1) stringResource(R.string.mission_band_with_version, base, version) else base
        }
    val announcement = stringResource(R.string.mission_confirmed_announce)
    Row(
        modifier
            .fillMaxWidth()
            .background(fill)
            .padding(horizontal = TeachTheme.spacing.md, vertical = TeachTheme.spacing.sm)
            .semantics(mergeDescendants = true) {
                heading()
                if (announce) {
                    liveRegion = LiveRegionMode.Polite
                    contentDescription = announcement
                }
            },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        TeachIcon(icon, null, tint = content, size = TeachTheme.layout.iconSizeSmall)
        TeachText(text, style = TeachTheme.typography.ui.labelL, color = content)
    }
}

@Composable
private fun OpenBody(
    mission: Mission,
    state: MissionCardState,
    topicTitle: String,
    replacesVersion: Int?,
    previousConfirmedAt: Instant?,
    diff: MissionDiff?,
    today: LocalDate,
    onConfirm: () -> Unit,
    onEdit: () -> Unit,
    keyNotice: KeyNotice? = null,
    budgetResetAt: Instant? = null,
) {
    if (replacesVersion != null) {
        TeachText(
            if (previousConfirmedAt != null) {
                stringResource(
                    R.string.mission_replaces,
                    replacesVersion,
                    formatMissionDate(previousConfirmedAt, today),
                )
            } else {
                stringResource(R.string.mission_replaces_nodate, replacesVersion)
            },
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
    }
    MissionSummary(mission, highlight = true, diff = diff)
    when (state) {
        MissionCardState.Unknown -> Unit
        MissionCardState.ConfirmError ->
            if (budgetResetAt != null) {
                // Nothing to fix and nothing lost: the proposal stays, "Riprova" below waits for the reset.
                LlmBudgetPanel(budgetResetAt)
            } else if (keyNotice != null) {
                // The proposal is still there: the key is what to fix, not the card (llm-key.md section 5).
                // Credit: "Riprova" confirms again; the card shows its own progress meanwhile (llm-key.md section 9.2).
                LlmKeyNotice(
                    keyNotice.variant,
                    LlmKeyContext.MissionConfirm,
                    keyNotice.id,
                    onRetry = onConfirm,
                    again = keyNotice.again,
                )
            } else {
                TeachStatusPanel(
                    StatusVariant.Danger,
                    stringResource(R.string.mission_error_title),
                    body = stringResource(R.string.mission_error_body),
                )
            }
        MissionCardState.Arriving ->
            TeachText(
                stringResource(R.string.mission_arriving),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
        else ->
            TeachText(
                stringResource(R.string.mission_question),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
    }
    if (state !=
        MissionCardState.Unknown
    ) {
        Actions(state, topicTitle, onConfirm, onEdit, retryLabel = keyNotice == null, budgetResetAt = budgetResetAt)
    }
}

@Composable
private fun Actions(
    state: MissionCardState,
    topicTitle: String,
    onConfirm: () -> Unit,
    onEdit: () -> Unit,
    retryLabel: Boolean = true,
    budgetResetAt: Instant? = null,
) {
    // "Riprova" is visible but disabled until the budget is back; the panel above does the announcing.
    val budgetWait = budgetResetAt != null && !rememberBudgetRenewed(budgetResetAt, announce = false)
    val budgetAvailability = budgetResetAt?.let { llmBudgetAvailability(it) }
    val confirming = state == MissionCardState.Confirming
    val arriving = state == MissionCardState.Arriving
    val retry = state == MissionCardState.ConfirmError && retryLabel
    val confirmLabel =
        stringResource(
            when {
                confirming -> R.string.mission_confirming
                retry -> R.string.mission_retry
                else -> R.string.mission_confirm
            },
        )
    val confirmDescription =
        when {
            confirming -> null
            retry -> stringResource(R.string.mission_retry_description, topicTitle)
            else -> stringResource(R.string.mission_confirm_description, topicTitle)
        }
    val editLabel = stringResource(R.string.mission_edit)
    val editDescription = stringResource(R.string.mission_edit_description, topicTitle)
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val stacked = LocalDensity.current.fontScale > STACKED_FONT_SCALE || maxWidth < STACKED_MAX_WIDTH
        val confirmButton: @Composable (Modifier) -> Unit = { m ->
            TeachButton(
                confirmLabel,
                onConfirm,
                m,
                enabled = !arriving && !budgetWait,
                loading = confirming,
                contentDescription = confirmDescription,
                disabledStateDescription = budgetAvailability,
            )
        }
        val editButton: @Composable (Modifier) -> Unit = { m ->
            TeachButton(
                editLabel,
                onEdit,
                m,
                variant = ButtonVariant.Secondary,
                enabled = !arriving && !confirming,
                contentDescription = editDescription,
            )
        }
        if (stacked) {
            // Reading order follows what is seen: the primary action first.
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                confirmButton(Modifier.fillMaxWidth())
                editButton(Modifier.fillMaxWidth())
            }
        } else {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs, Alignment.End),
            ) {
                editButton(Modifier)
                confirmButton(Modifier)
            }
        }
    }
}

@Composable
private fun ConfirmedBody(
    mission: Mission,
    confirmedAt: Instant?,
    research: ResearchPointer,
    today: LocalDate,
    onOpenMission: () -> Unit,
    onOpenResources: () -> Unit,
) {
    if (confirmedAt != null) {
        TeachText(
            stringResource(R.string.mission_confirmed_on, formatMissionDate(confirmedAt, today)),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
    }
    ResearchCardNote(research, mission.version, onOpenResources)
    TeachText(
        mission.why,
        style = TeachTheme.typography.ui.body,
        color = TeachTheme.colors.ink,
        maxLines = CONFIRMED_WHY_LINES,
        overflow = TextOverflow.Ellipsis,
    )
    var expanded by rememberSaveable { mutableStateOf(false) }
    TeachDisclosure(stringResource(R.string.mission_show_details), expanded, { expanded = !expanded }) {
        MissionSummary(mission, highlight = false, showWhy = false, showStartingPoint = false)
    }
    TeachButton(stringResource(R.string.mission_see_mission), onOpenMission, variant = ButtonVariant.Text)
}

@Composable
private fun SupersededBody(
    mission: Mission,
    shown: Boolean,
    onToggle: () -> Unit,
) {
    val title = stringResource(R.string.mission_superseded_show)
    val hidden = stringResource(R.string.mission_superseded_hidden)
    // The band already says "Sostituita..."; the hidden state is spoken with the button, not shown twice.
    TeachDisclosure(title, shown, onToggle, description = if (shown) null else "$hidden $title") {
        MissionSummary(mission, highlight = false)
    }
}

/**
 * Everything the chat needs to draw the card of a `mission_proposal` part (mission.md design section 2).
 * Built by the Topic screen from [MissionViewModel] and the chat state.
 */
@Immutable
class MissionChatContext(
    val topicTitle: String,
    val mission: TopicMission?,
    val confirm: ConfirmState,
    val turnInProgress: Boolean,
    /** What to say about the source research (a pointer to the Fonti tab, never its details). */
    val research: ResearchPointer,
    val onConfirm: (version: Int) -> Unit,
    val onEdit: () -> Unit,
    val onOpenMission: () -> Unit,
    val onOpenResources: () -> Unit,
)

/** The card of [part] with the state derived from the data in [context]. */
@Composable
fun MissionProposalCard(
    part: MessagePart.MissionProposal,
    context: MissionChatContext,
    modifier: Modifier = Modifier,
) {
    val state =
        missionCardState(
            part.mission,
            context.mission,
            context.turnInProgress,
            context.confirm,
            part.replacesVersion,
        )
    val current = context.mission?.current
    val diff = diffAgainstCurrent(part.mission, part.replacesVersion, context.mission)
    MissionCard(
        mission = part.mission,
        state = state,
        topicTitle = context.topicTitle,
        replacesVersion = part.replacesVersion,
        confirmedAt = current?.takeIf { it.version == part.mission.version }?.confirmedAt,
        diff = diff,
        previousConfirmedAt = current?.takeIf { it.version == part.replacesVersion }?.confirmedAt,
        research = if (state == MissionCardState.Confirmed) context.research else ResearchPointer.None,
        onConfirm = { context.onConfirm(part.mission.version) },
        onEdit = context.onEdit,
        onOpenMission = context.onOpenMission,
        onOpenResources = context.onOpenResources,
        modifier = modifier,
        keyNotice = (context.confirm as? ConfirmState.Failed)?.keyNotice,
        budgetResetAt = (context.confirm as? ConfirmState.Failed)?.budgetResetAt,
    )
}

@Composable
private fun CardPreviewContent() {
    val today = LocalDate.of(2026, 11, 8)
    val diff = missionDiff(PreviewData.draft2, PreviewData.confirmed1)
    val at = PreviewData.confirmed1.confirmedAt
    Column(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.lg),
    ) {
        fun card(
            mission: Mission,
            state: MissionCardState,
            replaces: Int? = null,
            d: MissionDiff? = null,
            note: ResearchPointer = ResearchPointer.None,
            budgetResetAt: Instant? = null,
        ): @Composable () -> Unit =
            {
                MissionCard(
                    mission,
                    state,
                    "Rust",
                    replaces,
                    at,
                    d,
                    at,
                    note,
                    {},
                    {},
                    {},
                    {},
                    today = today,
                    budgetResetAt = budgetResetAt,
                )
            }
        card(PreviewData.first, MissionCardState.Open)()
        card(PreviewData.first, MissionCardState.ConfirmError, budgetResetAt = Instant.now().plusSeconds(3 * 3600L))()
        card(PreviewData.draft2, MissionCardState.Open, 1, diff)()
        card(
            PreviewData.draftAddsStart,
            MissionCardState.Open,
            1,
            missionDiff(PreviewData.draftAddsStart, PreviewData.confirmedNoStart),
        )()
        card(PreviewData.first, MissionCardState.Arriving)()
        card(PreviewData.first, MissionCardState.Confirming)()
        card(PreviewData.first, MissionCardState.ConfirmError)()
        card(PreviewData.confirmed1, MissionCardState.Confirmed, note = ResearchPointer(ResearchStatus.InProgress))()
        card(
            PreviewData.confirmed1,
            MissionCardState.Confirmed,
            note = ResearchPointer(ResearchStatus.Ready, 8, 1),
        )()
        card(PreviewData.confirmed1, MissionCardState.Superseded)()
    }
}

@Preview(name = "Card light", showBackground = true, widthDp = 640, heightDp = 5400)
@Composable
private fun CardLightPreview() = TeachTheme(darkTheme = false) { CardPreviewContent() }

@Preview(name = "Card dark", showBackground = true, widthDp = 640, heightDp = 5400)
@Composable
private fun CardDarkPreview() = TeachTheme(darkTheme = true) { CardPreviewContent() }

@Preview(name = "Card compact font 2.0", showBackground = true, widthDp = 360, heightDp = 7600, fontScale = 2f)
@Composable
private fun CardLargePreview() = TeachTheme(darkTheme = false) { CardPreviewContent() }
