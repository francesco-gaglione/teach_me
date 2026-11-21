package com.teachme.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Body of `GET /api/v1/health` (api-conventions §3). */
@Serializable
data class HealthResponse(
    val status: String,
    val version: String,
    @SerialName("api_version") val apiVersion: Int,
    @SerialName("catalog_version") val catalogVersion: Int,
    @SerialName("server_time") val serverTime: String,
)

/** Outcome of the connection test, the four states of api-conventions §3. */
sealed interface ConnectionResult {
    data class Connected(
        val health: HealthResponse,
    ) : ConnectionResult

    /** HTTP 401: the session ended. The global handler takes the user to the login; the test shows nothing. */
    data object SessionEnded : ConnectionResult

    /** Timeout, DNS failure, connection refused, TLS error, malformed or missing URL. */
    data class Unreachable(
        val reason: UnreachableReason = UnreachableReason.Other,
    ) : ConnectionResult

    /** Anything else; [httpStatus] is null if no status was received. */
    data class Unexpected(
        val httpStatus: Int?,
        val error: ApiError? = null,
    ) : ConnectionResult
}

/** Why the backend could not be reached; lets the UI say more than "unreachable". */
enum class UnreachableReason { Timeout, UnknownHost, ConnectionRefused, Tls, Other }
