package com.teachme.core.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AudioManifest
import com.teachme.core.model.AudioPreferences
import com.teachme.core.model.AudioRepository
import com.teachme.core.model.PlaybackSpeed
import com.teachme.core.model.TtsVoices
import com.teachme.core.network.AudioApi
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

/** Thin adapter over [AudioApi]: audio has no cache on the device (clips are cached on disk by the player's OkHttp client, see ClipHttp). */
@Singleton
class DefaultAudioRepository
    @Inject
    constructor(
        private val api: AudioApi,
    ) : AudioRepository {
        override suspend fun getManifest(lessonId: String): ApiResult<AudioManifest> = api.getManifest(lessonId)

        override suspend fun getVoices(): ApiResult<TtsVoices> = api.getVoices()
    }

/**
 * Reading speed, kept on the device in the same DataStore as the backend address (it is not an account setting: audio.md
 * section 6). Settings and the lesson screen read the same flow.
 */
class DataStoreAudioPreferences(
    private val store: DataStore<Preferences>,
) : AudioPreferences {
    override val speed: Flow<PlaybackSpeed> =
        store.data.map { PlaybackSpeed.fromWire(it[KEY_SPEED]) }.distinctUntilChanged()

    override suspend fun setSpeed(speed: PlaybackSpeed) {
        store.edit { it[KEY_SPEED] = speed.wire }
    }

    private companion object {
        val KEY_SPEED = stringPreferencesKey("audio_speed")
    }
}

@Module
@InstallIn(SingletonComponent::class)
abstract class AudioBindingsModule {
    @Binds
    abstract fun bindAudioRepository(impl: DefaultAudioRepository): AudioRepository
}

@Module
@InstallIn(SingletonComponent::class)
object AudioPreferencesModule {
    @Provides
    @Singleton
    fun provideAudioPreferences(
        @ApplicationContext context: Context,
    ): AudioPreferences = DataStoreAudioPreferences(context.settingsDataStore)
}
