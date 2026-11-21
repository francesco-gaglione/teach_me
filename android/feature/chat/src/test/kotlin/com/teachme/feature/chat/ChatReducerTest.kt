package com.teachme.feature.chat

import com.teachme.core.model.ChatEvent
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.ChatRole
import com.teachme.core.model.MessagePart
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

class ChatReducerTest {
    private val streaming = msg("a2", status = ChatMessageStatus.Streaming, text = null, second = 2)

    @Test
    fun `upsert keeps messages ordered by creation time`() {
        val list = listOf(msg("m1"), msg("m3")).upsert(msg("m2"))
        assertEquals(listOf("m1", "m2", "m3"), list.map { it.id })
    }

    @Test
    fun `upsert of an identical message returns the same list`() {
        val list = listOf(msg("m1"))
        assertSame(list, list.upsert(msg("m1")))
    }

    @Test
    fun `an older version never replaces a newer one`() {
        val list = listOf(msg("a1", updated = 10)).upsert(msg("a1", status = ChatMessageStatus.Streaming, updated = 5))
        assertEquals(ChatMessageStatus.Complete, list.single().status)
    }

    @Test
    fun `a retry replaces the failed message with the streaming one`() {
        val failed = msg("a1", status = ChatMessageStatus.Failed, text = "parz", updated = 5)
        val list = listOf(failed).upsert(msg("a1", status = ChatMessageStatus.Streaming, text = null, updated = 9))
        assertEquals(emptyList<MessagePart>(), list.single().parts)
    }

    @Test
    fun `between two streaming versions the one reflecting the later event wins`() {
        val local =
            listOf(msg("a1", status = ChatMessageStatus.Streaming, text = "abcdef", updated = 1, lastEventId = 20))
        val stale = msg("a1", status = ChatMessageStatus.Streaming, text = "abc", updated = 2, lastEventId = 12)
        assertSame(local, local.upsert(stale))
        val fresher = msg("a1", status = ChatMessageStatus.Streaming, text = "abcdefg", updated = 2, lastEventId = 21)
        assertEquals(
            "abcdefg",
            (
                local
                    .upsert(fresher)
                    .single()
                    .parts
                    .single() as MessagePart.Text
            ).text,
        )
    }

    @Test
    fun `a delta already reflected in the message is not applied twice`() {
        val base = listOf(msg("a1", status = ChatMessageStatus.Streaming, text = "Perché vuoi", lastEventId = 11))
        var list = base.applyDelta(ChatEvent.MessageDelta("a1", 0, "ché "), 10)
        list = list.applyDelta(ChatEvent.MessageDelta("a1", 0, "vuoi"), 11)
        assertSame(base, list)
        list = list.applyDelta(ChatEvent.MessageDelta("a1", 0, " imparare"), 12)
        assertEquals("Perché vuoi imparare", (list.single().parts.single() as MessagePart.Text).text)
        assertEquals(12L, list.single().lastEventId)
        // Replaying the same delta again is a no-op.
        assertSame(list, list.applyDelta(ChatEvent.MessageDelta("a1", 0, " imparare"), 12))
    }

    @Test
    fun `deltas append in order and create the part when missing`() {
        var list = listOf(streaming)
        list = list.applyDelta(ChatEvent.MessageDelta("a2", 0, "Per"), 1)
        list = list.applyDelta(ChatEvent.MessageDelta("a2", 0, "ché "), 2)
        list = list.applyDelta(ChatEvent.MessageDelta("a2", 1, "Due"), 3)
        assertEquals(listOf(MessagePart.Text("Perché "), MessagePart.Text("Due")), list.single().parts)
    }

    @Test
    fun `message_completed replaces the accumulated text`() {
        var list = listOf(streaming).applyDelta(ChatEvent.MessageDelta("a2", 0, "bozza"), 1)
        list = list.applyEvent(ChatEvent.MessageCompleted(msg("a2", text = "definitivo", updated = 9)))
        assertEquals("definitivo", (list.single().parts.single() as MessagePart.Text).text)
        assertEquals(ChatMessageStatus.Complete, list.single().status)
    }

    @Test
    fun `deltas of a completed or unknown message are ignored`() {
        val done = listOf(msg("a1"))
        assertSame(done, done.applyDelta(ChatEvent.MessageDelta("a1", 0, "x"), 5))
        assertSame(done, done.applyDelta(ChatEvent.MessageDelta("zzz", 0, "x"), 5))
    }

