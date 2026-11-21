package com.teachme.feature.folder

import com.teachme.core.model.ChatMessage
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.ChatRole
import com.teachme.core.model.MessagePart
import com.teachme.feature.mission.TopicTab
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

class LessonToastLogicTest {
    private fun card(jobId: String) =
        ChatMessage(
            id = "m-$jobId",
            topicId = "t1",
            role = ChatRole.Assistant,
            status = ChatMessageStatus.Complete,
            parts = listOf(MessagePart.LessonRequest(jobId)),
            error = null,
            createdAt = Instant.EPOCH,
            updatedAt = Instant.EPOCH,
        )

    @Test
    fun `never a toast on the Lessons tab`() {
        assertTrue(suppressLessonToast(TopicTab.Lessons, "j1", emptyList()))
    }

    @Test
    fun `on the chat the card of the same job replaces the toast`() {
        assertTrue(suppressLessonToast(TopicTab.Chat, "j1", listOf(card("j1"))))
        assertFalse(suppressLessonToast(TopicTab.Chat, "j2", listOf(card("j1"))))
        assertFalse(suppressLessonToast(TopicTab.Chat, null, listOf(card("j1"))))
    }

    @Test
    fun `other tabs always toast`() {
        assertFalse(suppressLessonToast(TopicTab.Progress, "j1", listOf(card("j1"))))
        assertFalse(suppressLessonToast(TopicTab.Mission, "j1", listOf(card("j1"))))
    }
}
