package com.teachme.sdui.render

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.rememberTextFieldValue
import com.teachme.core.model.LlmKeyVariant
import com.teachme.sdui.R
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.logic.ExerciseReducers
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.Outcome
import com.teachme.sdui.model.Exercise
import com.teachme.sdui.model.OpenAnswer
import kotlinx.coroutines.delay

private val ANSWER_MIN_HEIGHT = 120.dp
private const val ANSWER_MAX_LINES = 10
private const val ANSWER_MIN_LINES = 4
private const val NEAR_LIMIT = 0.9f
private const val SLOW_EVALUATION_MS = 8_000L
private const val YOUR_ANSWER_COLLAPSED_LINES = 6
private const val YOUR_ANSWER_COLLAPSED_CHARS = 360
private const val TALK_QUESTION_CHARS = 120
private const val TALK_ANSWER_CHARS = 600
private const val CLOSED_ALPHA = 0.6f

/**
 * An exercise of a lesson that is already `completed` and has no answer: disabled, with the reason under it
 * (lesson.md section 7.3). Answered exercises and open lessons are drawn as they are.
 */
@Composable
internal fun ReadOnlyIfClosed(
    block: Exercise,
    content: @Composable () -> Unit,
) {
    val env = LocalSdui.current
    if (!env.readOnly || env.states.get(block.id).done) {
        content()
        return
    }
    val reason = stringResource(R.string.sdui_exercise_closed)
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        Box(Modifier.alpha(CLOSED_ALPHA)) {
            Box(Modifier.clearAndSetSemantics { disabled() }) { content() }
            // Swallows taps: the card is only there to be read.
            Box(Modifier.matchParentSize().pointerInput(Unit) { detectTapGestures { } })
        }
        TeachText(
            reason,
            Modifier.semantics { contentDescription = reason },
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

/** `open_answer` (docs/design/components/open_answer.md). The rubric never reaches the app; the Grader answers inside the request. */
@Composable
internal fun OpenAnswerBlock(block: OpenAnswer) {
    val env = LocalSdui.current
    val state = env.states.get(block.id)
    val update = { f: (ExerciseState) -> ExerciseState -> env.states.update(block.id, f) }
    val colors = TeachTheme.colors
    val field = rememberTextFieldValue(state.texts[ExerciseReducers.ANSWER_KEY].orEmpty(), block.id)
    val canSubmit = ExerciseReducers.canSubmit(state)
    val submit = { update { ExerciseReducers.submit(it) } }
    val count = field.value.text.length
    val nearLimit = count >= (block.maxChars * NEAR_LIMIT).toInt()
    val atLimit = count >= block.maxChars
    val question = plain(block.question)
    val description = stringResource(R.string.sdui_answer_a11y, question, count, block.maxChars)

    ExerciseCard(
        required = block.required,
        icon = TeachIcons.PenLine,
        typeLabel = stringResource(R.string.sdui_type_open_answer),
        onPrimary = if (canSubmit) ({ submit() }) else null,
    ) {
        ExerciseCodeSlot(block.code, block.id)
        QuestionAndInstruction(block.question, stringResource(R.string.sdui_open_instruction), block.lang)
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
            var focused by remember { mutableStateOf(false) }
            AnswerFrame(focused = focused, locked = state.submitted) {
                BasicTextField(
                    value = field.value,
                    onValueChange = { new ->
                        val bounded =
                            if (new.text.length > block.maxChars) TextFieldValue(new.text.take(block.maxChars)) else new
                        val changed = bounded.text != field.value.text
                        field.value = bounded
                        if (changed) update { ExerciseReducers.setAnswer(it, bounded.text, block.maxChars) }
                    },
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .heightIn(min = ANSWER_MIN_HEIGHT)
                            .semantics { contentDescription = description },
                    readOnly = state.submitted,
                    minLines = ANSWER_MIN_LINES,
                    maxLines = ANSWER_MAX_LINES,
                    textStyle =
                        TeachTheme.typography.reading.body
                            .copy(color = colors.ink),
                    cursorBrush = SolidColor(colors.accent),
                    keyboardOptions =
                        KeyboardOptions(
                            capitalization = KeyboardCapitalization.Sentences,
                            autoCorrectEnabled = true,
                            // The keyboard spell-checks in the language of the Topic (ADR 0019).
                            hintLocales = localesFor(env.contentLanguage),
                        ),
                    interactionSource = null,
                )
            }
            if (!state.submitted) {
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (atLimit) {
                        TeachText(
                            stringResource(R.string.sdui_answer_limit),
                            Modifier.weight(1f),
                            style = TeachTheme.typography.ui.bodyS,
                            color = colors.inkSoft,
                        )
                    }
                    TeachText(
                        "$count / ${block.maxChars}",
                        style = if (nearLimit) TeachTheme.typography.ui.label else TeachTheme.typography.ui.bodyS,
                        color = colors.inkSoft,
                    )
                }
            }
        }
        if (!state.submitted) {
            PrimaryAction(
                label = stringResource(R.string.sdui_send),
                enabled = canSubmit,
                disabledReason = stringResource(R.string.sdui_open_reason),
                onClick = { submit() },
            )
        } else {
            OpenAnswerResult(block, state, question)
        }
    }
}

