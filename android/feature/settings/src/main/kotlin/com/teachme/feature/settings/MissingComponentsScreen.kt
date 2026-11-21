package com.teachme.feature.settings

import android.content.ClipData
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.ChipVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachCard
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachListSkeleton
import com.teachme.core.designsystem.components.TeachScreen
import com.teachme.core.designsystem.components.TeachSpinner
import com.teachme.core.designsystem.components.TeachStatusChip
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachToastHost
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.components.TeachTopBar
import com.teachme.core.model.ComponentRequestExample
import com.teachme.core.model.ComponentRequestGroup
import com.teachme.core.model.LoadError
import com.teachme.core.model.dayMonthFormatter
import com.teachme.core.ui.LoadErrorState
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.ZoneId
import java.util.Locale

/** `settings/missing-components`: the ranking of `missing_component` asks (component-requests.md). */
@Composable
fun MissingComponentsRoute(
    onBack: () -> Unit,
    onOpenSettings: () -> Unit,
    viewModel: MissingComponentsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    MissingComponentsScreen(
        state = state,
        onBack = onBack,
        onRetry = viewModel::refresh,
        onLoadMore = viewModel::loadMore,
        onOpenSettings = onOpenSettings,
    )
}

@Composable
internal fun MissingComponentsScreen(
    state: MissingComponentsUiState,
    onBack: () -> Unit,
    onRetry: () -> Unit,
    onLoadMore: () -> Unit,
    onOpenSettings: () -> Unit,
) {
    val toast = remember { TeachToastState() }
    TeachScreen(
        topBar = {
            TeachTopBar(
                title = stringResource(R.string.missing_title),
                navigation = {
                    TeachIconButton(
                        TeachIcons.ChevronLeft,
                        stringResource(R.string.settings_back),
                        onBack,
                        tint = TeachTheme.colors.ink,
                    )
                },
            )
        },
        toast = { TeachToastHost(toast) },
    ) {
        val groups = state.groups
        when {
            groups == null && state.loadError != null ->
                LoadErrorState(state.loadError, onRetry = onRetry, onOpenSettings = onOpenSettings)
            groups == null ->
                Box(Modifier.padding(TeachTheme.layout.screenMarginCompact)) {
                    TeachListSkeleton(
                        description = stringResource(R.string.missing_loading),
                        stateText = stringResource(R.string.missing_loading_state),
                        withIcon = false,
                    )
                }
            groups.isEmpty() -> EmptyState()
            else -> GroupList(groups, state, toast, onRetry, onLoadMore)
        }
    }
}

@Composable
private fun EmptyState() {
    Column(
        Modifier.fillMaxSize().padding(TeachTheme.layout.screenMarginCompact),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        TeachIcon(TeachIcons.Puzzle, contentDescription = null, tint = TeachTheme.colors.inkMuted, size = 48.dp)
        TeachText(stringResource(R.string.missing_empty_title), style = TeachTheme.typography.ui.title3)
        TeachText(
            stringResource(R.string.missing_empty_body),
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

@Composable
private fun GroupList(
    groups: List<ComponentRequestGroup>,
    state: MissingComponentsUiState,
    toast: TeachToastState,
    onRetry: () -> Unit,
    onLoadMore: () -> Unit,
) {
    val listState = rememberLazyListState()
    val nearEnd by remember { derivedNearEnd(listState) }
    LaunchedEffect(nearEnd, state.nextCursor) { if (nearEnd) onLoadMore() }
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        LazyColumn(
            Modifier.widthIn(max = TeachTheme.layout.contentMaxWidth).fillMaxWidth(),
            state = listState,
            contentPadding =
                androidx.compose.foundation.layout
                    .PaddingValues(TeachTheme.layout.screenMarginCompact),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            item {
                TeachText(
                    stringResource(R.string.missing_intro),
                    style = TeachTheme.typography.ui.body,
                    color = TeachTheme.colors.inkSoft,
                )
            }
            itemsIndexed(groups, key = { _, g -> g.wanted }) { _, group -> GroupCard(group, toast) }
            if (state.pageLoading) {
                item {
                    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                        TeachSpinner(size = 24.dp)
                    }
                }
            }
            if (state.pageFailed) {
                item {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
                        TeachText(
                            stringResource(R.string.missing_page_failed),
                            style = TeachTheme.typography.ui.bodyS,
                            color = TeachTheme.colors.inkSoft,
                        )
                        TeachButton(
                            stringResource(R.string.missing_page_retry),
                            onLoadMore,
                            variant = ButtonVariant.Text,
                        )
                    }
                }
            }
            if (state.loadError != null) {
                item {
                    TeachButton(stringResource(R.string.missing_refresh_retry), onRetry, variant = ButtonVariant.Text)
                }
            }
        }
    }
}

private fun derivedNearEnd(listState: androidx.compose.foundation.lazy.LazyListState) =
    androidx.compose.runtime.derivedStateOf {
        val info = listState.layoutInfo
        val last = info.visibleItemsInfo.lastOrNull()?.index ?: 0
        info.totalItemsCount > 0 && last >= info.totalItemsCount - 3
    }

internal fun formatDay(
    instant: Instant?,
    zone: ZoneId = ZoneId.systemDefault(),
    locale: Locale = Locale.getDefault(),
): String? = instant?.atZone(zone)?.toLocalDate()?.format(dayMonthFormatter(locale, abbreviated = true))

