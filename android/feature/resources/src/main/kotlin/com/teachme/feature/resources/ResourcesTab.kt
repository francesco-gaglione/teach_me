package com.teachme.feature.resources

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LoadError
import com.teachme.core.model.ResourcesView
import com.teachme.core.model.TopicState
import com.teachme.core.model.dayMonthFormatter
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyNotice
import com.teachme.core.ui.LoadErrorState
import java.time.Instant
import java.time.ZoneId
import java.util.Locale

private val TWO_COLUMNS_MIN_WIDTH = 1100.dp
private val SIDE_COLUMN = 280.dp
private val COLUMN_GAP = 32.dp
private val READING_COLUMN = 600.dp
private val EMPTY_TOP_PADDING = 48.dp
private val SKELETON_BAR_HEIGHT = 96.dp

/** Everything the Fonti tab can ask of its owner. */
@Immutable
class ResourcesTabActions(
    val onRetryLoad: () -> Unit,
    val onRetry: () -> Unit,
    val onStartResearch: () -> Unit,
    /** The empty state sends the user to the chat (composer focused). */
    val onGoToChat: () -> Unit,
    val onOpenSettings: () -> Unit,
    /** The tab was left: "Nuova" marks go away. */
    val onLeave: () -> Unit,
)

private data class Geometry(
    val start: Dp,
    val mainWidth: Dp,
    val twoColumns: Boolean,
)

private fun geometry(
    available: Dp,
    margin: Dp,
): Geometry {
    val main = minOf(READING_COLUMN, available - margin * 2)
    val two = available >= TWO_COLUMNS_MIN_WIDTH
    val group = if (two) main + COLUMN_GAP + SIDE_COLUMN else main
    return Geometry(((available - group) / 2).coerceAtLeast(margin), main, two)
}

/**
 * The Fonti tab of a Topic (docs/design/screens/resources.md): empty, searching, done, no sources and failed states.
 * The whole page scrolls; [header] (top bar, chip and tabs) scrolls with it on short windows.
 *
 * @param missionWhy the confirmed mission's reason, quoted while the search runs.
 * @param currentMissionVersion the confirmed mission version, to offer "Aggiorna le fonti" when it is newer.
 */
@Composable
fun ResourcesTab(
    topicState: TopicState,
    missionWhy: String?,
    currentMissionVersion: Int?,
    hasProposal: Boolean,
    state: ResourcesUiState,
    actions: ResourcesTabActions,
    documents: DocumentsUiState,
    documentActions: DocumentsActions,
    toast: TeachToastState,
    modifier: Modifier = Modifier,
    header: (@Composable () -> Unit)? = null,
) {
    val widthClass = currentWindowWidthClass()
    val context = LocalContext.current
    val copy = rememberCopyLink(toast)
    val currentOnLeave = rememberUpdatedState(actions.onLeave)
    DisposableEffect(Unit) { onDispose { currentOnLeave.value() } }
    val open: (String) -> Unit = { openInCustomTab(context, it) }
    BoxWithConstraints(modifier.fillMaxSize()) {
        val geometry = geometry(maxWidth, screenMargin(widthClass))
        val scroll = rememberScrollState()
        Column(Modifier.fillMaxSize().verticalScroll(scroll)) {
            header?.invoke()
            when (val load = state.load) {
                ResourcesLoad.Loading -> {
                    DocumentsOnly(geometry, topicState, documents, documentActions)
                    Skeleton(geometry)
                }
                is ResourcesLoad.Failed -> {
                    DocumentsOnly(geometry, topicState, documents, documentActions)
                    Box(Modifier.padding(start = geometry.start).width(geometry.mainWidth)) {
                        LoadErrorState(load.error, actions.onRetryLoad, actions.onOpenSettings)
                    }
                }
                ResourcesLoad.Ready ->
                    state.view?.let { view ->
                        val screen = screenFor(topicState, view, currentMissionVersion, state.settled)
                        ReadyContent(
                            screen,
                            geometry,
                            missionWhy,
                            hasProposal,
                            state,
                            actions,
                            documents,
                            documentActions,
                            open,
                            copy,
                        )
                    }
            }
        }
        state.announcement?.takeIf { it.ready }?.let { announcement ->
            val text = pluralStringResource(R.plurals.res_announce_ready, announcement.count, announcement.count)
            key(announcement.id) {
                Box(
                    Modifier.size(1.dp).semantics {
                        liveRegion = LiveRegionMode.Polite
                        contentDescription = text
                    },
                )
            }
        }
    }
}

