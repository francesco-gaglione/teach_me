package com.teachme.feature.lesson

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.CardVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachCard
import com.teachme.core.designsystem.components.TeachDisclosure
import com.teachme.core.designsystem.components.TeachErrorState
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachRowDivider
import com.teachme.core.designsystem.components.TeachSpinner
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.JobError
import com.teachme.core.model.JobProgress
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LearningRecord
import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LoadError
import com.teachme.core.model.TopicJob
import com.teachme.core.model.TopicState
import com.teachme.core.model.isLlmBudgetCode
import com.teachme.core.model.jobBudgetReset
import com.teachme.core.model.llmKeyVariantOfCode
import com.teachme.core.ui.LlmBudgetPanel
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyNotice
import com.teachme.core.ui.llmBudgetAvailability
import com.teachme.core.ui.rememberBudgetRenewed
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.render.ProvideStandaloneSdui
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import java.time.Duration
import java.time.Instant
import com.teachme.core.ui.R as UiR

/** Rows of the lessons list still below the visible part when the next page is requested (topic.md F8). */
private const val LESSONS_PREFETCH_ROWS = 3
private val LESSON_ROW_MIN_HEIGHT = 96.dp
private val LESSON_NUMBER_CIRCLE = 32.dp
private val SLOW_GENERATION = Duration.ofMinutes(4)
private const val CLOCK_TICK_MS = 30_000L
private const val GENERATION_CLOSE_MS = 600L
private const val MISSION_WHY_LINES = 4
private val PHASE_MIN_HEIGHT = 40.dp
private val DETAIL_INDENT = 28.dp
private val ROW_SKELETON_HEIGHT = 144.dp
private val EXPANDED_GAP = 32.dp

/** Everything the lessons tab can ask of its owner. */
@Immutable
class LessonsTabActions(
    val onGenerate: () -> Unit,
    val onRetryGeneration: () -> Unit,
    val onRetryLoad: () -> Unit,
    val onLoadMore: () -> Unit,
    val onOpenLesson: (Lesson) -> Unit,
    /** "Definisci la mission" / "Vai alla proposta": the chat, composer focused. */
    val onGoToChat: () -> Unit,
    val onOpenMission: () -> Unit,
    val onOpenResources: () -> Unit,
    val onOpenRegister: () -> Unit,
    val onOpenSettings: () -> Unit,
    /** The tab was left: the "Nuova" mark goes away. */
    val onLeave: () -> Unit,
    /** "Ripassa questo argomento" (topic.md F3.B2); only offered when [LessonsTab] gets a review entry. */
    val onOpenReview: () -> Unit = {},
)

/** Cards due in this Topic and the minutes they take (topic.md F3.B2); `cards` is already capped to one session. */
@Immutable
data class TopicReviewEntry(
    val cards: Int,
    val minutes: Int,
)

/**
 * The Lezioni tab (docs/design/screens/topic.md section F3-F5): the card "Prossima lezione" and the list of lessons. The
 * whole page scrolls; [header] (top bar, chip and tabs) scrolls with it on short windows. [wide] adds the context pane.
 */
@Composable
fun LessonsTab(
    topicState: TopicState,
    researchFailed: Boolean,
    state: LessonsUiState,
    missionWhy: String?,
    recentRecords: List<LearningRecord>,
    actions: LessonsTabActions,
    modifier: Modifier = Modifier,
    header: (@Composable () -> Unit)? = null,
    /** Titles of the Topic's `ready` documents (topic.md F10): the card and the pane say the lessons follow them. */
    pdfTitles: List<String> = emptyList(),
    /** Null hides the "Ripassa questo argomento" row (nothing due, or the check failed). */
    review: TopicReviewEntry? = null,
) {
    val currentOnLeave = rememberUpdatedState(actions.onLeave)
    DisposableEffect(Unit) { onDispose { currentOnLeave.value() } }
    val canGenerate =
        state.load == LessonsLoad.Ready &&
            cardFor(topicState, researchFailed, state.lessons, state.job) is LessonsCard.Free
    val onGenerate = rememberUpdatedState(actions.onGenerate)
    val onOpenReview = rememberUpdatedState(actions.onOpenReview)
    val reviewVisible = rememberUpdatedState(review != null)
    BoxWithConstraints(
        modifier
            .fillMaxSize()
            .onPreviewKeyEvent { event ->
                // Alt+G = "Genera lezione", only while the card shows it (topic.md F6).
                if (canGenerate &&
                    !state.starting &&
                    event.type == KeyEventType.KeyDown &&
                    event.isAltPressed &&
                    !event.isCtrlPressed &&
                    event.key == Key.G
                ) {
                    onGenerate.value()
                    true
                } else if (reviewVisible.value &&
                    event.type == KeyEventType.KeyDown &&
                    event.isAltPressed &&
                    !event.isCtrlPressed &&
                    event.key == Key.R
                ) {
                    // Alt+R = "Ripassa questo argomento", only while the row is there (topic.md F3.B2).
                    onOpenReview.value()
                    true
                } else {
                    false
                }
            }.testTag("lessons-tab"),
    ) {
        val margin = screenMargin(currentWindowWidthClass())
        val layout = TeachTheme.layout
        val main = minOf(layout.readingMaxWidth, maxWidth - margin * 2)
        val twoColumns = maxWidth >= layout.contextPaneBreakpoint && header == null
        val group = if (twoColumns) main + layout.contextPaneGap + layout.contextPaneWidth else main
        val start = ((maxWidth - group) / 2).coerceAtLeast(margin)
        val scroll = rememberScrollState()
        // The next page is requested when the learner is near the end of the page (topic.md F4).
        val prefetch =
            with(androidx.compose.ui.platform.LocalDensity.current) {
                (
                    LESSON_ROW_MIN_HEIGHT *
                        LESSONS_PREFETCH_ROWS
                ).roundToPx()
            }
        val onLoadMore = rememberUpdatedState(actions.onLoadMore)
        LaunchedEffect(scroll, prefetch) {
            snapshotFlow { scroll.maxValue - scroll.value <= prefetch }
                .distinctUntilChanged()
                .filter { it }
                .collect { onLoadMore.value() }
        }
        Column(Modifier.fillMaxSize().verticalScroll(scroll)) {
            header?.invoke()
            when (val load = state.load) {
                LessonsLoad.Loading -> Skeleton(start, main)
                is LessonsLoad.Failed ->
                    Box(Modifier.padding(start = start).width(main)) {
                        LoadFailed(load.error, actions.onRetryLoad, actions.onOpenSettings)
                    }
                LessonsLoad.Ready ->
                    Row(Modifier.padding(start = start, top = TeachTheme.spacing.md, bottom = TeachTheme.spacing.xl)) {
                        Column(
                            Modifier.width(main),
                        ) { Content(topicState, researchFailed, state, actions, pdfTitles, review) }
                        if (twoColumns) {
                            Spacer(Modifier.width(layout.contextPaneGap))
                            Box(Modifier.width(layout.contextPaneWidth)) {
                                ContextPane(missionWhy, recentRecords, actions, pdfTitles)
                            }
                        }
                    }
            }
        }
        Announcer(state)
    }
}

