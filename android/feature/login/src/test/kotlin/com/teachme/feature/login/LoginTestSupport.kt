package com.teachme.feature.login

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AuthState
import com.teachme.core.model.AuthUser
import com.teachme.core.model.LoginOutcome
import com.teachme.core.model.PendingCounts
import com.teachme.core.model.PendingSync
import com.teachme.core.model.SessionRepository
import com.teachme.core.model.UserScope
import com.teachme.core.network.AuthApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map

internal class FakeSessions(
    initial: AuthState,
) : SessionRepository {
    private val _state = MutableStateFlow(initial)
    override val state: StateFlow<AuthState> = _state
    val signIns = mutableListOf<Triple<String, String, AuthUser>>()
    var signInGate: (suspend () -> Unit)? = null
    var signInFails = false

    override val scope: Flow<UserScope?> = _state.filter { it !is AuthState.Loading }.map { null }

    override suspend fun current(): UserScope? = null

    override suspend fun awaitReady(): AuthState = _state.first { it !is AuthState.Loading }

    override suspend fun signIn(
        baseUrl: String,
        token: String,
        user: AuthUser,
    ) {
        signInGate?.invoke()
        if (signInFails) throw java.io.IOException("disk full")
        signIns += Triple(baseUrl, token, user)
    }

    override suspend fun signOut() = Unit

    override suspend fun expire(token: String) = false

    override suspend fun updateUser(user: AuthUser) = Unit
}

internal class FakeAuth : AuthApi {
    data class LoginCall(
        val baseUrl: String,
        val username: String,
        val password: String,
        val deviceName: String?,
    )

    val logins = mutableListOf<LoginCall>()
    var outcome: suspend () -> LoginOutcome = { LoginOutcome.InvalidCredentials }

    override suspend fun login(
        baseUrl: String,
        username: String,
        password: String,
        deviceName: String?,
    ): LoginOutcome {
        logins += LoginCall(baseUrl, username, password, deviceName)
        return outcome()
    }

    override suspend fun logout(
        baseUrl: String,
        token: String,
    ) = true

    override suspend fun me(): ApiResult<AuthUser> = ApiResult.Failure(ApiFailure.Unauthorized)

    override suspend fun changePassword(
        currentPassword: String,
        newPassword: String,
    ): ApiResult<Unit> = ApiResult.Success(Unit)

    override suspend fun setLlmKey(apiKey: String): ApiResult<AuthUser> = ApiResult.Failure(ApiFailure.Unauthorized)

    override suspend fun removeLlmKey(): ApiResult<Unit> = ApiResult.Success(Unit)

    override suspend fun setTtsVoice(voice: String): ApiResult<AuthUser> = ApiResult.Failure(ApiFailure.Unauthorized)
}

internal class FakePending(
    var counts: PendingCounts = PendingCounts(),
) : PendingSync {
    override suspend fun counts() = counts

    override suspend fun flush(timeoutMs: Long) = counts
}