    @Test
    fun `a delta on an unknown part is ignored but the text part index gap is padded`() {
        val unknownPart = streaming.copy(parts = listOf(MessagePart.Unknown))
        val list = listOf(unknownPart)
        assertSame(list, list.applyDelta(ChatEvent.MessageDelta("a2", 0, "x"), 5))
        val padded = list.applyDelta(ChatEvent.MessageDelta("a2", 2, "y"), 6)
        assertEquals(listOf(MessagePart.Unknown, MessagePart.Unknown, MessagePart.Text("y")), padded.single().parts)
    }

    @Test
    fun `message_failed keeps the partial text and the error`() {
        val failed =
            streaming.copy(
                status = ChatMessageStatus.Failed,
                parts = listOf(MessagePart.Text("parz")),
                updatedAt = streaming.updatedAt.plusSeconds(3),
            )
        val list = listOf(streaming).applyEvent(ChatEvent.MessageFailed(failed))
        assertEquals(ChatMessageStatus.Failed, list.single().status)
        assertEquals(ChatRole.Assistant, list.single().role)
    }

    @Test
    fun `input info counts code points not chars`() {
        assertEquals(4, chatInputInfo("  😀😀ab ").length)
        assertEquals(true, chatInputInfo("a".repeat(8001)).isTooLong)
        assertEquals(8001 - 8000, chatInputInfo("a".repeat(8001)).overBy)
        assertEquals(true, chatInputInfo("   ").isEmpty)
        assertEquals(false, chatInputInfo("a".repeat(6999)).showCounter)
        assertEquals(true, chatInputInfo("a".repeat(7000)).showCounter)
    }

    @Test
    fun `the content key ignores older messages prepended and reacts to the last message`() {
        val base = listOf(msg("m3", text = "c"), msg("m4", text = "d"))
        val key = lastMessageKey(base)
        assertEquals(key, lastMessageKey(listOf(msg("m1", text = "a")) + base))
        assertEquals(false, key == lastMessageKey(base + msg("m5", text = "e")))
        assertEquals(false, key == lastMessageKey(base.dropLast(1) + msg("m4", text = "dd")))
        assertEquals(
            false,
            key == lastMessageKey(base.dropLast(1) + msg("m4", status = ChatMessageStatus.Failed, text = "d")),
        )
    }

    @Test
    fun `mission events leave the messages untouched and a delta never lands on a card`() {
        val mission =
            com.teachme.core.model.Mission(
                "t1",
                1,
                com.teachme.core.model.MissionStatus.Draft,
                "x",
                listOf("a"),
                emptyList(),
                emptyList(),
                java.time.Instant.EPOCH,
                null,
                null,
            )
        val card = MessagePart.MissionProposal(mission, null)
        val list = listOf(msg("a1", status = ChatMessageStatus.Streaming, second = 1).copy(parts = listOf(card)))
        assertSame(list, list.applyEvent(ChatEvent.MissionProposed(mission)))
        assertSame(list, list.applyEvent(ChatEvent.MissionConfirmed(mission)))
        assertSame(list, list.applyDelta(ChatEvent.MessageDelta("a1", 0, "zzz"), 10))
    }

    private fun withCard(
        id: String,
        jobId: String,
    ) = msg(id).copy(parts = listOf(MessagePart.Text("ok"), MessagePart.LessonRequest(jobId)))

    @Test
    fun `a message with only a lesson request is visible`() {
        assertEquals(true, msg("a1").copy(parts = listOf(MessagePart.LessonRequest("j1"))).hasVisibleParts)
    }

    @Test
    fun `the most recent card of a job is the one that announces`() {
        val list = listOf(withCard("a1", "j1"), withCard("a2", "j2"), withCard("a3", "j1"))
        assertEquals("a3", list.latestLessonRequestMessageId("j1"))
        assertEquals("a2", list.latestLessonRequestMessageId("j2"))
        assertEquals(null, list.latestLessonRequestMessageId("j3"))
        assertEquals(true, list.hasLessonRequest("j1"))
        assertEquals(false, list.hasLessonRequest("j3"))
        assertEquals(setOf("a2", "a3"), list.lessonRequestAnnouncers())
    }
}
