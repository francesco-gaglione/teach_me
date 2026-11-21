package com.teachme.sdui.logic

import com.teachme.sdui.model.Blank
import com.teachme.sdui.model.ChoiceOption
import com.teachme.sdui.model.FillBlank
import com.teachme.sdui.model.OrderStep
import com.teachme.sdui.model.OrderSteps
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ExerciseReducersTest {
    private fun opts(vararg ids: String) = ids.map { ChoiceOption(it, "t$it", "f$it") }

    private val single = QuizSingle("q", question = "?", options = opts("a", "b", "c"), correct = "b")
    private val multi = QuizMulti("m", question = "?", options = opts("a", "b", "c", "d"), correct = listOf("a", "c"))
    private val fill =
        FillBlank(
            "f",
            text = "{{x}} e {{y}}",
            blanks = listOf(Blank("x", listOf("uno")), Blank("y", listOf("due", "2"))),
        )
    private val order = OrderSteps("o", prompt = "?", steps = listOf("s1", "s2", "s3", "s4").map { OrderStep(it, it) })

    @Test
    fun `quiz single can change the choice until checked and not after`() {
        var s = ExerciseReducers.selectSingle(ExerciseState(), "a")
        s = ExerciseReducers.selectSingle(s, "c")
        assertEquals(listOf("c"), s.selection)
        assertFalse(s.done)
        val checked = ExerciseReducers.checkQuizSingle(single, s, nowMs = 7)
        assertTrue(checked.done)
        assertEquals(Outcome.Incorrect, checked.outcome)
        assertEquals(Outcome.Incorrect, checked.firstOutcome)
        assertEquals(7L, checked.firstCheckedAtMs)
        assertEquals(checked, ExerciseReducers.selectSingle(checked, "b"))
        assertEquals(checked, ExerciseReducers.checkQuizSingle(single, checked, 9))
        assertFalse("quiz_single is never retried", ExerciseReducers.canRetry(single, checked, locked = false))
    }

    @Test
    fun `check needs a selection`() {
        assertEquals(ExerciseState(), ExerciseReducers.checkQuizSingle(single, ExerciseState(), 1))
        assertEquals(ExerciseState(), ExerciseReducers.checkQuizMulti(multi, ExerciseState(), 1))
    }

    @Test
    fun `quiz multi toggles, checks and may be retried once with the first outcome kept`() {
        var s = ExerciseReducers.toggleMulti(ExerciseState(), "a")
        s = ExerciseReducers.toggleMulti(s, "b")
        s = ExerciseReducers.toggleMulti(s, "b")
        assertEquals(listOf("a"), s.selection)
        val first = ExerciseReducers.checkQuizMulti(multi, s, 1)
        assertEquals(Outcome.Partial, first.outcome)
        assertTrue(ExerciseReducers.canRetry(multi, first, locked = false))
        assertFalse(ExerciseReducers.canRetry(multi, first, locked = true))
        val retrying = ExerciseReducers.retry(first, clearSelection = true)
        assertTrue(retrying.retrying)
        assertTrue(retrying.selection.isEmpty())
        assertNull(retrying.outcome)
        assertTrue(retrying.done)
        val second =
            ExerciseReducers.checkQuizMulti(
                multi,
                ExerciseReducers.toggleMulti(ExerciseReducers.toggleMulti(retrying, "a"), "c"),
                2,
            )
        assertEquals(Outcome.Correct, second.outcome)
        assertEquals(Outcome.Partial, second.firstOutcome)
        assertEquals(1L, second.firstCheckedAtMs)
        assertEquals(2, second.attempts)
        assertFalse("no third attempt", ExerciseReducers.canRetry(multi, second, false))
    }

    @Test
    fun `a correct first attempt offers no retry`() {
        val s = ExerciseReducers.checkQuizMulti(multi, ExerciseState(selection = listOf("a", "c")), 1)
        assertFalse(ExerciseReducers.canRetry(multi, s, false))
    }

    @Test
    fun `fill blank keeps the typed text on retry and records per blank results`() {
        var s = ExerciseState()
        s = ExerciseReducers.setBlankText(s, "x", "UNO ")
        s = ExerciseReducers.setBlankText(s, "y", "tre")
        val checked = ExerciseReducers.checkFillBlank(fill, s, 5)
        assertEquals(Outcome.Partial, checked.outcome)
        assertEquals(mapOf("x" to true, "y" to false), checked.blankResults)
        assertEquals(checked, ExerciseReducers.setBlankText(checked, "x", "zzz"))
        val retry = ExerciseReducers.retry(checked, clearSelection = false)
        assertEquals("tre", retry.texts["y"])
        assertTrue(retry.blankResults.isEmpty())
        val second = ExerciseReducers.checkFillBlank(fill, ExerciseReducers.setBlankText(retry, "y", "2"), 6)
        assertEquals(Outcome.Correct, second.outcome)
        assertEquals(Outcome.Partial, second.firstOutcome)
    }

    @Test
    fun `order starts shuffled, move changes it, check pins the order and retry keeps it`() {
        val start = ExerciseReducers.currentOrder(order, ExerciseState())
        assertNotEquals(order.steps.map { it.id }, start)
        assertEquals(start, ExerciseReducers.currentOrder(order, ExerciseState()))
        val moved = ExerciseReducers.moveStep(order, ExerciseState(), 0, 3)
        assertEquals(start.drop(1) + start.first(), moved.order)
        assertEquals(moved, ExerciseReducers.moveStep(order, moved, 0, 9))
        val solved = ExerciseState(order = listOf("s1", "s2", "s3", "s4"))
        val ok = ExerciseReducers.checkOrder(order, solved, 1)
        assertEquals(Outcome.Correct, ok.outcome)
        val bad = ExerciseReducers.checkOrder(order, ExerciseState(), 1)
        assertEquals(Outcome.Incorrect, bad.outcome)
        assertEquals(start, bad.order)
        assertEquals(bad, ExerciseReducers.moveStep(order, bad, 0, 1))
        assertEquals(start, ExerciseReducers.retry(bad, false).order)
    }

    @Test
    fun `corrupt saved order falls back to the shuffle`() {
        val s = ExerciseState(order = listOf("s1", "zzz", "s3", "s4"))
        assertEquals(ExerciseReducers.currentOrder(order, ExerciseState()), ExerciseReducers.currentOrder(order, s))
    }

    @Test
    fun `flashcard rating reopens and changes`() {
        var s = ExerciseReducers.reveal(ExerciseState())
        assertFalse(s.done)
        s = ExerciseReducers.rate(s, Rating.Hard)
        assertTrue(s.done)
        s = ExerciseReducers.reopenRating(s)
        assertTrue(s.ratingOpen && s.done)
        s = ExerciseReducers.rate(s, Rating.Easy)
        assertEquals(Rating.Easy, s.rating)
        assertFalse(s.ratingOpen)
    }

    @Test
    fun `open answer is bounded and submits only with text`() {
        assertFalse(ExerciseReducers.canSubmit(ExerciseState()))
        val s = ExerciseReducers.setAnswer(ExerciseState(), "   ", 10)
        assertFalse(ExerciseReducers.canSubmit(s))
        val t = ExerciseReducers.setAnswer(s, "abcdefghijklmno", 10)
        assertEquals("abcdefghij", t.texts[ExerciseReducers.ANSWER_KEY])
        val sent = ExerciseReducers.submit(t)
        assertTrue(sent.done)
        assertEquals(sent, ExerciseReducers.setAnswer(sent, "other", 10))
    }

    @Test
    fun `checklist toggles items`() {
        var s = ExerciseReducers.toggleItem(ExerciseState(), "i1")
        s = ExerciseReducers.toggleItem(s, "i2")
        s = ExerciseReducers.toggleItem(s, "i1")
        assertEquals(listOf("i2"), s.selection)
        assertFalse(s.done)
    }

    @Test
    fun `why disclosure toggles`() {
        val open = ExerciseReducers.toggleWhy(ExerciseState(), "a")
        assertEquals(listOf("a"), open.whyOpen)
        assertTrue(ExerciseReducers.toggleWhy(open, "a").whyOpen.isEmpty())
    }
}
