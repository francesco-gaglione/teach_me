package com.teachme.core.network

import com.teachme.core.model.ApiError
import com.teachme.core.model.ApiErrorResponse
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.ConnectionResult
import com.teachme.core.model.HealthResponse
import com.teachme.core.model.UnreachableReason
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.network.sockets.ConnectTimeoutException
import io.ktor.client.plugins.HttpRequestTimeoutException
import io.ktor.client.plugins.timeout
import io.ktor.client.request.get
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsText
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.CancellationException
import java.io.IOException
import java.net.ConnectException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import javax.inject.Inject
import javax.inject.Singleton
import javax.net.ssl.SSLException

internal const val TEST_CONNECT_TIMEOUT_MS = 5_000L
internal const val TEST_REQUEST_TIMEOUT_MS = 10_000L

/** Connection test against `GET /api/v1/health` (api-conventions §3). */
interface HealthApi {
    /** Tests the stored settings, or [override] if given (values typed but not yet saved). */
    suspend fun check(override: BackendSettings? = null): ConnectionResult
}

@Singleton
class KtorHealthApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : HealthApi {
        override suspend fun check(override: BackendSettings?): ConnectionResult =
            try {
                client
                    .get("/api/v1/health") {
                        // The connection test fails fast: tighter than the default timeouts of api-conventions §12.
                        timeout {
                            connectTimeoutMillis = TEST_CONNECT_TIMEOUT_MS
                            requestTimeoutMillis = TEST_REQUEST_TIMEOUT_MS
                        }
                        override?.let { attributes.put(BackendOverrideKey, it) }
                    }.toConnectionResult()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                e.toUnreachableOrNull()?.let { ConnectionResult.Unreachable(it) }
                    ?: ConnectionResult.Unexpected(httpStatus = null)
            }
    }

/** Maps transport failures to a reason; null means "not a connectivity problem". */
internal fun Throwable.toUnreachableOrNull(): UnreachableReason? {
    var cause: Throwable? = this
    while (cause != null) {
        when (cause) {
            is InvalidBackendUrlException, is SessionMismatchException -> return UnreachableReason.Other
            is HttpRequestTimeoutException,
            is ConnectTimeoutException,
            is SocketTimeoutException,
            -> return UnreachableReason.Timeout
            is UnknownHostException -> return UnreachableReason.UnknownHost
            is ConnectException -> return UnreachableReason.ConnectionRefused
            is SSLException -> return UnreachableReason.Tls
        }
        cause = cause.cause
    }
    return if (this is IOException) UnreachableReason.Other else null
}

internal suspend fun HttpResponse.toConnectionResult(): ConnectionResult =
    when {
        status == HttpStatusCode.OK ->
            try {
                ConnectionResult.Connected(body<HealthResponse>())
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                ConnectionResult.Unexpected(httpStatus = status.value)
            }
        status == HttpStatusCode.Unauthorized -> ConnectionResult.SessionEnded
        else -> ConnectionResult.Unexpected(httpStatus = status.value, error = parseApiError())
    }

/** Parses the §7 error body; returns null if the body is not in that format. */
internal suspend fun HttpResponse.parseApiError(): ApiError? =
    try {
        ApiJson.decodeFromString<ApiErrorResponse>(bodyAsText()).error
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        null
    }