@Composable
private fun LoadFailed(
    error: LoadError,
    onRetry: () -> Unit,
    onOpenSettings: () -> Unit,
) {
    TeachErrorState(
        title = stringResource(R.string.lessons_load_failed),
        body =
            stringResource(
                when (error) {
                    // A 401 gets no message of its own (login.md): the sign-in opens, the retry reloads.
                    LoadError.Network, LoadError.Unauthorized -> UiR.string.ui_load_network_body
                    LoadError.Server -> UiR.string.ui_load_server_body
                },
            ),
        primaryLabel = stringResource(R.string.lessons_retry),
        onPrimary = onRetry,
    )
}

@Composable
private fun Skeleton(
    start: Dp,
    width: Dp,
) {
    val loading = stringResource(R.string.lessons_loading)
    Column(
        Modifier
            .padding(start = start, top = TeachTheme.spacing.md)
            .width(width)
            .semantics(mergeDescendants = true) { stateDescription = loading },
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        Box(
            Modifier
                .fillMaxWidth()
                .height(
                    ROW_SKELETON_HEIGHT,
                ).background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.md),
        )
        repeat(3) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(
                        LESSON_ROW_MIN_HEIGHT,
                    ).background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.md),
            )
        }
    }
}

@Composable
private fun Content(
    topicState: TopicState,
    researchFailed: Boolean,
    state: LessonsUiState,
    actions: LessonsTabActions,
    pdfTitles: List<String> = emptyList(),
    review: TopicReviewEntry? = null,
) {
    val card = cardFor(topicState, researchFailed, state.lessons, state.job)
    val compact = currentWindowWidthClass() == WindowWidthClass.Compact
    NextLessonCard(card, state, actions, pdfTitles)
    if (review != null) {
        Spacer(Modifier.height(TeachTheme.spacing.md))
        ReviewTopicRow(review, actions.onOpenReview)
    }
    if (state.lessons.isNotEmpty()) {
        Spacer(Modifier.height(if (compact) TeachTheme.spacing.lg else EXPANDED_GAP))
        TeachText(
            stringResource(R.string.lessons_list_title),
            Modifier.semantics { heading() },
            style = TeachTheme.typography.ui.title3,
        )
        Spacer(Modifier.height(TeachTheme.spacing.xs))
        LessonList(state, actions)
    }
}

/** "Ripassa questo argomento · 8 schede" with "Circa 2 minuti" under it: one text-button target, no card, no fill. */
@Composable
private fun ReviewTopicRow(
    review: TopicReviewEntry,
    onOpen: () -> Unit,
) {
    val cards = pluralStringResource(R.plurals.lessons_review_cards, review.cards, review.cards)
    val minutes = pluralStringResource(R.plurals.lessons_review_minutes, review.minutes, review.minutes)
    val description = stringResource(R.string.lessons_review_a11y, cards, minutes)
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = TeachTheme.layout.minTouchTarget)
            .clickable(role = Role.Button, onClick = onOpen)
            .semantics(mergeDescendants = true) { contentDescription = description }
            .testTag("lessons-review-row"),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TeachIcon(
            TeachIcons.Layers,
            contentDescription = null,
            tint = TeachTheme.colors.accent,
            size = TeachTheme.layout.iconSizeSmall,
        )
        Column {
            TeachText(
                stringResource(R.string.lessons_review_title, cards),
                style = TeachTheme.typography.ui.label,
                color = TeachTheme.colors.accent,
            )
            TeachText(
                stringResource(R.string.lessons_review_minutes_line, minutes),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
        }
    }
}