/** What follows "Invia": evaluating, graded, or "Risposta registrata" (open_answer.md, States). */
@Composable
private fun OpenAnswerResult(
    block: OpenAnswer,
    state: ExerciseState,
    question: String,
) {
    val env = LocalSdui.current
    val closed = env.readOnly
    val answer = state.texts[ExerciseReducers.ANSWER_KEY].orEmpty()
    when {
        state.evaluating -> {
            val slow by produceState(false, state.evaluating) {
                value = false
                delay(SLOW_EVALUATION_MS)
                value = true
            }
            TeachStatusPanel(
                StatusVariant.Progress,
                stringResource(if (slow) R.string.sdui_open_evaluating_slow else R.string.sdui_open_evaluating),
                modifier = Modifier.semantics { contentDescription = "" },
            )
        }
        state.graded -> {
            val verdict = state.outcome ?: Outcome.Incorrect
            val variant =
                when (verdict) {
                    Outcome.Correct -> StatusVariant.Correct
                    Outcome.Partial -> StatusVariant.Partial
                    Outcome.Incorrect -> StatusVariant.Incorrect
                }
            val word =
                stringResource(
                    when (verdict) {
                        Outcome.Correct -> R.string.sdui_open_correct
                        Outcome.Partial -> R.string.sdui_open_partial
                        Outcome.Incorrect -> R.string.sdui_open_incorrect
                    },
                )
            TeachStatusPanel(variant, word) {
                // The feedback is drawn whole: never truncated, never an inner scroll.
                InlineText(
                    state.feedback.orEmpty(),
                    TeachTheme.typography.ui.bodyL,
                    interactive = false,
                    lang = block.lang,
                )
            }
            YourAnswer(answer)
            if (verdict != Outcome.Correct) {
                TalkToTutor(question, answer, env)
            }
        }
        else -> {
            // With the key known missing or refused, the panel says why and offers the key (llm-key.md 5.1).
            val keyProblem = env.keyProblem.takeIf { !closed && !state.notEvaluable }
            val body =
                when {
                    state.notEvaluable -> R.string.sdui_open_not_evaluable
                    keyProblem == LlmKeyVariant.Missing -> R.string.sdui_open_key_missing_body
                    keyProblem == LlmKeyVariant.Rejected -> R.string.sdui_open_key_rejected_body
                    keyProblem == LlmKeyVariant.Credit -> R.string.sdui_open_pending_credit
                    closed -> R.string.sdui_open_registered_closed
                    state.serverPending -> R.string.sdui_open_registered_pending
                    else -> R.string.sdui_open_registered_local
                }
            TeachStatusPanel(
                StatusVariant.Info,
                stringResource(R.string.sdui_answer_registered),
                body = stringResource(body),
            ) {
                if (state.reevalFailed) {
                    TeachText(
                        stringResource(R.string.sdui_open_reeval_failed),
                        Modifier.padding(top = TeachTheme.spacing.xs),
                        style = TeachTheme.typography.ui.bodyS,
                        color = TeachTheme.colors.ink,
                    )
                }
            }
            // The credit has no way to Settings: "Valuta ora" is its "Riprova" (llm-key.md 9.5).
            if (keyProblem == LlmKeyVariant.Missing || keyProblem == LlmKeyVariant.Rejected) {
                TeachButton(
                    stringResource(
                        if (keyProblem ==
                            LlmKeyVariant.Missing
                        ) {
                            R.string.sdui_open_key_enter
                        } else {
                            R.string.sdui_open_key_replace
                        },
                    ),
                    { env.host.openKeySettings(keyProblem) },
                    variant = ButtonVariant.Secondary,
                    leadingIcon = TeachIcons.KeyRound,
                )
            } else if (!closed && !state.notEvaluable) {
                TeachButton(
                    stringResource(R.string.sdui_open_reeval),
                    { env.host.reevaluate(block.id) },
                    variant = ButtonVariant.Secondary,
                )
            }
            YourAnswer(answer)
        }
    }
}

