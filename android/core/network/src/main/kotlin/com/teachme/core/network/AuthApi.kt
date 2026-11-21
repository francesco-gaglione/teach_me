package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AuthUser
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.LoginOutcome
import com.teachme.core.model.normalizedBaseUrl
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.plugins.timeout
import io.ktor.client.request.delete
import io.ktor.client.request.get
import io.ktor.client.request.post
import io.ktor.client.request.put
import io.ktor.client.request.setBody
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentType
import io.ktor.http.isSuccess
import io.ktor.http.path
import kotlinx.coroutines.CancellationException
import javax.inject.Inject
import javax.inject.Singleton

internal const val LOGIN_CONNECT_TIMEOUT_MS = 5_000L

/** The password hash on the backend is slow on purpose (login.md): the total timeout is longer than the default. */
internal const val LOGIN_REQUEST_TIMEOUT_MS = 15_000L
internal const val LOGOUT_TIMEOUT_MS = 5_000L

/** Endpoints of `docs/contracts/users.md` section 2. */
interface AuthApi {
    /**
     * Checks the address with `GET /health` (no authentication), then signs in with `POST /auth/login`. The address
     * is the one typed, not yet saved. Never throws; the password is not kept anywhere.
     */
    suspend fun login(
        baseUrl: String,
        username: String,
        password: String,
        deviceName: String?,
    ): LoginOutcome

    /** `POST /auth/logout` with the given session, best effort: false when it did not go through. */
    suspend fun logout(
        baseUrl: String,
        token: String,
    ): Boolean

    /** `GET /me`. */
    suspend fun me(): ApiResult<AuthUser>

    /** `PUT /me/password`: `422` with `current_password: wrong`, `new_password: too_short | too_long`. */
    suspend fun changePassword(
        currentPassword: String,
        newPassword: String,
    ): ApiResult<Unit>

    /** `PUT /me/llm-key`: returns the user with `has_llm_key = true`. */
    suspend fun setLlmKey(apiKey: String): ApiResult<AuthUser>

    /** `DELETE /me/llm-key`. */
    suspend fun removeLlmKey(): ApiResult<Unit>

    /** `PUT /me/tts-voice` (audio.md section 4.2): `422 unknown_voice`, `409` when the feature is off. */
    suspend fun setTtsVoice(voice: String): ApiResult<AuthUser>
}

@Singleton
class KtorAuthApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : AuthApi {
        override suspend fun login(
            baseUrl: String,
            username: String,
            password: String,
            deviceName: String?,
        ): LoginOutcome {
            val address = baseUrl.normalizedBaseUrl()
            val target = BackendSettings(baseUrl = address)
            try {
                when (val alive = checkAlive(target)) {
                    null -> Unit
                    else -> return alive
                }
                val response =
                    client.post {
                        url { path("api", "v1", "auth", "login") }
                        attributes.put(BackendOverrideKey, target)
                        timeout {
                            connectTimeoutMillis = LOGIN_CONNECT_TIMEOUT_MS
                            requestTimeoutMillis = LOGIN_REQUEST_TIMEOUT_MS
                        }
                        contentType(ContentType.Application.Json)
                        setBody(LoginRequest(username, password, deviceName))
                    }
                return response.toLoginOutcome(address)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val reason =
                    e.toUnreachableOrNull() ?: return LoginOutcome.Unexpected(httpStatus = null, requestId = null)
                return LoginOutcome.Unreachable(reason)
            }
        }

        /** `GET /health`: null when it answers like a teach_me backend, else the outcome to show. */
        private suspend fun checkAlive(target: BackendSettings): LoginOutcome? {
            val response =
                client.get {
                    url { path("health") }
                    attributes.put(BackendOverrideKey, target)
                    timeout {
                        connectTimeoutMillis = LOGIN_CONNECT_TIMEOUT_MS
                        requestTimeoutMillis = LOGIN_REQUEST_TIMEOUT_MS
                    }
                }
            val status = response.status
            return when {
                status.isSuccess() -> {
                    val ok =
                        runCatching { ApiJson.decodeFromString<LivenessDto>(response.bodyAsText()).status == "ok" }
                            .getOrDefault(false)
                    if (ok) null else LoginOutcome.NotRecognized
                }
                status == HttpStatusCode.NotFound -> LoginOutcome.NotRecognized
                status == HttpStatusCode.MethodNotAllowed -> LoginOutcome.NotRecognized
                status.value == 503 -> LoginOutcome.NotReady
                else -> LoginOutcome.Unexpected(status.value, response.parseApiError()?.requestId)
            }
        }

        private suspend fun HttpResponse.toLoginOutcome(address: String): LoginOutcome {
            if (status.isSuccess()) {
                val session =
                    try {
                        body<SessionDto>()
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        return LoginOutcome.NotRecognized
                    }
                return LoginOutcome.Success(address, session.token, session.user.toModel())
            }
            val http = status.value
            val error = parseApiError()
            return when (val failure = toFailure(error)) {
                ApiFailure.InvalidCredentials -> LoginOutcome.InvalidCredentials
                is ApiFailure.RateLimited -> LoginOutcome.RateLimited(failure.retryAfterSecs)
                ApiFailure.Unavailable -> LoginOutcome.NotReady
                // The route does not exist: an older backend, or something that is not teach_me at all.
                ApiFailure.NotFound -> LoginOutcome.NotRecognized
                else -> if (http == 405) LoginOutcome.NotRecognized else LoginOutcome.Unexpected(http, error?.requestId)
            }
        }

        override suspend fun logout(
            baseUrl: String,
            token: String,
        ): Boolean =
            try {
                client
                    .post {
                        url { path("api", "v1", "auth", "logout") }
                        attributes.put(BackendOverrideKey, BackendSettings(baseUrl.normalizedBaseUrl(), token))
                        attributes.put(IgnoreUnauthorizedKey, true)
                        timeout {
                            connectTimeoutMillis = LOGOUT_TIMEOUT_MS
                            requestTimeoutMillis = LOGOUT_TIMEOUT_MS
                        }
                    }.status
                    .isSuccess()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                false
            }

        override suspend fun me(): ApiResult<AuthUser> =
            apiCall({ client.get { url { path("api", "v1", "me") } } }) { it.body<UserDto>().toModel() }

        override suspend fun changePassword(
            currentPassword: String,
            newPassword: String,
        ): ApiResult<Unit> =
            apiCall({
                client.put {
                    url { path("api", "v1", "me", "password") }
                    contentType(ContentType.Application.Json)
                    setBody(ChangePasswordRequest(currentPassword, newPassword))
                }
            }) { }

        override suspend fun setLlmKey(apiKey: String): ApiResult<AuthUser> =
            apiCall({
                client.put {
                    url { path("api", "v1", "me", "llm-key") }
                    contentType(ContentType.Application.Json)
                    setBody(SetLlmKeyRequest(apiKey))
                }
            }) { it.body<UserDto>().toModel() }

        override suspend fun removeLlmKey(): ApiResult<Unit> =
            apiCall({ client.delete { url { path("api", "v1", "me", "llm-key") } } }) { }

        override suspend fun setTtsVoice(voice: String): ApiResult<AuthUser> =
            apiCall({
                client.put {
                    url { path("api", "v1", "me", "tts-voice") }
                    contentType(ContentType.Application.Json)
                    setBody(SetTtsVoiceRequest(voice))
                }
            }) { it.body<UserDto>().toModel() }
    }