// --- the card ---------------------------------------------------------------------------------------------------------

@Composable
private fun cardPadding(): Dp =
    if (currentWindowWidthClass() ==
        WindowWidthClass.Compact
    ) {
        TeachTheme.spacing.md
    } else {
        20.dp
    }

@Composable
private fun NextLessonCard(
    card: LessonsCard,
    state: LessonsUiState,
    actions: LessonsTabActions,
    pdfTitles: List<String> = emptyList(),
) {
    val title = stringResource(R.string.lessons_card_title)
    // On success the four phases stay closed (all checked) for 600 ms, then crossfade to the ready card (topic.md F3.C).
    var closing by remember { mutableStateOf<TopicJob?>(null) }
    var lastPreparing by remember { mutableStateOf<TopicJob?>(null) }
    val reducedMotion = TeachTheme.reducedMotion
    val preparing = (card as? LessonsCard.Preparing)?.job
    // The "Leggo le fonti" row stays once the step has been seen (the job only carries the current step).
    var readingSeenFor by remember { mutableStateOf<String?>(null) }
    if (preparing?.progress?.step == "reading") readingSeenFor = preparing.id
    LaunchedEffect(preparing != null) {
        if (preparing != null) {
            lastPreparing = preparing
        } else {
            val previous = lastPreparing
            lastPreparing = null
            val succeeded = state.job?.let { it.id == previous?.id && it.status == JobStatus.Succeeded } == true
            if (previous != null && succeeded && !reducedMotion) {
                closing = previous
                try {
                    delay(GENERATION_CLOSE_MS)
                } finally {
                    closing = null
                }
            }
        }
    }
    // Keep the latest preparing job fresh while the generation runs.
    SideEffect { if (preparing != null) lastPreparing = preparing }
    val closingJob = closing
    Crossfade(
        targetState = closingJob,
        animationSpec = tween(if (reducedMotion) 0 else TeachTheme.motion.baseMs),
        modifier = Modifier.fillMaxWidth().semantics { paneTitle = title },
        label = "lesson-card",
    ) { closingNow ->
        Box(Modifier.fillMaxWidth()) {
            if (closingNow != null) {
                PreparingCard(closingNow, completing = true, readingSeen = readingSeenFor == closingNow.id)
                return@Box
            }
            when (card) {
                is LessonsCard.Blocked -> BlockedCard(card, actions)
                is LessonsCard.Free -> ReadyCard(card.hasLessons, null, state, actions, pdfTitles)
                is LessonsCard.Unfinished -> ReadyCard(true, card.lesson, state, actions, pdfTitles)
                is LessonsCard.Preparing -> PreparingCard(card.job, readingSeen = readingSeenFor == card.job.id)
                is LessonsCard.Failed -> FailedPanel(card.job, state, actions)
            }
        }
    }
}

