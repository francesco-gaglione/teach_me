package com.teachme.core.model

/** One page of a cursor-paginated list (api-conventions §8). [nextCursor] null means last page. */
data class Page<T>(
    val items: List<T>,
    val nextCursor: String?,
)

/** Outcome of a backend call. Failures are values, never exceptions (except coroutine cancellation). */
sealed interface ApiResult<out T> {
    data class Success<T>(
        val value: T,
        /**
         * The `Date` header of the response, when it had a valid one: the server's clock, with a precision of one second.
         * It is the only instant of the server that comes with a success that carries none of its own (llm-key.md 9.4).
         */
        val serverDate: java.time.Instant? = null,
    ) : ApiResult<T>

    data class Failure(
        val failure: ApiFailure,
        /** The `Date` header of the failed response, when it had a valid one (see [Success.serverDate]). */
        val serverDate: java.time.Instant? = null,
    ) : ApiResult<Nothing>
}

/** Why a backend call failed; the UI decides the text from this, not from the backend `message`. */
sealed interface ApiFailure {
    /** Transport problem: no network, timeout, DNS, TLS, bad base URL. */
    data class Network(
        val reason: UnreachableReason,
    ) : ApiFailure

    /** HTTP 401 `unauthorized`: the session ended. A single handler reacts to it (login.md), screens stay silent. */
    data object Unauthorized : ApiFailure

    /** HTTP 401 `invalid_credentials`: wrong user name or password at login. */
    data object InvalidCredentials : ApiFailure

    /** HTTP 429 `rate_limited`; [retryAfterSecs] comes from `Retry-After` when present and numeric. */
    data class RateLimited(
        val retryAfterSecs: Int?,
    ) : ApiFailure

    /** HTTP 503 `unavailable`: the backend is starting or under maintenance. */
    data object Unavailable : ApiFailure

    /** HTTP 409 `llm_key_missing`: the user has no LLM key (users.md section 3). Nothing was created. */
    data object LlmKeyMissing : ApiFailure

    /** HTTP 502 `llm_key_rejected`: the LLM service refused the user's key. Nothing was created. */
    data object LlmKeyRejected : ApiFailure

    /**
     * HTTP 502 `llm_credit_exhausted`: the service accepted the key but the credit is used up (users.md section 3).
     * Nothing was created. Not a key problem: the key is fine, retrying later may work.
     */
    data object LlmCreditExhausted : ApiFailure

    /** HTTP 404 (`not_found`). */
    data object NotFound : ApiFailure

    /** HTTP 409 `conflict`: the resource is in a state that does not allow the action (e.g. a turn is running). */
    data object Conflict : ApiFailure

    /**
     * HTTP 409 with one of the review `code`s (reviews.md sections 3.2, 4 and 5). An unknown 409 code stays
     * [Conflict]. Only the reviews endpoints answer these.
     */
    sealed interface ReviewConflict : ApiFailure {
        /** `already_reviewed`: the card's version changed (the vote was already applied, or the card changed). */
        data object AlreadyReviewed : ReviewConflict

        /** `card_suspended`: a vote on a suspended card. */
        data object CardSuspended : ReviewConflict

        /** `nothing_to_undo`: never graded, or already undone. */
        data object NothingToUndo : ReviewConflict

        /** `undo_expired`: the vote is older than 10 minutes or from an earlier model. */
        data object UndoExpired : ReviewConflict

        /** `card_superseded`: unsuspending a card whose record was replaced. */
        data object CardSuperseded : ReviewConflict
    }

    /** HTTP 400 `invalid_id`. */
    data object InvalidId : ApiFailure

    /** HTTP 400 other than `invalid_id` (`bad_request`). */
    data class BadRequest(
        val message: String,
    ) : ApiFailure

    /** HTTP 422 `validation_failed`; [details] may be empty when the body had no field attribution. */
    data class Validation(
        val details: List<ApiErrorDetail>,
        val message: String = "",
    ) : ApiFailure {
        fun hasIssue(
            field: String,
            issue: String,
        ): Boolean = details.any { it.field == field && it.issue == issue }
    }

    /** Anything else; [httpStatus] is null when the response could not be decoded. */
    data class Unexpected(
        val httpStatus: Int?,
        val error: ApiError? = null,
    ) : ApiFailure
}

inline fun <T, R> ApiResult<T>.map(transform: (T) -> R): ApiResult<R> =
    when (this) {
        is ApiResult.Success -> ApiResult.Success(transform(value), serverDate)
        is ApiResult.Failure -> this
    }
