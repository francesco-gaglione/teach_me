package com.teachme.sdui.logic

import com.teachme.sdui.model.Exercise
import com.teachme.sdui.model.FillBlank
import com.teachme.sdui.model.OrderSteps
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle

/**
 * State transitions of the exercises, pure and testable. The UI calls them through
 * `ExerciseStates.update`. All of them ignore input once the exercise is [ExerciseState.checked].
 */
object ExerciseReducers {
    private fun ExerciseState.recordCheck(
        outcome: Outcome,
        nowMs: Long,
        blankResults: Map<String, Boolean> = emptyMap(),
    ): ExerciseState =
        copy(
            attempts = attempts + 1,
            checked = true,
            outcome = outcome,
            blankResults = blankResults,
            firstOutcome = firstOutcome ?: outcome,
            firstCheckedAtMs = firstCheckedAtMs ?: nowMs,
        )

    // --- quizzes ---------------------------------------------------------------------------------------

    fun selectSingle(
        state: ExerciseState,
        optionId: String,
    ): ExerciseState = if (state.checked) state else state.copy(selection = listOf(optionId))

    fun toggleMulti(
        state: ExerciseState,
        optionId: String,
    ): ExerciseState {
        if (state.checked) return state
        val next = if (optionId in state.selection) state.selection - optionId else state.selection + optionId
        return state.copy(selection = next)
    }

    fun checkQuizSingle(
        block: QuizSingle,
        state: ExerciseState,
        nowMs: Long,
    ): ExerciseState {
        val chosen = state.selection.firstOrNull()
        if (state.checked || chosen == null) return state
        return state.recordCheck(Grading.quizSingle(block, chosen), nowMs)
    }

    fun checkQuizMulti(
        block: QuizMulti,
        state: ExerciseState,
        nowMs: Long,
    ): ExerciseState {
        if (state.checked || state.selection.isEmpty()) return state
        return state.recordCheck(Grading.quizMulti(block, state.selection.toSet()), nowMs)
    }

    fun toggleWhy(
        state: ExerciseState,
        optionId: String,
    ): ExerciseState =
        state.copy(whyOpen = if (optionId in state.whyOpen) state.whyOpen - optionId else state.whyOpen + optionId)

    // --- fill_blank ------------------------------------------------------------------------------------

    fun setBlankText(
        state: ExerciseState,
        blankId: String,
        text: String,
    ): ExerciseState = if (state.checked) state else state.copy(texts = state.texts + (blankId to text))

    /** Every blank has at least one non-space character. */
    fun fillBlankComplete(
        block: FillBlank,
        state: ExerciseState,
    ): Boolean = block.blanks.all { state.texts[it.id].orEmpty().isNotBlank() }

    fun checkFillBlank(
        block: FillBlank,
        state: ExerciseState,
        nowMs: Long,
    ): ExerciseState {
        if (state.checked || !fillBlankComplete(block, state)) return state
        val result = Grading.fillBlank(block, state.texts)
        return state.recordCheck(result.outcome, nowMs, result.perBlank)
    }

    // --- order_steps -----------------------------------------------------------------------------------

    /** The order on screen: the learner's, or the deterministic shuffle until they move something. */
    fun currentOrder(
        block: OrderSteps,
        state: ExerciseState,
    ): List<String> {
        val ids = block.steps.map { it.id }
        val saved = state.order
        return if (saved.size == ids.size &&
            saved.toSet() == ids.toSet()
        ) {
            saved
        } else {
            OrderShuffle.shuffle(block.id, ids)
        }
    }

    fun moveStep(
        block: OrderSteps,
        state: ExerciseState,
        from: Int,
        to: Int,
    ): ExerciseState {
        if (state.checked) return state
        val order = currentOrder(block, state).toMutableList()
        if (from !in order.indices || to !in order.indices || from == to) return state
        order.add(to, order.removeAt(from))
        return state.copy(order = order)
    }