@Composable
private fun BlockedCard(
    card: LessonsCard.Blocked,
    actions: LessonsTabActions,
) {
    val (title, body, action, onAction) =
        when {
            card.topicState == TopicState.Researching && card.researchFailed ->
                Quad(
                    R.string.lessons_blocked_failed_title,
                    R.string.lessons_blocked_failed_body,
                    R.string.lessons_blocked_failed_action,
                    actions.onOpenResources,
                )
            card.topicState == TopicState.Researching ->
                Quad(
                    R.string.lessons_blocked_researching_title,
                    R.string.lessons_blocked_researching_body,
                    R.string.lessons_blocked_researching_action,
                    actions.onOpenResources,
                )
            card.topicState == TopicState.MissionProposed ->
                Quad(
                    R.string.lessons_blocked_proposed_title,
                    R.string.lessons_blocked_proposed_body,
                    R.string.lessons_blocked_proposed_action,
                    actions.onGoToChat,
                )
            else ->
                Quad(
                    R.string.lessons_blocked_new_title,
                    R.string.lessons_blocked_new_body,
                    R.string.lessons_blocked_new_action,
                    actions.onGoToChat,
                )
        }
    TeachCard(Modifier.fillMaxWidth(), CardVariant.Static) {
        Column(
            Modifier.fillMaxWidth().padding(cardPadding()),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            TeachText(stringResource(title), style = TeachTheme.typography.ui.title3)
            TeachText(stringResource(body), style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
            TeachButton(stringResource(action), onAction, variant = ButtonVariant.Secondary)
        }
    }
}

private data class Quad(
    val title: Int,
    val body: Int,
    val action: Int,
    val onAction: () -> Unit,
)

@Composable
private fun ReadyCard(
    hasLessons: Boolean,
    unfinished: Lesson?,
    state: LessonsUiState,
    actions: LessonsTabActions,
    pdfTitles: List<String> = emptyList(),
) {
    val colors = TeachTheme.colors
    TeachCard(Modifier.fillMaxWidth(), CardVariant.Emphasis) {
        Column(
            Modifier.fillMaxWidth().padding(cardPadding()),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            TeachText(
                stringResource(R.string.lessons_card_title),
                style = TeachTheme.typography.ui.title3,
                color = colors.ink,
            )
            TeachText(
                when {
                    unfinished != null -> stringResource(R.string.lessons_card_unfinished_body, unfinished.title)
                    hasLessons -> stringResource(R.string.lessons_card_next_body)
                    else -> stringResource(R.string.lessons_card_first_body)
                },
                style = TeachTheme.typography.ui.body,
                color = colors.ink,
            )
            if (unfinished == null) {
                TeachText(
                    stringResource(R.string.lessons_card_note),
                    style = TeachTheme.typography.ui.bodyS,
                    color = colors.ink,
                )
                if (pdfTitles.isNotEmpty()) {
                    TeachText(
                        if (pdfTitles.size == 1) {
                            stringResource(R.string.lessons_card_pdf_one, pdfTitles.first())
                        } else {
                            stringResource(R.string.lessons_card_pdf_many)
                        },
                        Modifier.testTag("lessons-card-pdf"),
                        style = TeachTheme.typography.ui.bodyS,
                        color = colors.ink,
                    )
                }
            }
            Spacer(Modifier.height(TeachTheme.spacing.xxs))
            // Refused for the key: nothing was created, the button stays (llm-key.md section 5).
            state.keyNotice?.let {
                LlmKeyNotice(
                    it.variant,
                    LlmKeyContext.GenerateLesson,
                    it.id,
                    onRetry = actions.onGenerate,
                    retrying = state.starting,
                    again = it.again,
                )
            }
            // Refused for today's budget (llm-key.md 10.1): the buttons below wait for the reset, the panel says when.
            state.budgetResetAt?.let { LlmBudgetPanel(it) }
            val budgetWait =
                state.budgetResetAt != null && !rememberBudgetRenewed(state.budgetResetAt, announce = false)
            val budgetAvailability = state.budgetResetAt?.let { llmBudgetAvailability(it) }
            if (unfinished != null) {
                TeachButton(
                    stringResource(R.string.lessons_resume),
                    { actions.onOpenLesson(unfinished) },
                    leadingIcon = TeachIcons.ArrowRightCircle,
                )
                TeachButton(
                    stringResource(
                        if (state.starting) R.string.lessons_starting else R.string.lessons_generate_another,
                    ),
                    actions.onGenerate,
                    variant = ButtonVariant.Text,
                    enabled = !budgetWait,
                    loading = state.starting,
                    disabledStateDescription = budgetAvailability,
                )
            } else {
                TeachButton(
                    stringResource(if (state.starting) R.string.lessons_starting else R.string.lessons_generate),
                    actions.onGenerate,
                    leadingIcon = TeachIcons.Sparkles,
                    enabled = !budgetWait,
                    loading = state.starting,
                    disabledStateDescription = budgetAvailability,
                )
            }
            if (state.actionError != null) {
                TeachStatusPanel(StatusVariant.Danger, stringResource(R.string.lessons_start_network_error))
            }
        }
    }
}

@Composable
private fun PreparingCard(
    job: TopicJob,
    completing: Boolean = false,
    readingSeen: Boolean = false,
) {
    val now by produceState(Instant.now(), job.id) {
        while (true) {
            delay(CLOCK_TICK_MS)
            value = Instant.now()
        }
    }
    val repair = job.isRepair() && !completing
    val phases = generationPhases(job, completing, readingSeen || job.progress?.step == "reading")
    val unknownStep = job.status == JobStatus.Running && GenerationPhase.fromStep(job.progress?.step) == null
    val slow = Duration.between(job.startedAt ?: job.createdAt, now) >= SLOW_GENERATION
    TeachCard(Modifier.fillMaxWidth(), CardVariant.Static) {
        Column(
            Modifier.fillMaxWidth().padding(cardPadding()),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            TeachText(stringResource(R.string.lessons_preparing_title), style = TeachTheme.typography.ui.title3)
            if (unknownStep) {
                PhaseRow(
                    stringResource(R.string.lessons_phase_generic),
                    PhaseState.Current,
                    job.progress?.message?.takeIf { it.isNotBlank() },
                )
            } else {
                phases.forEach { (phase, rawState) ->
                    // Repair: "Scrivo" becomes "Correggo" and "Controllo" goes back to the future (topic.md F3.C).
                    val state =
                        when {
                            repair && phase == GenerationPhase.Writing -> PhaseState.Current
                            repair && phase == GenerationPhase.Checking -> PhaseState.Todo
                            repair && phase == GenerationPhase.Planning -> PhaseState.Done
                            repair && phase == GenerationPhase.Saving -> PhaseState.Todo
                            else -> rawState
                        }
                    val label =
                        if (repair && phase == GenerationPhase.Writing) {
                            R.string.lessons_phase_repairing
                        } else {
                            when (phase) {
                                GenerationPhase.Planning -> R.string.lessons_phase_planning
                                GenerationPhase.Reading -> R.string.lessons_phase_reading
                                GenerationPhase.Writing -> R.string.lessons_phase_writing
                                GenerationPhase.Checking -> R.string.lessons_phase_checking
                                GenerationPhase.Saving -> R.string.lessons_phase_saving
                            }
                        }
                    val detail =
                        when {
                            state != PhaseState.Current -> null
                            repair -> stringResource(R.string.lessons_repair_detail)
                            job.status == JobStatus.Queued -> stringResource(R.string.lessons_phase_queued)
                            else -> job.progress?.message?.takeIf { it.isNotBlank() }
                        }
                    PhaseRow(stringResource(label), state, detail)
                }
            }
            TeachText(
                stringResource(if (slow) R.string.lessons_preparing_slow else R.string.lessons_preparing_footer),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
        }
    }
    if (!completing) PhaseAnnouncer(job)
}

@Composable
private fun PhaseRow(
    label: String,
    state: PhaseState,
    detail: String?,
) {
    val colors = TeachTheme.colors
    val stateText =
        stringResource(
            when (state) {
                PhaseState.Done -> R.string.lessons_phase_done
                PhaseState.Current -> R.string.lessons_phase_current
                PhaseState.Todo -> R.string.lessons_phase_todo
            },
        )
    Column(
        Modifier
            .fillMaxWidth()
            .heightIn(min = PHASE_MIN_HEIGHT)
            .semantics(mergeDescendants = true) { stateDescription = stateText },
        verticalArrangement = Arrangement.Center,
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            val size = TeachTheme.layout.iconSizeSmall
            when (state) {
                PhaseState.Done -> TeachIcon(TeachIcons.Check, null, tint = colors.correct, size = size)
                PhaseState.Current -> TeachSpinner(size = size)
                PhaseState.Todo -> TeachIcon(TeachIcons.Circle, null, tint = colors.inkMuted, size = size)
            }
            TeachText(
                label,
                style =
                    if (state ==
                        PhaseState.Current
                    ) {
                        TeachTheme.typography.ui.bodyL
                    } else {
                        TeachTheme.typography.ui.body
                    },
                color =
                    when (state) {
                        PhaseState.Done -> colors.inkSoft
                        PhaseState.Current -> colors.ink
                        PhaseState.Todo -> colors.inkMuted
                    },
            )
        }
        if (detail != null) {
            TeachText(
                detail,
                Modifier.padding(start = DETAIL_INDENT),
                style = TeachTheme.typography.ui.bodyS,
                color = colors.inkSoft,
            )
        }
    }
}

/** Invisible polite live region: speaks only the change of phase ("Preparo la lezione: scrivo."), never the details. */
@Composable
private fun PhaseAnnouncer(job: TopicJob) {
    val phase = GenerationPhase.fromStep(job.progress?.step) ?: return
    val label =
        stringResource(
            when (phase) {
                GenerationPhase.Planning -> R.string.lessons_phase_planning
                GenerationPhase.Reading -> R.string.lessons_phase_reading
                GenerationPhase.Writing ->
                    if (job.isRepair()) R.string.lessons_phase_repairing else R.string.lessons_phase_writing
                GenerationPhase.Checking -> R.string.lessons_phase_checking
                GenerationPhase.Saving -> R.string.lessons_phase_saving
            },
        )
    val text = stringResource(R.string.lessons_announce_phase, label.replaceFirstChar { it.lowercase() })
    key(phase, job.isRepair()) {
        Box(
            Modifier.size(1.dp).semantics {
                liveRegion = LiveRegionMode.Polite
                contentDescription = text
            },
        )
    }
}

/** The end of a generation, spoken once: "Lezione pronta." / "Preparazione interrotta. Riprova disponibile." */
@Composable
private fun Announcer(state: LessonsUiState) {
    val announcement = state.announcement ?: return
    val title = state.lesson(state.newLessonId)?.title
    val text =
        when {
            announcement.ready && title != null -> stringResource(R.string.lessons_announce_ready_title, title)
            announcement.ready -> stringResource(R.string.lessons_announce_ready)
            else -> stringResource(R.string.lessons_announce_failed)
        }
    key(announcement.id) {
        Box(
            Modifier.size(1.dp).semantics {
                liveRegion = LiveRegionMode.Polite
                contentDescription = text
            },
        )
    }
}

/** Title and body of a failed generation, by `error.code` (topic.md F3.D). */
internal fun generationFailure(code: String?): Pair<Int, Int> =
    when (code) {
        "invalid_output" -> R.string.lessons_failed_invalid_title to R.string.lessons_failed_invalid_body
        "upstream_failed" -> R.string.lessons_failed_upstream_title to R.string.lessons_failed_upstream_body
        "rate_limited" -> R.string.lessons_failed_rate_title to R.string.lessons_failed_rate_body
        "timeout" -> R.string.lessons_failed_timeout_title to R.string.lessons_failed_timeout_body
        "interrupted" -> R.string.lessons_failed_interrupted_title to R.string.lessons_failed_interrupted_body
        else -> R.string.lessons_failed_generic_title to R.string.lessons_failed_generic_body
    }

@Composable
private fun FailedPanel(
    job: TopicJob,
    state: LessonsUiState,
    actions: LessonsTabActions,
) {
    val keyVariant = job.error?.code.llmKeyVariantOfCode()
    if (keyVariant != null) {
        // The key, not the work, is the problem: "Riprova" only once a key has been saved (llm-key.md section 6).
        LlmKeyNotice(
            keyVariant,
            LlmKeyContext.LessonGenerationJob,
            job.id,
            onRetry = actions.onRetryGeneration,
            retrying = state.retrying,
            again = state.keyNotice?.again ?: 0,
        )
        return
    }
    if (job.error?.code.isLlmBudgetCode() || state.budgetResetAt != null) {
        // The job stopped for the daily budget, or "Riprova" was just refused for it: wait, do not retry blindly.
        val resetAt = remember(job.id, job.finishedAt, state.budgetResetAt) { job.jobBudgetReset(state.budgetResetAt) }
        LlmBudgetPanel(
            resetAt,
            retryText = stringResource(if (state.retrying) R.string.lessons_retrying else R.string.lessons_retry),
            onRetry = actions.onRetryGeneration,
            retrying = state.retrying,
            retryIcon = TeachIcons.RotateCcw,
        )
        return
    }
    val (title, body) = generationFailure(job.error?.code)
    var details by rememberSaveable { mutableStateOf(false) }
    TeachStatusPanel(
        StatusVariant.Danger,
        stringResource(title),
        body =
            if (state.actionError != null) {
                stringResource(R.string.lessons_start_network_error)
            } else {
                stringResource(body)
            },
    ) {
        state.keyNotice?.let { LlmKeyNotice(it.variant, LlmKeyContext.GenerateLesson, it.id) }
        TeachButton(
            stringResource(if (state.retrying) R.string.lessons_retrying else R.string.lessons_retry),
            actions.onRetryGeneration,
            Modifier.padding(top = TeachTheme.spacing.xs),
            variant = ButtonVariant.Secondary,
            loading = state.retrying,
            leadingIcon = TeachIcons.RotateCcw,
        )
        job.error
            ?.message
            ?.takeIf { it.isNotBlank() }
            ?.let { message ->
                TeachDisclosure(
                    title = stringResource(R.string.lessons_details),
                    expanded = details,
                    onToggle = { details = !details },
                ) {
                    androidx.compose.foundation.text.selection.SelectionContainer {
                        TeachText(message, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
                    }
                }
            }
    }
}

// --- the list ---------------------------------------------------------------------------------------------------------

@Composable
private fun LessonList(
    state: LessonsUiState,
    actions: LessonsTabActions,
) {
    TeachCard(Modifier.fillMaxWidth(), CardVariant.Static) {
        Column {
            state.lessons.forEachIndexed { index, lesson ->
                if (index > 0) TeachRowDivider()
                val completionFailed = state.completionJobs[lesson.id]?.status == JobStatus.Failed
                LessonRow(
                    lesson = lesson,
                    isNew = lesson.id == state.newLessonId,
                    registerFailed = lesson.status == LessonStatus.Completed && completionFailed,
                    sections = state.sectionProgress[lesson.id],
                    onClick = { actions.onOpenLesson(lesson) },
                )
            }
            if (state.pageLoading) {
                TeachRowDivider()
                Box(
                    Modifier
                        .fillMaxWidth()
                        .height(LESSON_ROW_MIN_HEIGHT)
                        .padding(TeachTheme.spacing.md)
                        .semantics { stateDescription = "" },
                ) {
                    Box(
                        Modifier
                            .fillMaxWidth(
                                0.6f,
                            ).height(
                                TeachTheme.spacing.md,
                            ).background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.sm),
                    )
                }
            }
            if (state.pageFailed) {
                TeachRowDivider()
                TeachButton(
                    stringResource(R.string.lessons_more_retry),
                    actions.onLoadMore,
                    variant = ButtonVariant.Text,
                )
            }
        }
    }
}

@Composable
private fun LessonRow(
    lesson: Lesson,
    isNew: Boolean,
    registerFailed: Boolean,
    sections: Pair<Int, Int>?,
    onClick: () -> Unit,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val statusText =
        when (lesson.status) {
            LessonStatus.NotStarted -> stringResource(R.string.lessons_status_not_started)
            LessonStatus.InProgress ->
                if (sections != null) {
                    pluralStringResource(
                        R.plurals.lessons_status_in_progress_sections,
                        sections.second,
                        sections.first,
                        sections.second,
                    )
                } else {
                    stringResource(R.string.lessons_status_in_progress)
                }
            LessonStatus.Completed ->
                if (registerFailed) {
                    stringResource(
                        R.string.lessons_status_completed_register,
                    )
                } else {
                    stringResource(R.string.lessons_status_completed)
                }
        }
    val minutes =
        if (lesson.estimatedMinutes >
            0
        ) {
            pluralStringResource(R.plurals.lessons_minutes, lesson.estimatedMinutes, lesson.estimatedMinutes)
        } else {
            null
        }
    val pdf = pdfLines(lesson.documentSources)
    val icon =
        when {
            lesson.status == LessonStatus.InProgress -> TeachIcons.CircleDot
            lesson.status == LessonStatus.Completed && registerFailed -> TeachIcons.AlertCircle
            lesson.status == LessonStatus.Completed -> TeachIcons.Check
            else -> TeachIcons.Circle
        }
    val iconTint = if (lesson.status == LessonStatus.InProgress) colors.accent else colors.inkSoft
    val description =
        buildString {
            append(stringResource(R.string.lessons_row_a11y_start, lesson.number, lesson.title))
            if (lesson.objective.isNotBlank()) {
                append(
                    ' ',
                ).append(stringResource(R.string.lessons_row_a11y_objective, lesson.objective))
            }
            append(' ').append(statusText).append('.')
            if (minutes != null) append(' ').append(minutes).append('.')
            pdf.lines.forEach { append(' ').append(pdfA11y(it)) }
            if (pdf.more >
                0
            ) {
                append(' ').append(pluralStringResource(R.plurals.lessons_pdf_a11y_more, pdf.more, pdf.more))
            }
        }
    val openLabel = stringResource(R.string.lessons_open)
    val focusManager = androidx.compose.ui.platform.LocalFocusManager.current
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = LESSON_ROW_MIN_HEIGHT)
            .onPreviewKeyEvent { event ->
                // Up/Down move the focus between the rows (topic.md F6); Enter and Space are the click.
                if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                when (event.key) {
                    Key.DirectionDown -> focusManager.moveFocus(FocusDirection.Down)
                    Key.DirectionUp -> focusManager.moveFocus(FocusDirection.Up)
                    else -> false
                }
            }.focusRing(state.focused, TeachTheme.shapes.md)
            .background(if (state.pressed) colors.surfacePressed else androidx.compose.ui.graphics.Color.Transparent)
            .semantics(mergeDescendants = true) { contentDescription = description }
            .clickable(
                interactionSource = state.source,
                indication = null,
                role = Role.Button,
                onClickLabel = openLabel,
                onClick = onClick,
            ).padding(start = TeachTheme.spacing.md, top = TeachTheme.spacing.sm, bottom = TeachTheme.spacing.sm),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        Box(
            Modifier.size(LESSON_NUMBER_CIRCLE).background(colors.surfaceSunken, TeachTheme.shapes.full),
            contentAlignment = Alignment.Center,
        ) {
            TeachText(lesson.number.toString(), style = TeachTheme.typography.ui.label, color = colors.ink)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachText(
                    lesson.title,
                    Modifier.weight(1f, fill = false),
                    style = TeachTheme.typography.ui.title3,
                    color = colors.ink,
                )
                if (isNew) {
                    TeachText(
                        stringResource(R.string.lessons_new),
                        Modifier
                            .background(
                                colors.infoSoft,
                                TeachTheme.shapes.full,
                            ).padding(horizontal = TeachTheme.spacing.xs, vertical = TeachTheme.spacing.xxs),
                        style = TeachTheme.typography.ui.labelS,
                        color = colors.info,
                    )
                }
            }
            if (lesson.objective.isNotBlank()) {
                TeachText(
                    lesson.objective,
                    style = TeachTheme.typography.ui.body,
                    color = colors.inkSoft,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Row(
                verticalAlignment = Alignment.Top,
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachIcon(icon, null, tint = iconTint, size = TeachTheme.layout.iconSizeSmall)
                TeachText(
                    if (minutes != null) "$statusText · $minutes" else statusText,
                    style = TeachTheme.typography.ui.bodyS,
                    color = colors.inkSoft,
                )
            }
            pdf.lines.forEach { PdfRow(pdfText(it)) }
            if (pdf.more > 0) PdfRow(stringResource(R.string.lessons_pdf_more, pdf.more))
        }
        TeachIcon(
            TeachIcons.ChevronRight,
            null,
            Modifier.padding(top = TeachTheme.spacing.xs, end = TeachTheme.spacing.xs),
            tint = colors.inkMuted,
        )
    }
}

