package com.teachme.feature.mission

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachDisclosure
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachSpinner
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.LoadError
import com.teachme.core.model.Mission
import com.teachme.core.model.Preference
import com.teachme.core.model.ResearchPointer
import com.teachme.core.model.ResearchStatus
import com.teachme.core.model.TopicMission
import kotlinx.coroutines.flow.first
import java.time.LocalDate
import com.teachme.core.designsystem.R as DsR

/** Two columns (mission + preferences) from this window width (mission.md design section 4.2). */
private val TWO_COLUMNS_MIN_WIDTH = 1100.dp
private val PREFERENCES_COLUMN = 280.dp
private val COLUMN_GAP = 32.dp
private val READING_COLUMN = 600.dp

/** Everything the Mission tab can ask of its owner. */
@Immutable
class MissionTabActions(
    val onConfirm: (version: Int) -> Unit,
    val onGoToChat: () -> Unit,
    val onRetryMission: () -> Unit,
    val onOpenSettings: () -> Unit,
    val onToggleVersions: () -> Unit,
    val onToggleVersion: (Int) -> Unit,
    val onShowMoreVersions: () -> Unit,
    val onRetryVersions: () -> Unit,
    val onRetryPreferences: () -> Unit,
    val onDeletePreference: (Preference) -> Unit,
    val onConfirmDelete: () -> Unit,
    val onDismissDelete: () -> Unit,
    val onOpenResources: () -> Unit = {},
    /** "Scrivilo al Tutor": the chat composer takes the focus with the starting-point hint (mission.md 4.6). */
    val onWriteStartingPoint: () -> Unit = {},
)

internal data class MissionGeometry(
    val start: Dp,
    val mainWidth: Dp,
    val twoColumns: Boolean,
)

internal fun missionGeometry(
    available: Dp,
    margin: Dp,
): MissionGeometry {
    val main = minOf(READING_COLUMN, available - margin * 2)
    val two = available >= TWO_COLUMNS_MIN_WIDTH
    val group = if (two) main + COLUMN_GAP + PREFERENCES_COLUMN else main
    return MissionGeometry(((available - group) / 2).coerceAtLeast(margin), main, two)
}

/**
 * The Mission tab of a Topic (docs/design/screens/mission.md section 4): pending draft, current mission,
 * honest note, older versions and preferences. The whole page scrolls; [header] (top bar, chip and tabs)
 * scrolls with it on short windows. [scrollToPreferencesSignal] brings the preferences into view (toast "Vedi").
 */
