package com.teachme.sdui.render

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CollectionInfo
import androidx.compose.ui.semantics.CollectionItemInfo
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.collectionInfo
import androidx.compose.ui.semantics.collectionItemInfo
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.teachme.core.designsystem.LocalIsDark
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.sdui.R
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.logic.ExerciseReducers
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.Outcome
import com.teachme.sdui.model.OrderStep
import com.teachme.sdui.model.OrderSteps

private val BADGE_SIZE = 28.dp

/** `order_steps` (docs/design/components/order_steps.md): drag, arrows, custom actions, keyboard. */
@Composable
internal fun OrderStepsBlock(block: OrderSteps) {
    val env = LocalSdui.current
    val state = env.states.get(block.id)
    val locked = env.states.isLocked(block.id)
    val update = { f: (ExerciseState) -> ExerciseState -> env.states.update(block.id, f) }
    val order = ExerciseReducers.currentOrder(block, state)
    val steps = remember(block) { block.steps.associateBy { it.id } }
    val correctOrder = remember(block) { block.steps.map { it.id } }
    val check = { update { ExerciseReducers.checkOrder(block, it, env.nowMs()) } }
    val move = { from: Int, to: Int -> update { ExerciseReducers.moveStep(block, it, from, to) } }

    val heights = remember { mutableStateMapOf<String, Int>() }
    var dragging by remember { mutableStateOf<String?>(null) }
    var dragOffset by remember { mutableFloatStateOf(0f) }
    var grabbed by remember { mutableStateOf<String?>(null) }
    var grabStart by remember { mutableStateOf<List<String>?>(null) }
    val requesters = remember(block) { block.steps.associate { it.id to FocusRequester() } }
    val latestOrder = rememberUpdatedState(order)
    val latestMove = rememberUpdatedState(move)
    val gapPx = with(LocalDensity.current) { TeachTheme.spacing.xs.toPx() }

    ExerciseCard(
        required = block.required,
        icon = TeachIcons.ListOrdered,
        typeLabel = stringResource(R.string.sdui_type_order_steps),
        onPrimary = if (!state.checked) ({ check() }) else null,
    ) {
        QuestionAndInstruction(block.prompt, stringResource(R.string.sdui_order_instruction), block.lang)
        if (state.retrying) SecondAttemptCaption()
        Column(
            Modifier.fillMaxWidth().semantics { collectionInfo = CollectionInfo(order.size, 1) },
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            order.forEachIndexed { position, id ->
                val step = steps.getValue(id)
                androidx.compose.runtime.key(id) {
                    StepRow(
                        step = step,
                        position = position,
                        count = order.size,
                        checked = state.checked,
                        inPlace = correctOrder.getOrNull(position) == id,
                        correctPosition = correctOrder.indexOf(id),
                        isDragging = dragging == id,
                        dragOffset = if (dragging == id) dragOffset else 0f,
                        isGrabbed = grabbed == id,
                        lang = block.lang,
                        requester = requesters.getValue(id),
                        onSize = { heights[id] = it },
                        onMove = { to -> move(position, to.coerceIn(0, order.lastIndex)) },
                        onDragStart = {
                            dragging = id
                            dragOffset = 0f
                        },
                        onDragEnd = {
                            dragging = null
                            dragOffset = 0f
                        },
                        onDrag = { dy ->
                            dragOffset += dy
                            val list = latestOrder.value
                            val idx = list.indexOf(id)
                            if (dragOffset > 0 && idx < list.lastIndex) {
                                val h = (heights[list[idx + 1]] ?: 0) + gapPx
                                if (dragOffset > h / 2) {
                                    latestMove.value(idx, idx + 1)
                                    dragOffset -= h
                                }
                            } else if (dragOffset < 0 && idx > 0) {
                                val h = (heights[list[idx - 1]] ?: 0) + gapPx
                                if (-dragOffset > h / 2) {
                                    latestMove.value(idx, idx - 1)
                                    dragOffset += h
                                }
                            }
                        },
                        onToggleGrab = {
                            if (grabbed == id) {
                                grabbed = null
                                grabStart = null
                            } else {
                                grabbed = id
                                grabStart = order
                            }
                        },
                        onCancelGrab = {
                            val start = grabStart
                            if (grabbed == id && start != null) update { it.copy(order = start) }
                            grabbed = null
                            grabStart = null
                        },
                        onFocusNeighbor = { delta -> requesters[order.getOrNull(position + delta)]?.requestFocus() },
                    )
                }
            }
        }
        if (!state.checked) {
            PrimaryAction(
                label = stringResource(R.string.sdui_check),
                enabled = true,
                disabledReason = "",
                onClick = { check() },
            )
        } else {
            val outcome = state.outcome ?: Outcome.Incorrect
            val open = state.showCorrectOrder ?: (outcome == Outcome.Incorrect)
            OutcomePanel(
                outcome,
                if (outcome == Outcome.Correct) block.feedback.correct else block.feedback.incorrect,
                block.lang,
                secondAttempt = state.attempts >= 2,
            )
            CorrectOrder(block.steps, open, block.lang) {
                update {
                    ExerciseReducers.toggleCorrectOrder(
                        it,
                        defaultOpen =
                            outcome == Outcome.Incorrect,
                    )
                }
            }
            if (ExerciseReducers.canRetry(block, state, locked)) {
                RetryButton { update { ExerciseReducers.retry(it, clearSelection = false) } }
            }
        }
    }
}