@Composable
private fun pdfText(line: PdfLine): String =
    when {
        line.title == null && line.singlePage -> stringResource(R.string.lessons_pdf_page, line.pageStart)
        line.title == null -> stringResource(R.string.lessons_pdf_pages, line.pageStart, line.pageEnd)
        line.singlePage -> stringResource(R.string.lessons_pdf_titled_page, line.title, line.pageStart)
        else -> stringResource(R.string.lessons_pdf_titled_pages, line.title, line.pageStart, line.pageEnd)
    }

@Composable
private fun pdfA11y(line: PdfLine): String =
    when {
        line.title == null && line.singlePage -> stringResource(R.string.lessons_pdf_a11y_page, line.pageStart)
        line.title == null -> stringResource(R.string.lessons_pdf_a11y_pages, line.pageStart, line.pageEnd)
        line.singlePage -> stringResource(R.string.lessons_pdf_a11y_titled_page, line.title, line.pageStart)
        else -> stringResource(R.string.lessons_pdf_a11y_titled_pages, line.title, line.pageStart, line.pageEnd)
    }

/** The "Dal tuo PDF" meta line: `file-text` icon and free-wrapping `bodyS` text, never an ellipsis. */
@Composable
private fun PdfRow(text: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachIcon(TeachIcons.FileText, null, tint = TeachTheme.colors.inkSoft, size = TeachTheme.layout.iconSizeSmall)
        TeachText(text, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
    }
}

