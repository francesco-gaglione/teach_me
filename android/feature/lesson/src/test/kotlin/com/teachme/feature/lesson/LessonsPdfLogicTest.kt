package com.teachme.feature.lesson

import com.teachme.core.model.DocumentSource
import com.teachme.core.model.JobStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LessonsPdfLogicTest {
    private fun doc(
        title: String,
        a: Int,
        b: Int,
    ) = DocumentSource("d-$title", title, a, b)

    @Test
    fun `no sources no lines`() {
        assertTrue(pdfLines(emptyList()).lines.isEmpty())
    }

    @Test
    fun `single document has no title and single page is detected`() {
        val one = pdfLines(listOf(doc("R", 101, 118)))
        assertEquals(listOf(PdfLine(null, 101, 118)), one.lines)
        assertTrue(pdfLines(listOf(doc("R", 101, 101))).lines.single().singlePage)
    }

    @Test
    fun `several documents keep titles and more than three are cut`() {
        val five = pdfLines((1..5).map { doc("D$it", it, it + 1) })
        assertEquals(listOf("D1", "D2", "D3"), five.lines.map { it.title })
        assertEquals(2, five.more)
        assertEquals(0, pdfLines((1..3).map { doc("D$it", 1, 2) }).more)
    }

    @Test
    fun `reading step maps to its phase and is hidden unless seen`() {
        assertEquals(GenerationPhase.Reading, GenerationPhase.fromStep("reading"))
        val job = job("lesson_generation", JobStatus.Running, step = "reading")
        assertTrue(generationPhases(job, false).none { it.first == GenerationPhase.Reading })
        val shown = generationPhases(job, false, showReading = true)
        assertEquals(PhaseState.Current, shown.first { it.first == GenerationPhase.Reading }.second)
        assertEquals(PhaseState.Done, shown.first { it.first == GenerationPhase.Planning }.second)
        assertEquals(PhaseState.Todo, shown.first { it.first == GenerationPhase.Writing }.second)
    }
}