/** The learner's own words, in a sunken box; long ones are folded behind "Mostra tutto". */
@Composable
private fun YourAnswer(answer: String) {
    if (answer.isBlank()) return
    val long = answer.length > YOUR_ANSWER_COLLAPSED_CHARS || answer.count { it == '\n' } >= YOUR_ANSWER_COLLAPSED_LINES
    var expanded by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
        TeachText(
            stringResource(R.string.sdui_open_your_answer),
            style = TeachTheme.typography.ui.label,
            color = TeachTheme.colors.inkSoft,
        )
        Column(
            Modifier
                .fillMaxWidth()
                .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.sm)
                .padding(TeachTheme.spacing.sm),
        ) {
            SelectionContainer {
                TeachText(
                    answer,
                    style = TeachTheme.typography.reading.body,
                    color = TeachTheme.colors.ink,
                    maxLines = if (long && !expanded) YOUR_ANSWER_COLLAPSED_LINES else Int.MAX_VALUE,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (long) {
            TeachButton(
                stringResource(if (expanded) R.string.sdui_open_show_less else R.string.sdui_open_show_all),
                { expanded = !expanded },
                variant = ButtonVariant.Text,
            )
        }
    }
}

/** "Ne parlo col Tutor": the chat opens with this answer already written (open_answer.md). */
@Composable
private fun TalkToTutor(
    question: String,
    answer: String,
    env: SduiEnvironment,
) {
    val prompt =
        stringResource(
            R.string.sdui_open_talk_prompt,
            question.take(TALK_QUESTION_CHARS),
            answer.take(TALK_ANSWER_CHARS),
        )
    TeachButton(
        stringResource(R.string.sdui_open_talk),
        { env.host.askAgent(prompt) },
        variant = ButtonVariant.Text,
    )
}

@Composable
private fun AnswerFrame(
    focused: Boolean,
    locked: Boolean,
    content: @Composable () -> Unit,
) {
    val colors = TeachTheme.colors
    Column(
        Modifier
            .fillMaxWidth()
            .background(colors.surfaceSunken, TeachTheme.shapes.sm)
            .border(
                if (focused) TeachTheme.stroke.focus else TeachTheme.stroke.control,
                if (focused && !locked) colors.focus else colors.border,
                TeachTheme.shapes.sm,
            ).padding(TeachTheme.spacing.sm),
    ) { content() }
}
