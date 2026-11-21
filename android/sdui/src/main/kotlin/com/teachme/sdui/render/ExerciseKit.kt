package com.teachme.sdui.render

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CollectionItemInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.collectionItemInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.sdui.R
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.logic.Outcome

internal val EXERCISE_CARD_PADDING = 20.dp
private val INDICATOR_SIZE = 24.dp
private val RADIO_DOT = 12.dp
private val KEY_HINT_SIZE = 24.dp
private val CHECKBOX_RADIUS = 6.dp
private val MIN_ACTION_WIDTH = 160.dp
private const val LARGE_FONT = 1.3f

@Composable
internal fun isLargeFont(): Boolean = LocalDensity.current.fontScale > LARGE_FONT

/**
 * The frame every exercise shares (docs/design/components/_exercises.md section 1): a static surface with
 * the type header, and the Ctrl+Enter shortcut for the primary action. The card never changes color
 * with the outcome: the answer lives in the options and the panel.
 */
@Composable
internal fun ExerciseCard(
    required: Boolean,
    icon: Int,
    typeLabel: String,
    onPrimary: (() -> Unit)?,
    modifier: Modifier = Modifier,
    onKey: (KeyEvent) -> Boolean = { false },
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = TeachTheme.colors
    val padding =
        if (currentWindowWidthClass() ==
            WindowWidthClass.Compact
        ) {
            TeachTheme.spacing.md
        } else {
            EXERCISE_CARD_PADDING
        }
    val header = stringResource(R.string.sdui_exercise_a11y, typeLabel)
    val env = LocalSdui.current
    Column(
        modifier =
            modifier
                .fillMaxWidth()
                .background(colors.surface, TeachTheme.shapes.md)
                .border(BorderStroke(TeachTheme.stroke.hairline, colors.line), TeachTheme.shapes.md)
                .onFocusChanged { env.focusInExercise = it.hasFocus }
                .onPreviewKeyEvent { event ->
                    if (event.type == KeyEventType.KeyDown &&
                        event.isCtrlPressed &&
                        (event.key == Key.Enter || event.key == Key.NumPadEnter)
                    ) {
                        onPrimary?.invoke()
                        onPrimary != null
                    } else {
                        onKey(event)
                    }
                }.padding(padding),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().semantics(mergeDescendants = true) { contentDescription = header },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            TeachIcon(icon, contentDescription = null, tint = colors.inkSoft, size = TeachTheme.layout.iconSizeSmall)
            TeachText(typeLabel, Modifier.weight(1f), style = TeachTheme.typography.ui.label, color = colors.inkSoft)
            if (!required) OptionalChip()
        }
        content()
    }
}

@Composable
private fun OptionalChip() {
    TeachText(
        text = stringResource(R.string.sdui_optional),
        modifier =
            Modifier
                .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.full)
                .padding(horizontal = TeachTheme.spacing.xs, vertical = TeachTheme.spacing.xxs),
        style = TeachTheme.typography.ui.labelS,
        color = TeachTheme.colors.inkSoft,
    )
}

/** The question (Literata 500 18/28, terms tappable) and the fixed instruction line under it. */
@Composable
internal fun QuestionAndInstruction(
    question: String?,
    instruction: String?,
    lang: String?,
) {
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        if (!question.isNullOrBlank()) {
            InlineText(markdown = question, style = TeachTheme.typography.reading.question, lang = lang)
        }
        if (instruction != null) {
            TeachText(instruction, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
        }
    }
}

/**
 * The primary action of an exercise ("Controlla", "Invia"): disabled until there is input, with the
 * reason always visible next to it, never a mute button. It disappears once used.
 */
@Composable
internal fun PrimaryAction(
    label: String,
    enabled: Boolean,
    disabledReason: String,
    onClick: () -> Unit,
) {
    val button: @Composable () -> Unit = {
        TeachButton(
            text = label,
            onClick = onClick,
            modifier =
                Modifier
                    .widthIn(min = MIN_ACTION_WIDTH)
                    .semantics { if (!enabled) stateDescription = disabledReason },
            enabled = enabled,
        )
    }
    val reason: @Composable (Modifier) -> Unit = { m ->
        if (!enabled) {
            TeachText(
                disabledReason,
                m,
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkSoft,
            )
        }
    }
    if (isLargeFont()) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            button()
            reason(Modifier)
        }
    } else {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            button()
            reason(Modifier.weight(1f))
        }
    }
}

