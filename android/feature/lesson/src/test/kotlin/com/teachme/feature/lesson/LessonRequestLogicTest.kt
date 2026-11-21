package com.teachme.feature.lesson

import com.teachme.core.model.JobResult
import com.teachme.core.model.JobStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LessonRequestLogicTest {
    private fun gen(
        status: JobStatus,
        id: String = "G1",
        step: String? = null,
        message: String? = null,
        done: Int? = null,
        errorCode: String? = null,
        result: JobResult? = null,
    ) = job(
        "lesson_generation",
        status,
        id = id,
        step = step,
        message = message,
        done = done,
        errorCode = errorCode,
        result = result,
    )

    private fun state(
        job: com.teachme.core.model.TopicJob?,
        latest: String? = "G1",
        titles: Map<String, String> = emptyMap(),
    ) = lessonRequestState(job, latest) { titles[it] }

    @Test
    fun `a job not in the loaded list is unknown, not an error`() {
        assertEquals(LessonRequestState.Unknown, state(null))
    }

    @Test
    fun `a job of another kind or with an unknown status is unknown`() {
        assertEquals(LessonRequestState.Unknown, state(job("research", JobStatus.Running)))
        assertEquals(LessonRequestState.Unknown, state(gen(JobStatus.Unknown)))
    }

    @Test
    fun `queued is queued`() {
        assertEquals(LessonRequestState.Queued, state(gen(JobStatus.Queued)))
    }

    @Test
    fun `running shows the current step`() {
        assertEquals(
            LessonRequestState.Running(GenerationPhase.Checking, repairing = false, message = null),
            state(gen(JobStatus.Running, step = "checking")),
        )
    }

    @Test
    fun `writing with a second document is a repair`() {
        val s = state(gen(JobStatus.Running, step = "writing", done = 2)) as LessonRequestState.Running
        assertEquals(true, s.repairing)
        val byMessage =
            state(
                gen(JobStatus.Running, step = "writing", message = "Correggo la lezione"),
            ) as LessonRequestState.Running
        assertEquals(false, byMessage.repairing)
    }

    @Test
    fun `an unknown step keeps only the message`() {
        assertEquals(
            LessonRequestState.Running(null, repairing = false, message = "Rifinisco"),
            state(gen(JobStatus.Running, step = "polishing", message = "Rifinisco")),
        )
        assertEquals(
            LessonRequestState.Running(null, repairing = false, message = null),
            state(gen(JobStatus.Running, step = "polishing", message = "  ")),
        )
    }

    @Test
    fun `succeeded gives the lesson and its title only when the list already has it`() {
        val done = gen(JobStatus.Succeeded, result = JobResult(lessonId = "L3", lessonNumber = 3))
        assertEquals(LessonRequestState.Ready("L3", 3, null), state(done))
        assertEquals(LessonRequestState.Ready("L3", 3, "Lifetimes"), state(done, titles = mapOf("L3" to "Lifetimes")))
    }

    @Test
    fun `Riprova is offered only on the last generation job of the Topic`() {
        val failed = gen(JobStatus.Failed, id = "G1", errorCode = "timeout")
        assertEquals(LessonRequestState.Failed("timeout", canRetry = true), state(failed, latest = "G1"))
        assertEquals(LessonRequestState.Failed("timeout", canRetry = false), state(failed, latest = "G2"))
        assertEquals(LessonRequestState.Failed("timeout", canRetry = false), state(failed, latest = null))
    }

    @Test
    fun `a retried job goes back to queued on the same card`() {
        assertEquals(LessonRequestState.Queued, state(gen(JobStatus.Queued, id = "G1"), latest = "G1"))
    }

    // --- announcements ---

    @Test
    fun `a step change is spoken, the same step again is not`() {
        val planning = LessonRequestState.Running(GenerationPhase.Planning, false, null)
        val writing = LessonRequestState.Running(GenerationPhase.Writing, false, "Scrivo la sezione 2")
        assertEquals(
            LessonRequestSpeech.Step(GenerationPhase.Planning, false),
            lessonRequestSpeech(LessonRequestState.Queued, planning),
        )
        assertEquals(LessonRequestSpeech.Step(GenerationPhase.Writing, false), lessonRequestSpeech(planning, writing))
        assertNull(lessonRequestSpeech(writing, writing.copy(message = "Scrivo la sezione 3")))
    }

    @Test
    fun `the end is spoken only after seeing it happen`() {
        val running = LessonRequestState.Running(GenerationPhase.Saving, false, null)
        assertEquals(
            LessonRequestSpeech.Ready(3),
            lessonRequestSpeech(running, LessonRequestState.Ready("L3", 3, null)),
        )
        assertEquals(
            LessonRequestSpeech.Failed(true),
            lessonRequestSpeech(running, LessonRequestState.Failed("timeout", true)),
        )
        assertEquals(
            LessonRequestSpeech.Failed(false),
            lessonRequestSpeech(LessonRequestState.Queued, LessonRequestState.Failed("timeout", false)),
        )
        // History loading: from unknown, or from a state that was already final, nothing is said.
        assertNull(lessonRequestSpeech(LessonRequestState.Unknown, LessonRequestState.Ready("L3", 3, null)))
        assertNull(lessonRequestSpeech(LessonRequestState.Failed("timeout", true), LessonRequestState.Queued))
    }

    @Test
    fun `an unknown step is never spoken`() {
        assertNull(lessonRequestSpeech(LessonRequestState.Queued, LessonRequestState.Running(null, false, "x")))
    }
}
