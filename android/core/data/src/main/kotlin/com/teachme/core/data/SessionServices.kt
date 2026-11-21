package com.teachme.core.data

import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.AuthState
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.SessionActions
import com.teachme.core.model.SessionRefresher
import com.teachme.core.model.SessionRepository
import com.teachme.core.network.AuthApi
import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

/** `GET /me` at start, back in the foreground (at most once a minute) and when Settings opens (users.md, llm-key.md). */
@Singleton
class DefaultSessionRefresher(
    private val sessions: SessionRepository,
    private val api: AuthApi,
    private val llmKey: LlmKeyMonitor,
    private val clock: AppClock,
) : SessionRefresher {
    @Inject
    constructor(
        sessions: SessionRepository,
        api: AuthApi,
        llmKey: LlmKeyMonitor,
    ) : this(sessions, api, llmKey, AppClock { System.currentTimeMillis() })

    private val lock = Mutex()
    private var lastAt = Long.MIN_VALUE

    override suspend fun refresh() = run(onlyIfStale = false)

    override suspend fun refreshIfStale() = run(onlyIfStale = true)

    private suspend fun run(onlyIfStale: Boolean) {
        lock.withLock {
            val before = (sessions.state.value as? AuthState.SignedIn)?.session ?: return
            val now = clock.nowMs()
            if (onlyIfStale && lastAt != Long.MIN_VALUE && now - lastAt < REFRESH_MIN_INTERVAL_MS) return
            lastAt = now
            val result = api.me()
            if (result !is ApiResult.Success) return // keep the cache; a 401 is the global handler's business
            val fresh = result.value
            // Another user signed in while the request was out: this answer is not theirs.
            if ((sessions.state.value as? AuthState.SignedIn)?.session?.token != before.token) return
            sessions.updateUser(fresh)
            if (fresh.hasLlmKey != before.user.hasLlmKey) {
                // The key was set or removed elsewhere (`user set-llm-key`): panels waiting for it may move on.
                if (fresh.hasLlmKey) llmKey.noteSaved(result.serverDate) else llmKey.noteRemoved()
            }
        }
    }

    private companion object {
        const val REFRESH_MIN_INTERVAL_MS = 60_000L
    }
}

/** Sign-out: the local session ends at once; the server is told afterwards, with its own scope so leaving a screen never cancels it. */
@Singleton
class DefaultSessionActions
    @Inject
    constructor(
        private val sessions: SessionRepository,
        private val api: AuthApi,
    ) : SessionActions {
        private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

        override suspend fun signOut() {
            val session = (sessions.state.value as? AuthState.SignedIn)?.session ?: return
            sessions.signOut()
            scope.launch(NonCancellable) { api.logout(session.baseUrl, session.token) }
        }
    }

@Module
@InstallIn(SingletonComponent::class)
abstract class SessionServicesModule {
    @Binds
    abstract fun bindRefresher(impl: DefaultSessionRefresher): SessionRefresher

    @Binds
    abstract fun bindActions(impl: DefaultSessionActions): SessionActions
}