@Composable
fun MissionTab(
    topicTitle: String,
    state: MissionUiState,
    turnInProgress: Boolean,
    actions: MissionTabActions,
    modifier: Modifier = Modifier,
    header: (@Composable () -> Unit)? = null,
    scrollToPreferences: Boolean = false,
    onScrolledToPreferences: () -> Unit = {},
    confirmedSignal: Int = 0,
    research: ResearchPointer = ResearchPointer.None,
    prefsTitleFocus: FocusRequester = remember { FocusRequester() },
    /** Titles of the Topic's `ready` documents (topic.md F10). */
    pdfTitles: List<String> = emptyList(),
) {
    val widthClass = currentWindowWidthClass()
    val scroll = rememberScrollState()
    // Positions in root coordinates: their difference does not change while the page scrolls.
    var contentRootY by remember { mutableFloatStateOf(0f) }
    var prefsRootY by remember { mutableFloatStateOf(Float.NaN) }
    val currentOnScrolled by rememberUpdatedState(onScrolledToPreferences)
    // "Vedi" on the toast is a pending request, consumed after the scroll (also when this tab was not shown yet).
    LaunchedEffect(scrollToPreferences) {
        if (!scrollToPreferences) return@LaunchedEffect
        snapshotFlow { prefsRootY }.first { !it.isNaN() }
        withFrameNanos { }
        scroll.animateScrollTo((prefsRootY - contentRootY).toInt().coerceAtLeast(0))
        currentOnScrolled()
    }
    // After a confirmation made on this tab the draft card goes away: announce it and focus the new title.
    val currentTitleFocus = remember { FocusRequester() }
    var handledConfirmed by remember { mutableIntStateOf(confirmedSignal) }
    var announceConfirmed by remember { mutableIntStateOf(0) }
    LaunchedEffect(confirmedSignal) {
        if (confirmedSignal == handledConfirmed) return@LaunchedEffect
        handledConfirmed = confirmedSignal
        announceConfirmed++
        withFrameNanos { }
        runCatching { currentTitleFocus.requestFocus() }
    }
    BoxWithConstraints(modifier.fillMaxSize()) {
        val geometry = missionGeometry(maxWidth, screenMargin(widthClass))
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(scroll)
                .onGloballyPositioned { contentRootY = it.positionInRoot().y },
        ) {
            header?.invoke()
            Spacer(Modifier.padding(top = TeachTheme.spacing.md))
            val main: @Composable () -> Unit = {
                MissionMain(topicTitle, state, turnInProgress, actions, currentTitleFocus, research, pdfTitles)
            }
            val prefs: @Composable (Modifier) -> Unit = { m ->
                PreferencesSection(
                    state,
                    actions.onRetryPreferences,
                    actions.onDeletePreference,
                    m.onGloballyPositioned { prefsRootY = it.positionInRoot().y },
                    prefsTitleFocus,
                )
            }
            if (geometry.twoColumns) {
                Row(Modifier.padding(start = geometry.start, bottom = TeachTheme.spacing.xl)) {
                    Column(Modifier.width(geometry.mainWidth)) { main() }
                    Spacer(Modifier.width(COLUMN_GAP))
                    Box(Modifier.width(PREFERENCES_COLUMN)) { prefs(Modifier) }
                }
            } else {
                Column(
                    Modifier.padding(start = geometry.start, bottom = TeachTheme.spacing.xl).width(geometry.mainWidth),
                ) {
                    main()
                    prefs(Modifier.padding(top = TeachTheme.spacing.xl))
                }
            }
        }
        if (announceConfirmed > 0) {
            val text = stringResource(R.string.mission_confirmed_announce)
            key(announceConfirmed) {
                Box(
                    Modifier.size(1.dp).semantics {
                        liveRegion = LiveRegionMode.Polite
                        contentDescription = text
                    },
                )
            }
        }
        state.dialog?.let {
            DeletePreferenceDialog(it, topicTitle, actions.onConfirmDelete, actions.onDismissDelete)
        }
    }
}

@Composable
private fun SectionTitle(
    text: String,
    modifier: Modifier = Modifier,
) {
    TeachText(text, modifier.semantics { heading() }, style = TeachTheme.typography.ui.title3)
}

@Composable
private fun MissionMain(
    topicTitle: String,
    state: MissionUiState,
    turnInProgress: Boolean,
    actions: MissionTabActions,
    currentTitleFocus: FocusRequester,
    research: ResearchPointer,
    pdfTitles: List<String>,
) {
    when (val load = state.load) {
        MissionLoad.Loading -> MissionLoading()
        is MissionLoad.Failed -> MissionLoadFailed(load.error, actions)
        MissionLoad.Ready ->
            MissionReady(
                topicTitle,
                state,
                turnInProgress,
                actions,
                currentTitleFocus,
                research,
                pdfTitles,
            )
    }
}

@Composable
private fun MissionLoading() {
    val description = stringResource(R.string.mission_loading)
    Row(
        Modifier.fillMaxWidth().semantics { stateDescription = description },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        TeachSpinner(size = TeachTheme.layout.iconSizeSmall)
        TeachText(description, style = TeachTheme.typography.ui.bodyL, color = TeachTheme.colors.inkSoft)
    }
}

