package com.teachme.core.network

import com.teachme.core.model.BackendSettings
import com.teachme.core.model.ExpectedUserScope
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.UserScope
import com.teachme.core.model.normalizedBaseUrl
import io.ktor.client.HttpClient
import io.ktor.client.engine.HttpClientEngine
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.api.createClientPlugin
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.request.header
import io.ktor.http.HttpHeaders
import io.ktor.http.URLBuilder
import io.ktor.http.Url
import io.ktor.http.encodedPath
import io.ktor.http.takeFrom
import io.ktor.serialization.kotlinx.json.json
import io.ktor.util.AttributeKey
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext

internal const val CONNECT_TIMEOUT_MS = 10_000L
internal const val REQUEST_TIMEOUT_MS = 30_000L

/** Request attribute: use these settings instead of the stored ones (connection test of unsaved values). */
internal val BackendOverrideKey = AttributeKey<BackendSettings>("TeachMeBackendOverride")

/** Request attribute: the token this request was sent with, so a `401` can be matched to the session it ended. */
internal val SentTokenKey = AttributeKey<String>("TeachMeSentToken")

/** Request attribute: a `401` on this call (logout) is not news, the global handler must not run. */
internal val IgnoreUnauthorizedKey = AttributeKey<Boolean>("TeachMeIgnoreUnauthorized")

/**
 * Reacts to a `401 unauthorized` on an authenticated request (login.md, global rule): one place for the whole app, so no
 * screen shows its own "invalid token" error. [token] is the one the request went out with.
 */
fun interface UnauthorizedHandler {
    suspend fun onUnauthorized(token: String)

    companion object {
        val None = UnauthorizedHandler { }
    }
}

/** Thrown when the stored base URL is missing or malformed. Treated as "unreachable". */
class InvalidBackendUrlException(
    message: String,
    cause: Throwable? = null,
) : RuntimeException(message, cause)

/** Thrown when the work belongs to a user who is not the signed-in one (see [ExpectedUserScope]): nothing is sent. */
class SessionMismatchException : RuntimeException("The session is not the one this request belongs to")

/**
 * Per-request plugin: reads the current session and applies base URL, `Authorization` and
 * `X-App-Version`. Callers pass paths relative to the base URL (e.g. `/api/v1/health`), so a
 * sign-in takes effect on the next request without rebuilding the client. A `401` on a request that carried a token is
 * reported to [unauthorized].
 */
private fun backendConfigPlugin(
    settings: SettingsRepository,
    appVersion: AppVersion,
    unauthorized: UnauthorizedHandler,
) = createClientPlugin("TeachMeBackendConfig") {
    onResponse { response ->
        val attributes = response.call.request.attributes
        val sent = attributes.getOrNull(SentTokenKey)
        if (response.status.value == 401 && sent != null && !attributes.contains(IgnoreUnauthorizedKey)) {
            unauthorized.onUnauthorized(sent)
        }
    }
    onRequest { request, _ ->
        val current = request.attributes.getOrNull(BackendOverrideKey) ?: settings.current()
        // Work done on behalf of one user never goes out with the session of another.
        currentCoroutineContext()[ExpectedUserScope]?.let { expected ->
            val live = UserScope(current.userId, current.baseUrl)
            if (expected.scope != live) throw SessionMismatchException()
        }
        val base = parseBaseUrl(current.baseUrl.normalizedBaseUrl())
        val path = request.url.encodedPath
        // takeFrom() replaces the whole URL: keep the query the caller built (cursor, limit).
        val query = request.url.parameters.build()
        request.url {
            takeFrom(URLBuilder(base))
            encodedPath = base.encodedPath.trimEnd('/') + path
            parameters.appendAll(query)
        }
        if (current.token.isNotBlank()) {
            request.header(HttpHeaders.Authorization, "Bearer ${current.token.trim()}")
            request.attributes.put(SentTokenKey, current.token)
        }
        request.header("X-App-Version", appVersion.value)
    }
}

internal fun parseBaseUrl(raw: String): Url {
    val text = raw.trim()
    if (!text.startsWith("http://") && !text.startsWith("https://")) {
        throw InvalidBackendUrlException("Base URL must start with http:// or https://")
    }
    return try {
        Url(text).also { if (it.host.isBlank()) throw InvalidBackendUrlException("Base URL has no host") }
    } catch (e: CancellationException) {
        throw e
    } catch (e: InvalidBackendUrlException) {
        throw e
    } catch (e: Exception) {
        throw InvalidBackendUrlException("Malformed base URL", e)
    }
}

/** Builds the Ktor client with the timeouts of api-conventions §12. [engine] is injectable for tests. */
fun createBackendHttpClient(
    engine: HttpClientEngine,
    settings: SettingsRepository,
    appVersion: AppVersion,
    unauthorized: UnauthorizedHandler = UnauthorizedHandler.None,
): HttpClient =
    HttpClient(engine) {
        expectSuccess = false
        // A redirect would send the Authorization header to another host: report it as unexpected.
        followRedirects = false
        install(ContentNegotiation) { json(ApiJson) }
        install(HttpTimeout) {
            connectTimeoutMillis = CONNECT_TIMEOUT_MS
            requestTimeoutMillis = REQUEST_TIMEOUT_MS
        }
        install(backendConfigPlugin(settings, appVersion, unauthorized))
    }
