package com.teachme.feature.review

import android.content.Context
import android.view.accessibility.AccessibilityManager
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ActionMenuItem
import com.teachme.core.designsystem.components.ButtonSize
import com.teachme.core.designsystem.components.TeachActionMenu
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachConfirmDialog
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachScreenFrame
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachTopBar
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.model.ReviewRating
import com.teachme.core.ui.actionPanel
import com.teachme.sdui.render.ProvideStandaloneSdui
import com.teachme.sdui.render.digitOf
import com.teachme.sdui.render.rememberHardwareKeyboard
import kotlinx.coroutines.android.awaitFrame
import java.time.Instant
import com.teachme.core.ui.R as UiR

private const val UNDO_VISIBLE_MS = 5_000
private const val ANCHORED_BUTTON_SPACE_DP = 80
private val CARD_GAP_WIDE = 24.dp

/** What the screen asks of its state holder. */
data class ReviewActions(
    val onClose: () -> Unit = {},
    val onReveal: () -> Unit = {},
    val onRate: (ReviewRating) -> Unit = {},
    val onUndo: () -> Unit = {},
    val onOpenSuspend: () -> Unit = {},
    val onDismissSuspend: () -> Unit = {},
    val onConfirmSuspend: () -> Unit = {},
    val onRetry: () -> Unit = {},
    val onOpenSettings: () -> Unit = {},
    val onSyncNow: () -> Unit = {},
    val onReviewAll: () -> Unit = {},
    val onOpenLesson: (String) -> Unit = {},
)

/** The review screen (review.md): full window on every width class. */
@Composable
fun ReviewRoute(
    onClose: () -> Unit,
    onOpenSettings: () -> Unit,
    onOpenLesson: (lessonId: String) -> Unit = {},
    viewModel: ReviewViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    LifecycleEventEffect(Lifecycle.Event.ON_START) { viewModel.syncNow() }
    // With accessibility services the row that says "Annulla" stays longer (never less than 5 s).
    LaunchedEffect(Unit) { viewModel.undoVisibleMs = recommendedUndoMs(context) }
    BackHandler(onBack = onClose)
    ReviewScreen(
        state = state,
        actions =
            ReviewActions(
                onClose = onClose,
                onReveal = viewModel::reveal,
                onRate = viewModel::rate,
                onUndo = viewModel::undo,
                onOpenSuspend = viewModel::openSuspend,
                onDismissSuspend = viewModel::dismissSuspend,
                onConfirmSuspend = viewModel::confirmSuspend,
                onRetry = viewModel::start,
                onOpenSettings = onOpenSettings,
                onSyncNow = viewModel::syncNow,
                onReviewAll = viewModel::reviewAll,
                onOpenLesson = onOpenLesson,
            ),
        canUndo = viewModel::canUndo,
    )
}

private fun recommendedUndoMs(context: Context): Long {
    val manager = context.getSystemService(Context.ACCESSIBILITY_SERVICE) as? AccessibilityManager
    if (manager == null) return UNDO_VISIBLE_MS.toLong()
    val flags = AccessibilityManager.FLAG_CONTENT_CONTROLS or AccessibilityManager.FLAG_CONTENT_TEXT
    return manager.getRecommendedTimeoutMillis(UNDO_VISIBLE_MS, flags).toLong().coerceAtLeast(UNDO_VISIBLE_MS.toLong())
}