/** The documents section while the web sources are still loading (or failed to): it does not wait for them. */
@Composable
private fun DocumentsOnly(
    geometry: Geometry,
    topicState: TopicState,
    documents: DocumentsUiState,
    actions: DocumentsActions,
) {
    Column(Modifier.padding(start = geometry.start, top = TeachTheme.spacing.md).width(geometry.mainWidth)) {
        DocumentsSection(
            documents,
            missionPending = topicState != TopicState.Researching && topicState != TopicState.Ready,
            bannerDocuments = emptyList(),
            acting = false,
            onRefreshSources = {},
            actions = actions,
        )
    }
}

@Composable
private fun Skeleton(geometry: Geometry) {
    val loading = stringResource(R.string.res_loading)
    Column(
        Modifier
            .padding(start = geometry.start, top = TeachTheme.spacing.md)
            .width(geometry.mainWidth)
            .semantics(mergeDescendants = true) { stateDescription = loading },
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        repeat(3) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(SKELETON_BAR_HEIGHT)
                    .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.md),
            )
        }
    }
}

@Composable
private fun ReadyContent(
    screen: ResourcesScreen,
    geometry: Geometry,
    missionWhy: String?,
    hasProposal: Boolean,
    state: ResourcesUiState,
    actions: ResourcesTabActions,
    documents: DocumentsUiState,
    documentActions: DocumentsActions,
    open: (String) -> Unit,
    copy: (String) -> Unit,
) {
    val results = screen as? ResourcesScreen.Results
    // The summary only exists once a research has finished and nothing else is going on (4.3, 6.2).
    val summary = results?.view?.takeIf { it.research != null && it.resources.isNotEmpty() }
    val staleBannerShown = results?.staleVersion != null && summary != null
    // A PDF added after the last search (13.8): its own banner, or merged into the stale-mission one.
    val newerDocuments =
        if (results == null) {
            emptyList()
        } else {
            documentsNewerThanResearch(
                documents.documents,
                results.view.research,
                results.view.job?.isActive == true,
                documents.dismissedBanner,
            )
        }
    val mergedBanner = staleBannerShown && newerDocuments.isNotEmpty()
    val refreshFromBanner = {
        documentActions.onDismissBanner(newerDocuments.map { it.id })
        actions.onStartResearch()
    }
    Spacer(Modifier.height(TeachTheme.spacing.md))
    val main: @Composable () -> Unit = {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.lg)) {
            DocumentsSection(
                documents,
                missionPending = screen == ResourcesScreen.Empty,
                bannerDocuments = if (mergedBanner) emptyList() else newerDocuments,
                acting = state.acting,
                onRefreshSources = actions.onStartResearch,
                actions = documentActions,
                modifier = Modifier.padding(bottom = TeachTheme.spacing.sm),
            )
            if (summary != null && !geometry.twoColumns) {
                SummaryRow(summary, showRefresh = !staleBannerShown, state.acting, actions.onStartResearch)
            }
            when (screen) {
                ResourcesScreen.Empty -> EmptyState(hasProposal, actions.onGoToChat)
                is ResourcesScreen.Running -> {
                    ResearchProgress(screen.job, screen.completing, screen.previous != null, missionWhy)
                    screen.previous?.let { Previous(it, state.newUrls, open, copy) }
                }
                is ResourcesScreen.Failed -> {
                    FailedPanel(
                        screen.job.id,
                        screen.job.error,
                        refreshing = screen.previous != null,
                        actionError = state.actionError,
                        acting = state.acting,
                        onRetry = actions.onRetry,
                        keyNotice = state.keyNotice,
                    )
                    screen.previous?.let { Previous(it, state.newUrls, open, copy, intro = false) }
                }
                is ResourcesScreen.Results -> {
                    state.actionError?.let { ActionErrorPanel(it) }
                    // A new search or an update refused for the key: the sources already here stay as they are.
                    state.keyNotice?.let {
                        LlmKeyNotice(
                            it.variant,
                            if (screen.view.resources.isEmpty()) {
                                LlmKeyContext.ResearchRequestEmpty
                            } else {
                                LlmKeyContext.ResearchRequest
                            },
                            it.id,
                            onRetry = actions.onStartResearch,
                            retrying = state.acting,
                            again = it.again,
                        )
                    }
                    if (screen.staleVersion != null && screen.view.resources.isNotEmpty()) {
                        StaleBanner(
                            screen.staleVersion,
                            state.acting,
                            if (mergedBanner) refreshFromBanner else actions.onStartResearch,
                            alsoPdf = mergedBanner,
                        )
                    }
                    if (screen.view.resources.isEmpty()) {
                        NoSourcesPanel(state.acting, actions.onStartResearch)
                    } else {
                        SourceGroups(screen.view, state.newUrls, open, copy)
                    }
                    GapsBlock(screen.view.gaps)
                }
            }
        }
    }
    if (geometry.twoColumns && summary != null) {
        Row(Modifier.padding(start = geometry.start, bottom = TeachTheme.spacing.xl)) {
            Column(Modifier.width(geometry.mainWidth)) { main() }
            Spacer(Modifier.width(COLUMN_GAP))
            Box(Modifier.width(SIDE_COLUMN)) {
                SideColumn(summary, showRefresh = !staleBannerShown, state.acting, actions.onStartResearch)
            }
        }
    } else {
        Column(Modifier.padding(start = geometry.start, bottom = TeachTheme.spacing.xl).width(geometry.mainWidth)) {
            main()
        }
    }
}

