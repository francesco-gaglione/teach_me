package com.teachme.feature.folder

import com.teachme.core.model.ChatMessage
import com.teachme.feature.chat.hasLessonRequest
import com.teachme.feature.mission.TopicTab

/**
 * Whether the toast about an ended generation is skipped (topic.md F3.C, chat.md design 10b.3): never on the Lessons tab,
 * and not on the chat when a card of that job is in it, because the card is the notice.
 */
internal fun suppressLessonToast(
    tab: TopicTab,
    endedJobId: String?,
    messages: List<ChatMessage>,
): Boolean =
    when (tab) {
        TopicTab.Lessons -> true
        TopicTab.Chat -> endedJobId != null && messages.hasLessonRequest(endedJobId)
        else -> false
    }
