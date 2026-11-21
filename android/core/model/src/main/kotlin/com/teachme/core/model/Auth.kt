package com.teachme.core.model

/** Outcome of the whole login: address check, then `POST /auth/login` (login.md, "Stati ed errori"). */
sealed interface LoginOutcome {
    data class Success(
        val baseUrl: String,
        val token: String,
        val user: AuthUser,
    ) : LoginOutcome {
        override fun toString(): String = "Success(baseUrl=$baseUrl, user=${user.id})"
    }

    /** `401 invalid_credentials`. */
    data object InvalidCredentials : LoginOutcome

    /** `429 rate_limited`; [retryAfterSecs] is null when `Retry-After` is missing or not a number. */
    data class RateLimited(
        val retryAfterSecs: Int?,
    ) : LoginOutcome

    /** Timeout, DNS, refused connection, TLS. */
    data class Unreachable(
        val reason: UnreachableReason,
    ) : LoginOutcome

    /** `503 unavailable`. */
    data object NotReady : LoginOutcome

    /** `404`, `405` or an answer that is not the documented JSON: not a teach_me backend (or not a multi-user one). */
    data object NotRecognized : LoginOutcome

    /** Anything else (`5xx`, `422`, `400`, an unknown code). */
    data class Unexpected(
        val httpStatus: Int?,
        val requestId: String?,
    ) : LoginOutcome
}

/** Why the user typed values are refused locally, before any request (login.md, "Validazione locale"). */
enum class AddressError { Empty, InvalidScheme, InvalidHost }

/** Local check of the backend address, before any request (login.md, "Validazione locale"). */
fun validateAddress(raw: String): AddressError? {
    val text = raw.normalizedBaseUrl()
    if (text.isEmpty()) return AddressError.Empty
    // "http://" alone is normalized to "http:" (trailing slashes removed): it has a scheme but no host.
    val validScheme = text.startsWith("http://") || text.startsWith("https://") || text == "http:" || text == "https:"
    if (!validScheme) return AddressError.InvalidScheme
    val host = runCatching { java.net.URI(text).host }.getOrNull()
    return if (host.isNullOrBlank()) AddressError.InvalidHost else null
}

/** `https://teach.tailnet.ts.net/api/v1` -> `teach.tailnet.ts.net`: what the screens show as the backend. */
fun hostOf(baseUrl: String): String = baseUrl.substringAfter("://").substringBefore('/').trim()
