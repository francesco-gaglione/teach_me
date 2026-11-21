package com.teachme.sdui.render

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CollectionInfo
import androidx.compose.ui.semantics.collectionInfo
import androidx.compose.ui.semantics.semantics
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.sdui.R
import com.teachme.sdui.logic.ExerciseReducers
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.Outcome
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle

@Composable
private fun markLabels(multi: Boolean) =
    ChoiceMarkLabels(
        chosenRight = stringResource(R.string.sdui_mark_chosen_right),
        chosenWrong = stringResource(R.string.sdui_mark_chosen_wrong),
        revealed = stringResource(if (multi) R.string.sdui_mark_was_correct else R.string.sdui_mark_correct_answer),
    )

/** `quiz_single` (docs/design/components/quiz_single.md). The options keep the author's order. */
@Composable
internal fun QuizSingleBlock(block: QuizSingle) {
    val env = LocalSdui.current
    val state = env.states.get(block.id)
    val update = { f: (ExerciseState) -> ExerciseState -> env.states.update(block.id, f) }
    val chosen = state.selection.firstOrNull()
    val group = rememberGroupFocus(block.options.size)
    val check = { update { ExerciseReducers.checkQuizSingle(block, it, env.nowMs()) } }
    val select = { id: String -> update { ExerciseReducers.selectSingle(it, id) } }
    val onKey = { e: KeyEvent ->
        choiceKeys(e, group, block.options.size, state.checked, onPick = { i ->
            select(block.options[i].id)
        }, moveSelects = true, onEnter = {
            if (chosen !=
                null
            ) {
                check()
            }
        })
    }

    ExerciseCard(
        required = block.required,
        icon = TeachIcons.CircleDot,
        typeLabel = stringResource(R.string.sdui_type_quiz_single),
        onPrimary = if (!state.checked && chosen != null) ({ check() }) else null,
        onKey = onKey,
    ) {
        ExerciseCodeSlot(block.code, block.id)
        QuestionAndInstruction(block.question, stringResource(R.string.sdui_quiz_single_instruction), block.lang)
        Column(
            Modifier.selectableGroup().semantics { collectionInfo = CollectionInfo(block.options.size, 1) },
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            val labels = markLabels(multi = false)
            block.options.forEachIndexed { i, option ->
                val isChosen = option.id == chosen
                val isCorrect = option.id == block.correct
                val mark =
                    when {
                        !state.checked -> ChoiceMark.None
                        isChosen && isCorrect -> ChoiceMark.ChosenRight
                        isChosen -> ChoiceMark.ChosenWrong
                        isCorrect -> ChoiceMark.RevealedRight
                        else -> ChoiceMark.None
                    }
                ChoiceRow(
                    data =
                        ChoiceRowData(
                            markdown = option.text,
                            selected = isChosen,
                            mark = mark,
                            // The chosen option's explanation is the panel; the revealed correct one carries its own.
                            feedback = if (mark == ChoiceMark.RevealedRight) option.feedback else null,
                            why =
                                if (state.checked &&
                                    mark == ChoiceMark.None
                                ) {
                                    option.feedback.takeIf { it.isNotBlank() }
                                } else {
                                    null
                                },
                            whyOpen = option.id in state.whyOpen,
                        ),
                    multi = false,
                    index = i,
                    count = block.options.size,
                    locked = state.checked,
                    onClick = {
                        select(option.id)
                        runCatching { group.requesters[i].requestFocus() }
                    },
                    onToggleWhy = { update { ExerciseReducers.toggleWhy(it, option.id) } },
                    keyHint = if (env.hardwareKeyboard && !state.checked) i + 1 else null,
                    lang = block.lang,
                    rowModifier = Modifier.groupRow(group, i),
                    markLabels = labels,
                )
            }
        }
        if (!state.checked) {
            PrimaryAction(
                label = stringResource(R.string.sdui_check),
                enabled = chosen != null,
                disabledReason = stringResource(R.string.sdui_quiz_single_reason),
                onClick = { check() },
            )
        } else {
            val feedback = block.options.firstOrNull { it.id == chosen }?.feedback
            OutcomePanel(state.outcome ?: Outcome.Incorrect, feedback, block.lang, secondAttempt = false)
        }
    }
}