    fun checkOrder(
        block: OrderSteps,
        state: ExerciseState,
        nowMs: Long,
    ): ExerciseState {
        if (state.checked) return state
        val order = currentOrder(block, state)
        // Pin the order that was checked, even if the learner never touched the list.
        return state.copy(order = order).recordCheck(Grading.order(block, order), nowMs)
    }

    fun toggleCorrectOrder(
        state: ExerciseState,
        defaultOpen: Boolean,
    ): ExerciseState = state.copy(showCorrectOrder = !(state.showCorrectOrder ?: defaultOpen))

    // --- retry -----------------------------------------------------------------------------------------

    /** Offered once, after an `incorrect` or `partial` first attempt, on multi / fill / order only. */
    fun canRetry(
        exercise: Exercise,
        state: ExerciseState,
        locked: Boolean,
    ): Boolean =
        (exercise is QuizMulti || exercise is FillBlank || exercise is OrderSteps) &&
            !locked &&
            state.checked &&
            state.attempts == 1 &&
            state.outcome != Outcome.Correct

    /**
     * Reopens the inputs and clears the marks. Quiz multi: the selection is emptied; fill and order keep
     * what was typed / arranged.
     */
    fun retry(
        state: ExerciseState,
        clearSelection: Boolean,
    ): ExerciseState =
        state.copy(
            checked = false,
            outcome = null,
            blankResults = emptyMap(),
            selection = if (clearSelection) emptyList() else state.selection,
            whyOpen = emptyList(),
            showCorrectOrder = null,
        )

    // --- flashcard -------------------------------------------------------------------------------------

    fun reveal(state: ExerciseState): ExerciseState = state.copy(revealed = true)

    fun rate(
        state: ExerciseState,
        rating: Rating,
    ): ExerciseState = state.copy(rating = rating, ratingOpen = false)

    fun reopenRating(state: ExerciseState): ExerciseState = state.copy(ratingOpen = true)

    // --- open_answer -----------------------------------------------------------------------------------

    fun setAnswer(
        state: ExerciseState,
        text: String,
        maxChars: Int,
    ): ExerciseState =
        if (state.submitted) {
            state
        } else {
            state.copy(
                texts =
                    state.texts + (ANSWER_KEY to text.take(maxChars.coerceAtLeast(0))),
            )
        }

    fun canSubmit(state: ExerciseState): Boolean = !state.submitted && state.texts[ANSWER_KEY].orEmpty().isNotBlank()

    /** "Invia": the answer counts as done at once; the Grader's verdict arrives later (see [graded], [registered]). */
    fun submit(state: ExerciseState): ExerciseState =
        if (canSubmit(state)) state.copy(submitted = true, evaluating = true) else state

    /** The Grader's verdict arrived (also late, from "Risposta registrata"). */
    fun graded(
        state: ExerciseState,
        verdict: Outcome,
        feedback: String,
    ): ExerciseState =
        state.copy(
            submitted = true,
            evaluating = false,
            outcome = verdict,
            feedback = feedback,
            firstOutcome = verdict,
            serverPending = false,
            notEvaluable = false,
            reevalFailed = false,
        )

    /** The answer is kept but not graded: [onServer] says whether the backend has it (`pending`) or only the local queue. */
    fun registered(
        state: ExerciseState,
        onServer: Boolean,
        manualRetry: Boolean = false,
    ): ExerciseState =
        state.copy(
            evaluating = false,
            serverPending = onServer,
            reevalFailed = manualRetry && onServer,
        )

    /** The backend refused the attempt for good (4xx): the answer stays saved on the device only. */
    fun notEvaluable(state: ExerciseState): ExerciseState =
        state.copy(evaluating = false, serverPending = false, notEvaluable = true)

    /** "Valuta ora": the panel goes back to "Sto valutando la tua risposta...". */
    fun evaluating(state: ExerciseState): ExerciseState = state.copy(evaluating = true, reevalFailed = false)

    const val ANSWER_KEY = "answer"

    // --- checklist -------------------------------------------------------------------------------------

    fun toggleItem(
        state: ExerciseState,
        itemId: String,
    ): ExerciseState =
        state.copy(selection = if (itemId in state.selection) state.selection - itemId else state.selection + itemId)
}
