package com.teachme.sdui.logic

import com.teachme.core.model.Attempt
import com.teachme.core.model.AttemptGrade
import com.teachme.core.model.AttemptOutcome
import com.teachme.sdui.model.Blank
import com.teachme.sdui.model.ChoiceOption
import com.teachme.sdui.model.FillBlank
import com.teachme.sdui.model.Flashcard
import com.teachme.sdui.model.OpenAnswer
import com.teachme.sdui.model.OrderStep
import com.teachme.sdui.model.OrderSteps
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

class AttemptsTest {
    private fun opts(vararg ids: String) = ids.map { ChoiceOption(it, "t$it", "f$it") }

    private val single = QuizSingle("q1", question = "?", options = opts("a", "b", "c"), correct = "b")
    private val multi = QuizMulti("q2", question = "?", options = opts("a", "b", "c"), correct = listOf("a", "c"))
    private val fill = FillBlank("q3", text = "{{x}}", blanks = listOf(Blank("x", listOf("borrow"))))
    private val order = OrderSteps("q4", prompt = "?", steps = listOf("s1", "s2", "s3").map { OrderStep(it, it) })
    private val card = Flashcard("q5", front = "f", back = "b")
    private val open = OpenAnswer("q6", question = "Perché?", maxChars = 600)

    private fun attempt(
        block: String,
        number: Int = 1,
        outcome: AttemptOutcome,
        answer: JsonObject = JsonObject(emptyMap()),
        feedback: String? = null,
    ) = Attempt(
        id = "$block-$number",
        blockId = block,
        blockType = "x",
        attemptNumber = number,
        answer = answer,
        outcome = outcome,
        grade = feedback?.let { AttemptGrade(outcome, it, null) },
        answeredAt = Instant.ofEpochMilli(1_000L * number),
    )

    // --- AttemptFactory ----------------------------------------------------------------------------------------------

    @Test
    fun `a checked quiz becomes an attempt with the local outcome and the answer of the contract`() {
        val old = ExerciseState(selection = listOf("b"))
        val new = ExerciseReducers.checkQuizSingle(single, old, 5)
        val a = AttemptFactory.fromTransition("L", single, old, new, nowMs = 99)!!
        assertEquals("q1", a.blockId)
        assertEquals("quiz_single", a.blockType)
        assertEquals(1, a.attemptNumber)
        assertEquals("correct", a.outcome)
        assertEquals(buildJsonObject { put("option", "b") }, a.answer)
        assertEquals(99L, a.answeredAtMs)

        val multiState = ExerciseReducers.checkQuizMulti(multi, ExerciseState(selection = listOf("a", "b")), 1)
        val m = AttemptFactory.fromTransition("L", multi, ExerciseState(), multiState, 1)!!
        assertEquals("partial", m.outcome)
        assertEquals(
            buildJsonObject { put("options", JsonArray(listOf(JsonPrimitive("a"), JsonPrimitive("b")))) },
            m.answer,
        )
    }

    @Test
    fun `a retry is attempt number 2 and typing alone makes no attempt`() {
        val first = ExerciseReducers.checkQuizMulti(multi, ExerciseState(selection = listOf("a")), 1)
        val retrying = ExerciseReducers.retry(first, clearSelection = true)
        assertNull(AttemptFactory.fromTransition("L", multi, first, retrying, 2))
        val picked = ExerciseReducers.toggleMulti(retrying, "a")
        assertNull(AttemptFactory.fromTransition("L", multi, retrying, picked, 2))
        val second = ExerciseReducers.checkQuizMulti(multi, ExerciseReducers.toggleMulti(picked, "c"), 3)
        val a = AttemptFactory.fromTransition("L", multi, picked, second, 3)!!
        assertEquals(2, a.attemptNumber)
        assertEquals("correct", a.outcome)
    }

