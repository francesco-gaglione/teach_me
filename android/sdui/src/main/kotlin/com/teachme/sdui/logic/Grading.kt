package com.teachme.sdui.logic

import com.teachme.sdui.model.FillBlank
import com.teachme.sdui.model.OrderSteps
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle
import java.text.Normalizer
import java.util.Locale

/** Result of a `fill_blank` check: one flag per blank id and the overall outcome. */
data class FillResult(
    val perBlank: Map<String, Boolean>,
    val outcome: Outcome,
)

// Java's \s is ASCII only: NBSP, thin space, ideographic space etc. are \p{Z}; zero-width ones are listed.
private val WHITESPACE = Regex("[\\s\\p{Z}\\u200B\\u200C\\u200D\\u2060\\uFEFF]+")

/** Local grading of closed exercises, exactly docs/contracts/sdui.md section 5 and 9.18. */
object Grading {
    /** Trim, collapse inner whitespace, NFC, lowercase unless [caseSensitive]. */
    fun normalize(
        text: String,
        caseSensitive: Boolean,
    ): String {
        val collapsed = WHITESPACE.replace(text, " ").trim()
        val nfc = Normalizer.normalize(collapsed, Normalizer.Form.NFC)
        return if (caseSensitive) nfc else nfc.lowercase(Locale.ROOT)
    }

    fun quizSingle(
        block: QuizSingle,
        chosen: String?,
    ): Outcome = if (chosen != null && chosen == block.correct) Outcome.Correct else Outcome.Incorrect

    /** Correct = same set; partial = intersects without matching; incorrect = no intersection. */
    fun quizMulti(
        block: QuizMulti,
        chosen: Set<String>,
    ): Outcome {
        val correct = block.correct.toSet()
        return when {
            chosen == correct -> Outcome.Correct
            chosen.intersect(correct).isNotEmpty() -> Outcome.Partial
            else -> Outcome.Incorrect
        }
    }

    fun fillBlank(
        block: FillBlank,
        typed: Map<String, String>,
    ): FillResult {
        val perBlank =
            block.blanks.associate { blank ->
                val given = normalize(typed[blank.id].orEmpty(), blank.caseSensitive)
                val accepted = blank.answers.map { normalize(it, blank.caseSensitive) }
                blank.id to (given.isNotEmpty() && given in accepted)
            }
        val right = perBlank.values.count { it }
        val outcome =
            when (right) {
                perBlank.size -> Outcome.Correct
                0 -> Outcome.Incorrect
                else -> Outcome.Partial
            }
        return FillResult(perBlank, outcome)
    }

    /** No partial credit in v1: right order or incorrect. */
    fun order(
        block: OrderSteps,
        order: List<String>,
    ): Outcome = if (order == block.steps.map { it.id }) Outcome.Correct else Outcome.Incorrect
}
