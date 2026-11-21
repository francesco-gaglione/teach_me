package com.teachme.feature.chat

import com.teachme.core.model.ChatMessage
import com.teachme.core.model.ChatRole
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

/** Space above a row (chat.md 4): none after a footer, tight between messages of the same role, a block otherwise. */
internal enum class RowGap { None, Tight, Block }

internal data class ChatRow(
    val message: ChatMessage,
    /** The "Tutor" label shows on the first assistant message of a series. */
    val showLabel: Boolean,
    /** Set when the previous message is from another local day (day separator above this row). */
    val day: LocalDate?,
    val gap: RowGap,
    val isLast: Boolean,
)

internal fun buildChatRows(
    messages: List<ChatMessage>,
    zone: ZoneId = ZoneId.systemDefault(),
): List<ChatRow> =
    messages.mapIndexed { index, message ->
        val previous = messages.getOrNull(index - 1)
        val date = message.createdAt.atZone(zone).toLocalDate()
        val newDay = previous != null && previous.createdAt.atZone(zone).toLocalDate() != date
        ChatRow(
            message = message,
            showLabel = message.role == ChatRole.Assistant && previous?.role != ChatRole.Assistant,
            day = if (newDay) date else null,
            gap =
                when {
                    previous == null -> RowGap.None
                    previous.role == ChatRole.Assistant && previous.status.hasFooter() -> RowGap.None
                    previous.role == message.role -> RowGap.Tight
                    else -> RowGap.Block
                },
            isLast = index == messages.lastIndex,
        )
    }

private fun com.teachme.core.model.ChatMessageStatus.hasFooter() =
    this == com.teachme.core.model.ChatMessageStatus.Complete

internal enum class DayKind { Today, Yesterday, ThisYear, OtherYear }

internal fun dayKind(
    date: LocalDate,
    today: LocalDate,
): DayKind =
    when {
        date == today -> DayKind.Today
        date == today.minusDays(1) -> DayKind.Yesterday
        date.year == today.year -> DayKind.ThisYear
        else -> DayKind.OtherYear
    }

internal fun today(zone: ZoneId = ZoneId.systemDefault()): LocalDate = Instant.now().atZone(zone).toLocalDate()