    @Test
    fun `fill blank and order answers follow the contract`() {
        val filled = ExerciseReducers.checkFillBlank(fill, ExerciseState(texts = mapOf("x" to "Borrow")), 1)
        val f = AttemptFactory.fromTransition("L", fill, ExerciseState(), filled, 1)!!
        assertEquals(buildJsonObject { put("blanks", buildJsonObject { put("x", "Borrow") }) }, f.answer)
        assertEquals("correct", f.outcome)

        val checked = ExerciseReducers.checkOrder(order, ExerciseState(), 1)
        val o = AttemptFactory.fromTransition("L", order, ExerciseState(), checked, 1)!!
        val steps = (o.answer["steps"] as JsonArray).map { (it as JsonPrimitive).content }
        assertEquals(setOf("s1", "s2", "s3"), steps.toSet())
        assertEquals(3, steps.size)
    }

    @Test
    fun `flashcards send their first rating only and open answers their trimmed text`() {
        val rated = ExerciseReducers.rate(ExerciseState(revealed = true), Rating.Good)
        val c = AttemptFactory.fromTransition("L", card, ExerciseState(revealed = true), rated, 1)!!
        assertEquals(buildJsonObject { put("rating", "good") }, c.answer)
        assertNull("no outcome is sent for a flashcard", c.outcome)
        val changed = ExerciseReducers.rate(rated, Rating.Easy)
        assertNull(
            "the first rating wins: a change is not sent",
            AttemptFactory.fromTransition("L", card, rated, changed, 2),
        )

        val typed = ExerciseReducers.setAnswer(ExerciseState(), "  Perché è stata spostata.  ", 600)
        val submitted = ExerciseReducers.submit(typed)
        assertTrue(submitted.evaluating)
        val o = AttemptFactory.fromTransition("L", open, typed, submitted, 1)!!
        assertEquals(buildJsonObject { put("text", "Perché è stata spostata.") }, o.answer)
        assertNull(o.outcome)
        assertTrue(o.isOpenAnswer)
    }

    // --- AttemptRestore ----------------------------------------------------------------------------------------------

    private val exercises = listOf(single, multi, fill, order, card, open)

    @Test
    fun `the backend restores exercises the device does not know`() {
        val attempts =
            listOf(
                attempt("q1", outcome = AttemptOutcome.Incorrect, answer = buildJsonObject { put("option", "c") }),
                attempt("q5", outcome = AttemptOutcome.Correct, answer = buildJsonObject { put("rating", "easy") }),
                attempt(
                    "q6",
                    outcome = AttemptOutcome.Partial,
                    answer = buildJsonObject { put("text", "Boh") },
                    feedback = "Manca il perché.",
                ),
            )
        val restored = AttemptRestore.apply(LessonSavedState(), exercises, attempts)
        val q1 = restored.exercise("q1")
        assertTrue(q1.checked && q1.done)
        assertEquals(Outcome.Incorrect, q1.outcome)
        assertEquals(listOf("c"), q1.selection)
        assertEquals(Rating.Easy, restored.exercise("q5").rating)
        val q6 = restored.exercise("q6")
        assertTrue(q6.graded)
        assertEquals(Outcome.Partial, q6.outcome)
        assertEquals("Manca il perché.", q6.feedback)
        assertEquals("Boh", q6.texts[ExerciseReducers.ANSWER_KEY])
    }

    @Test
    fun `a pending open answer counts as done and waits for the grader`() {
        val restored =
            AttemptRestore.apply(
                LessonSavedState(),
                exercises,
                listOf(attempt("q6", outcome = AttemptOutcome.Pending, answer = buildJsonObject { put("text", "x") })),
            )
        val q6 = restored.exercise("q6")
        assertTrue(q6.done)
        assertTrue(q6.serverPending)
        assertFalse(q6.graded)
    }