@Composable
private fun Previous(
    view: ResourcesView,
    newUrls: Set<String>,
    open: (String) -> Unit,
    copy: (String) -> Unit,
    intro: Boolean = true,
) {
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.lg)) {
        if (intro && view.resources.isNotEmpty()) {
            val version = view.research?.missionVersion
            TeachText(
                if (version != null) {
                    stringResource(R.string.res_progress_previous, version)
                } else {
                    stringResource(R.string.res_progress_previous_unknown)
                },
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
        }
        if (view.resources.isNotEmpty()) SourceGroups(view, newUrls, open, copy)
        GapsBlock(view.gaps)
    }
}

@Composable
private fun EmptyState(
    hasProposal: Boolean,
    onGoToChat: () -> Unit,
) {
    Column(
        Modifier.fillMaxWidth().padding(top = EMPTY_TOP_PADDING - TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        TeachText(
            stringResource(R.string.res_empty_title),
            Modifier.semantics { heading() },
            style = TeachTheme.typography.ui.title2,
        )
        TeachText(
            stringResource(R.string.res_empty_body),
            style = TeachTheme.typography.ui.bodyL,
            color = TeachTheme.colors.ink,
        )
        TeachButton(
            stringResource(if (hasProposal) R.string.res_empty_action_proposal else R.string.res_empty_action),
            onGoToChat,
            variant = ButtonVariant.Secondary,
        )
    }
}

@Composable
private fun ActionErrorPanel(error: LoadError) {
    TeachStatusPanel(
        StatusVariant.Danger,
        stringResource(R.string.res_failed_title_refresh),
        body =
            stringResource(
                when (error) {
                    LoadError.Network, LoadError.Unauthorized -> R.string.res_failed_network
                    LoadError.Server -> R.string.res_failed_server
                },
            ),
    )
}

private fun formatResearchDate(
    instant: Instant,
    now: Instant = Instant.now(),
    zone: ZoneId = ZoneId.systemDefault(),
): String {
    val date = instant.atZone(zone)
    return dayMonthFormatter(Locale.getDefault(), withYear = date.year != now.atZone(zone).year).format(date)
}

/** Compact and medium: the side column becomes a line on top (resources.md 3.2). */
@Composable
private fun SummaryRow(
    view: ResourcesView,
    showRefresh: Boolean,
    acting: Boolean,
    onRefresh: () -> Unit,
) {
    val research = view.research ?: return
    val count = countText(view.resources.size)
    val text =
        research.finishedAt?.let {
            stringResource(R.string.res_row_summary, count, formatResearchDate(it), research.missionVersion)
        } ?: stringResource(R.string.res_row_summary_no_date, count, research.missionVersion)
    Column {
        TeachText(text, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
        if (showRefresh) {
            TeachButton(
                stringResource(if (acting) R.string.res_refresh_busy else R.string.res_refresh),
                onRefresh,
                variant = ButtonVariant.Text,
                loading = acting,
            )
        }
    }
}

/** Expanded: "La ricerca", without a box, like the context pane of the chat (resources.md 3.1). */
@Composable
private fun SideColumn(
    view: ResourcesView,
    showRefresh: Boolean,
    acting: Boolean,
    onRefresh: () -> Unit,
) {
    val research = view.research ?: return
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachText(
            stringResource(R.string.res_side_title),
            Modifier.semantics { heading() },
            style = TeachTheme.typography.ui.title3,
        )
        val muted = TeachTheme.colors.inkSoft
        val body = TeachTheme.typography.ui.bodyS
        research.finishedAt?.let {
            TeachText(stringResource(R.string.res_side_when, formatResearchDate(it)), style = body, color = muted)
        }
        TeachText(stringResource(R.string.res_side_mission, research.missionVersion), style = body, color = muted)
        val knowledge = view.knowledge.size
        val community = view.community.size
        val total = countText(view.resources.size)
        TeachText(
            if (community > 0) {
                stringResource(R.string.res_side_count_split, total, knowledge, community)
            } else {
                stringResource(R.string.res_side_count_knowledge_only, total)
            },
            style = body,
            color = muted,
        )
        if (showRefresh) {
            TeachButton(
                stringResource(if (acting) R.string.res_refresh_busy else R.string.res_refresh),
                onRefresh,
                variant = ButtonVariant.Secondary,
                loading = acting,
                modifier = Modifier.padding(top = TeachTheme.spacing.xs),
            )
        }
    }
}

// Previews

@Composable
private fun PreviewTab(
    topicState: TopicState,
    state: ResourcesUiState,
    currentMissionVersion: Int? = 2,
    documents: DocumentsUiState = DocumentsUiState(load = DocumentsLoad.Ready),
) {
    val toast = remember { TeachToastState() }
    Box(Modifier.background(TeachTheme.colors.bg)) {
        ResourcesTab(
            topicState,
            PreviewData.WHY,
            currentMissionVersion,
            hasProposal = false,
            state,
            ResourcesTabActions({}, {}, {}, {}, {}, {}),
            documents,
            PreviewData.noDocumentActions,
            toast,
        )
    }
}

private fun ready(view: ResourcesView) = ResourcesUiState(load = ResourcesLoad.Ready, view = view)

private val emptyView = ResourcesView(emptyList(), emptyList(), false, null, null)

@Preview(name = "Empty", showBackground = true, widthDp = 640, heightDp = 500)
@Composable
private fun EmptyPreview() = TeachTheme(darkTheme = false) { PreviewTab(TopicState.Interviewing, ready(emptyView)) }

@Preview(name = "Running step", showBackground = true, widthDp = 640, heightDp = 800)
@Composable
private fun RunningPreview() =
    TeachTheme(darkTheme = false) {
        PreviewTab(
            TopicState.Researching,
            ready(emptyView.copy(job = PreviewData.job(JobStatus.Running, "searching", "Ho consultato 14 risultati"))),
        )
    }

@Preview(name = "Done wide", showBackground = true, widthDp = 1280, heightDp = 1400)
@Composable
private fun DoneWidePreview() =
    TeachTheme(darkTheme = false) {
        PreviewTab(TopicState.Ready, ready(PreviewData.done), 2)
    }

@Preview(name = "Done wide dark", showBackground = true, widthDp = 1280, heightDp = 1400)
@Composable
private fun DoneWideDarkPreview() =
    TeachTheme(darkTheme = true) {
        PreviewTab(TopicState.Ready, ready(PreviewData.done), 2)
    }

@Preview(name = "Done stale banner", showBackground = true, widthDp = 640, heightDp = 1600)
@Composable
private fun StalePreview() = TeachTheme(darkTheme = false) { PreviewTab(TopicState.Ready, ready(PreviewData.done), 3) }

@Preview(name = "Done compact", showBackground = true, widthDp = 360, heightDp = 1800)
@Composable
private fun DoneCompactPreview() =
    TeachTheme(darkTheme = false) {
        PreviewTab(TopicState.Ready, ready(PreviewData.done), 2)
    }

@Preview(name = "Done compact font 2.0", showBackground = true, widthDp = 360, heightDp = 3200, fontScale = 2f)
@Composable
private fun DoneLargePreview() =
    TeachTheme(darkTheme = false) {
        PreviewTab(TopicState.Ready, ready(PreviewData.done), 2)
    }

@Preview(name = "No sources", showBackground = true, widthDp = 640, heightDp = 700)
@Composable
private fun NoSourcesPreview() =
    TeachTheme(darkTheme = false) {
        PreviewTab(TopicState.Ready, ready(PreviewData.noSources))
    }

@Preview(name = "Communities skipped", showBackground = true, widthDp = 640, heightDp = 900)
@Composable
private fun SkippedPreview() =
    TeachTheme(darkTheme = false) {
        PreviewTab(TopicState.Ready, ready(PreviewData.skipped), 2)
    }

@Preview(name = "Failed", showBackground = true, widthDp = 640, heightDp = 500)
@Composable
private fun FailedPreview() =
    TeachTheme(darkTheme = false) {
        PreviewTab(
            TopicState.Researching,
            ready(emptyView.copy(job = PreviewData.job(JobStatus.Failed, errorCode = "upstream_failed"))),
        )
    }

@Preview(name = "Refreshing", showBackground = true, widthDp = 640, heightDp = 1800)
@Composable
private fun RefreshingPreview() =
    TeachTheme(darkTheme = false) {
        PreviewTab(
            TopicState.Ready,
            ready(PreviewData.done.copy(job = PreviewData.job(JobStatus.Running, "reading"))),
            3,
        )
    }

@Preview(name = "Refresh failed dark", showBackground = true, widthDp = 640, heightDp = 1800)
@Composable
private fun RefreshFailedPreview() =
    TeachTheme(darkTheme = true) {
        PreviewTab(
            TopicState.Ready,
            ready(PreviewData.done.copy(job = PreviewData.job(JobStatus.Failed, errorCode = "timeout"))),
            3,
        )
    }

@Preview(name = "Loading", showBackground = true, widthDp = 640, heightDp = 400)
@Composable
private fun LoadingPreview() = TeachTheme(darkTheme = false) { PreviewTab(TopicState.Researching, ResourcesUiState()) }