@Composable
private fun CorrectOrder(
    steps: List<OrderStep>,
    open: Boolean,
    lang: String?,
    onToggle: () -> Unit,
) {
    val stateText = stringResource(if (open) R.string.sdui_state_open else R.string.sdui_state_closed)
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachButton(
            text = stringResource(if (open) R.string.sdui_order_hide_correct else R.string.sdui_order_show_correct),
            onClick = onToggle,
            variant = ButtonVariant.Text,
            leadingIcon = if (open) TeachIcons.ChevronUp else TeachIcons.ChevronDown,
            modifier = Modifier.semantics { stateDescription = stateText },
        )
        if (open) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.md)
                    .padding(TeachTheme.spacing.md),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                steps.forEachIndexed { i, step ->
                    Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                        TeachText(
                            "${i + 1}.",
                            style = TeachTheme.typography.ui.label,
                            color = TeachTheme.colors.inkSoft,
                        )
                        InlineText(step.text, TeachTheme.typography.ui.bodyL, interactive = false, lang = lang)
                    }
                }
            }
        }
    }
}

@Composable
private fun StepRow(
    step: OrderStep,
    position: Int,
    count: Int,
    checked: Boolean,
    inPlace: Boolean,
    correctPosition: Int,
    isDragging: Boolean,
    dragOffset: Float,
    isGrabbed: Boolean,
    lang: String?,
    requester: FocusRequester,
    onSize: (Int) -> Unit,
    onMove: (Int) -> Unit,
    onDragStart: () -> Unit,
    onDragEnd: () -> Unit,
    onDrag: (Float) -> Unit,
    onToggleGrab: () -> Unit,
    onCancelGrab: () -> Unit,
    onFocusNeighbor: (Int) -> Unit,
) {
    val colors = TeachTheme.colors
    val interaction = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    val raised = isDragging || isGrabbed
    val (fill, borderColor, borderWidth) =
        when {
            checked && inPlace -> Triple(colors.correctSoft, colors.correct, TeachTheme.stroke.selected)
            checked -> Triple(colors.incorrectSoft, colors.incorrect, TeachTheme.stroke.selected)
            raised -> Triple(colors.surfaceRaised, colors.accent, TeachTheme.stroke.selected)
            else -> Triple(colors.surface, colors.border, TeachTheme.stroke.control)
        }
    val label = stringResource(R.string.sdui_step_position, position + 1, count)
    val stateText =
        when {
            checked && inPlace -> stringResource(R.string.sdui_step_state_right)
            checked -> stringResource(R.string.sdui_step_state_wrong, correctPosition + 1)
            isGrabbed -> stringResource(R.string.sdui_step_state_grabbed, position + 1, count)
            else -> label
        }
    val toStart = stringResource(R.string.sdui_step_to_start)
    val toEnd = stringResource(R.string.sdui_step_to_end)
    val up = stringResource(R.string.sdui_step_up)
    val down = stringResource(R.string.sdui_step_down)
    val shadow = TeachTheme.elevation.raised
    val dark = LocalIsDark.current
    val latestPosition = rememberUpdatedState(position)
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .zIndex(if (isDragging) 1f else 0f)
                .graphicsLayer { translationY = dragOffset }
                .then(
                    if (raised) {
                        val alpha = if (dark) shadow.alphaDark else shadow.alphaLight
                        Modifier.shadow(
                            shadow.blur,
                            shape,
                            ambientColor =
                                androidx.compose.ui.graphics.Color.Black
                                    .copy(alpha),
                            spotColor =
                                androidx.compose.ui.graphics.Color.Black
                                    .copy(alpha),
                        )
                    } else {
                        Modifier
                    },
                ).onSizeChanged { onSize(it.height) }
                .focusRing(interaction.focused, shape)
                .clip(shape)
                .background(fill, shape)
                .border(BorderStroke(borderWidth, borderColor), shape)
                .semantics(mergeDescendants = true) {
                    collectionItemInfo = CollectionItemInfo(position, 1, 0, 1)
                    stateDescription = stateText
                    if (!checked) {
                        customActions =
                            buildList {
                                if (position > 0) {
                                    add(
                                        CustomAccessibilityAction(up) {
                                            onMove(position - 1)
                                            true
                                        },
                                    )
                                }
                                if (position <
                                    count - 1
                                ) {
                                    add(
                                        CustomAccessibilityAction(down) {
                                            onMove(position + 1)
                                            true
                                        },
                                    )
                                }
                                if (position > 0) {
                                    add(
                                        CustomAccessibilityAction(toStart) {
                                            onMove(0)
                                            true
                                        },
                                    )
                                }
                                if (position <
                                    count - 1
                                ) {
                                    add(
                                        CustomAccessibilityAction(toEnd) {
                                            onMove(count - 1)
                                            true
                                        },
                                    )
                                }
                            }
                    }
                }.focusRequester(requester)
                .focusable(interactionSource = interaction.source)
                .onKeyEvent { e ->
                    if (checked || e.type != KeyEventType.KeyDown) return@onKeyEvent false
                    val delta =
                        when (e.key) {
                            Key.DirectionDown -> 1
                            Key.DirectionUp -> -1
                            else -> 0
                        }
                    when {
                        e.key == Key.Spacebar -> {
                            onToggleGrab()
                            true
                        }
                        e.key == Key.Escape && isGrabbed -> {
                            onCancelGrab()
                            true
                        }
                        delta != 0 && (isGrabbed || e.isAltPressed) -> {
                            onMove(latestPosition.value + delta)
                            true
                        }
                        delta != 0 -> {
                            onFocusNeighbor(delta)
                            true
                        }
                        else -> false
                    }
                }.heightIn(min = TeachTheme.layout.choiceMinHeight)
                .padding(horizontal = TeachTheme.spacing.xs, vertical = TeachTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        Box(
            Modifier.size(BADGE_SIZE).background(colors.surfaceSunken, TeachTheme.shapes.full),
            contentAlignment = Alignment.Center,
        ) {
            TeachText(
                "${position + 1}",
                style = TeachTheme.typography.ui.label,
                color = colors.inkSoft,
                textAlign = TextAlign.Center,
            )
        }
        if (!checked) {
            Box(
                Modifier
                    .size(TeachTheme.layout.minTouchTarget)
                    .pointerInput(step.id) {
                        detectDragGestures(
                            onDragStart = { onDragStart() },
                            onDragEnd = { onDragEnd() },
                            onDragCancel = { onDragEnd() },
                        ) { change, amount ->
                            change.consume()
                            onDrag(amount.y)
                        }
                    }.clearAndSetSemantics { },
                contentAlignment = Alignment.Center,
            ) {
                TeachIcon(TeachIcons.GripVertical, contentDescription = null, tint = colors.inkSoft)
            }
        }
        Column(
            Modifier.weight(1f).padding(vertical = TeachTheme.spacing.xs),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
        ) {
            InlineText(step.text, TeachTheme.typography.ui.bodyL, interactive = false, lang = lang)
            if (checked) {
                val color = if (inPlace) colors.correct else colors.incorrect
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                ) {
                    TeachIcon(
                        if (inPlace) TeachIcons.CheckCircle else TeachIcons.XCircle,
                        contentDescription = null,
                        tint = color,
                        size = TeachTheme.layout.iconSizeSmall,
                    )
                    TeachText(
                        stringResource(if (inPlace) R.string.sdui_step_right else R.string.sdui_step_wrong),
                        style = TeachTheme.typography.ui.label,
                        color = color,
                    )
                }
            }
        }
        if (!checked) {
            val arrows: @Composable () -> Unit = {
                TeachIconButton(
                    TeachIcons.ChevronUp,
                    up,
                    onClick = { onMove(position - 1) },
                    enabled = position > 0,
                    tint = colors.ink,
                )
                TeachIconButton(
                    TeachIcons.ChevronDown,
                    down,
                    onClick = { onMove(position + 1) },
                    enabled =
                        position < count - 1,
                    tint = colors.ink,
                )
            }
            if (currentWindowWidthClass() == WindowWidthClass.Compact) Column { arrows() } else Row { arrows() }
        }
    }
}
