package com.teachme.core.model

import java.time.Instant

/** One recent ask for a missing component (`docs/contracts/component-requests.md`). */
data class ComponentRequestExample(
    val purpose: String,
    val lessonTitle: String,
    val topicTitle: String,
    val requestedAt: Instant?,
)

/** All the asks for the same `wanted` name, as the backend ranks them (count descending). */
data class ComponentRequestGroup(
    val wanted: String,
    val count: Int,
    val firstRequestedAt: Instant?,
    val lastRequestedAt: Instant?,
    val catalogVersion: Int,
    val examples: List<ComponentRequestExample>,
)

interface ComponentRequestRepository {
    /** One page of `GET /component-requests`; pass the previous `nextCursor` for the next one. */
    suspend fun list(cursor: String?): ApiResult<Page<ComponentRequestGroup>>
}