// --- the context pane (>= 1100 dp) --------------------------------------------------------------------------------------

@Composable
private fun ContextPane(
    missionWhy: String?,
    records: List<LearningRecord>,
    actions: LessonsTabActions,
    pdfTitles: List<String> = emptyList(),
) {
    val colors = TeachTheme.colors
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.lg)) {
        if (!missionWhy.isNullOrBlank()) {
            var expanded by rememberSaveable { mutableStateOf(false) }
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                TeachText(
                    stringResource(R.string.lessons_pane_why),
                    style = TeachTheme.typography.ui.label,
                    color = colors.inkSoft,
                )
                TeachText(
                    missionWhy,
                    style = TeachTheme.typography.ui.body,
                    maxLines = if (expanded) Int.MAX_VALUE else MISSION_WHY_LINES,
                    overflow = TextOverflow.Ellipsis,
                )
                if (pdfTitles.isNotEmpty()) {
                    Row(
                        Modifier.testTag("lessons-pane-pdf"),
                        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                    ) {
                        TeachIcon(
                            TeachIcons.FileText,
                            null,
                            tint = colors.inkSoft,
                            size = TeachTheme.layout.iconSizeSmall,
                        )
                        TeachText(
                            if (pdfTitles.size == 1) {
                                stringResource(R.string.lessons_pane_pdf, pdfTitles.first())
                            } else {
                                stringResource(R.string.lessons_pane_pdf_many)
                            },
                            style = TeachTheme.typography.ui.bodyS,
                            color = colors.inkSoft,
                        )
                    }
                    TeachButton(
                        stringResource(R.string.lessons_pane_pdf_go),
                        actions.onOpenResources,
                        variant = ButtonVariant.Text,
                    )
                }
                Row {
                    TeachButton(
                        stringResource(R.string.lessons_pane_see_mission),
                        actions.onOpenMission,
                        variant = ButtonVariant.Text,
                    )
                    TeachButton(
                        stringResource(
                            if (expanded) R.string.lessons_pane_show_less else R.string.lessons_pane_show_all,
                        ),
                        { expanded = !expanded },
                        variant = ButtonVariant.Text,
                    )
                }
            }
        }
        if (records.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                TeachText(
                    stringResource(R.string.lessons_pane_demonstrated),
                    style = TeachTheme.typography.ui.label,
                    color = colors.inkSoft,
                )
                records.forEach {
                    Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                        TeachIcon(TeachIcons.Check, null, tint = colors.inkSoft, size = TeachTheme.layout.iconSizeSmall)
                        ProvideStandaloneSdui {
                            InlineText(it.title, TeachTheme.typography.ui.body, interactive = false)
                        }
                    }
                }
                TeachButton(
                    stringResource(R.string.lessons_pane_see_all),
                    actions.onOpenRegister,
                    variant = ButtonVariant.Text,
                )
            }
        }
    }
}