/** "9 nov - 12 nov", or a single date when both fall on the same day. */
internal fun formatRange(
    first: Instant?,
    last: Instant?,
    zone: ZoneId = ZoneId.systemDefault(),
    locale: Locale = Locale.getDefault(),
): String? {
    val a = formatDay(first, zone, locale)
    val b = formatDay(last, zone, locale)
    return when {
        a == null -> b
        b == null || a == b -> a
        else -> "$a - $b"
    }
}

@Composable
private fun GroupCard(
    group: ComponentRequestGroup,
    toast: TeachToastState,
) {
    var expanded by rememberSaveable(group.wanted) { mutableStateOf(false) }
    val colors = TeachTheme.colors
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    val copied = stringResource(R.string.missing_copied)
    val countText = stringResource(R.string.missing_count, group.count)
    val description =
        pluralStringResource(
            R.plurals.missing_card_description,
            group.count,
            group.wanted,
            group.count,
            group.catalogVersion,
        )
    val range = formatRange(group.firstRequestedAt, group.lastRequestedAt)
    TeachCard(Modifier.fillMaxWidth()) {
        Column(
            Modifier.fillMaxWidth().padding(TeachTheme.spacing.md),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            Column(
                Modifier.semantics(mergeDescendants = true) { contentDescription = description },
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
                    // `wanted` is shown as is (monospace, never translated); long press copies it.
                    Box(
                        Modifier
                            .weight(1f)
                            .pointerInput(group.wanted) {
                                detectTapGestures(
                                    onLongPress = {
                                        scope.launch {
                                            clipboard.setClipEntry(ClipEntry(ClipData.newPlainText(null, group.wanted)))
                                            toast.show(copied)
                                        }
                                    },
                                )
                            },
                    ) {
                        TeachText(
                            group.wanted.replace("_", "_​"),
                            style =
                                TeachTheme.typography.ui.labelL.copy(
                                    fontFamily = TeachTheme.typography.code.inline.fontFamily,
                                ),
                        )
                    }
                    TeachStatusChip(countText, TeachIcons.Puzzle, variant = ChipVariant.Neutral)
                }
                TeachText(
                    listOfNotNull(
                        stringResource(R.string.missing_catalog, group.catalogVersion),
                        range,
                    ).joinToString(" · "),
                    style = TeachTheme.typography.ui.bodyS,
                    color = colors.inkSoft,
                )
                group.examples.firstOrNull()?.let {
                    TeachText(
                        it.purpose,
                        style = TeachTheme.typography.ui.bodyS,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            if (group.examples.isNotEmpty()) {
                TeachButton(
                    stringResource(
                        if (expanded) R.string.missing_hide_examples else R.string.missing_show_examples,
                        group.examples.size,
                    ),
                    { expanded = !expanded },
                    variant = ButtonVariant.Text,
                )
            }
            if (expanded) {
                group.examples.forEach { Example(it) }
            }
        }
    }
}

@Composable
private fun Example(example: ComponentRequestExample) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
        TeachText(
            "${example.lessonTitle} · ${example.topicTitle}",
            style =
                TeachTheme.typography.ui.bodyS
                    .copy(fontWeight = FontWeight.SemiBold),
        )
        TeachText(
            listOfNotNull(example.purpose, formatDay(example.requestedAt)).joinToString("  "),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

private val previewGroups =
    listOf(
        ComponentRequestGroup(
            "interactive_timeline",
            4,
            Instant.parse("2026-11-09T15:02:11Z"),
            Instant.parse("2026-11-12T08:40:01Z"),
            1,
            listOf(
                ComponentRequestExample(
                    "Mostrare l'ordine degli eventi della vita di un valore",
                    "Ownership e move",
                    "Rust",
                    Instant.parse("2026-11-12T08:40:01Z"),
                ),
            ),
        ),
        ComponentRequestGroup(
            "venn_diagram",
            1,
            Instant.parse("2026-11-10T10:00:00Z"),
            Instant.parse("2026-11-10T10:00:00Z"),
            1,
            emptyList(),
        ),
    )

@Preview(name = "Missing components", showBackground = true)
@Composable
private fun MissingLightPreview() =
    TeachTheme(darkTheme = false) {
        MissingComponentsScreen(MissingComponentsUiState(groups = previewGroups), {}, {}, {}, {})
    }

@Preview(name = "Missing components dark", showBackground = true)
@Composable
private fun MissingDarkPreview() =
    TeachTheme(darkTheme = true) {
        MissingComponentsScreen(MissingComponentsUiState(groups = previewGroups), {}, {}, {}, {})
    }

@Preview(name = "Missing components empty", showBackground = true)
@Composable
private fun MissingEmptyPreview() =
    TeachTheme(darkTheme = false) {
        MissingComponentsScreen(MissingComponentsUiState(groups = emptyList()), {}, {}, {}, {})
    }

@Preview(name = "Missing components error", showBackground = true)
@Composable
private fun MissingErrorPreview() =
    TeachTheme(darkTheme = false) {
        MissingComponentsScreen(MissingComponentsUiState(loadError = LoadError.Network), {}, {}, {}, {})
    }
