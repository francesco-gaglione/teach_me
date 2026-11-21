package com.teachme.core.data

import android.content.Context
import android.util.Log
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.FlashInbox
import com.teachme.core.model.InMemoryFlashInbox
import com.teachme.core.model.LessonStateStore
import com.teachme.core.model.LibraryStateRepository
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.SessionRepository
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.UserScope
import com.teachme.core.model.UserScopes
import com.teachme.core.network.UnauthorizedHandler
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton

internal val Context.settingsDataStore: DataStore<Preferences> by preferencesDataStore(name = "settings")

/**
 * Address, session token and the id of the user the token belongs to. The token is wrapped by a [TokenCipher] (Android
 * Keystore) before it reaches DataStore; the legacy plain `token` key of the single-user versions is never read (the
 * old static token is not valid any more) and is removed from the disk at the first read.
 */
class DataStoreSettingsRepository(
    private val store: DataStore<Preferences>,
    private val cipher: TokenCipher,
) : SettingsRepository {
    private fun Preferences.toSettings(): BackendSettings {
        val token = this[KEY_TOKEN_ENC]?.let(cipher::decrypt).orEmpty()
        return BackendSettings(
            baseUrl = this[KEY_BASE_URL].orEmpty(),
            token = token,
            // A token that cannot be read means no session: the user id would point at nothing.
            userId = if (token.isEmpty()) "" else this[KEY_USER_ID].orEmpty(),
        )
    }

    override val settings: Flow<BackendSettings> = store.data.map { it.toSettings() }

    /**
     * Every request asks for the settings: they are kept in memory (decrypting with the Keystore at each request would be
     * waste) and replaced by [update], the only writer. The first read also removes the old plain token of the single-user
     * versions from the disk at once.
     */
    @Volatile
    private var cached: BackendSettings? = null

    override suspend fun current(): BackendSettings {
        cached?.let { return it }
        val prefs = store.data.first()
        if (prefs.contains(KEY_LEGACY_TOKEN)) store.edit { it.remove(KEY_LEGACY_TOKEN) }
        return prefs.toSettings().also { cached = it }
    }

    override suspend fun update(settings: BackendSettings) {
        store.edit { prefs ->
            prefs[KEY_BASE_URL] = settings.baseUrl
            if (settings.token.isEmpty()) {
                prefs.remove(KEY_TOKEN_ENC)
                prefs.remove(KEY_USER_ID)
            } else {
                prefs[KEY_TOKEN_ENC] = cipher.encrypt(settings.token)
                prefs[KEY_USER_ID] = settings.userId
            }
            prefs.remove(KEY_LEGACY_TOKEN)
        }
        cached = settings
    }

    private companion object {
        val KEY_BASE_URL = stringPreferencesKey("base_url")
        val KEY_TOKEN_ENC = stringPreferencesKey("session_token")
        val KEY_USER_ID = stringPreferencesKey("session_user_id")
        val KEY_LEGACY_TOKEN = stringPreferencesKey("token")
    }
}

private val Context.libraryDataStore: DataStore<Preferences> by preferencesDataStore(name = "library_state")

/** Remembers the selected folder across restarts (home.md, Expanded: initial selection), one per user. */
@Singleton
class DataStoreLibraryStateRepository
    @Inject
    constructor(
        @ApplicationContext private val context: Context,
        private val scopes: UserScopes,
    ) : LibraryStateRepository {
        private suspend fun key() = scopes.current()?.let { stringPreferencesKey("last_folder_id:${it.key}") }

        override suspend fun lastFolderId(): String? {
            val key = key() ?: return null
            return context.libraryDataStore.data.first()[key]
        }

        override suspend fun setLastFolderId(id: String?) {
            val key = key() ?: return
            context.libraryDataStore.edit { prefs ->
                if (id == null) prefs.remove(key) else prefs[key] = id
            }
        }
    }

internal val Context.lessonDataStore: DataStore<Preferences> by preferencesDataStore(name = "lesson_state")

/** Separator between a user scope and the original name in the keys that are kept per user. */
internal const val SCOPE_SEPARATOR = "|"

/**
 * Lesson progress, one preference per lesson key. Writes go through their own scope so that leaving the
 * lesson screen right after an answer never loses it.
 */
@Singleton
class DataStoreLessonStateStore
    @Inject
    constructor(
        @ApplicationContext private val context: Context,
        private val scopes: UserScopes,
    ) : LessonStateStore {
        private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

        /** One entry per lesson key and user: progress and downloaded lessons are the user's (ADR 0008). */
        private fun keyOf(
            owner: UserScope,
            key: String,
        ) = stringPreferencesKey("${owner.key}$SCOPE_SEPARATOR$key")

        override suspend fun load(key: String): String? {
            val owner = scopes.current() ?: return null
            return context.lessonDataStore.data.first()[keyOf(owner, key)]
        }

        override fun save(
            key: String,
            json: String,
        ) {
            scope.launch {
                try {
                    val owner = scopes.current() ?: return@launch
                    context.lessonDataStore.edit { it[keyOf(owner, key)] = json }
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    // Disk full or similar: the progress of this session is lost, the app is not.
                    Log.w("LessonStateStore", "cannot write state of a lesson (${e::class.simpleName})")
                }
            }
        }
    }

@Module
@InstallIn(SingletonComponent::class)
abstract class DataModule {
    @Binds
    abstract fun bindSessionRepository(impl: DefaultSessionRepository): SessionRepository

    @Binds
    abstract fun bindUserScopes(impl: DefaultSessionRepository): UserScopes

    @Binds
    abstract fun bindLessonStateStore(impl: DataStoreLessonStateStore): LessonStateStore

    @Binds
    abstract fun bindLibraryStateRepository(impl: DataStoreLibraryStateRepository): LibraryStateRepository

    @Binds
    abstract fun bindLlmKeyMonitor(impl: DefaultLlmKeyMonitor): LlmKeyMonitor
}

@Module
@InstallIn(SingletonComponent::class)
object SessionProvidesModule {
    @Provides
    @Singleton
    fun provideTokenCipher(): TokenCipher = KeystoreTokenCipher()

    @Provides
    @Singleton
    fun provideSettingsRepository(
        @ApplicationContext context: Context,
        cipher: TokenCipher,
    ): SettingsRepository = DataStoreSettingsRepository(context.settingsDataStore, cipher)

    @Provides
    @Singleton
    fun provideUnauthorizedHandler(session: dagger.Lazy<SessionRepository>): UnauthorizedHandler =
        UnauthorizedHandler { token -> session.get().expire(token) }
}

@Module
@InstallIn(SingletonComponent::class)
object FlashModule {
    @Provides
    @Singleton
    fun provideFlashInbox(): FlashInbox = InMemoryFlashInbox()
}