// --- previews ----------------------------------------------------------------------------------------------------------

private val previewLesson =
    Lesson(
        "l3",
        "t",
        3,
        "Lifetimes in pratica",
        "Saper dire quale riferimento vive più a lungo in una funzione con due parametri.",
        8,
        LessonStatus.InProgress,
        1,
        1,
        Instant.EPOCH,
        null,
    )

private fun previewJob(
    status: JobStatus,
    step: String? = null,
    message: String? = null,
    done: Int? = null,
    error: JobError? = null,
) = TopicJob(
    "j",
    "t",
    "lesson_generation",
    status,
    step?.let {
        JobProgress(it, message, done, 3)
    },
    error,
    null,
    1,
    Instant.EPOCH,
    Instant.EPOCH,
    null,
)

private val previewActions = LessonsTabActions({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})

@Composable
private fun CardSamples(state: LessonsUiState) {
    Column(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        NextLessonCard(cardFor(TopicState.Ready, false, state.lessons, state.job), state, previewActions)
        if (state.lessons.isNotEmpty()) LessonList(state, previewActions)
    }
}

private val readyState =
    LessonsUiState(
        load = LessonsLoad.Ready,
        lessons =
            listOf(
                previewLesson,
                previewLesson.copy(
                    id = "l2",
                    number = 2,
                    title = "Ownership e borrowing",
                    status = LessonStatus.Completed,
                ),
                previewLesson.copy(
                    id = "l1",
                    number = 1,
                    title = "Il modello di memoria",
                    status = LessonStatus.NotStarted,
                ),
            ),
        newLessonId = "l3",
        sectionProgress = mapOf("l3" to (2 to 4)),
    )

