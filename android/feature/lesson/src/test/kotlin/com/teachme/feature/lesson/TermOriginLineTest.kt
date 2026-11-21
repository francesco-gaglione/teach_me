package com.teachme.feature.lesson

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TermOriginLineTest {
    private val numbers = mapOf("A" to 2, "B" to 4)
    private val numberOf: (String) -> Int? = { numbers[it] }

    @Test
    fun sameLessonIsIntroducedOnly() {
        assertEquals(TermOriginLine.Introduced(2), termOriginLine("A", "A", numberOf))
    }

    @Test
    fun differentLessonsGiveBothNumbers() {
        assertEquals(TermOriginLine.IntroducedRevised(2, 4), termOriginLine("A", "B", numberOf))
    }

    @Test
    fun nullFirstIsRevisedOnlyNeverIntroduced() {
        assertEquals(TermOriginLine.Revised(4), termOriginLine(null, "B", numberOf))
    }

    @Test
    fun unresolvableHalfIsDropped() {
        assertEquals(TermOriginLine.Introduced(2), termOriginLine("A", "gone", numberOf))
        assertEquals(TermOriginLine.Revised(4), termOriginLine("gone", "B", numberOf))
    }

    @Test
    fun nothingResolvableMeansNoLine() {
        assertNull(termOriginLine("x", "y", numberOf))
        assertNull(termOriginLine("x", "x", numberOf))
        assertNull(termOriginLine(null, "y", numberOf))
        assertNull(termOriginLine(null, null, numberOf))
    }

    @Test
    fun openButtonStaysWhenTheNumberDoesNotResolve() {
        assertTrue(shouldOfferOpenLesson(LessonOrigin("gone", null, null), canOpen = true))
        assertFalse(shouldOfferOpenLesson(LessonOrigin("A", 2, "t"), canOpen = false))
        assertFalse(shouldOfferOpenLesson(null, canOpen = true))
    }
}
