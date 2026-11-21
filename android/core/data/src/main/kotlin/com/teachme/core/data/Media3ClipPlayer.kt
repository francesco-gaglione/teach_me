package com.teachme.core.data

import android.content.Context
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.PlaybackParameters
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.okhttp.OkHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.ProgressiveMediaSource
import com.teachme.core.model.ClipPlayer
import com.teachme.core.model.ClipPlayerFactory
import com.teachme.core.model.PlaybackEnd
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.normalizedBaseUrl
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import java.io.File

private const val PITCH_UNCHANGED = 1f

/**
 * ExoPlayer behind [ClipPlayer]. A clip URL is relative to `/api/v1` (audio.md section 5): it is resolved against the
 * stored backend address and requested with the same `Authorization: Bearer` header as the API client, through the
 * disk-cached client of [ClipHttp] (a replay is served from the cache). The ExoPlayer
 * instance is created on the first [play] and released with [release].
 */
@UnstableApi
class Media3ClipPlayer(
    private val context: Context,
    private val settings: SettingsRepository,
    private val client: OkHttpClient,
) : ClipPlayer {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var player: ExoPlayer? = null
    private var starting: Job? = null
    private var onEnded: ((PlaybackEnd) -> Unit)? = null
    private var released = false
    private var speed = 1f

    private val generation = PlaybackGeneration()

    // An event of a clip replaced by play() or stop() can still arrive: only the current clip's ones count.
    private val listener =
        object : Player.Listener {
            override fun onPlaybackStateChanged(playbackState: Int) {
                val exo = player ?: return
                if (playbackState == Player.STATE_ENDED &&
                    exo.playbackState == Player.STATE_ENDED &&
                    generation.isCurrent(exo.currentMediaItem?.mediaId)
                ) {
                    finish(PlaybackEnd.Completed)
                }
            }

            override fun onPlayerError(error: PlaybackException) {
                if (generation.isCurrent(player?.currentMediaItem?.mediaId)) finish(PlaybackEnd.Error)
            }
        }

    private fun finish(end: PlaybackEnd) {
        val callback = onEnded
        onEnded = null
        player?.stop()
        callback?.invoke(end)
    }

    override fun play(
        clipUrl: String,
        onEnded: (PlaybackEnd) -> Unit,
    ) {
        if (released) return
        stop()
        this.onEnded = onEnded
        val mediaId = generation.next()
        starting =
            scope.launch {
                val current = settings.current()
                val url = ClipHttp.resolve(current.baseUrl.normalizedBaseUrl(), clipUrl)
                if (url == null) {
                    finish(PlaybackEnd.Error)
                    return@launch
                }
                val factory =
                    OkHttpDataSource
                        .Factory(client)
                        .setDefaultRequestProperties(ClipHttp.authorization(current.token))
                val item =
                    MediaItem
                        .Builder()
                        .setUri(url.toString())
                        .setMediaId(mediaId)
                        .build()
                val source = ProgressiveMediaSource.Factory(factory).createMediaSource(item)
                val exo = player ?: createPlayer().also { player = it }
                exo.playbackParameters = PlaybackParameters(speed, PITCH_UNCHANGED)
                exo.setMediaSource(source)
                exo.prepare()
                exo.play()
            }
    }

    override fun setSpeed(factor: Float) {
        speed = factor
        player?.playbackParameters = PlaybackParameters(factor, PITCH_UNCHANGED)
    }

    private fun createPlayer(): ExoPlayer =
        ExoPlayer
            .Builder(context)
            .setAudioAttributes(
                AudioAttributes
                    .Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_SPEECH)
                    .build(),
                true,
            ).setHandleAudioBecomingNoisy(true)
            .build()
            .also { it.addListener(listener) }

    override fun stop() {
        generation.invalidate()
        starting?.cancel()
        starting = null
        onEnded = null
        player?.stop()
        player?.clearMediaItems()
    }

    override fun release() {
        if (released) return
        stop()
        released = true
        player?.removeListener(listener)
        player?.release()
        player = null
        scope.cancel()
    }
}

@Module
@InstallIn(SingletonComponent::class)
object ClipPlayerModule {
    @Provides
    @androidx.annotation.OptIn(UnstableApi::class)
    fun provideClipPlayerFactory(
        @ApplicationContext context: Context,
        settings: SettingsRepository,
    ): ClipPlayerFactory {
        // One cache for the process: the player is created per lesson screen, a cache directory cannot be shared.
        val client = ClipHttp.client(File(context.cacheDir, "audio_clips"))
        return ClipPlayerFactory { Media3ClipPlayer(context, settings, client) }
    }
}