@Preview(name = "Lessons light", showBackground = true, widthDp = 420, heightDp = 1100)
@Composable
private fun LessonsLightPreview() = TeachTheme(darkTheme = false) { CardSamples(readyState) }

@Preview(name = "Lessons dark", showBackground = true, widthDp = 420, heightDp = 1100)
@Composable
private fun LessonsDarkPreview() = TeachTheme(darkTheme = true) { CardSamples(readyState) }

@Preview(name = "Lessons preparing", showBackground = true, widthDp = 420, heightDp = 700)
@Composable
private fun LessonsPreparingPreview() =
    TeachTheme(darkTheme = false) {
        CardSamples(
            LessonsUiState(
                load = LessonsLoad.Ready,
                job = previewJob(JobStatus.Running, "writing", "Scrivo la sezione 2"),
            ),
        )
    }

@Preview(name = "Lessons repair dark", showBackground = true, widthDp = 420, heightDp = 700)
@Composable
private fun LessonsRepairPreview() =
    TeachTheme(darkTheme = true) {
        CardSamples(
            LessonsUiState(
                load = LessonsLoad.Ready,
                job = previewJob(JobStatus.Running, "writing", "Correggo la lezione", done = 2),
            ),
        )
    }

@Preview(name = "Lessons failed", showBackground = true, widthDp = 420, heightDp = 700)
@Composable
private fun LessonsFailedPreview() =
    TeachTheme(darkTheme = false) {
        CardSamples(
            LessonsUiState(
                load = LessonsLoad.Ready,
                job = previewJob(JobStatus.Failed, error = JobError("invalid_output", "3 documenti non validi")),
            ),
        )
    }

@Preview(name = "Lessons font 2.0", showBackground = true, widthDp = 360, heightDp = 1800, fontScale = 2f)
@Composable
private fun LessonsLargePreview() = TeachTheme(darkTheme = false) { CardSamples(readyState) }