/** `quiz_multi` (docs/design/components/quiz_multi.md): exact match or partial, retry once. */
@Composable
internal fun QuizMultiBlock(block: QuizMulti) {
    val env = LocalSdui.current
    val state = env.states.get(block.id)
    val locked = env.states.isLocked(block.id)
    val update = { f: (ExerciseState) -> ExerciseState -> env.states.update(block.id, f) }
    val group = rememberGroupFocus(block.options.size)
    val check = { update { ExerciseReducers.checkQuizMulti(block, it, env.nowMs()) } }
    val toggle = { id: String -> update { ExerciseReducers.toggleMulti(it, id) } }
    val canCheck = !state.checked && state.selection.isNotEmpty()
    val onKey = { e: KeyEvent ->
        choiceKeys(e, group, block.options.size, state.checked, onPick = { i ->
            toggle(block.options[i].id)
        }, moveSelects = false, onEnter = {})
    }

    ExerciseCard(
        required = block.required,
        icon = TeachIcons.SquareCheck,
        typeLabel = stringResource(R.string.sdui_type_quiz_multi),
        onPrimary = if (canCheck) ({ check() }) else null,
        onKey = onKey,
    ) {
        ExerciseCodeSlot(block.code, block.id)
        QuestionAndInstruction(block.question, stringResource(R.string.sdui_quiz_multi_instruction), block.lang)
        if (state.retrying) SecondAttemptCaption()
        Column(
            Modifier.selectableGroup().semantics { collectionInfo = CollectionInfo(block.options.size, 1) },
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            val labels = markLabels(multi = true)
            block.options.forEachIndexed { i, option ->
                val isChosen = option.id in state.selection
                val isCorrect = option.id in block.correct
                val mark =
                    when {
                        !state.checked -> ChoiceMark.None
                        isChosen && isCorrect -> ChoiceMark.ChosenRight
                        isChosen -> ChoiceMark.ChosenWrong
                        isCorrect -> ChoiceMark.RevealedRight
                        else -> ChoiceMark.None
                    }
                val showFeedback = mark != ChoiceMark.None
                ChoiceRow(
                    data =
                        ChoiceRowData(
                            markdown = option.text,
                            selected = isChosen,
                            mark = mark,
                            feedback = if (showFeedback) option.feedback else null,
                            why =
                                if (state.checked &&
                                    !showFeedback
                                ) {
                                    option.feedback.takeIf { it.isNotBlank() }
                                } else {
                                    null
                                },
                            whyOpen = option.id in state.whyOpen,
                        ),
                    multi = true,
                    index = i,
                    count = block.options.size,
                    locked = state.checked,
                    onClick = {
                        toggle(option.id)
                        runCatching { group.requesters[i].requestFocus() }
                    },
                    onToggleWhy = { update { ExerciseReducers.toggleWhy(it, option.id) } },
                    keyHint = if (env.hardwareKeyboard && !state.checked) i + 1 else null,
                    lang = block.lang,
                    rowModifier = Modifier.groupRow(group, i),
                    markLabels = labels,
                )
            }
        }
        if (!state.checked) {
            PrimaryAction(
                label = stringResource(R.string.sdui_check),
                enabled = canCheck,
                disabledReason = stringResource(R.string.sdui_quiz_multi_reason),
                onClick = { check() },
            )
        } else {
            val outcome = state.outcome ?: Outcome.Incorrect
            OutcomePanel(
                outcome,
                null,
                block.lang,
                secondAttempt = state.attempts >= 2,
                plainBody = multiSummary(block, state, outcome),
            )
            if (ExerciseReducers.canRetry(block, state, locked)) {
                RetryButton { update { ExerciseReducers.retry(it, clearSelection = true) } }
            }
        }
    }
}

@Composable
private fun multiSummary(
    block: QuizMulti,
    state: ExerciseState,
    outcome: Outcome,
): String {
    val total = block.correct.size
    val found = state.selection.count { it in block.correct }
    val wrong = state.selection.size - found
    return if (outcome == Outcome.Incorrect) {
        stringResource(R.string.sdui_multi_none)
    } else {
        pluralStringResource(R.plurals.sdui_multi_found, total, found, total) +
            (if (wrong > 0) pluralStringResource(R.plurals.sdui_multi_wrong, wrong, wrong) else "") + "."
    }
}

/**
 * Keys shared by the choice groups: digits pick a row, Up/Down move the focus (and select it in a radio
 * group), Enter checks when something is chosen. Returns true when the key was used.
 */
internal fun choiceKeys(
    event: KeyEvent,
    group: GroupFocus,
    count: Int,
    locked: Boolean,
    onPick: (Int) -> Unit,
    moveSelects: Boolean,
    onEnter: () -> Unit,
): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    digitOf(event)?.let { digit ->
        if (digit > count || locked) return false
        onPick(digit - 1)
        group.requesters[digit - 1].requestFocus()
        return true
    }
    val delta =
        when (event.key) {
            Key.DirectionDown -> 1
            Key.DirectionUp -> -1
            else -> 0
        }
    if (delta != 0 && group.focused >= 0) {
        val next = (group.focused + delta).coerceIn(0, count - 1)
        if (next != group.focused) {
            group.requesters[next].requestFocus()
            if (moveSelects && !locked) onPick(next)
        }
        return true
    }
    if ((event.key == Key.Enter || event.key == Key.NumPadEnter) && !locked && group.focused >= 0) {
        onEnter()
        return true
    }
    return false
}