@Composable
fun ReviewScreen(
    state: ReviewUiState,
    actions: ReviewActions,
    canUndo: () -> Boolean,
    modifier: Modifier = Modifier,
    now: Instant = Instant.now(),
) {
    val hardwareKeyboard = rememberHardwareKeyboard()
    val cardFocus = remember { FocusRequester() }
    val rateFocus = remember { FocusRequester() }
    val currentId = state.current?.id
    // New card: focus on its front; revealed: focus on the title of the grades (review.md section 10.2).
    LaunchedEffect(currentId, state.revealed) {
        if (currentId == null) return@LaunchedEffect
        awaitFrame()
        runCatching { (if (state.revealed) rateFocus else cardFocus).requestFocus() }
    }
    TeachScreenFrame {
        ProvideStandaloneSdui {
            Box(modifier.fillMaxSize().onPreviewKeyEvent { handleKey(it, state, actions, canUndo) }) {
                Column(Modifier.fillMaxSize()) {
                    ReviewTopBar(state, actions, canUndo)
                    ReviewBody(state, actions, hardwareKeyboard, cardFocus, rateFocus, now)
                }
                SentAnnouncement(state.sentAnnouncement)
                state.suspendDialog?.let { ui ->
                    TeachConfirmDialog(
                        title = stringResource(R.string.review_suspend_title),
                        body = stringResource(R.string.review_suspend_body),
                        confirmLabel = stringResource(R.string.review_suspend_confirm),
                        dismissLabel = stringResource(UiR.string.ui_cancel),
                        onConfirm = actions.onConfirmSuspend,
                        onDismiss = actions.onDismissSuspend,
                        loading = ui.submitting,
                        loadingLabel = stringResource(R.string.review_suspend_loading),
                        error =
                            ui.error?.action?.let {
                                actionPanel(
                                    it,
                                    stringResource(R.string.review_suspend_not_done),
                                    actions.onOpenSettings,
                                )
                            },
                    )
                }
            }
        }
    }
}

/** Keyboard shortcuts (review.md section 10.1). Grades work only on the back: no vote without seeing the answer. */
private fun handleKey(
    event: KeyEvent,
    state: ReviewUiState,
    actions: ReviewActions,
    canUndo: () -> Boolean,
): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    return when {
        event.key == Key.Escape || (event.isAltPressed && event.key == Key.DirectionLeft) -> {
            actions.onClose()
            true
        }
        event.isCtrlPressed && event.key == Key.Z -> {
            if (canUndo()) actions.onUndo()
            true
        }
        state.phase == ReviewPhase.Card &&
            !state.revealed &&
            (event.key == Key.Spacebar || event.key == Key.Enter || event.key == Key.NumPadEnter) -> {
            actions.onReveal()
            true
        }
        state.phase == ReviewPhase.Card && state.revealed && event.isAltPressed && event.key == Key.L -> {
            val lessonId = state.current?.lessonId
            if (lessonId != null) actions.onOpenLesson(lessonId)
            lessonId != null
        }
        state.phase == ReviewPhase.Card && state.revealed -> {
            val rating = digitOf(event)?.let { RATINGS.getOrNull(it - 1) }
            if (rating != null) actions.onRate(rating)
            rating != null
        }
        else -> false
    }
}

@Composable
private fun ReviewTopBar(
    state: ReviewUiState,
    actions: ReviewActions,
    canUndo: () -> Boolean,
) {
    var menuOpen by rememberSaveable { mutableStateOf(false) }
    val canSuspend = state.current != null
    val hasMenu = canSuspend || state.votes.isNotEmpty()
    val topic = state.topicTitle
    val topicDescription = topic?.let { stringResource(R.string.review_title_topic_a11y, it) }
    TeachTopBar(
        title =
            if (topic !=
                null
            ) {
                stringResource(R.string.review_title_topic, topic)
            } else {
                stringResource(R.string.review_title)
            },
        titleModifier =
            if (topicDescription != null) {
                Modifier.clearAndSetSemantics { contentDescription = topicDescription }
            } else {
                Modifier
            },
        navigation = {
            TeachIconButton(
                TeachIcons.X,
                stringResource(R.string.review_close),
                actions.onClose,
                tint = TeachTheme.colors.ink,
            )
        },
        actions = {
            ReviewQueueIndicator(state.pendingCount, actions.onSyncNow)
            if (hasMenu) {
                Box {
                    TeachIconButton(TeachIcons.MoreVertical, stringResource(R.string.review_more_actions), {
                        menuOpen =
                            true
                    })
                    val undoLabel = stringResource(R.string.review_menu_undo)
                    val suspendLabel = stringResource(R.string.review_menu_suspend)
                    TeachActionMenu(
                        expanded = menuOpen,
                        onDismiss = { menuOpen = false },
                        title = stringResource(R.string.review_menu_title),
                        items =
                            buildList {
                                if (canUndo()) add(ActionMenuItem(undoLabel, TeachIcons.RotateCcw, actions.onUndo))
                                if (canSuspend) {
                                    add(
                                        ActionMenuItem(suspendLabel, TeachIcons.Pause, actions.onOpenSuspend),
                                    )
                                }
                            },
                    )
                }
            }
        },
    )
}

