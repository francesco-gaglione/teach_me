package com.teachme.core.network

import com.teachme.core.model.ApiResult
import com.teachme.core.model.DueReviews
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.SuspendedReviews
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.request.parameter
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.contentType
import io.ktor.http.path
import javax.inject.Inject
import javax.inject.Singleton

/** Raw endpoints of `docs/contracts/reviews.md` section 3. One call per endpoint. */
interface ReviewsApi {
    suspend fun due(
        limit: Int,
        topicId: String? = null,
    ): ApiResult<DueReviews>

    suspend fun rate(review: QueuedReview): ApiResult<ReviewCard>

    suspend fun undo(cardId: String): ApiResult<ReviewCard>

    suspend fun suspend(cardId: String): ApiResult<ReviewCard>

    suspend fun unsuspend(cardId: String): ApiResult<ReviewCard>

    suspend fun suspended(
        limit: Int,
        cursor: String?,
    ): ApiResult<SuspendedReviews>
}

@Singleton
class KtorReviewsApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : ReviewsApi {
        override suspend fun due(
            limit: Int,
            topicId: String?,
        ): ApiResult<DueReviews> =
            apiCall({
                client.get {
                    url { path("api", "v1", "reviews", "due") }
                    parameter("limit", limit)
                    if (topicId != null) parameter("topic_id", topicId)
                }
            }) { it.body<DueReviewsDto>().toModel() }

        override suspend fun rate(review: QueuedReview): ApiResult<ReviewCard> =
            apiCall({
                client.post {
                    url { path("api", "v1", "reviews", review.cardId) }
                    contentType(ContentType.Application.Json)
                    setBody(review.toRequest())
                }
            }) { it.body<ReviewCardDto>().toModel() }

        override suspend fun undo(cardId: String): ApiResult<ReviewCard> = action(cardId, "undo")

        override suspend fun suspend(cardId: String): ApiResult<ReviewCard> = action(cardId, "suspend")

        override suspend fun unsuspend(cardId: String): ApiResult<ReviewCard> = action(cardId, "unsuspend")

        override suspend fun suspended(
            limit: Int,
            cursor: String?,
        ): ApiResult<SuspendedReviews> =
            apiCall({
                client.get {
                    url { path("api", "v1", "reviews", "suspended") }
                    parameter("limit", limit)
                    if (cursor != null) parameter("cursor", cursor)
                }
            }) { it.body<SuspendedReviewsDto>().toModel() }

        private suspend fun action(
            cardId: String,
            name: String,
        ): ApiResult<ReviewCard> =
            apiCall({ client.post { url { path("api", "v1", "reviews", cardId, name) } } }) {
                it.body<ReviewCardDto>().toModel()
            }
    }
