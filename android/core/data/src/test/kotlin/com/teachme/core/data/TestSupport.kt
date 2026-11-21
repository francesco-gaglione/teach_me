package com.teachme.core.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.LegacyDataClaimer
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.UserScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Rule
import org.junit.rules.TemporaryFolder

/** In memory, for the tests that are not about DataStore. */
internal class FakeSettings(
    initial: BackendSettings = BackendSettings(),
) : SettingsRepository {
    val state = MutableStateFlow(initial)
    override val settings: Flow<BackendSettings> = state

    override suspend fun current() = state.value

    override suspend fun update(settings: BackendSettings) {
        state.value = settings
    }
}

/** Remembers which scopes claimed the data of the single-user install. */
internal class RecordingClaimer : LegacyDataClaimer {
    val claims = mutableListOf<UserScope>()

    override suspend fun claim(scope: UserScope) {
        claims += scope
    }
}

/** Not a cipher at all, but it lets a test see what reaches the disk. */
internal class ReversingCipher : TokenCipher {
    override fun encrypt(plain: String) = "enc:" + plain.reversed()

    override fun decrypt(stored: String): String? =
        stored
            .removePrefix("enc:")
            .takeIf {
                stored.startsWith("enc:")
            }?.reversed()
}

/** DataStores over real files, as a new process would open them. */
internal abstract class DataStoreTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val scopes = mutableListOf<CoroutineScope>()

    @After
    fun closeStores() = scopes.forEach { it.cancel() }

    protected fun newScope(): CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.IO).also { scopes += it }

    protected fun store(name: String): DataStore<Preferences> =
        PreferenceDataStoreFactory.create(scope = newScope()) { folder.root.resolve("$name.preferences_pb") }

    /** The process dies: every open DataStore is closed. */
    protected fun killProcess() {
        // Joined: a DataStore on the same file must not be opened while the old one is still active.
        runBlocking { scopes.forEach { it.coroutineContext[Job]?.cancelAndJoin() } }
        scopes.clear()
    }
}
