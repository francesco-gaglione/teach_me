package com.teachme.feature.chat

import com.teachme.core.model.CHAT_MAX_CHARS

/** Shows the character counter from this many code points (chat.md section 13). */
internal const val CHAT_COUNTER_THRESHOLD = 7000

/** Length of a draft as the backend counts it: Unicode code points of the trimmed text (contract section 2). */
internal fun chatLength(text: String): Int = text.trim().let { it.codePointCount(0, it.length) }

internal data class ChatInputInfo(
    val length: Int,
) {
    val isEmpty: Boolean get() = length == 0
    val isTooLong: Boolean get() = length > CHAT_MAX_CHARS
    val overBy: Int get() = (length - CHAT_MAX_CHARS).coerceAtLeast(0)
    val showCounter: Boolean get() = length >= CHAT_COUNTER_THRESHOLD
    val isValid: Boolean get() = !isEmpty && !isTooLong
}

internal fun chatInputInfo(text: String) = ChatInputInfo(chatLength(text))
