package com.teachme.core.data

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.Page

/** Safety net against a backend that keeps returning cursors: 100 pages of 200 items. */
internal const val MAX_PAGES = 100
internal const val PAGE_LIMIT = 200

/** Follows `next_cursor` until the last page; the first failure aborts and is returned. */
internal suspend fun <T> fetchAll(fetchPage: suspend (cursor: String?) -> ApiResult<Page<T>>): ApiResult<List<T>> {
    val all = mutableListOf<T>()
    var cursor: String? = null
    repeat(MAX_PAGES) {
        when (val result = fetchPage(cursor)) {
            is ApiResult.Failure -> return result
            is ApiResult.Success -> {
                all += result.value.items
                cursor = result.value.nextCursor ?: return ApiResult.Success(all)
            }
        }
    }
    // The backend never ended the cursor chain: a truncated list would look complete, so fail instead.
    return ApiResult.Failure(ApiFailure.Unexpected(httpStatus = null))
}
