package com.teachme.sdui.logic

import com.teachme.core.model.Attempt
import com.teachme.core.model.AttemptOutcome
import com.teachme.core.model.QueuedAttempt
import com.teachme.sdui.model.Exercise
import com.teachme.sdui.model.FillBlank
import com.teachme.sdui.model.Flashcard
import com.teachme.sdui.model.OpenAnswer
import com.teachme.sdui.model.OrderSteps
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** Wire name of the local [Outcome] (`correct` | `partial` | `incorrect`, lessons.md section 5). */
fun Outcome.wire(): String =
    when (this) {
        Outcome.Correct -> "correct"
        Outcome.Partial -> "partial"
        Outcome.Incorrect -> "incorrect"
    }

fun Outcome.toAttemptOutcome(): AttemptOutcome =
    when (this) {
        Outcome.Correct -> AttemptOutcome.Correct
        Outcome.Partial -> AttemptOutcome.Partial
        Outcome.Incorrect -> AttemptOutcome.Incorrect
    }

/** The local [Outcome] of a backend one; pending and unknown values have none. */
fun AttemptOutcome.toLocalOutcome(): Outcome? =
    when (this) {
        AttemptOutcome.Correct -> Outcome.Correct
        AttemptOutcome.Partial -> Outcome.Partial
        AttemptOutcome.Incorrect -> Outcome.Incorrect
        AttemptOutcome.Pending, AttemptOutcome.Unknown -> null
    }

private fun Rating.wire() = name.lowercase()

private fun ratingOf(wire: String?): Rating? = Rating.entries.firstOrNull { it.wire() == wire }

/**
 * Turns what the learner just did on an exercise into the attempt to register (lessons.md sections 2.1 and 5).
 * Pure: the screen calls [fromTransition] after every state change and queues what it returns.
 */
object AttemptFactory {
    fun fromTransition(
        lessonId: String,
        block: Exercise,
        old: ExerciseState,
        new: ExerciseState,
        nowMs: Long,
    ): QueuedAttempt? {
        fun attempt(
            number: Int,
            answer: JsonObject,
            outcome: Outcome?,
        ) = QueuedAttempt(
            lessonId = lessonId,
            blockId = block.id,
            blockType = block.type,
            attemptNumber = number,
            answer = answer,
            outcome = outcome?.wire(),
            answeredAtMs = nowMs,
        )
        return when (block) {
            is QuizSingle, is QuizMulti, is FillBlank, is OrderSteps -> {
                // A new "Controlla": the counter grew and the exercise is checked.
                if (!new.checked || new.attempts <= old.attempts || new.attempts !in 1..2) return null
                val outcome = new.outcome ?: return null
                attempt(new.attempts, answerOf(block, new), outcome)
            }
            is Flashcard -> {
                // Only the first rating counts ("vince il primo"): a changed rating is not sent again.
                val rating = new.rating ?: return null
                if (old.rating != null) return null
                attempt(1, buildJsonObject { put("rating", rating.wire()) }, null)
            }
            is OpenAnswer -> {
                if (!new.submitted || old.submitted) return null
                attempt(
                    1,
                    buildJsonObject { put("text", new.texts[ExerciseReducers.ANSWER_KEY].orEmpty().trim()) },
                    null,
                )
            }
            else -> null
        }
    }

    /** The `answer` of a closed exercise, in the shape of lessons.md section 2.1. */
    fun answerOf(
        block: Exercise,
        state: ExerciseState,
    ): JsonObject =
        when (block) {
            is QuizSingle -> buildJsonObject { put("option", state.selection.firstOrNull().orEmpty()) }
            is QuizMulti -> buildJsonObject { put("options", JsonArray(state.selection.map(::JsonPrimitive))) }
            is FillBlank ->
                buildJsonObject {
                    put(
                        "blanks",
                        buildJsonObject { block.blanks.forEach { put(it.id, state.texts[it.id].orEmpty()) } },
                    )
                }
            is OrderSteps ->
                buildJsonObject {
                    put("steps", JsonArray(ExerciseReducers.currentOrder(block, state).map(::JsonPrimitive)))
                }
            else -> JsonObject(emptyMap())
        }

    /** The same open answer, to be sent again ("Valuta ora"): the pair is the same, so the backend re-grades a pending one. */
    fun openAnswerAgain(
        lessonId: String,
        block: OpenAnswer,
        state: ExerciseState,
        nowMs: Long,
    ) = QueuedAttempt(
        lessonId,
        block.id,
        block.type,
        attemptNumber = 1,
        answer = buildJsonObject { put("text", state.texts[ExerciseReducers.ANSWER_KEY].orEmpty().trim()) },
        outcome = null,
        answeredAtMs = nowMs,
    )
}

