package com.teachme.feature.chat

import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.lazy.LazyListState

/** Scroll constants of chat.md section 13. */
internal const val PREFETCH_OLDER_INDEX = 5
private const val FAR = 100_000f

/**
 * "At the bottom" (chat.md 7): the lower edge of the last item is at most [thresholdPx] below the lower
 * edge of the visible area.
 */
internal fun LazyListState.isAtBottom(thresholdPx: Float): Boolean {
    val info = layoutInfo
    val last = info.visibleItemsInfo.lastOrNull() ?: return true
    if (last.index != info.totalItemsCount - 1) return false
    val visibleEnd = info.viewportEndOffset - info.afterContentPadding
    return last.offset + last.size - visibleEnd <= thresholdPx
}

/** Jumps to the very bottom without animation. */
internal suspend fun LazyListState.scrollToBottom() {
    val lastIndex = layoutInfo.totalItemsCount - 1
    if (lastIndex < 0) return
    if (layoutInfo.visibleItemsInfo.lastOrNull()?.index != lastIndex) scrollToItem(lastIndex)
    scrollBy(FAR)
}

/** True if the top of the last item is still inside the visible area (the streaming message is "anchored"). */
internal fun LazyListState.lastItemTopVisible(): Boolean {
    val info = layoutInfo
    val last = info.visibleItemsInfo.lastOrNull() ?: return true
    return last.index != info.totalItemsCount - 1 || last.offset >= 0
}
