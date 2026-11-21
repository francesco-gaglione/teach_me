package com.teachme.core.network

import com.teachme.core.model.ApiError
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import io.ktor.client.statement.HttpResponse
import io.ktor.http.HttpHeaders
import io.ktor.http.isSuccess
import kotlinx.coroutines.CancellationException
import java.time.Instant
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.time.format.DateTimeParseException

/**
 * Runs a backend call and turns every outcome into an [ApiResult]: success parsed by [parse],
 * HTTP errors mapped by [toFailure], transport errors mapped to [ApiFailure.Network].
 */
internal suspend fun <T> apiCall(
    request: suspend () -> HttpResponse,
    parse: suspend (HttpResponse) -> T,
): ApiResult<T> {
    val response =
        try {
            request()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            val reason = e.toUnreachableOrNull()
            return ApiResult.Failure(
                if (reason != null) ApiFailure.Network(reason) else ApiFailure.Unexpected(httpStatus = null),
            )
        }
    if (!response.status.isSuccess()) return ApiResult.Failure(response.toFailure(), response.serverDate())
    return try {
        ApiResult.Success(parse(response), response.serverDate())
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        // A 2xx whose body is not the documented shape (or a failure while reading it).
        val reason = e.toUnreachableOrNull()
        ApiResult.Failure(
            if (reason != null) ApiFailure.Network(reason) else ApiFailure.Unexpected(response.status.value),
        )
    }
}

/** Maps an error response (api-conventions §7). The `code` wins over the HTTP status when it is known. */
internal suspend fun HttpResponse.toFailure(): ApiFailure = toFailure(parseApiError())

/** Same, for a caller that already read the body (it can only be read once). */
internal fun HttpResponse.toFailure(error: ApiError?): ApiFailure {
    val http = status.value
    return when {
        http == 401 && error?.code == "invalid_credentials" -> ApiFailure.InvalidCredentials
        http == 401 || error?.code == "unauthorized" -> ApiFailure.Unauthorized
        http == 429 && error?.code == "rate_limited" ->
            ApiFailure.RateLimited(
                headers["Retry-After"]?.trim()?.toIntOrNull(),
            )
        http == 409 && error?.code == "llm_key_missing" -> ApiFailure.LlmKeyMissing
        http == 502 && error?.code == "llm_key_rejected" -> ApiFailure.LlmKeyRejected
        http == 502 && error?.code == "llm_credit_exhausted" -> ApiFailure.LlmCreditExhausted
        http == 503 && error?.code == "unavailable" -> ApiFailure.Unavailable
        error?.code == "invalid_id" -> ApiFailure.InvalidId
        http == 404 -> ApiFailure.NotFound
        http == 409 -> reviewConflictOf(error?.code) ?: ApiFailure.Conflict
        http == 422 && error?.code == "validation_failed" ->
            ApiFailure.Validation(error.details.orEmpty(), error.message)
        http == 400 && (error == null || error.code == "bad_request") ->
            ApiFailure.BadRequest(error?.message.orEmpty())
        else -> ApiFailure.Unexpected(http, error)
    }
}

/** The `Date` header (RFC 1123, whole seconds) as an instant; null when it is missing or not valid. */
internal fun HttpResponse.serverDate(): Instant? =
    headers[HttpHeaders.Date]?.let { raw ->
        try {
            ZonedDateTime.parse(raw.trim(), DateTimeFormatter.RFC_1123_DATE_TIME).toInstant()
        } catch (_: DateTimeParseException) {
            null
        }
    }

/** The review `409` codes (api-conventions section 6, reviews.md); null for any other code. */
internal fun reviewConflictOf(code: String?): ApiFailure.ReviewConflict? =
    when (code) {
        "already_reviewed" -> ApiFailure.ReviewConflict.AlreadyReviewed
        "card_suspended" -> ApiFailure.ReviewConflict.CardSuspended
        "nothing_to_undo" -> ApiFailure.ReviewConflict.NothingToUndo
        "undo_expired" -> ApiFailure.ReviewConflict.UndoExpired
        "card_superseded" -> ApiFailure.ReviewConflict.CardSuperseded
        else -> null
    }