@Composable
private fun MissionLoadFailed(
    error: LoadError,
    actions: MissionTabActions,
) {
    TeachStatusPanel(
        StatusVariant.Danger,
        stringResource(R.string.mission_load_title),
        body =
            stringResource(
                when (error) {
                    // A 401 gets no message of its own (login.md): the sign-in opens, the retry reloads.
                    LoadError.Network, LoadError.Unauthorized -> R.string.mission_load_network
                    LoadError.Server -> R.string.mission_load_server
                },
            ),
    ) {
        TeachButton(
            stringResource(R.string.mission_load_retry),
            actions.onRetryMission,
            variant = ButtonVariant.Secondary,
        )
    }
}

@Composable
private fun MissionReady(
    topicTitle: String,
    state: MissionUiState,
    turnInProgress: Boolean,
    actions: MissionTabActions,
    currentTitleFocus: FocusRequester,
    research: ResearchPointer,
    pdfTitles: List<String>,
) {
    val mission = state.mission
    val today = remember { LocalDate.now() }
    val draft = mission?.draft
    val current = mission?.current
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.lg)) {
        if (draft != null) {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
                SectionTitle(stringResource(R.string.mission_pending_title))
                val cardState = missionCardState(draft, mission, turnInProgress, state.confirm)
                MissionCard(
                    mission = draft,
                    state = cardState,
                    topicTitle = topicTitle,
                    replacesVersion = current?.version,
                    confirmedAt = null,
                    diff = diffAgainstCurrent(draft, current?.version, mission),
                    previousConfirmedAt = current?.confirmedAt,
                    research = ResearchPointer.None,
                    onConfirm = { actions.onConfirm(draft.version) },
                    onEdit = actions.onGoToChat,
                    onOpenMission = {},
                    onOpenResources = {},
                    today = today,
                    keyNotice = (state.confirm as? ConfirmState.Failed)?.keyNotice,
                    budgetResetAt = (state.confirm as? ConfirmState.Failed)?.budgetResetAt,
                )
            }
        }
        if (current != null) {
            val invite =
                if (showStartingPointInvite(
                        mission,
                    )
                ) {
                    StartingPointInvite(topicTitle, actions.onWriteStartingPoint)
                } else {
                    null
                }
            CurrentMission(current, actions, today, currentTitleFocus, invite, pdfTitles)
        } else {
            EmptyMission(topicTitle, hasDraft = draft != null, actions)
        }
        if (current != null) ResearchTabNote(research, current.version, actions.onOpenResources)
        PreviousVersions(mission, state, actions, today)
    }
}

@Composable
private fun CurrentMission(
    current: Mission,
    actions: MissionTabActions,
    today: LocalDate,
    titleFocus: FocusRequester,
    invite: StartingPointInvite?,
    pdfTitles: List<String> = emptyList(),
) {
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
        SectionTitle(stringResource(R.string.mission_current_title), Modifier.focusRequester(titleFocus).focusable())
        TeachText(
            stringResource(
                R.string.mission_current_meta,
                current.version,
                current.confirmedAt?.let {
                    formatMissionDate(
                        it,
                        today,
                    )
                } ?: formatMissionDate(current.createdAt, today),
            ),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
        MissionSummary(current, highlight = true, startingPointInvite = invite)
        if (pdfTitles.isNotEmpty()) PdfLine(pdfTitles, actions.onOpenResources)
        TeachText(
            stringResource(R.string.mission_current_hint),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
        TeachButton(stringResource(R.string.mission_go_chat), actions.onGoToChat, variant = ButtonVariant.Text)
    }
}

@Composable
private fun EmptyMission(
    topicTitle: String,
    hasDraft: Boolean,
    actions: MissionTabActions,
) {
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
        if (hasDraft) {
            TeachText(
                stringResource(R.string.mission_empty_with_draft),
                style = TeachTheme.typography.ui.bodyL,
                color = TeachTheme.colors.inkSoft,
            )
        } else {
            SectionTitle(stringResource(R.string.mission_current_title))
            TeachText(
                stringResource(R.string.mission_empty_body, topicTitle),
                style = TeachTheme.typography.ui.bodyL,
                color = TeachTheme.colors.inkSoft,
            )
            TeachButton(
                stringResource(R.string.mission_empty_action),
                actions.onGoToChat,
                variant = ButtonVariant.Secondary,
            )
        }
    }
}

