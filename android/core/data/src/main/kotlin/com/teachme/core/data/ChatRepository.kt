package com.teachme.core.data

import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatRepository
import com.teachme.core.model.ChatStreamSignal
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.observeEvent
import com.teachme.core.model.observeMessage
import com.teachme.core.network.ChatApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Thin adapter: the chat has no cache, every call goes to the backend. Every message and stream event that
 * says something about the key (a failure for a key code, a finished answer) is reported to [llmKey] on its way in, with
 * its own date: this is the one place where chat data enters the app (see [LlmKeyMonitor.noteObserved]).
 */
@Singleton
class DefaultChatRepository
    @Inject
    constructor(
        private val api: ChatApi,
        private val llmKey: LlmKeyMonitor,
    ) : ChatRepository {
        override suspend fun listMessages(
            topicId: String,
            cursor: String?,
        ) = api.listMessages(topicId, cursor, limit = CHAT_PAGE_LIMIT).alsoOnSuccess { page ->
            page.items.forEach(llmKey::observeMessage)
        }

        override suspend fun sendMessage(
            topicId: String,
            text: String,
            lessonId: String?,
        ) = api.sendMessage(topicId, text, lessonId).alsoOnSuccess {
            llmKey.observeMessage(it.user)
            llmKey.observeMessage(it.assistant)
        }

        override suspend fun retryMessage(
            topicId: String,
            messageId: String,
        ) = api.retryMessage(topicId, messageId).alsoOnSuccess(llmKey::observeMessage)

        override fun openEvents(
            topicId: String,
            lastEventId: Long?,
        ): Flow<ChatStreamSignal> =
            api.openEvents(topicId, lastEventId).onEach { signal ->
                if (signal is ChatStreamSignal.Event) llmKey.observeEvent(signal.event)
            }
    }

/** Default page size of the contract (section 8 of api-conventions). */
internal const val CHAT_PAGE_LIMIT = 50

internal inline fun <T> ApiResult<T>.alsoOnSuccess(action: (T) -> Unit): ApiResult<T> {
    if (this is ApiResult.Success) action(value)
    return this
}