@Composable
internal fun outcomeTitle(outcome: Outcome): String =
    stringResource(
        when (outcome) {
            Outcome.Correct -> R.string.sdui_outcome_correct
            Outcome.Partial -> R.string.sdui_outcome_partial
            Outcome.Incorrect -> R.string.sdui_outcome_incorrect
        },
    )

/** The result panel: "Corretto" / "Quasi" / "Non proprio" and the explanation, brought into view when it appears. */
@Composable
internal fun OutcomePanel(
    outcome: Outcome,
    bodyMarkdown: String?,
    lang: String?,
    secondAttempt: Boolean,
    modifier: Modifier = Modifier,
    /** Text generated by the app (not markdown), shown before [bodyMarkdown]. */
    plainBody: String? = null,
) {
    val variant =
        when (outcome) {
            Outcome.Correct -> StatusVariant.Correct
            Outcome.Partial -> StatusVariant.Partial
            Outcome.Incorrect -> StatusVariant.Incorrect
        }
    val requester = remember { BringIntoViewRequester() }
    LaunchedEffect(Unit) { requester.bringIntoView() }
    TeachStatusPanel(
        variant = variant,
        title = outcomeTitle(outcome),
        modifier = modifier.bringIntoViewRequester(requester),
    ) {
        if (plainBody != null) {
            TeachText(plainBody, style = TeachTheme.typography.ui.bodyL, color = TeachTheme.colors.ink)
        }
        if (!bodyMarkdown.isNullOrBlank()) {
            InlineText(markdown = bodyMarkdown, style = TeachTheme.typography.ui.bodyL, lang = lang)
        }
        if (secondAttempt) {
            TeachText(
                stringResource(R.string.sdui_second_attempt_counts_first),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
        }
    }
}

/** "Riprova" under the panel, and the caption while the second attempt is open. */
@Composable
internal fun RetryButton(onRetry: () -> Unit) {
    TeachButton(
        text = stringResource(R.string.sdui_retry),
        onClick = onRetry,
        variant = ButtonVariant.Text,
        leadingIcon = TeachIcons.RotateCcw,
    )
}

@Composable
internal fun SecondAttemptCaption() {
    TeachText(
        stringResource(R.string.sdui_second_attempt),
        style = TeachTheme.typography.ui.bodyS,
        color = TeachTheme.colors.inkSoft,
    )
}

/** 24 dp box with the key digit. Same on every option (no cue), hidden from TalkBack, only with a hardware keyboard. */
@Composable
fun KeyHint(digit: Int) {
    Box(
        Modifier
            .size(KEY_HINT_SIZE)
            .background(TeachTheme.colors.surface, TeachTheme.shapes.sm)
            .border(BorderStroke(TeachTheme.stroke.hairline, TeachTheme.colors.line), TeachTheme.shapes.sm)
            .clearAndSetSemantics { },
        contentAlignment = Alignment.Center,
    ) {
        TeachText(
            digit.toString(),
            style = TeachTheme.typography.ui.labelS,
            color = TeachTheme.colors.inkMuted,
            textAlign = TextAlign.Center,
        )
    }
}

/** Digit 1-9 typed without modifiers, or null. */
fun digitOf(event: KeyEvent): Int? {
    if (event.type != KeyEventType.KeyDown || event.isCtrlPressed || event.isAltPressed) return null
    val digits = listOf(Key.One, Key.Two, Key.Three, Key.Four, Key.Five, Key.Six, Key.Seven, Key.Eight, Key.Nine)
    val pad =
        listOf(
            Key.NumPad1,
            Key.NumPad2,
            Key.NumPad3,
            Key.NumPad4,
            Key.NumPad5,
            Key.NumPad6,
            Key.NumPad7,
            Key.NumPad8,
            Key.NumPad9,
        )
    val i = digits.indexOf(event.key).takeIf { it >= 0 } ?: pad.indexOf(event.key).takeIf { it >= 0 } ?: return null
    return i + 1
}

/** What a [ChoiceRow] shows after "Controlla". */
internal enum class ChoiceMark { None, ChosenRight, ChosenWrong, RevealedRight }

internal class ChoiceRowData(
    val markdown: String,
    val selected: Boolean,
    val mark: ChoiceMark,
    /** Explanation shown inside the row (null: nothing, or hidden behind "Perche no?"). */
    val feedback: String?,
    /** A "Perche no?" disclosure instead of a plain feedback. */
    val why: String?,
    val whyOpen: Boolean,
)

/**
 * One option of a quiz or one item of a checklist. Before the check every row looks identical but for its
 * text and its selection (no cues, principle P7); after it, the mark is icon + words + color.
 */
@Composable
internal fun ChoiceRow(
    data: ChoiceRowData,
    multi: Boolean,
    index: Int,
    count: Int,
    locked: Boolean,
    onClick: () -> Unit,
    onToggleWhy: () -> Unit,
    keyHint: Int?,
    lang: String?,
    modifier: Modifier = Modifier,
    markLabels: ChoiceMarkLabels? = null,
    /** Applied to the interactive row itself (focus requester of the group), not to the wrapper. */
    rowModifier: Modifier = Modifier,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    val (fill, borderColor, borderWidth) =
        when {
            data.mark == ChoiceMark.ChosenRight ->
                Triple(
                    colors.correctSoft,
                    colors.correct,
                    TeachTheme.stroke.selected,
                )
            data.mark == ChoiceMark.ChosenWrong ->
                Triple(
                    colors.incorrectSoft,
                    colors.incorrect,
                    TeachTheme.stroke.selected,
                )
            data.mark == ChoiceMark.RevealedRight ->
                Triple(
                    colors.correctSoft,
                    colors.correct,
                    TeachTheme.stroke.control,
                )
            data.selected -> Triple(colors.accentSoft, colors.accent, TeachTheme.stroke.selected)
            state.pressed && !locked -> Triple(colors.surfacePressed, colors.border, TeachTheme.stroke.control)
            else -> Triple(colors.surface, colors.border, TeachTheme.stroke.control)
        }
    val markText =
        when (data.mark) {
            ChoiceMark.ChosenRight -> markLabels?.chosenRight
            ChoiceMark.ChosenWrong -> markLabels?.chosenWrong
            ChoiceMark.RevealedRight -> markLabels?.revealed
            ChoiceMark.None -> null
        }
    val markColor = if (data.mark == ChoiceMark.ChosenWrong) colors.incorrect else colors.correct
    val interactionModifier =
        if (multi) {
            Modifier.toggleable(
                value = data.selected,
                interactionSource = state.source,
                indication = null,
                role = Role.Checkbox,
                onValueChange = { if (!locked) onClick() },
            )
        } else {
            Modifier.selectable(
                selected = data.selected,
                interactionSource = state.source,
                indication = null,
                role = Role.RadioButton,
                onClick = { if (!locked) onClick() },
            )
        }
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
        Row(
            modifier =
                rowModifier
                    .fillMaxWidth()
                    .focusRing(state.focused, shape)
                    .clip(shape)
                    .background(fill, shape)
                    .border(BorderStroke(borderWidth, borderColor), shape)
                    .semantics {
                        collectionItemInfo = CollectionItemInfo(index, 1, 0, 1)
                        if (markText != null) stateDescription = markText
                    }.then(interactionModifier)
                    .heightIn(min = TeachTheme.layout.choiceMinHeight)
                    .padding(horizontal = TeachTheme.spacing.md, vertical = TeachTheme.spacing.sm),
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            Indicator(multi = multi, selected = data.selected, mark = data.mark)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                InlineText(
                    markdown = data.markdown,
                    style = TeachTheme.typography.ui.bodyL,
                    interactive = false,
                    lang = lang,
                )
                if (markText != null) {
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TeachIcon(
                            if (data.mark == ChoiceMark.ChosenWrong) TeachIcons.XCircle else TeachIcons.CheckCircle,
                            contentDescription = null,
                            tint = markColor,
                            size = TeachTheme.layout.iconSizeSmall,
                        )
                        TeachText(markText, style = TeachTheme.typography.ui.label, color = markColor)
                    }
                }
                if (!data.feedback.isNullOrBlank()) {
                    InlineText(markdown = data.feedback, style = TeachTheme.typography.ui.body, lang = lang)
                }
            }
            if (keyHint != null) KeyHint(keyHint)
        }
        if (data.why != null) {
            WhyDisclosure(open = data.whyOpen, feedback = data.why, onToggle = onToggleWhy, lang = lang)
        }
    }
}