@Composable
private fun PreviousVersions(
    mission: TopicMission?,
    state: MissionUiState,
    actions: MissionTabActions,
    today: LocalDate,
) {
    val count =
        (state.versions as? VersionsLoad.Loaded)?.let { state.previousVersions.size } ?: otherVersionsCount(mission)
    if (count <= 0) return
    TeachDisclosure(
        stringResource(R.string.mission_versions_title, count),
        state.versionsOpen,
        actions.onToggleVersions,
    ) {
        when (state.versions) {
            VersionsLoad.NotLoaded, VersionsLoad.Loading ->
                Row(
                    Modifier.padding(vertical = TeachTheme.spacing.xs),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                ) {
                    TeachSpinner(size = TeachTheme.layout.iconSizeSmall)
                    TeachText(
                        stringResource(R.string.mission_versions_loading),
                        style = TeachTheme.typography.ui.bodyS,
                        color = TeachTheme.colors.inkSoft,
                    )
                }
            VersionsLoad.Failed ->
                Column {
                    TeachText(
                        stringResource(R.string.mission_versions_failed),
                        style = TeachTheme.typography.ui.bodyS,
                        color = TeachTheme.colors.inkSoft,
                    )
                    TeachButton(
                        stringResource(R.string.mission_load_retry),
                        actions.onRetryVersions,
                        variant = ButtonVariant.Text,
                    )
                }
            is VersionsLoad.Loaded -> {
                val items = state.previousVersions
                Column {
                    items.take(state.versionsShown).forEach { version ->
                        VersionRow(
                            version,
                            expanded = state.expandedVersion == version.version,
                            today = today,
                            onToggle = { actions.onToggleVersion(version.version) },
                        )
                    }
                    if (items.size > state.versionsShown) {
                        TeachButton(
                            stringResource(R.string.mission_versions_more),
                            actions.onShowMoreVersions,
                            variant = ButtonVariant.Text,
                        )
                    }
                }
            }
        }
    }
}

/** A past version: name and meta in a 56 dp row that expands, read-only, into the sections (one at a time). */
@Composable
fun VersionRow(
    mission: Mission,
    expanded: Boolean,
    today: LocalDate,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val name = stringResource(R.string.mission_version_name, mission.version)
    val meta =
        mission.confirmedAt?.let { stringResource(R.string.mission_version_confirmed, formatMissionDate(it, today)) }
            ?: stringResource(R.string.mission_version_unconfirmed, formatMissionDate(mission.createdAt, today))
    val description = stringResource(R.string.mission_version_description, name, meta)
    val stateText =
        stringResource(
            if (expanded) DsR.string.teach_state_open else DsR.string.teach_state_closed,
        )
    Column(modifier.fillMaxWidth()) {
        Column(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 56.dp)
                .semantics(mergeDescendants = true) {
                    stateDescription = stateText
                    contentDescription = description
                }.clickable(role = Role.Button, onClick = onToggle)
                .padding(vertical = TeachTheme.spacing.xs),
            verticalArrangement = Arrangement.Center,
        ) {
            TeachText(name, style = TeachTheme.typography.ui.label, color = TeachTheme.colors.ink)
            TeachText(meta, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
        }
        if (expanded) {
            MissionSummary(mission, highlight = false, modifier = Modifier.padding(bottom = TeachTheme.spacing.sm))
        }
    }
}

private val previewState =
    MissionUiState(
        load = MissionLoad.Ready,
        mission = TopicMission(PreviewData.confirmed1, PreviewData.draft2.copy(version = 3)),
        prefsLoad = PreferencesLoad.Ready,
        preferences = listOf(PreviewData.prefTopic, PreviewData.prefGlobal),
        versionsOpen = true,
        versions =
            VersionsLoad.Loaded(
                listOf(PreviewData.draft2.copy(status = com.teachme.core.model.MissionStatus.Superseded)),
            ),
        expandedVersion = 2,
    )

