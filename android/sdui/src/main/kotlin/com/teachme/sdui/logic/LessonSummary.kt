package com.teachme.sdui.logic

import com.teachme.sdui.inline.InlineMarkdown
import com.teachme.sdui.model.Exercise
import com.teachme.sdui.model.FillBlank
import com.teachme.sdui.model.OpenAnswer
import com.teachme.sdui.model.OrderSteps
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle

/** How a section went, judged on first attempts only (docs/design/screens/lesson.md section 7). */
enum class SectionResult { CorrectFirstTry, CorrectSecondTry, Review }

data class SectionSummary(
    val sectionIndex: Int,
    val title: String?,
    val result: SectionResult,
)

/** An exercise whose first attempt was not fully right: offered under "Da rivedere". */
data class ReviewItem(
    val blockId: String,
    val sectionIndex: Int,
    /** Plain text of the question, markup removed. */
    val question: String,
)

/** How an open answer stands (lesson.md section 7.1). */
enum class OpenStatus { Correct, Review, Pending }

data class OpenAnswerSummary(
    val blockId: String,
    val sectionIndex: Int,
    /** Plain text of the question, markup removed. */
    val question: String,
    val status: OpenStatus,
)

data class LessonSummary(
    val sections: List<SectionSummary>,
    val toReview: List<ReviewItem>,
    val openAnswers: List<OpenAnswerSummary> = emptyList(),
)

/**
 * Local part of the summary (lesson.md section 7.1), computed at once from the learner's own results. Closed exercises
 * with a verdict are summed up per section; open answers get their own row (graded, to review, or "registered");
 * flashcards have no verdict and optional exercises the learner skipped are ignored.
 */
object SummaryBuilder {
    fun build(
        sections: LessonSections,
        states: ExerciseLookup,
        termText: (String) -> String? = { null },
    ): LessonSummary {
        val summaries = mutableListOf<SectionSummary>()
        val review = mutableListOf<ReviewItem>()
        val open = mutableListOf<OpenAnswerSummary>()
        for (section in sections.sections) {
            section.exercises.filterIsInstance<OpenAnswer>().filter { states(it.id).submitted }.forEach {
                val state = states(it.id)
                val question = questionOf(it, termText)
                val status =
                    when (state.outcome) {
                        null -> OpenStatus.Pending
                        Outcome.Correct -> OpenStatus.Correct
                        else -> OpenStatus.Review
                    }
                open += OpenAnswerSummary(it.id, section.index, question, status)
                // Partial and incorrect answers are offered under "Da rivedere" with "Rivedi".
                if (status == OpenStatus.Review) review += ReviewItem(it.id, section.index, question)
            }
            val graded = section.exercises.filter { it.hasVerdict() && states(it.id).firstOutcome != null }
            if (graded.isEmpty()) continue
            val results = graded.map { resultOf(states(it.id)) }
            val overall =
                when {
                    results.any { it == SectionResult.Review } -> SectionResult.Review
                    results.any { it == SectionResult.CorrectSecondTry } -> SectionResult.CorrectSecondTry
                    else -> SectionResult.CorrectFirstTry
                }
            summaries += SectionSummary(section.index, section.title, overall)
            graded
                .filter { states(it.id).firstOutcome != Outcome.Correct }
                .forEach { review += ReviewItem(it.id, section.index, questionOf(it, termText)) }
        }
        return LessonSummary(summaries, review, open)
    }

    private fun Exercise.hasVerdict() =
        this is QuizSingle || this is QuizMulti || this is FillBlank || this is OrderSteps

    private fun resultOf(state: ExerciseState): SectionResult =
        when {
            state.firstOutcome == Outcome.Correct -> SectionResult.CorrectFirstTry
            state.attempts >= 2 && state.outcome == Outcome.Correct -> SectionResult.CorrectSecondTry
            else -> SectionResult.Review
        }

    private fun questionOf(
        exercise: Exercise,
        termText: (String) -> String?,
    ): String {
        val raw =
            when (exercise) {
                is QuizSingle -> exercise.question
                is QuizMulti -> exercise.question
                is FillBlank -> exercise.text.replace(Regex("""\{\{[a-z0-9_-]+\}\}"""), "...")
                is OrderSteps -> exercise.prompt
                is OpenAnswer -> exercise.question
                else -> ""
            }
        return InlineMarkdown.plainText(raw, termText)
    }
}