    @Test
    fun `the device wins when it is ahead or when the attempt is still in the queue`() {
        val local = ExerciseReducers.checkQuizSingle(single, ExerciseState(selection = listOf("b")), 1)
        val saved = LessonSavedState().withExercise("q1", local)
        val stale =
            listOf(attempt("q1", outcome = AttemptOutcome.Incorrect, answer = buildJsonObject { put("option", "a") }))
        assertEquals(local, AttemptRestore.apply(saved, exercises, stale).exercise("q1"))

        val behind = LessonSavedState().withExercise("q1", ExerciseState(selection = listOf("b")))
        val unsent = setOf("q1" to 1)
        assertEquals(behind.exercise("q1"), AttemptRestore.apply(behind, exercises, stale, unsent).exercise("q1"))
    }

    @Test
    fun `a second attempt from the backend restores the first outcome and the latest`() {
        val attempts =
            listOf(
                attempt(
                    "q2",
                    1,
                    AttemptOutcome.Partial,
                    buildJsonObject { put("options", JsonArray(listOf(JsonPrimitive("a")))) },
                ),
                attempt(
                    "q2",
                    2,
                    AttemptOutcome.Correct,
                    buildJsonObject {
                        put("options", JsonArray(listOf(JsonPrimitive("a"), JsonPrimitive("c"))))
                    },
                ),
            )
        val q2 = AttemptRestore.apply(LessonSavedState(), exercises, attempts).exercise("q2")
        assertEquals(2, q2.attempts)
        assertEquals(Outcome.Correct, q2.outcome)
        assertEquals(Outcome.Partial, q2.firstOutcome)
        assertEquals(listOf("a", "c"), q2.selection)
    }

    // --- open answer reducers ----------------------------------------------------------------------------------------

    @Test
    fun `open answer states move from evaluating to graded or registered`() {
        val sent = ExerciseReducers.submit(ExerciseReducers.setAnswer(ExerciseState(), "ciao", 600))
        assertTrue(sent.submitted && sent.evaluating && sent.done)
        val graded = ExerciseReducers.graded(sent, Outcome.Correct, "Bene.")
        assertTrue(graded.graded)
        assertFalse(graded.evaluating)
        val registered = ExerciseReducers.registered(sent, onServer = true, manualRetry = true)
        assertTrue(registered.serverPending && registered.reevalFailed && !registered.evaluating)
        assertTrue(ExerciseReducers.notEvaluable(sent).notEvaluable)
        // A late verdict clears the "registered" marks.
        val late = ExerciseReducers.graded(registered, Outcome.Partial, "Quasi.")
        assertFalse(late.serverPending || late.reevalFailed)
    }

    @Test
    fun `an open answer is offered again with the same pair`() {
        val state = ExerciseReducers.submit(ExerciseReducers.setAnswer(ExerciseState(), " ciao ", 600))
        val again = AttemptFactory.openAnswerAgain("L", open, state, 5)
        assertEquals(1, again.attemptNumber)
        assertEquals("q6", again.blockId)
        assertEquals(buildJsonObject { put("text", "ciao") }, again.answer)
    }

    // --- summary -----------------------------------------------------------------------------------------------------

    @Test
    fun `the local summary lists open answers by how they stand`() {
        val second = OpenAnswer("q7", question = "E poi?", maxChars = 600)
        val third = OpenAnswer("q8", question = "Infine?", maxChars = 600)
        val sections = LessonSections(listOf(Section(0, null, null, listOf(open, second, third))))
        val states =
            mapOf(
                "q6" to ExerciseReducers.graded(ExerciseState(submitted = true), Outcome.Correct, "Bene."),
                "q7" to ExerciseReducers.graded(ExerciseState(submitted = true), Outcome.Partial, "Quasi."),
                "q8" to ExerciseState(submitted = true),
            )
        val summary = SummaryBuilder.build(sections, states = { states[it] ?: ExerciseState() })
        assertEquals(
            listOf(OpenStatus.Correct, OpenStatus.Review, OpenStatus.Pending),
            summary.openAnswers.map { it.status },
        )
        assertEquals(listOf("q7"), summary.toReview.map { it.blockId })
        assertTrue(summary.sections.isEmpty())
    }
}