private val noActions =
    MissionTabActions({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})

@Composable
private fun TabPreview(
    state: MissionUiState,
    research: ResearchPointer = ResearchPointer(ResearchStatus.InProgress),
) {
    Box(Modifier.background(TeachTheme.colors.bg)) {
        MissionTab("Rust", state, false, noActions, research = research)
    }
}

@Preview(name = "Mission expanded", showBackground = true, widthDp = 1280, heightDp = 1100)
@Composable
private fun MissionExpandedPreview() = TeachTheme(darkTheme = false) { TabPreview(previewState) }

@Preview(name = "Mission expanded dark", showBackground = true, widthDp = 1280, heightDp = 1100)
@Composable
private fun MissionExpandedDarkPreview() = TeachTheme(darkTheme = true) { TabPreview(previewState) }

@Preview(name = "Mission compact", showBackground = true, widthDp = 360, heightDp = 1800)
@Composable
private fun MissionCompactPreview() = TeachTheme(darkTheme = false) { TabPreview(previewState) }

@Preview(name = "Mission empty", showBackground = true, widthDp = 600, heightDp = 700)
@Composable
private fun MissionEmptyPreview() =
    TeachTheme(darkTheme = false) {
        TabPreview(
            MissionUiState(
                load = MissionLoad.Ready,
                mission = TopicMission(null, null),
                prefsLoad = PreferencesLoad.Ready,
            ),
            ResearchPointer.None,
        )
    }

@Preview(name = "Mission starting point invite", showBackground = true, widthDp = 600, heightDp = 900)
@Composable
private fun MissionInvitePreview() =
    TeachTheme(darkTheme = false) {
        TabPreview(
            MissionUiState(
                load = MissionLoad.Ready,
                mission = TopicMission(PreviewData.confirmedNoStart, null),
                prefsLoad = PreferencesLoad.Ready,
            ),
            ResearchPointer.None,
        )
    }

@Preview(name = "Mission starting point invite dark", showBackground = true, widthDp = 600, heightDp = 900)
@Composable
private fun MissionInviteDarkPreview() =
    TeachTheme(darkTheme = true) {
        TabPreview(
            MissionUiState(
                load = MissionLoad.Ready,
                mission = TopicMission(PreviewData.confirmedNoStart, null),
                prefsLoad = PreferencesLoad.Ready,
            ),
            ResearchPointer.None,
        )
    }

@Preview(name = "Mission first draft", showBackground = true, widthDp = 600, heightDp = 1300)
@Composable
private fun MissionDraftPreview() =
    TeachTheme(darkTheme = false) {
        TabPreview(
            MissionUiState(
                load = MissionLoad.Ready,
                mission = TopicMission(null, PreviewData.first),
                prefsLoad = PreferencesLoad.Loading,
            ),
            ResearchPointer.None,
        )
    }

@Preview(name = "Mission sources outdated", showBackground = true, widthDp = 600, heightDp = 1300)
@Composable
private fun MissionStaleSourcesPreview() =
    TeachTheme(darkTheme = true) {
        TabPreview(previewState, ResearchPointer(ResearchStatus.Ready, 8, 0))
    }

/** "Studi dal tuo PDF" under the mission, with the way to the Fonti tab (topic.md F10). Not part of the mission text. */
@Composable
internal fun PdfLine(
    titles: List<String>,
    onOpenResources: () -> Unit,
) {
    Column(Modifier.testTag("mission-pdf-line")) {
        Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            TeachIcon(
                TeachIcons.FileText,
                null,
                tint = TeachTheme.colors.inkSoft,
                size = TeachTheme.layout.iconSizeSmall,
            )
            TeachText(
                if (titles.size == 1) {
                    stringResource(R.string.mission_pdf_line, titles.first())
                } else {
                    stringResource(R.string.mission_pdf_line_many)
                },
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
        }
        TeachButton(stringResource(R.string.mission_pdf_go), onOpenResources, variant = ButtonVariant.Text)
    }
}
