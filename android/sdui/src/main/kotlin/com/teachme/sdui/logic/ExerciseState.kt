package com.teachme.sdui.logic

import kotlinx.serialization.Serializable

@Serializable
enum class Outcome { Correct, Partial, Incorrect }

/** Self-assessment of a flashcard, worst to best (docs/design/components/flashcard.md). */
@Serializable
enum class Rating { Again, Hard, Good, Easy }

/**
 * Everything the learner did on one block, keyed by `block.id` and persisted per lesson. One shape for
 * all exercises (and the checklist, whose checked item ids live in [selection]) keeps persistence and
 * restore trivial; each exercise reads only the fields it uses.
 *
 * Lifecycle (docs/design/components/_exercises.md section 2): idle -> answering -> checked -> (retrying ->
 * checked). The first check is the attempt that counts ([firstOutcome]); a retry never changes it.
 */
@Serializable
data class ExerciseState(
    /** Chosen option ids (quizzes) or checked item ids (checklist). */
    val selection: List<String> = emptyList(),
    /** Typed text: blank id -> text for `fill_blank`, `"answer"` for `open_answer`. Drafts included. */
    val texts: Map<String, String> = emptyMap(),
    /** Current order of step ids in `order_steps`; empty until the learner moves something. */
    val order: List<String> = emptyList(),
    /** How many times the learner pressed "Controlla" (0, 1 or 2). */
    val attempts: Int = 0,
    /** Inputs locked and feedback visible. False while answering and while retrying. */
    val checked: Boolean = false,
    /** Outcome of the latest check. */
    val outcome: Outcome? = null,
    /** Per-blank result of the latest check (`fill_blank`). */
    val blankResults: Map<String, Boolean> = emptyMap(),
    /** Outcome of the first check: the one that counts for the review. */
    val firstOutcome: Outcome? = null,
    val firstCheckedAtMs: Long? = null,
    /** Flashcard: back shown. */
    val revealed: Boolean = false,
    val rating: Rating? = null,
    /** Flashcard: the rating buttons were reopened with "Cambia". */
    val ratingOpen: Boolean = false,
    /** `open_answer`: sent and accepted locally. */
    val submitted: Boolean = false,
    /** `order_steps`: the "Mostra l'ordine corretto" disclosure was toggled (null = default). */
    val showCorrectOrder: Boolean? = null,
    /** Quiz options whose "Perche no?" disclosure is open. */
    val whyOpen: List<String> = emptyList(),
    /** `open_answer`: waiting for the Grader inside the request (never restored after a restart). */
    val evaluating: Boolean = false,
    /** `open_answer`: the Grader's feedback (md inline); the verdict is [outcome]. Null while not graded. */
    val feedback: String? = null,
    /** `open_answer`: the backend has the answer but could not grade it yet (`outcome: pending`). */
    val serverPending: Boolean = false,
    /** `open_answer`: the backend refused the attempt (4xx): "Non sono riuscito a valutare questa risposta". */
    val notEvaluable: Boolean = false,
    /** `open_answer`: the automatic second attempt to grade a pending answer was already used. */
    val autoRetried: Boolean = false,
    /** `open_answer`: the last "Valuta ora" ended pending again. */
    val reevalFailed: Boolean = false,
) {
    /** The learner gave an answer (right or wrong): it counts for unlocking "Continua". */
    val done: Boolean get() = attempts > 0 || rating != null || submitted

    /** Second attempt in progress: reopened after "Riprova", not yet checked again. */
    val retrying: Boolean get() = attempts == 1 && !checked

    /** `open_answer` graded by the backend: [outcome] is the verdict and [feedback] is set. */
    val graded: Boolean get() = submitted && outcome != null && feedback != null
}
