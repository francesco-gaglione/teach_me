package com.teachme.feature.settings

import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachErrorState
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachListSkeleton
import com.teachme.core.designsystem.components.TeachRowDivider
import com.teachme.core.designsystem.components.TeachScreen
import com.teachme.core.designsystem.components.TeachSpinner
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachToastHost
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.components.TeachTopBar
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.LoadError
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.dayMonthFormatter
import com.teachme.core.ui.intervalText
import com.teachme.sdui.inline.InlineMarkdown
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.render.ProvideStandaloneSdui
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.ZoneId
import java.util.Locale

private const val FRONT_MAX_LINES = 3
private const val NEAR_END_ROWS = 5
private const val BIG_FONT_SCALE = 1.8f
private const val SUPERSEDED_TOAST_MS = 6_000L
private const val UNDO_FAILED_TOAST_MS = 4_000L
private const val ROW_EXIT_MS = 200

/** `settings/suspended`: the cards the user suspended by hand, one tap to bring each back (suspended.md). */
@Composable
fun SuspendedRoute(
    onBack: () -> Unit,
    viewModel: SuspendedViewModel = hiltViewModel(),
    now: Instant = Instant.now(),
    zone: ZoneId = ZoneId.systemDefault(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val toast = remember { TeachToastState() }
    val scope = rememberCoroutineScope()
    val resources = LocalResources.current
    val focusEvents = remember { Channel<String?>(Channel.CONFLATED) }
    var toastJob by remember { mutableStateOf<Job?>(null) }

    // The card whose "Annulla" is on screen: Ctrl+Z does the same while the toast is visible.
    var undoable by remember { mutableStateOf<String?>(null) }

    val undoLabel = stringResource(R.string.suspended_undo)
    val nowText = stringResource(R.string.suspended_toast_now)
    val plainText = stringResource(R.string.suspended_toast_plain)
    val supersededText = stringResource(R.string.suspended_toast_superseded)
    val undoFailedText = stringResource(R.string.suspended_toast_undo_failed)
    val undoGoneText = stringResource(R.string.suspended_toast_undo_gone)
    val undoSupersededText = stringResource(R.string.suspended_toast_undo_superseded)

    fun showToast(
        text: String,
        action: String? = null,
        onAction: (() -> Unit)? = null,
        durationMs: Long? = null,
        cardId: String? = null,
    ) {
        toastJob?.cancel()
        undoable = cardId
        toastJob =
            scope.launch {
                if (action == null) {
                    toast.showFor(text, durationMs ?: UNDO_FAILED_TOAST_MS)
                } else {
                    toast.show(text, action, onAction)
                }
                undoable = null
            }
    }

    fun undo(cardId: String) {
        toastJob?.cancel()
        toast.dismiss()
        undoable = null
        viewModel.undo(cardId)
    }

    LaunchedEffect(viewModel) {
        viewModel.effects.collect { effect ->
            when (effect) {
                is SuspendedEffect.Reactivated -> {
                    val seconds = effect.dueInSeconds
                    val text =
                        when {
                            seconds == null -> plainText
                            seconds <= 0 -> nowText
                            else ->
                                resources
                                    .intervalText(
                                        seconds,
                                    )?.let { resources.getString(R.string.suspended_toast_later, it) }
                                    ?: nowText
                        }
                    showToast(text, undoLabel, { undo(effect.cardId) }, cardId = effect.cardId)
                }
                SuspendedEffect.Superseded -> showToast(supersededText, durationMs = SUPERSEDED_TOAST_MS)
                SuspendedEffect.UndoFailed -> showToast(undoFailedText, durationMs = UNDO_FAILED_TOAST_MS)
                SuspendedEffect.UndoGone -> showToast(undoGoneText, durationMs = UNDO_FAILED_TOAST_MS)
                SuspendedEffect.UndoSuperseded -> showToast(undoSupersededText, durationMs = SUPERSEDED_TOAST_MS)
                is SuspendedEffect.MoveFocus -> focusEvents.trySend(effect.rowId)
            }
        }
    }

    SuspendedScreen(
        state = state,
        toast = toast,
        onBack = onBack,
        onRetry = viewModel::refresh,
        onLoadMore = viewModel::loadMore,
        onReactivate = viewModel::reactivate,
        focusEvents = focusEvents.receiveAsFlow(),
        now = now,
        zone = zone,
        modifier =
            Modifier.onPreviewKeyEvent { event ->
                val id = undoable
                if (id != null && event.type == KeyEventType.KeyDown && event.isCtrlPressed && event.key == Key.Z) {
                    undo(id)
                    true
                } else {
                    false
                }
            },
    )
}

@Composable
internal fun SuspendedScreen(
    state: SuspendedUiState,
    toast: TeachToastState,
    onBack: () -> Unit,
    onRetry: () -> Unit,
    onLoadMore: () -> Unit,
    onReactivate: (String) -> Unit,
    modifier: Modifier = Modifier,
    /** Row id to focus (null: the title) after a row left; see [SuspendedEffect.MoveFocus]. */
    focusEvents: Flow<String?> = emptyFlow(),
    now: Instant = Instant.now(),
    zone: ZoneId = ZoneId.systemDefault(),
) {
    val titleFocus = remember { FocusRequester() }
    val rowFocus = remember { mutableMapOf<String, FocusRequester>() }
    LaunchedEffect(focusEvents) {
        focusEvents.collect { id ->
            // The row may be off screen (not composed): then the title is the fallback.
            val moved = id?.let { rowFocus[it] }?.let { runCatching { it.requestFocus() }.getOrDefault(false) } == true
            if (!moved) runCatching { titleFocus.requestFocus() }
        }
    }
    TeachScreen(
        modifier = modifier,
        topBar = {
            TeachTopBar(
                title = stringResource(R.string.suspended_title),
                titleModifier = Modifier.focusRequester(titleFocus).focusable(),
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
        val rows = state.rows
        when {
            rows == null && state.loadError != null -> LoadFailed(state.loadError, onRetry)
            rows == null -> Loading(state.slowLoading)
            state.isEmpty -> EmptyState()
            else -> RowsList(rows, state, now, zone, rowFocus, onLoadMore, onReactivate)
        }
    }
}

@Composable
private fun Loading(slow: Boolean) {
    Box(Modifier.padding(screenMargin())) {
        TeachListSkeleton(
            description = stringResource(R.string.suspended_loading),
            stateText = stringResource(R.string.suspended_loading_state),
            withIcon = false,
            slowText = if (slow) stringResource(R.string.suspended_loading_slow) else null,
        )
    }
}

@Composable
private fun LoadFailed(
    error: LoadError,
    onRetry: () -> Unit,
) {
    val server = error == LoadError.Server
    TeachErrorState(
        title =
            stringResource(
                if (server) R.string.suspended_load_server_title else R.string.suspended_load_network_title,
            ),
        body =
            stringResource(
                if (server) R.string.suspended_load_server_body else R.string.suspended_load_network_body,
            ),
        primaryLabel = stringResource(R.string.suspended_retry),
        onPrimary = onRetry,
    )
}

@Composable
private fun EmptyState() {
    Column(
        Modifier.fillMaxSize().padding(screenMargin()),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        TeachIcon(TeachIcons.Pause, contentDescription = null, tint = TeachTheme.colors.inkSoft, size = 48.dp)
        TeachText(stringResource(R.string.suspended_empty_title), style = TeachTheme.typography.ui.title2)
        TeachText(
            stringResource(R.string.suspended_empty_body),
            style = TeachTheme.typography.ui.bodyL,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

@Composable
private fun RowsList(
    rows: List<SuspendedRow>,
    state: SuspendedUiState,
    now: Instant,
    zone: ZoneId,
    rowFocus: MutableMap<String, FocusRequester>,
    onLoadMore: () -> Unit,
    onReactivate: (String) -> Unit,
) {
    val listState = rememberLazyListState()
    val nearEnd by remember { derivedStateOf { listState.nearEnd() } }
    LaunchedEffect(nearEnd, state.nextCursor, rows.size) { if (nearEnd) onLoadMore() }
    val widthClass = currentWindowWidthClass()
    val bigFont = LocalDensity.current.fontScale >= BIG_FONT_SCALE
    // At 200 % the button goes under the text even on a tablet (suspended.md section 5).
    val sideBySide = widthClass != WindowWidthClass.Compact && !bigFont
    val colors = TeachTheme.colors
    val shape = TeachTheme.shapes.md
    // A row leaves with a 200 ms fade while the ones below slide up (the collapse); with "reduce animations" it is instant.
    val reduced = TeachTheme.reducedMotion
    val exitSpec: FiniteAnimationSpec<Float>? = if (reduced) null else tween(ROW_EXIT_MS)
    val moveSpec: FiniteAnimationSpec<IntOffset>? = if (reduced) null else tween(ROW_EXIT_MS)
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(
            Modifier
                .widthIn(max = TeachTheme.layout.contentMaxWidth)
                .fillMaxWidth()
                .padding(horizontal = screenMargin(widthClass))
                .padding(bottom = TeachTheme.spacing.md),
        ) {
            TeachText(
                pluralStringResource(R.plurals.suspended_count, state.count, state.count),
                Modifier.padding(vertical = TeachTheme.spacing.xs).semantics { heading() },
                style = TeachTheme.typography.ui.label,
                color = colors.inkSoft,
            )
            ProvideStandaloneSdui {
                LazyColumn(
                    Modifier
                        .weight(1f, fill = false)
                        .clip(shape)
                        .background(colors.surface)
                        .border(BorderStroke(TeachTheme.stroke.hairline, colors.line), shape),
                    state = listState,
                ) {
                    items(rows, key = { it.card.id }) { row ->
                        val focus = remember(row.card.id) { FocusRequester() }
                        DisposableEffect(row.card.id, focus) {
                            rowFocus[row.card.id] = focus
                            onDispose { if (rowFocus[row.card.id] === focus) rowFocus.remove(row.card.id) }
                        }
                        Column(
                            Modifier.animateItem(
                                fadeInSpec = exitSpec,
                                placementSpec = moveSpec,
                                fadeOutSpec = exitSpec,
                            ),
                        ) {
                            SuspendedRowItem(
                                row = row,
                                now = now,
                                zone = zone,
                                sideBySide = sideBySide,
                                unlimitedFront = bigFont,
                                focus = focus,
                                onReactivate = { onReactivate(row.card.id) },
                            )
                            TeachRowDivider()
                        }
                    }
                    if (state.pageLoading) item(key = "page-loading") { PageLoading() }
                    if (state.pageFailed) item(key = "page-failed") { PageFailed(onLoadMore) }
                }
            }
        }
    }
}

private fun LazyListState.nearEnd(): Boolean {
    val info = layoutInfo
    val last = info.visibleItemsInfo.lastOrNull()?.index ?: return false
    return info.totalItemsCount > 0 && last >= info.totalItemsCount - NEAR_END_ROWS
}

@Composable
private fun PageLoading() {
    Row(
        Modifier.fillMaxWidth().padding(TeachTheme.spacing.md),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TeachSpinner(size = 24.dp)
        TeachText(
            stringResource(R.string.suspended_page_loading),
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

@Composable
private fun PageFailed(onRetry: () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(TeachTheme.spacing.sm), horizontalAlignment = Alignment.CenterHorizontally) {
        TeachText(
            stringResource(R.string.suspended_page_failed),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
        TeachButton(stringResource(R.string.suspended_retry), onRetry, variant = ButtonVariant.Text)
    }
}

@Composable
private fun whenText(
    at: Instant?,
    now: Instant,
    zone: ZoneId,
): String {
    // The backend always sends it for a suspended card; "today" is the harmless reading of a missing one.
    val whenSuspended = if (at == null) SuspendedWhen.Today else suspendedWhen(at, now, zone)
    return when (whenSuspended) {
        SuspendedWhen.Today -> stringResource(R.string.suspended_when_today)
        SuspendedWhen.Yesterday -> stringResource(R.string.suspended_when_yesterday)
        is SuspendedWhen.On -> {
            val locale = androidx.compose.ui.text.intl.Locale.current.platformLocale
            stringResource(
                R.string.suspended_when_on,
                whenSuspended.date.format(dayMonthFormatter(locale, whenSuspended.withYear, abbreviated = true)),
            )
        }
    }
}

@Composable
private fun originText(card: ReviewCard): String {
    val number = card.lessonNumber
    val title = card.lessonTitle
    return if (number != null && !title.isNullOrBlank()) {
        stringResource(R.string.suspended_origin, card.topicTitle, number, title)
    } else {
        card.topicTitle
    }
}

@Composable
private fun SuspendedRowItem(
    row: SuspendedRow,
    now: Instant,
    zone: ZoneId,
    sideBySide: Boolean,
    unlimitedFront: Boolean,
    focus: FocusRequester,
    onReactivate: () -> Unit,
) {
    val card = row.card
    val colors = TeachTheme.colors
    val plainFront = remember(card.front) { InlineMarkdown.plainText(card.front) }
    val whenText = whenText(card.suspendedAt, now, zone)
    val lessonNumber = card.lessonNumber
    val lessonTitle = card.lessonTitle
    val groupDescription =
        if (lessonNumber != null && !lessonTitle.isNullOrBlank()) {
            stringResource(
                R.string.suspended_row_a11y,
                plainFront,
                card.topicTitle,
                lessonNumber,
                InlineMarkdown.plainText(lessonTitle),
                whenText,
            )
        } else {
            stringResource(R.string.suspended_row_a11y_topic, plainFront, card.topicTitle, whenText)
        }
    val reactivateLabel = stringResource(R.string.suspended_reactivate)
    val busy = row.phase == RowPhase.Busy
    val failed = row.phase == RowPhase.Failed
    val frontForButton = plainFront.frontForButton()

    val text: @Composable (Modifier) -> Unit = { modifier ->
        Column(
            modifier.focusRequester(focus).focusable().semantics(mergeDescendants = true) {
                contentDescription = groupDescription
                customActions =
                    listOf(
                        CustomAccessibilityAction(reactivateLabel) {
                            if (!busy) onReactivate()
                            true
                        },
                    )
            },
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
        ) {
            InlineText(
                card.front,
                style = TeachTheme.typography.ui.bodyL,
                interactive = false,
                maxLines = if (unlimitedFront) Int.MAX_VALUE else FRONT_MAX_LINES,
            )
            TeachText(
                originText(card),
                style = TeachTheme.typography.ui.body,
                color = colors.inkSoft,
                maxLines = if (unlimitedFront) Int.MAX_VALUE else 1,
                overflow = TextOverflow.Ellipsis,
            )
            TeachText(whenText, style = TeachTheme.typography.ui.bodyS, color = colors.inkSoft)
        }
    }
    val button: @Composable () -> Unit = {
        TeachButton(
            text =
                stringResource(
                    when (row.phase) {
                        RowPhase.Idle -> R.string.suspended_reactivate
                        RowPhase.Busy -> R.string.suspended_reactivating
                        RowPhase.Failed -> R.string.suspended_retry
                    },
                ),
            onClick = onReactivate,
            variant = ButtonVariant.Secondary,
            loading = busy,
            leadingIcon = if (busy) null else TeachIcons.RotateCcw,
            contentDescription =
                stringResource(
                    when (row.phase) {
                        RowPhase.Idle -> R.string.suspended_reactivate_a11y
                        RowPhase.Busy -> R.string.suspended_reactivating_a11y
                        RowPhase.Failed -> R.string.suspended_retry_a11y
                    },
                    frontForButton,
                ),
        )
    }

    Column(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 72.dp)
            .padding(horizontal = TeachTheme.spacing.md, vertical = TeachTheme.spacing.sm),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        if (sideBySide) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                text(Modifier.weight(1f))
                button()
            }
        } else {
            text(Modifier)
        }
        if (failed) RowFailure()
        if (!sideBySide) button()
    }
}

@Composable
private fun RowFailure() {
    Row(
        Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Assertive },
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TeachIcon(
            TeachIcons.CloudOff,
            contentDescription = null,
            tint = TeachTheme.colors.inkSoft,
            size = TeachTheme.layout.iconSizeSmall,
        )
        TeachText(
            stringResource(R.string.suspended_row_failed),
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

// region Previews

private val previewNow = Instant.parse("2026-11-16T10:00:00Z")

private fun previewCard(
    id: String,
    front: String,
    suspendedAt: String,
) = ReviewCard(
    id = id,
    topicId = "t",
    topicTitle = "Rust",
    lessonId = "l",
    lessonNumber = 2,
    lessonTitle = "Result e operatore ?",
    kind = com.teachme.core.model.ReviewCardKind.Record,
    front = front,
    back = "",
    status = com.teachme.core.model.ReviewCardStatus.Suspended,
    suspension = com.teachme.core.model.SuspensionReason.User,
    suspendedAt = Instant.parse(suspendedAt),
    state = com.teachme.core.model.ReviewState.Review,
    due = null,
    lastReviewedAt = null,
    reviewCount = 3,
    intervals = null,
)

private val previewRows =
    listOf(
        SuspendedRow(previewCard("1", "Quando compila l'operatore `?` dentro una funzione?", "2026-11-15T09:00:00Z")),
        SuspendedRow(previewCard("2", "Che cosa restituisce `Option::take`?", "2026-11-12T09:00:00Z")),
        SuspendedRow(previewCard("3", "A cosa serve `Rc<RefCell<T>>`?", "2025-03-02T09:00:00Z")),
    )

@Composable
private fun PreviewHost(
    state: SuspendedUiState,
    dark: Boolean = false,
) {
    TeachTheme(darkTheme = dark) {
        SuspendedScreen(
            state,
            remember { TeachToastState() },
            {},
            {},
            {},
            {},
            now = previewNow,
            zone = ZoneId.of("UTC"),
        )
    }
}

@Preview(name = "Suspended", showBackground = true)
@Composable
private fun PreviewList() = PreviewHost(SuspendedUiState(previewRows, count = 3))

@Preview(name = "Suspended dark", showBackground = true)
@Composable
private fun PreviewListDark() = PreviewHost(SuspendedUiState(previewRows, count = 3), dark = true)

@Preview(name = "Suspended busy and failed", showBackground = true)
@Composable
private fun PreviewBusyFailed() =
    PreviewHost(
        SuspendedUiState(
            listOf(
                previewRows[0].copy(phase = RowPhase.Busy),
                previewRows[1].copy(phase = RowPhase.Failed),
                previewRows[2],
            ),
            count = 3,
        ),
    )

@Preview(name = "Suspended empty", showBackground = true)
@Composable
private fun PreviewEmpty() = PreviewHost(SuspendedUiState(emptyList()))

@Preview(name = "Suspended loading", showBackground = true)
@Composable
private fun PreviewLoading() = PreviewHost(SuspendedUiState())

@Preview(name = "Suspended error", showBackground = true)
@Composable
private fun PreviewError() = PreviewHost(SuspendedUiState(loadError = LoadError.Network))

@Preview(name = "Suspended font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun PreviewLargeFont() = PreviewHost(SuspendedUiState(previewRows, count = 3))

@Preview(name = "Suspended tablet", showBackground = true, device = "spec:width=1280dp,height=800dp,dpi=240")
@Composable
private fun PreviewTablet() = PreviewHost(SuspendedUiState(previewRows, count = 3))

// endregion