internal class ChoiceMarkLabels(
    val chosenRight: String,
    val chosenWrong: String,
    val revealed: String,
)

@Composable
private fun WhyDisclosure(
    open: Boolean,
    feedback: String,
    onToggle: () -> Unit,
    lang: String?,
) {
    val state =
        stringResource(if (open) R.string.sdui_state_open else R.string.sdui_state_closed)
    Column(Modifier.padding(start = TeachTheme.spacing.md)) {
        TeachButton(
            text = stringResource(R.string.sdui_why_not),
            onClick = onToggle,
            variant = ButtonVariant.Text,
            leadingIcon = if (open) TeachIcons.ChevronDown else TeachIcons.ChevronRight,
            modifier = Modifier.semantics { stateDescription = state },
        )
        if (open) InlineText(markdown = feedback, style = TeachTheme.typography.ui.body, lang = lang)
    }
}

@Composable
private fun Indicator(
    multi: Boolean,
    selected: Boolean,
    mark: ChoiceMark,
) {
    val colors = TeachTheme.colors
    val ring =
        when (mark) {
            ChoiceMark.ChosenRight, ChoiceMark.RevealedRight -> colors.correct
            ChoiceMark.ChosenWrong -> colors.incorrect
            ChoiceMark.None -> if (selected) colors.accent else colors.border
        }
    val ringWidth = TeachTheme.stroke
    val density = LocalDensity.current
    Box(Modifier.size(INDICATOR_SIZE).clearAndSetSemantics { }, contentAlignment = Alignment.Center) {
        Canvas(Modifier.size(INDICATOR_SIZE)) {
            val w = (if (selected) ringWidth.selected else ringWidth.control).toPx()
            if (multi) {
                val corner = with(density) { CHECKBOX_RADIUS.toPx() }
                if (selected) {
                    drawRoundRect(
                        ring,
                        cornerRadius =
                            androidx.compose.ui.geometry
                                .CornerRadius(corner),
                        size = size,
                    )
                } else {
                    drawRoundRect(
                        ring,
                        topLeft = Offset(w / 2, w / 2),
                        size = Size(size.width - w, size.height - w),
                        cornerRadius =
                            androidx.compose.ui.geometry
                                .CornerRadius(corner),
                        style = Stroke(w),
                    )
                }
            } else {
                drawCircle(ring, radius = (size.minDimension - w) / 2, style = Stroke(w))
                if (selected) drawCircle(ring, radius = RADIO_DOT.toPx() / 2)
            }
        }
        if (multi && selected) {
            TeachIcon(
                TeachIcons.Check,
                contentDescription = null,
                tint = colors.onAccent,
                size = TeachTheme.layout.iconSizeSmall,
            )
        }
    }
}

/** Roving focus helper for a group of rows: Up/Down move the focus, optionally selecting, digits pick a row. */
internal class GroupFocus(
    val requesters: List<FocusRequester>,
) {
    var focused: Int = -1
}

@Composable
internal fun rememberGroupFocus(count: Int): GroupFocus =
    remember(count) { GroupFocus(List(count) { FocusRequester() }) }

internal fun Modifier.groupRow(
    group: GroupFocus,
    index: Int,
): Modifier =
    this
        .focusRequester(group.requesters[index])
        .onFocusChanged { if (it.isFocused) group.focused = index }
