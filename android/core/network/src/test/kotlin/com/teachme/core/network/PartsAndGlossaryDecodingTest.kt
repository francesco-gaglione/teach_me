package com.teachme.core.network

import com.teachme.core.model.MessagePart
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PartsAndGlossaryDecodingTest {
    private fun message(parts: String) =
        ApiJson
            .decodeFromString<ChatMessageDto>(
                """{"id":"m1","topic_id":"t1","role":"assistant","status":"complete","parts":$parts,
                    |"created_at":"2026-11-18T10:00:00Z","updated_at":"2026-11-18T10:00:00Z"}
                """.trimMargin(),
            ).toModel()

    private fun term(extra: String) =
        ApiJson
            .decodeFromString<GlossaryTermDto>("""{"id":"g1","key":"k","term":"ownership"$extra}""")
            .toModel()

    @Test
    fun lessonRequestPartIsParsed() {
        val parts = message("""[{"type":"lesson_request","job_id":"job-9"}]""").parts
        assertEquals(listOf<MessagePart>(MessagePart.LessonRequest("job-9")), parts)
    }

    @Test
    fun textAndLessonRequestInOneMessage() {
        val msg = message("""[{"type":"text","text":"Ci penso io."},{"type":"lesson_request","job_id":"j1"}]""")
        assertEquals(listOf(MessagePart.Text("Ci penso io."), MessagePart.LessonRequest("j1")), msg.parts)
        assertEquals("Ci penso io.", msg.plainText)
        assertEquals(true, msg.hasVisibleParts)
    }

    @Test
    fun lessonRequestAloneIsVisibleBecauseTheCardDrawsIt() {
        assertTrue(message("""[{"type":"lesson_request","job_id":"j1"}]""").hasVisibleParts)
    }

    @Test
    fun brokenLessonRequestBecomesUnknown() {
        val parts = message("""[{"type":"lesson_request"},{"type":"lesson_request","job_id":5}]""").parts
        assertEquals(listOf(MessagePart.Unknown, MessagePart.Unknown), parts)
    }

    @Test
    fun firstLessonIdIsReadWhenPresent() {
        assertEquals("l1", term(""","first_lesson_id":"l1","lesson_id":"l2"""").firstLessonId)
    }

    @Test
    fun firstLessonIdNullAndAbsentAreTheSame() {
        assertNull(term(""","first_lesson_id":null""").firstLessonId)
        assertNull(term("").firstLessonId)
    }
}
