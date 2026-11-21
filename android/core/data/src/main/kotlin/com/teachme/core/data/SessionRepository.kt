package com.teachme.core.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.teachme.core.model.AuthState
import com.teachme.core.model.AuthUser
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.LegacyDataClaimer
import com.teachme.core.model.LlmProvider
import com.teachme.core.model.RememberedUser
import com.teachme.core.model.Session
import com.teachme.core.model.SessionRepository
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.SignedOutReason
import com.teachme.core.model.UserScope
import com.teachme.core.model.normalizedBaseUrl
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

internal val Context.sessionDataStore: DataStore<Preferences> by preferencesDataStore(name = "session")

/**
 * The one owner of "who is signed in" (ADR 0008). The address and the token (wrapped) live in [SettingsRepository]; the
 * user profile and the reason a session ended live in [profile]. After the session ends the user stays remembered, so
 * the login screen can be prefilled and the queued data of that user stays theirs ([scope]).
 */
@Singleton
class DefaultSessionRepository(
    private val settings: SettingsRepository,
    private val profile: DataStore<Preferences>,
    private val legacy: LegacyDataClaimer,
    appScope: CoroutineScope,
) : SessionRepository {
    @Inject
    constructor(
        settings: SettingsRepository,
        @ApplicationContext context: Context,
        legacy: LegacyDataClaimer,
    ) : this(settings, context.sessionDataStore, legacy, CoroutineScope(SupervisorJob() + Dispatchers.IO))

    private val _state = MutableStateFlow<AuthState>(AuthState.Loading)
    override val state: StateFlow<AuthState> = _state.asStateFlow()

    private val lock = Mutex()

    init {
        appScope.launch { lock.withLock { _state.value = load() } }
    }

    override val scope: Flow<UserScope?> =
        state
            .filter { it !is AuthState.Loading }
            .map {
                when (it) {
                    is AuthState.SignedIn -> it.session.scope
                    is AuthState.SignedOut ->
                        it.user
                            ?.takeIf { _ ->
                                it.baseUrl.isNotBlank()
                            }?.let { u -> UserScope(u.id, it.baseUrl) }
                    AuthState.Loading -> null
                }
            }.distinctUntilChanged()

    override suspend fun current(): UserScope? = scope.first()

    override suspend fun awaitReady(): AuthState = state.first { it !is AuthState.Loading }

    private suspend fun load(): AuthState {
        val stored = settings.current()
        val prefs = profile.data.first()
        val remembered = remembered(prefs)
        if (stored.token.isNotEmpty() &&
            stored.userId.isNotEmpty() &&
            remembered != null &&
            remembered.id == stored.userId
        ) {
            return AuthState.SignedIn(Session(stored.baseUrl, stored.token, user(prefs, remembered)))
        }
        val reason =
            when {
                stored.baseUrl.isBlank() -> SignedOutReason.FirstRun
                // An address without any user ever: the single-user install after the update (its static token is gone).
                remembered == null -> SignedOutReason.Upgrade
                else ->
                    prefs[KEY_REASON]?.let { name -> SignedOutReason.entries.firstOrNull { it.name == name } }
                        ?: SignedOutReason.Expired
            }
        if (stored.token.isNotEmpty() || stored.userId.isNotEmpty()) {
            // A token without a user (the old static one) or one that cannot be matched: drop it for good.
            settings.update(BackendSettings(stored.baseUrl))
        }
        return AuthState.SignedOut(reason, stored.baseUrl, remembered)
    }

    override suspend fun signIn(
        baseUrl: String,
        token: String,
        user: AuthUser,
    ) = lock.withLock {
        val address = baseUrl.normalizedBaseUrl()
        // Data of the single-user install goes to the first user who signs in; it must be there before anyone reads.
        legacy.claim(UserScope(user.id, address))
        settings.update(BackendSettings(address, token, user.id))
        profile.edit { prefs ->
            writeUser(prefs, user)
            prefs.remove(KEY_REASON)
        }
        _state.value = AuthState.SignedIn(Session(address, token, user))
    }

    override suspend fun signOut() {
        end(SignedOutReason.LoggedOut, expectedToken = null)
    }

    override suspend fun expire(token: String): Boolean = end(SignedOutReason.Expired, expectedToken = token)

    private suspend fun end(
        reason: SignedOutReason,
        expectedToken: String?,
    ): Boolean =
        lock.withLock {
            val current = (_state.value as? AuthState.SignedIn)?.session ?: return@withLock false
            if (expectedToken != null && current.token != expectedToken) return@withLock false
            // A failed write (disk full) must not crash a sign-out or a 401: the session is over for the app now. If the
            // token is still on the disk, the next start finds it and the first 401 ends it again.
            try {
                settings.update(BackendSettings(current.baseUrl))
                profile.edit { it[KEY_REASON] = reason.name }
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                android.util.Log.w("Session", "cannot save the end of the session (${e::class.simpleName})")
            }
            _state.value = AuthState.SignedOut(reason, current.baseUrl, current.user.remembered())
            true
        }

    override suspend fun updateUser(user: AuthUser) {
        lock.withLock {
            val current = (_state.value as? AuthState.SignedIn)?.session ?: return@withLock
            if (current.user.id != user.id) return@withLock
            profile.edit { writeUser(it, user) }
            _state.value = AuthState.SignedIn(current.copy(user = user))
        }
    }

    private fun remembered(prefs: Preferences): RememberedUser? {
        val id = prefs[KEY_USER_ID]?.takeIf { it.isNotEmpty() } ?: return null
        return RememberedUser(id, prefs[KEY_USERNAME].orEmpty(), prefs[KEY_DISPLAY_NAME].orEmpty())
    }

    private fun user(
        prefs: Preferences,
        remembered: RememberedUser,
    ) = AuthUser(
        id = remembered.id,
        username = remembered.username,
        displayName = remembered.displayName,
        hasLlmKey = prefs[KEY_HAS_LLM_KEY] ?: false,
        llmProvider = LlmProvider.fromApi(prefs[KEY_LLM_PROVIDER]),
        ttsVoice = prefs[KEY_TTS_VOICE],
    )

    private fun writeUser(
        prefs: androidx.datastore.preferences.core.MutablePreferences,
        user: AuthUser,
    ) {
        prefs[KEY_USER_ID] = user.id
        prefs[KEY_USERNAME] = user.username
        prefs[KEY_DISPLAY_NAME] = user.displayName
        prefs[KEY_HAS_LLM_KEY] = user.hasLlmKey
        prefs[KEY_LLM_PROVIDER] = user.llmProvider.name.lowercase()
        // DataStore cannot hold null: no voice means no key.
        val voice = user.ttsVoice
        if (voice != null) prefs[KEY_TTS_VOICE] = voice else prefs.remove(KEY_TTS_VOICE)
    }

    private fun AuthUser.remembered() = RememberedUser(id, username, displayName)

    private companion object {
        val KEY_USER_ID = stringPreferencesKey("user_id")
        val KEY_USERNAME = stringPreferencesKey("username")
        val KEY_DISPLAY_NAME = stringPreferencesKey("display_name")
        val KEY_HAS_LLM_KEY = booleanPreferencesKey("has_llm_key")
        val KEY_LLM_PROVIDER = stringPreferencesKey("llm_provider")
        val KEY_TTS_VOICE = stringPreferencesKey("tts_voice")
        val KEY_REASON = stringPreferencesKey("signed_out_reason")
    }
}
