package com.teachme.sdui.render

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachTextField
import com.teachme.core.designsystem.components.rememberTextFieldValue
import com.teachme.sdui.R
import com.teachme.sdui.inline.InlineAction
import com.teachme.sdui.inline.InlineLabels
import com.teachme.sdui.inline.InlineMarkdown
import com.teachme.sdui.inline.appendInline
import com.teachme.sdui.logic.ExerciseReducers
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.Outcome
import com.teachme.sdui.model.Blank
import com.teachme.sdui.model.BlankSegment
import com.teachme.sdui.model.FillBlank
import com.teachme.sdui.model.splitBlankText

private const val LIST_MODE_FONT_SCALE = 1.5f
private const val LIST_MODE_MAX_WIDTH_DP = 360
private val FIELD_ROW_HEIGHT = 48.dp
private val CHIP_SIZE = 28.dp
private const val MAX_SHOWN_ALTERNATIVES = 2

@Composable
private fun useListMode(): Boolean {
    val density = LocalDensity.current
    val width =
        with(density) {
            LocalWindowInfo.current.containerSize.width
                .toDp()
                .value
        }
    return density.fontScale > LIST_MODE_FONT_SCALE || width < LIST_MODE_MAX_WIDTH_DP
}

/** `fill_blank` (docs/design/components/fill_blank.md): free recall; every field has the same width. */
@Composable
internal fun FillBlankBlock(block: FillBlank) {
    val env = LocalSdui.current
    val state = env.states.get(block.id)
    val locked = env.states.isLocked(block.id)
    val update = { f: (ExerciseState) -> ExerciseState -> env.states.update(block.id, f) }
    val complete = ExerciseReducers.fillBlankComplete(block, state)
    val check = { update { ExerciseReducers.checkFillBlank(block, it, env.nowMs()) } }
    val listMode = useListMode()
    val segments = remember(block.text) { splitBlankText(block.text) }
    val order = remember(segments) { segments.filterIsInstance<BlankSegment.Slot>().map { it.blankId } }

    ExerciseCard(
        required = block.required,
        icon = TeachIcons.TextCursorInput,
        typeLabel = stringResource(R.string.sdui_type_fill_blank),
        onPrimary = if (!state.checked && complete) ({ check() }) else null,
    ) {
        QuestionAndInstruction(null, stringResource(R.string.sdui_fill_instruction), block.lang)
        if (state.retrying) SecondAttemptCaption()
        BlankTextFlow(block, segments, order, state, listMode, complete, update, check)
        if (listMode) {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
                order.forEachIndexed { i, id ->
                    val blank = block.blanks.first { it.id == id }
                    val field = rememberTextFieldValue(state.texts[id].orEmpty(), block.id)
                    TeachTextField(
                        value = field.value,
                        onValueChange = {
                            val changed = it.text != field.value.text
                            field.value = it
                            if (changed) update { s -> ExerciseReducers.setBlankText(s, blank.id, it.text) }
                        },
                        label = stringResource(R.string.sdui_blank_label, i + 1),
                        modifier = Modifier.fillMaxWidth(),
                        readOnly = state.checked,
                        keyboardOptions = blankKeyboard(last = i == order.lastIndex),
                    )
                }
            }
        }
        if (!state.checked) {
            PrimaryAction(
                label = stringResource(R.string.sdui_check),
                enabled = complete,
                disabledReason = stringResource(R.string.sdui_fill_reason),
                onClick = { check() },
            )
        } else {
            BlankResults(block, order, state)
            val outcome = state.outcome ?: Outcome.Incorrect
            val body = if (outcome == Outcome.Correct) block.feedback.correct else block.feedback.incorrect
            OutcomePanel(outcome, body, block.lang, secondAttempt = state.attempts >= 2)
            if (ExerciseReducers.canRetry(block, state, locked)) {
                RetryButton { update { ExerciseReducers.retry(it, clearSelection = false) } }
            }
        }
    }
}

private fun blankKeyboard(last: Boolean) =
    KeyboardOptions(
        capitalization = KeyboardCapitalization.None,
        autoCorrectEnabled = false,
        keyboardType = KeyboardType.Text,
        imeAction = if (last) ImeAction.Done else ImeAction.Next,
    )