@Composable
private fun ReviewBody(
    state: ReviewUiState,
    actions: ReviewActions,
    hardwareKeyboard: Boolean,
    cardFocus: FocusRequester,
    rateFocus: FocusRequester,
    now: Instant,
) {
    when (state.phase) {
        ReviewPhase.Loading -> ReviewColumn { ReviewLoading(state.slowLoading) }
        ReviewPhase.Failed ->
            ReviewLoadError(
                state.loadError ?: com.teachme.core.model.LoadError.Network,
                actions.onRetry,
                actions.onClose,
            )
        ReviewPhase.Empty ->
            ReviewColumn {
                ReviewEmpty(state.emptyNextDueAt, now, actions.onClose, topicTitle = state.topicTitle)
            }
        ReviewPhase.TopicGone -> ReviewTopicGone(actions.onReviewAll, actions.onClose)
        ReviewPhase.Finished ->
            ReviewColumn {
                Column {
                    UndoRow(state.notice, actions.onUndo)
                    FinishContent(state.finishSummary(), now, actions.onClose, actions.onRetry)
                }
            }
        ReviewPhase.Card -> CardPhase(state, actions, hardwareKeyboard, cardFocus, rateFocus)
    }
}

/** The card, its reveal button and the grades. Compact anchors "Mostra risposta" to the bottom (review.md 9.3). */
@Composable
private fun CardPhase(
    state: ReviewUiState,
    actions: ReviewActions,
    hardwareKeyboard: Boolean,
    cardFocus: FocusRequester,
    rateFocus: FocusRequester,
) {
    val card = state.current ?: return
    val compact = currentWindowWidthClass() == WindowWidthClass.Compact
    val anchored = compact && !state.revealed
    val gap = if (compact) TeachTheme.spacing.md else CARD_GAP_WIDE
    val bottom = if (anchored) (ANCHORED_BUTTON_SPACE_DP * LocalDensity.current.fontScale).dp else TeachTheme.spacing.lg
    ReviewColumn(scroll = false) {
        Box(Modifier.fillMaxSize()) {
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(
                        rememberScrollState(),
                    ).padding(top = TeachTheme.spacing.xxs, bottom = bottom),
            ) {
                ProgressRow(state.index + 1, state.total)
                UndoRow(state.notice, actions.onUndo)
                Spacer(Modifier.height(TeachTheme.spacing.xs))
                ReviewCardView(
                    card,
                    state.index + 1,
                    state.total,
                    state.revealed,
                    showHint = state.index == 0,
                    focus = cardFocus,
                    firstOfSession = state.index == 0,
                    onOpenLesson = actions.onOpenLesson,
                    hardwareKeyboard = hardwareKeyboard,
                )
                Spacer(Modifier.height(gap))
                if (state.revealed) {
                    RateTitle(rateFocus)
                    Spacer(Modifier.height(TeachTheme.spacing.xs))
                    RatingButtons(card, actions.onRate, hardwareKeyboard)
                } else if (!anchored) {
                    RevealButton(actions.onReveal, hardwareKeyboard)
                }
            }
            if (anchored) {
                RevealButton(
                    actions.onReveal,
                    hardwareKeyboard,
                    Modifier.align(Alignment.BottomCenter).padding(bottom = TeachTheme.spacing.md),
                )
            }
        }
    }
}

/** The only primary button while the front is shown: it is not a grade, so it cannot steer the self-assessment. */
@Composable
private fun RevealButton(
    onReveal: () -> Unit,
    hardwareKeyboard: Boolean,
    modifier: Modifier = Modifier,
) {
    Box(modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
        TeachButton(stringResource(R.string.review_reveal), onReveal, Modifier.fillMaxWidth(), size = ButtonSize.Large)
        if (hardwareKeyboard) {
            TeachText(
                stringResource(R.string.review_reveal_hint),
                modifier =
                    Modifier
                        .padding(end = TeachTheme.spacing.md)
                        .border(
                            BorderStroke(TeachTheme.stroke.hairline, TeachTheme.colors.onAccent),
                            TeachTheme.shapes.sm,
                        ).padding(horizontal = TeachTheme.spacing.xs)
                        .clearAndSetSemantics { },
                style = TeachTheme.typography.ui.labelS,
                color = TeachTheme.colors.onAccent,
            )
        }
    }
}