/** Brings the local state of a lesson up to date with what the backend has (lessons.md section 3.1, lesson.md 8.1). */
object AttemptRestore {
    /**
     * Applies the backend [attempts] to [saved]. The backend wins where the device is behind (another device, cleared
     * data); where the device already has the same or a later attempt it is kept. [unsent] are the (block, attempt number)
     * pairs still in the local queue: those stay as they are.
     */
    fun apply(
        saved: LessonSavedState,
        exercises: List<Exercise>,
        attempts: List<Attempt>,
        unsent: Set<Pair<String, Int>> = emptySet(),
    ): LessonSavedState {
        var result = saved
        val byBlock = attempts.groupBy { it.blockId }
        for (block in exercises) {
            val server = byBlock[block.id]?.sortedBy { it.attemptNumber }.orEmpty()
            if (server.isEmpty()) continue
            val local = result.exercise(block.id)
            val latest = server.last()
            if ((block.id to latest.attemptNumber) in unsent) continue
            val restored = restoreOne(block, local, server) ?: continue
            result = result.withExercise(block.id, restored)
        }
        return result
    }

    private fun restoreOne(
        block: Exercise,
        local: ExerciseState,
        server: List<Attempt>,
    ): ExerciseState? {
        val first = server.firstOrNull { it.attemptNumber == 1 } ?: server.first()
        val latest = server.last()
        return when (block) {
            is OpenAnswer -> restoreOpen(local, latest)
            is Flashcard ->
                if (local.rating != null) {
                    null
                } else {
                    ratingOf(latest.answer["rating"]?.jsonPrimitive?.contentOrNull)?.let {
                        local.copy(revealed = true, rating = it, ratingOpen = false)
                    }
                }
            is QuizSingle, is QuizMulti, is FillBlank, is OrderSteps -> {
                // The device is ahead or equal: keep it (it also has the marks of the fill blanks).
                if (local.attempts >= latest.attemptNumber && local.checked) return null
                val outcome = latest.outcome.toLocalOutcome() ?: return null
                val firstOutcome = first.outcome.toLocalOutcome() ?: outcome
                withAnswer(block, local, latest.answer).copy(
                    attempts = latest.attemptNumber,
                    checked = true,
                    outcome = outcome,
                    firstOutcome = firstOutcome,
                    firstCheckedAtMs = first.answeredAt?.toEpochMilli() ?: local.firstCheckedAtMs,
                    blankResults = fillMarks(block, local, latest.answer),
                )
            }
            else -> null
        }
    }

    private fun fillMarks(
        block: Exercise,
        local: ExerciseState,
        answer: JsonObject,
    ): Map<String, Boolean> {
        if (block !is FillBlank) return emptyMap()
        val typed = blanksOf(answer)
        return Grading.fillBlank(block, typed.ifEmpty { local.texts }).perBlank
    }

    private fun blanksOf(answer: JsonObject): Map<String, String> =
        runCatching {
            answer["blanks"]?.jsonObject?.mapValues { it.value.jsonPrimitive.content }.orEmpty()
        }.getOrDefault(emptyMap())

    private fun withAnswer(
        block: Exercise,
        local: ExerciseState,
        answer: JsonObject,
    ): ExerciseState =
        runCatching {
            when (block) {
                is QuizSingle -> local.copy(selection = listOfNotNull(answer["option"]?.jsonPrimitive?.contentOrNull))
                is QuizMulti ->
                    local.copy(selection = answer["options"]?.jsonArray?.map { it.jsonPrimitive.content }.orEmpty())
                is FillBlank -> local.copy(texts = local.texts + blanksOf(answer))
                is OrderSteps ->
                    local.copy(order = answer["steps"]?.jsonArray?.map { it.jsonPrimitive.content }.orEmpty())
                else -> local
            }
        }.getOrDefault(local)

    private fun restoreOpen(
        local: ExerciseState,
        server: Attempt,
    ): ExerciseState? {
        val text = server.answer["text"]?.jsonPrimitive?.contentOrNull
        val base =
            local.copy(
                submitted = true,
                evaluating = false,
                notEvaluable = false,
                texts =
                    if (text != null) local.texts + (ExerciseReducers.ANSWER_KEY to text) else local.texts,
            )
        val verdict = server.outcome.toLocalOutcome()
        val feedback = server.grade?.feedback
        return if (verdict != null && feedback != null) {
            if (local.graded && local.outcome == verdict && local.feedback == feedback) return null
            base.copy(
                outcome = verdict,
                feedback = feedback,
                firstOutcome = verdict,
                serverPending = false,
                reevalFailed = false,
            )
        } else {
            // Pending (or a newer backend's value): the answer counts as done, to be graded.
            if (local.submitted && local.graded) return null
            base.copy(serverPending = true)
        }
    }
}
