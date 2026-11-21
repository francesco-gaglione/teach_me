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
import org.junit.Assert.assertTrue
import org.junit.Test

class GradingTest {
    private fun options(vararg ids: String) = ids.map { ChoiceOption(it, "opzione $it", "fb $it") }

    private val single = QuizSingle("q", question = "?", options = options("a", "b", "c"), correct = "b")
    private val multi =
        QuizMulti("m", question = "?", options = options("a", "b", "c", "d"), correct = listOf("a", "c"))

    @Test
    fun `quiz single`() {
        assertEquals(Outcome.Correct, Grading.quizSingle(single, "b"))
        assertEquals(Outcome.Incorrect, Grading.quizSingle(single, "a"))
        assertEquals(Outcome.Incorrect, Grading.quizSingle(single, null))
    }

    @Test
    fun `quiz multi is exact, partial on intersection, incorrect without`() {
        assertEquals(Outcome.Correct, Grading.quizMulti(multi, setOf("c", "a")))
        assertEquals(Outcome.Partial, Grading.quizMulti(multi, setOf("a")))
        assertEquals(Outcome.Partial, Grading.quizMulti(multi, setOf("a", "b")))
        assertEquals(Outcome.Partial, Grading.quizMulti(multi, setOf("a", "b", "c", "d")))
        assertEquals(Outcome.Incorrect, Grading.quizMulti(multi, setOf("b", "d")))
        assertEquals(Outcome.Incorrect, Grading.quizMulti(multi, emptySet()))
    }

    private fun fill(vararg blanks: Blank) =
        FillBlank(
            "f",
            text =
                blanks.joinToString(" ") {
                    "{{${it.id}}}"
                },
            blanks = blanks.toList(),
        )

    @Test
    fun `normalization trims collapses spaces lowercases and applies NFC`() {
        assertEquals("a b", Grading.normalize("  A \t  B ", caseSensitive = false))
        assertEquals("A B", Grading.normalize("  A   B ", caseSensitive = true))
        // "e" + combining acute vs precomposed
        assertEquals(Grading.normalize("café", false), Grading.normalize("café", false))
        // Turkish dotted I must not depend on the device locale
        assertEquals("i", Grading.normalize("I", false))
    }

    @Test
    fun `unicode spaces are compressed like ascii ones`() {
        assertEquals("a b", Grading.normalize("\u00a0a\u2009\u00a0b\u3000", caseSensitive = false))
        assertEquals("a b", Grading.normalize("a\u200b b", caseSensitive = false))
        val block = fill(Blank("b1", listOf("&mut self")))
        assertEquals(Outcome.Correct, Grading.fillBlank(block, mapOf("b1" to "&mut\u00a0self")).outcome)
    }

    @Test
    fun `fill blank accepts any listed answer after normalization`() {
        val block = fill(Blank("b1", listOf("move", "Moved")))
        assertEquals(Outcome.Correct, Grading.fillBlank(block, mapOf("b1" to "  MOVE ")).outcome)
        assertEquals(Outcome.Correct, Grading.fillBlank(block, mapOf("b1" to "moved")).outcome)
        assertEquals(Outcome.Incorrect, Grading.fillBlank(block, mapOf("b1" to "copy")).outcome)
        assertEquals(Outcome.Incorrect, Grading.fillBlank(block, emptyMap()).outcome)
    }

    @Test
    fun `fill blank case sensitive`() {
        val block = fill(Blank("b1", listOf("String"), caseSensitive = true))
        assertEquals(Outcome.Correct, Grading.fillBlank(block, mapOf("b1" to " String ")).outcome)
        assertEquals(Outcome.Incorrect, Grading.fillBlank(block, mapOf("b1" to "string")).outcome)
    }

    @Test
    fun `fill blank inner spaces are compressed`() {
        val block = fill(Blank("b1", listOf("&mut  x")))
        assertEquals(Outcome.Correct, Grading.fillBlank(block, mapOf("b1" to "&mut x")).outcome)
    }

    @Test
    fun `fill blank partial when some blanks are right`() {
        val block = fill(Blank("b1", listOf("uno")), Blank("b2", listOf("due")), Blank("b3", listOf("tre")))
        val r = Grading.fillBlank(block, mapOf("b1" to "uno", "b2" to "x", "b3" to "TRE"))
        assertEquals(Outcome.Partial, r.outcome)
        assertEquals(mapOf("b1" to true, "b2" to false, "b3" to true), r.perBlank)
        assertEquals(Outcome.Incorrect, Grading.fillBlank(block, mapOf("b1" to "x")).outcome)
    }

    @Test
    fun `order is exact or incorrect`() {
        val block = OrderSteps("o", prompt = "?", steps = listOf("x", "y", "z").map { OrderStep(it, it) })
        assertEquals(Outcome.Correct, Grading.order(block, listOf("x", "y", "z")))
        assertEquals(Outcome.Incorrect, Grading.order(block, listOf("x", "z", "y")))
    }

    @Test
    fun `fill blank needs every blank filled to be checked`() {
        val block = fill(Blank("b1", listOf("uno")), Blank("b2", listOf("due")))
        val s = ExerciseReducers.setBlankText(ExerciseState(), "b1", "uno")
        assertFalse(ExerciseReducers.fillBlankComplete(block, s))
        assertEquals(s, ExerciseReducers.checkFillBlank(block, s, 1L))
        val full = ExerciseReducers.setBlankText(s, "b2", "  ")
        assertFalse(ExerciseReducers.fillBlankComplete(block, full))
        assertTrue(ExerciseReducers.fillBlankComplete(block, ExerciseReducers.setBlankText(s, "b2", "x")))
    }
}