@Composable
private fun BlankTextFlow(
    block: FillBlank,
    segments: List<BlankSegment>,
    order: List<String>,
    state: ExerciseState,
    listMode: Boolean,
    complete: Boolean,
    update: ((ExerciseState) -> ExerciseState) -> Unit,
    check: () -> Unit,
) {
    val env = LocalSdui.current
    val colors = TeachTheme.colors
    val base = TeachTheme.typography.reading.body
    val codeStyle = TeachTheme.typography.code.inline
    val density = LocalDensity.current
    val labels = remember { InlineLabels(term = { it }, cite = { _, t -> t }) }
    val annotated =
        remember(segments, base, colors, env) {
            buildAnnotatedString {
                val ranges = mutableListOf<IntRange>()
                val actions = mutableListOf<InlineAction>()
                for (segment in segments) {
                    when (segment) {
                        is BlankSegment.Literal ->
                            appendInline(
                                InlineMarkdown.parse(segment.text),
                                base,
                                colors,
                                codeStyle,
                                env,
                                false,
                                labels,
                                ranges,
                                actions,
                            )
                        is BlankSegment.Slot ->
                            appendInlineContent(
                                "blank_${segment.blankId}",
                                "[${order.indexOf(segment.blankId) + 1}]",
                            )
                    }
                }
            }
        }
    val slotWidth = with(density) { (if (listMode) CHIP_SIZE else TeachTheme.layout.blankFieldWidth).toSp() }
    val slotHeight = with(density) { (if (listMode) CHIP_SIZE else FIELD_ROW_HEIGHT).toSp() }
    val focusManager = LocalFocusManager.current
    // A fixed line height (reading.body 28sp) clips inline placeholders: a taller slot is then drawn over the
    // neighbouring lines. Grow the line height to the slot so every line holding a blank fits it.
    val lineStyle =
        base.copy(
            color = colors.ink,
            lineHeight = if (base.lineHeight.isSp) maxOf(base.lineHeight.value, slotHeight.value).sp else slotHeight,
            lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
        )
    val content: Map<String, InlineTextContent> =
        order
            .mapIndexed { i, id ->
                val blank = block.blanks.first { it.id == id }
                "blank_$id" to
                    InlineTextContent(Placeholder(slotWidth, slotHeight, PlaceholderVerticalAlign.Center)) {
                        if (listMode) {
                            NumberChip(i + 1)
                        } else {
                            val field = rememberTextFieldValue(state.texts[id].orEmpty(), block.id)
                            BlankField(
                                blank = blank,
                                number = i + 1,
                                count = order.size,
                                field = field.value,
                                onValueChange = {
                                    val changed = it.text != field.value.text
                                    field.value = it
                                    if (changed) update { s -> ExerciseReducers.setBlankText(s, blank.id, it.text) }
                                },
                                checked = state.checked,
                                result = state.blankResults[id],
                                last = i == order.lastIndex,
                                onNext = { focusManager.moveFocus(FocusDirection.Next) },
                                onDone = { if (complete) check() else focusManager.clearFocus() },
                            )
                        }
                    }
            }.toMap()
    BasicText(
        text = annotated,
        inlineContent = content,
        style = lineStyle,
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun NumberChip(number: Int) {
    Box(
        Modifier
            .size(CHIP_SIZE)
            .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.sm),
        contentAlignment = Alignment.Center,
    ) {
        TeachText("($number)", style = TeachTheme.typography.ui.labelS, color = TeachTheme.colors.inkSoft)
    }
}

@Composable
private fun BlankField(
    blank: Blank,
    number: Int,
    count: Int,
    field: TextFieldValue,
    onValueChange: (TextFieldValue) -> Unit,
    checked: Boolean,
    result: Boolean?,
    last: Boolean,
    onNext: () -> Unit,
    onDone: () -> Unit,
) {
    val colors = TeachTheme.colors
    val source = remember { MutableInteractionSource() }
    val focused by source.collectIsFocusedAsState()
    val (fill, border, width) =
        when {
            checked && result == true -> Triple(colors.correctSoft, colors.correct, TeachTheme.stroke.selected)
            checked -> Triple(colors.incorrectSoft, colors.incorrect, TeachTheme.stroke.selected)
            focused -> Triple(colors.surfaceSunken, colors.focus, TeachTheme.stroke.focus)
            else -> Triple(colors.surfaceSunken, colors.border, TeachTheme.stroke.control)
        }
    val description = stringResource(R.string.sdui_blank_a11y, number, count)
    val resultText =
        if (!checked) {
            null
        } else if (result == true) {
            stringResource(R.string.sdui_blank_state_correct)
        } else {
            stringResource(R.string.sdui_blank_state_incorrect, blank.answers.firstOrNull().orEmpty())
        }
    Box(
        Modifier.width(TeachTheme.layout.blankFieldWidth).heightIn(min = FIELD_ROW_HEIGHT),
        contentAlignment = Alignment.Center,
    ) {
        Row(
            Modifier
                .width(TeachTheme.layout.blankFieldWidth)
                .size(width = TeachTheme.layout.blankFieldWidth, height = TeachTheme.layout.blankFieldHeight)
                .background(fill, TeachTheme.shapes.sm)
                .border(width, border, TeachTheme.shapes.sm)
                .padding(start = TeachTheme.spacing.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BasicTextField(
                value = field,
                onValueChange = onValueChange,
                modifier =
                    Modifier
                        .weight(1f)
                        .semantics {
                            contentDescription = description
                            if (resultText != null) stateDescription = resultText
                        },
                readOnly = checked,
                singleLine = true,
                textStyle =
                    TeachTheme.typography.code.inline
                        .copy(color = colors.ink),
                cursorBrush = SolidColor(colors.accent),
                keyboardOptions = blankKeyboard(last),
                keyboardActions = KeyboardActions(onNext = { onNext() }, onDone = { onDone() }),
                interactionSource = source,
            )
            if (checked) {
                TeachIcon(
                    if (result == true) TeachIcons.CheckCircle else TeachIcons.XCircle,
                    contentDescription = null,
                    tint = if (result == true) colors.correct else colors.incorrect,
                    size = TeachTheme.layout.iconSizeSmall,
                    modifier = Modifier.padding(horizontal = TeachTheme.spacing.xxs),
                )
            }
        }
    }
}

@Composable
private fun BlankResults(
    block: FillBlank,
    order: List<String>,
    state: ExerciseState,
) {
    val colors = TeachTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        order.forEachIndexed { i, id ->
            val blank = block.blanks.first { it.id == id }
            val right = state.blankResults[id] == true
            val others =
                blank.answers
                    .drop(1)
                    .filter { it.isNotBlank() }
                    .take(MAX_SHOWN_ALTERNATIVES)
            val line =
                if (right) {
                    stringResource(R.string.sdui_blank_result_correct, i + 1)
                } else {
                    stringResource(R.string.sdui_blank_result_incorrect, i + 1)
                }
            val detail =
                if (right) {
                    null
                } else {
                    buildString {
                        append(stringResource(R.string.sdui_blank_expected, blank.answers.firstOrNull().orEmpty()))
                        if (others.isNotEmpty()) {
                            append(
                                ' ',
                            ).append(
                                stringResource(
                                    R.string.sdui_blank_also,
                                    others.joinToString(", ") {
                                        "«$it»"
                                    },
                                ),
                            )
                        }
                    }
                }
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(
                        min = TeachTheme.layout.minTouchTarget,
                    ).semantics(mergeDescendants = true) {},
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachIcon(
                    if (right) TeachIcons.CheckCircle else TeachIcons.XCircle,
                    contentDescription = null,
                    tint = if (right) colors.correct else colors.incorrect,
                    size = TeachTheme.layout.iconSizeSmall,
                )
                Column(Modifier.weight(1f)) {
                    TeachText(
                        line,
                        style = TeachTheme.typography.ui.label,
                        color = if (right) colors.correct else colors.incorrect,
                    )
                    if (detail != null) TeachText(detail, style = TeachTheme.typography.ui.body, color = colors.inkSoft)
                }
            }
        }
    }
}
